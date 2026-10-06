import { HttpError } from './http.js';
import { POLICY_ACTION_GRANT, PolicyDeniedError, PolicyEvaluationError, evaluateWithTrustedGrants } from './core-policy.js';

const ACTING_PRINCIPAL_HEADER = 'x-sorrel-acting-principal';

const PRIVILEGED_ADMIN_COLLECTIONS = new Set(['repositories', 'policies']);

/**
 * AuthAdapter identity is authoritative. Acting headers require explicit local
 * demo mode; authorization always happens in Core via configured grants.
 *
 * @param {import('node:http').IncomingMessage} request
 * @param {{ session?: { principal?: { type: string, id: string } } | null }} [context]
 * @returns {{ type: string, id: string }}
 */
export function resolveActingPrincipal(request, context = {}) {
  const fromSession = context.session?.principal;
  if (
    fromSession &&
    typeof fromSession === 'object' &&
    typeof fromSession.type === 'string' &&
    typeof fromSession.id === 'string'
  ) {
    return fromSession;
  }

  if (!context.localDemo || context.authAdapter?.mode !== 'dev') {
    throw new HttpError(401, 'a verified Hub session is required', 'authentication_required');
  }
  return parseActingPrincipal(request);
}

/** Bind attribution to the verified or explicit-demo session identity. */
export function bindSessionPrincipal(attributes, context, field = 'authorPrincipal') {
  if (!context.session?.principal) return attributes;
  const principal = context.session.principal;
  const bound = { ...attributes, [field]: principal };
  if (field === 'authorPrincipal') bound.authorRef = `${principal.type}:${principal.id}`;
  return bound;
}

export function parseActingPrincipal(request) {
  const rawHeader = request.headers[ACTING_PRINCIPAL_HEADER];
  const rawValue = Array.isArray(rawHeader) ? rawHeader[0] : rawHeader;

  if (!rawValue) {
    throw new HttpError(403, 'acting principal is required for privileged admin actions', 'policy_denied');
  }

  try {
    const principal = JSON.parse(rawValue);
    if (!principal || typeof principal !== 'object' || typeof principal.type !== 'string' || typeof principal.id !== 'string') {
      throw new Error('invalid principal shape');
    }
    return principal;
  } catch {
    throw new HttpError(403, 'acting principal header must be valid JSON', 'policy_denied');
  }
}

export async function assertPrivilegedAdminAccess(request, body, collectionName, context) {
  if (!PRIVILEGED_ADMIN_COLLECTIONS.has(collectionName)) {
    return undefined;
  }

  const actingPrincipal = resolveActingPrincipal(request, context);
  const grantRefs = body.grantRefs ?? [];
  const resource = collectionName === 'repositories'
    ? projectScope(context, body.projectId) : collectionResource(collectionName, body, context);
  const policyContext = {
    trustedPoliciesById: context.trustedPoliciesById ?? {},
    policyRef: body.policyRef,
    authorityRootRef: body.authorityRootRef,
    policyRefs: body.policyRefs ?? [],
  };

  try {
    return await evaluateWithTrustedGrants(
      actingPrincipal,
      POLICY_ACTION_GRANT,
      resource,
      grantRefs,
      context.trustedGrantsById ?? {},
      policyContext,
    );
  } catch (error) {
    if (error instanceof PolicyEvaluationError) {
      throw new HttpError(error.statusCode ?? 400, error.message, error.code);
    }
    throw error;
  }
}

/** Evaluate exact action/resource pairs once per request; never cache across sessions. */
export async function assertCoreAccess(context, action, resource) {
  const key = JSON.stringify([action, resource]);
  context.authorization ??= new Map();
  if (!context.authorization.has(key)) {
    context.authorization.set(key, evaluateWithTrustedGrants(
      context.session.principal, action, resource, [], context.trustedGrantsById,
      { trustedPoliciesById: context.trustedPoliciesById },
    ));
  }
  return await context.authorization.get(key);
}

export async function canAccess(context, action, resource) {
  try { await assertCoreAccess(context, action, resource); return true; }
  catch (error) { if (error instanceof PolicyDeniedError) return false; throw error; }
}

function notFound() { throw new HttpError(404, 'resource not found', 'not_found'); }
function scopeString(value, name) {
  if (typeof value !== 'string' || !value.trim()) {
    throw new HttpError(400, `${name} is required`, 'invalid_request_body');
  }
  return value.trim();
}

export function projectScope(context, id) {
  const project = context.store.getProject(scopeString(id, 'projectId'));
  if (!project) notFound();
  return { kind: 'project', id: project.id };
}

function proposalScope(context, id) {
  const proposal = context.store.getProposal(scopeString(id, 'proposalId'));
  if (!proposal) notFound();
  return collectionResource('proposals', proposal, context);
}

export function collectionResource(collection, item, context) {
  if (collection === 'organizations') return { kind: 'org', id: scopeString(item.id ?? '*', 'id') };
  if (collection === 'review-comments') return proposalScope(context, item.proposalId);
  if (collection === 'policies' && !item.projectId) {
    return { kind: 'org', id: scopeString(item.organizationId, 'organizationId') };
  }
  const project = projectScope(context, item.projectId);
  if (item.organizationId && context.store.getProject(project.id).organizationId !== item.organizationId.trim()) {
    throw new HttpError(400, 'organizationId must match the project organization', 'model_validation_failed');
  }
  if (collection === 'proposals' && item.repositoryId) {
    const repository = context.store.getRepository(item.repositoryId);
    if (!repository) notFound();
    if (repository.projectId !== project.id) throw new HttpError(400, 'repositoryId must belong to the project', 'model_validation_failed');
  }
  if (collection === 'workflow-runs' && item.proposalId && proposalScope(context, item.proposalId).id !== project.id) {
    throw new HttpError(400, 'proposalId must belong to the project', 'model_validation_failed');
  }
  if (collection === 'repositories' && item.id) return { kind: 'repo', id: item.id };
  return project;
}

export const READ_ACTIONS = {
  organizations: 'org.read', repositories: 'repo.read', policies: 'policy.read',
  proposals: 'proposal.read', 'review-comments': 'review.comment.read', 'workflow-runs': 'workflow.run.read',
};
const WRITE_ACTIONS = {
  organizations: 'org.write', repositories: POLICY_ACTION_GRANT, policies: POLICY_ACTION_GRANT,
  proposals: 'proposal.write', 'review-comments': 'review.comment.write', 'workflow-runs': 'workflow.run.write',
};

export async function assertCollectionRead(context, collection, item) {
  if (!item) notFound();
  let resource;
  try { resource = collectionResource(collection, item, context); }
  catch (error) { if (error instanceof HttpError && ['not_found', 'model_validation_failed'].includes(error.code)) notFound(); throw error; }
  if (!await canAccess(context, READ_ACTIONS[collection], resource)) notFound();
}

export async function filterCollection(context, collection, items) {
  const visible = [];
  for (const item of items) {
    let resource;
    try { resource = collectionResource(collection, item, context); }
    catch (error) { if (error instanceof HttpError && ['not_found', 'model_validation_failed'].includes(error.code)) continue; throw error; }
    if (await canAccess(context, READ_ACTIONS[collection], resource)) visible.push(item);
  }
  return visible;
}

/** Guard existing records and their actual parents before considering client updates. */
export async function assertCollectionWrite(context, collection, body, existing) {
  if (existing) {
    await assertCollectionRead(context, collection, existing);
    for (const field of ['id', 'projectId', 'organizationId', 'proposalId']) {
      if (body[field] !== undefined && body[field] !== existing[field]) {
        throw new HttpError(400, `${field} cannot change`, 'immutable_parent');
      }
    }
  }
  const item = existing ? { ...existing, ...body } : body;
  // Resolve the original stored scope; proposed references are checked separately.
  const resource = existing ? collectionResource(collection, existing, context) :
    collection === 'organizations' || collection === 'review-comments' || collection === 'policies' && !body.projectId
      ? collectionResource(collection, body, context) : projectScope(context, body.projectId);
  if (!existing && collection !== 'organizations') {
    const readAction = collection === 'repositories' ? 'project.read' : READ_ACTIONS[collection];
    if (!await canAccess(context, readAction, resource)) notFound();
  }
  if (body.organizationId && resource.kind === 'project' &&
      context.store.getProject(resource.id).organizationId !== body.organizationId) {
    throw new HttpError(400, 'organizationId must match the project organization', 'model_validation_failed');
  }
  if (collection === 'review-comments' && !existing) {
    await assertCollectionRead(context, 'proposals', context.store.getProposal(item.proposalId));
  }
  await assertCoreAccess(context, WRITE_ACTIONS[collection], resource);
  if (collection === 'proposals') {
    if (['approved', 'merged', 'rejected'].includes(body.status) && body.status !== existing?.status) {
      await assertCoreAccess(context, 'proposal.review', resource);
    }
    await assertProposalReferences(context, item);
  }
  if (collection === 'workflow-runs' && item.proposalId) {
    const proposal = context.store.getProposal(item.proposalId);
    await assertCollectionRead(context, 'proposals', proposal);
    const parent = proposalScope(context, item.proposalId);
    if (parent.id !== resource.id) throw new HttpError(400, 'proposalId must belong to the project', 'model_validation_failed');
    await assertCoreAccess(context, 'proposal.read', parent);
  }
}

export async function assertProposalReferences(context, item) {
  const project = projectScope(context, item.projectId);
  if (item.repositoryId) {
    const repository = context.store.getRepository(scopeString(item.repositoryId, 'repositoryId'));
    if (!repository) notFound();
    await assertCollectionRead(context, 'repositories', repository);
    if (repository.projectId !== project.id) throw new HttpError(400, 'repositoryId must belong to the project', 'model_validation_failed');
  }
  if (item.syncRepoId) await assertCoreAccess(context, 'repo.read', { kind: 'repo', id: scopeString(item.syncRepoId, 'syncRepoId') });
  if (item.workflowRunIds !== undefined && !Array.isArray(item.workflowRunIds)) {
    throw new HttpError(400, 'workflowRunIds must be an array', 'invalid_request_body');
  }
  for (const id of item.workflowRunIds ?? []) {
    const run = context.store.getWorkflowRun(id);
    if (!run) notFound();
    await assertCollectionRead(context, 'workflow-runs', run);
    if (run.projectId !== project.id) throw new HttpError(400, 'workflowRunIds must belong to the project', 'model_validation_failed');
  }
}
