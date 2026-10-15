import path from 'node:path';
import { randomUUID, createHash } from 'node:crypto';
import { coreIntegration } from './core-integration.js';
import { FsRepoSyncStore, encodePathSegment } from './fs-sync-store.js';
import { HttpError, readJsonBody, sendJson } from './http.js';
import { mirrorProposal } from './proposal-mutations.js';
import { browseSnapshotChanges } from './sync-browser.js';
import { assertCollectionRead, assertProposalReferences } from './policy-guard.js';

const hash = value => createHash('sha256').update(JSON.stringify(value)).digest('hex');
const principal = context => ({ type: context.session.principal.type, id: context.session.principal.id });
const blocked = (message, details) => new HttpError(409, message, 'integration_blocked', details);
function fields(body, allowed) {
  if (Object.keys(body).some(key => !allowed.includes(key))) throw new HttpError(400, 'unsupported integration request field', 'invalid_request');
}
export function resolveRequiredChecks(env = process.env) {
  const config = JSON.parse(env.SORREL_HUB_REQUIRED_CHECKS_JSON ?? '{}');
  if (!config || typeof config !== 'object' || Array.isArray(config)) throw new Error('SORREL_HUB_REQUIRED_CHECKS_JSON must be a repository-keyed object');
  // Core strictly validates each versioned repository configuration at use time.
  return config;
}
export function createProposalIntegration(store, options) {
  const requiredChecks = options.requiredChecks ?? resolveRequiredChecks(options.env);
  const available = store.sync instanceof FsRepoSyncStore && typeof store.rootDir === 'string';
  const capabilities = { version: 1, available, requiredCheckReporting: 'operator-configured', nonAuthorApprovals: 1,
    limitation: available ? null : 'filesystem metadata and sync stores are required' };
  const write = (collection, record) => { store.storeRecord(collection, record); return record; };
  const lookup = (collection, id) => store[collection].get(id);
  function proposalInputs(proposal) {
    const provenance = lookup('proposalAuthors', proposal.id);
    if (!provenance || !proposal.authorPrincipal || provenance.principal.type !== proposal.authorPrincipal.type || provenance.principal.id !== proposal.authorPrincipal.id) {
      throw blocked('legacy or unverified proposal author; create a new authenticated proposal');
    }
    if (!proposal.syncRepoId) throw blocked('proposal requires a sync repository');
    const project = store.getProject(proposal.projectId);
    if (!project?.repositoryIds?.includes(proposal.syncRepoId) && store.getRepository(proposal.syncRepoId)?.projectId !== proposal.projectId) throw blocked('sync repository must be linked to the proposal project');
    const ref = (branch, lane) => {
      if (branch && lane && branch !== lane) throw blocked('branch and lane must name the same canonical ref');
      return branch || lane;
    };
    const sourceRef = ref(proposal.sourceBranch, proposal.sourceLane);
    const targetRef = ref(proposal.targetBranch, proposal.targetLane);
    if (!sourceRef || !targetRef) throw blocked('proposal requires explicit source and target refs');
    const sourceSnapshot = store.sync.getRef(proposal.syncRepoId, sourceRef);
    const targetSnapshot = store.sync.getRef(proposal.syncRepoId, targetRef);
    if (!sourceSnapshot || !targetSnapshot) throw blocked('source or target ref is missing');
    const sourceGeneration = store.sync.getRefRecord(proposal.syncRepoId, sourceRef).generation ?? 'legacy';
    const targetGeneration = store.sync.getRefRecord(proposal.syncRepoId, targetRef).generation ?? 'legacy';
    return { sourceGeneration, targetGeneration, proposalId: proposal.id, projectId: proposal.projectId, repositoryId: proposal.repositoryId ?? null,
      repoId: proposal.syncRepoId, sourceRef, targetRef, sourceSnapshot, targetSnapshot, author: provenance.principal,
      review: { proposal, workflows: (proposal.workflowRunIds ?? []).map(id => store.getWorkflowRun(id)), comments: store.listReviewComments({ proposalId: proposal.id }) } };
  }
  function request(context, proposal, operation, extra = {}) {
    const inputs = proposalInputs(proposal);
    return { operation, root: path.join(store.sync.rootDir, encodePathSegment(inputs.repoId)), inputs,
      requirements: requiredChecks[inputs.repoId] ?? null, principal: principal(context),
      grants: Object.values(context.trustedGrantsById), policies: Object.values(context.trustedPoliciesById), ...extra };
  }
  function evidence(binding) {
    return { binding, approvals: [...store.proposalApprovals.values()].filter(a => a.bindingId === binding.id).map(({ id, ...a }) => a),
      checks: [...store.proposalChecks.values()].filter(c => c.bindingId === binding.id).map(({ id, ...c }) => c) };
  }
  function finish(operation, receipt) {
    const proposal = store.getProposal(operation.proposalId);
    if (JSON.stringify(proposal) !== JSON.stringify(operation.originalProposal) && JSON.stringify(proposal) !== JSON.stringify(operation.intendedProposal)) {
      throw blocked('publication recovery found contradictory proposal metadata; operator repair required');
    }
    store.storeRecord('proposals', operation.intendedProposal);
    write('proposalResults', { id: operation.proposalId, receipt });
    mirrorProposal(operation.intendedProposal, options);
    return write('integrationOperations', { ...operation, state: 'complete', outcome: { state: 'integrated', receipt } });
  }
  function recover() {
    for (const operation of store.integrationOperations.values()) {
      if (operation.version !== 1 || !['pending', 'complete'].includes(operation.state) || operation.id !== operation.receipt?.operationId || operation.proposalId !== operation.receipt?.proposalId) throw blocked('unsupported or corrupt publication journal; operator repair required');
      if (operation.state !== 'pending') continue;
      const record = store.sync.getRefRecord(operation.repoId, operation.targetRef);
      if (record?.receipt && JSON.stringify(record.receipt) === JSON.stringify(operation.receipt) && record.snapshot === operation.receipt.candidateSnapshot) {
        // Retry every durability barrier before exposing a successful receipt.
        for (const id of operation.closure) store.sync.get(operation.repoId, id);
        store.sync.setRef(operation.repoId, operation.targetRef, record.snapshot, operation.closure, record.receipt, record.generation);
        finish(operation, record.receipt);
      } else if (record?.snapshot === operation.expectedTargetSnapshot && record?.receipt?.operationId !== operation.id) {
        write('integrationOperations', { ...operation, state: 'complete', outcome: { state: 'not_published', reason: 'publication did not commit; refresh readiness and retry with a new key' } });
      } else {
        throw blocked('publication recovery found contradictory ref evidence; no ref was overwritten');
      }
    }
  }
  async function handle(requestMessage, response, context, proposalId, subResource) {
    const proposal = store.getProposal(proposalId);
    await assertCollectionRead(context, 'proposals', proposal);
    await assertProposalReferences(context, proposal);
    if (!available) throw new HttpError(503, capabilities.limitation, 'integration_unavailable');
    if (!['integration', 'integration-changes'].includes(subResource) && requestMessage.method !== 'POST') throw new HttpError(405, 'this subresource requires POST', 'method_not_allowed');
    recover();
    const completed = lookup('proposalResults', proposalId);
    if (completed) {
      const operation = lookup('integrationOperations', completed.receipt?.operationId);
      if (!operation || operation.state !== 'complete' || operation.outcome?.state !== 'integrated' || JSON.stringify(operation.receipt) !== JSON.stringify(completed.receipt)) throw blocked('publication receipt has no completed journal evidence');
    }
    if (requestMessage.method === 'GET' && subResource === 'integration-changes') {
      const binding = lookup('proposalBindings', proposalId)?.binding;
      if (!binding?.candidateSnapshot || context.url.searchParams.get('bindingId') !== binding.id) throw blocked('explicit current candidate binding is required');
      return sendJson(response, 200, { data: browseSnapshotChanges({ syncRepoId: binding.inputs.repoId, targetSnapshot: binding.inputs.targetSnapshot, sourceSnapshot: binding.candidateSnapshot }, store.sync) });
    }
    if (requestMessage.method === 'GET' && subResource === 'integration') {
      if (completed) return sendJson(response, 200, { data: { state: 'integrated', receipt: completed.receipt } });
      const binding = lookup('proposalBindings', proposalId)?.binding;
      if (!binding) return sendJson(response, 200, { data: { state: 'unprepared', requiredChecksConfigured: Boolean(requiredChecks[proposal.syncRepoId]?.checks?.length) } });
      const readiness = await coreIntegration(request(context, proposal, 'readiness', evidence(binding)));
      return sendJson(response, 200, { data: { state: binding.conflicts.length ? 'conflicted' : readiness.ready ? 'ready' : 'blocked', binding, ...readiness } });
    }
    if (requestMessage.method !== 'POST') throw new HttpError(405, 'integration subresource requires GET or POST', 'method_not_allowed');
    const body = await readJsonBody(requestMessage, context.limits.requestBodyBytes);
    if (subResource === 'integrate') {
      fields(body, ['bindingId', 'expectedTargetSnapshot', 'idempotencyKey']);
      if (typeof body.idempotencyKey !== 'string' || body.idempotencyKey.length < 8 || body.idempotencyKey.length > 128) throw new HttpError(400, 'idempotencyKey must be 8–128 characters', 'invalid_request');
      const operationId = hash([proposalId, body.idempotencyKey]);
      const retry = lookup('integrationOperations', operationId);
      const operationInput = { bindingId: body.bindingId, expectedTargetSnapshot: body.expectedTargetSnapshot, idempotencyKey: body.idempotencyKey, principal: principal(context) };
      if (retry) {
        if (JSON.stringify(retry.request) !== JSON.stringify(operationInput)) throw blocked('idempotency key was already used with different inputs');
        return sendJson(response, 200, { data: retry.outcome });
      }
      if (completed) throw blocked('proposal already has a publication receipt');
      const binding = lookup('proposalBindings', proposalId)?.binding;
      if (!binding || binding.id !== body.bindingId || binding.inputs.targetSnapshot !== body.expectedTargetSnapshot) throw blocked('binding or expected target mismatch');
      const evaluation = request(context, proposal, 'integrate', evidence(binding));
      const evaluatedInputs = JSON.stringify(evaluation);
      const result = await coreIntegration(evaluation);
      if (!result.ready) throw blocked('integration remains blocked', { blockers: result.blockers });
      // All ref writers and proposal/evidence mutations share the enclosing queue.
      // Re-read after the asynchronous Core evaluation immediately before CAS.
      if (store.sync.getRef(binding.inputs.repoId, binding.inputs.targetRef) !== body.expectedTargetSnapshot || store.sync.getRef(binding.inputs.repoId, binding.inputs.sourceRef) !== binding.inputs.sourceSnapshot) throw blocked('source or target moved');
      if (evaluatedInputs !== JSON.stringify(request(context, store.getProposal(proposalId), 'integrate', evidence(binding)))) throw blocked('review inputs, requirements or authority changed during evaluation');
      context.datasetLease?.assert();
      const receipt = { version: 1, operationId, bindingId: binding.id, proposalId, repoId: binding.inputs.repoId,
        targetRef: binding.inputs.targetRef, previousSnapshot: body.expectedTargetSnapshot, candidateSnapshot: binding.candidateSnapshot,
        sourceSnapshot: binding.inputs.sourceSnapshot, publishedBy: principal(context), publishedAt: new Date().toISOString() };
      const operation = write('integrationOperations', { version: 1, id: operationId, binding, proposalId, repoId: receipt.repoId,
        targetRef: receipt.targetRef, expectedTargetSnapshot: body.expectedTargetSnapshot, request: operationInput,
        originalProposal: proposal, intendedProposal: { ...proposal, status: 'merged', updatedAt: receipt.publishedAt },
        receipt, closure: result.closure, state: 'pending' });
      options.integrationFault?.('journal');
      store.sync.publishRef(receipt.repoId, receipt.targetRef, body.expectedTargetSnapshot, receipt.candidateSnapshot, result.closure, receipt, () => {
        context.datasetLease?.assert();
        if (result.authorityValidUntil && Date.now() >= Date.parse(result.authorityValidUntil)) throw blocked('authority lifetime changed; retry with fresh validation');
      });
      options.integrationFault?.('ref');
      const outcome = finish(operation, receipt).outcome;
      options.integrationFault?.('metadata');
      return sendJson(response, 200, { data: outcome });
    }
    if (completed) throw blocked('proposal already has a publication receipt');
    if (subResource === 'integration') {
      fields(body, []);
      if (!['open', 'approved'].includes(proposal.status)) throw blocked('open the proposal before preparing integration');
      const result = await coreIntegration(request(context, proposal, 'prepare', { nonce: randomUUID() }));
      write('proposalBindings', { id: proposalId, binding: result.binding });
      return sendJson(response, 201, { data: { state: result.binding.conflicts.length ? 'conflicted' : 'prepared', binding: result.binding } });
    }
    const binding = lookup('proposalBindings', proposalId)?.binding;
    if (!binding || binding.id !== body.bindingId) throw blocked('explicit current bindingId is required');
    if (subResource === 'approvals') {
      fields(body, ['bindingId', 'candidateSnapshot']);
      if (body.candidateSnapshot !== binding.candidateSnapshot) throw blocked('approval candidate mismatch');
      const result = await coreIntegration(request(context, proposal, 'approve', evidence(binding)));
      write('proposalApprovals', { id: hash([binding.id, context.session.principal.type, context.session.principal.id]), ...result.approval });
      return sendJson(response, 201, { data: result.approval });
    }
    fields(body, ['bindingId', 'candidateSnapshot', 'sourceSnapshot', 'targetSnapshot', 'requirementsDigest', 'checkId', 'attempt', 'runId', 'outcome']);
    const result = await coreIntegration(request(context, proposal, 'report', { ...evidence(binding), report: { ...body, principal: principal(context) } }));
    write('proposalChecks', { id: hash([binding.id, body.checkId, body.attempt]), ...result.report });
    return sendJson(response, 201, { data: result.report });
  }
  return { capabilities, recover, handle };
}
