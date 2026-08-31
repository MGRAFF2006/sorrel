/**
 * Collaboration convenience routes for CLI / agents.
 *
 * Mental model: Hub owns proposals + reviews; companions (CLI, hub-web) call
 * these endpoints rather than inventing Hub-local permission logic.
 */

import { HttpError, readJsonBody, sendJson, sendMethodNotAllowed } from '../http.js';
import { StoreNotFoundError } from '../store.js';

/**
 * POST /collaboration/lane-submit
 *
 * Creates (or reuses) an open proposal bound to a sync repo + source lane tip.
 * Idempotent when the same syncRepoId + sourceLane + sourceSnapshot already
 * has an open/draft proposal — returns that proposal instead of duplicating.
 */
export async function handleCollaborationRoute(request, response, context) {
  const { url } = context;
  const path = url.pathname.replace(/\/$/, '') || '/';

  if (path === '/collaboration/lane-submit') {
    if (request.method !== 'POST') {
      return sendMethodNotAllowed(response, ['POST']);
    }
    return await laneSubmit(request, response, context);
  }

  if (path === '/collaboration/proposal-summary') {
    if (request.method !== 'GET') {
      return sendMethodNotAllowed(response, ['GET']);
    }
    return proposalSummary(response, context);
  }

  throw new HttpError(404, 'collaboration route not found', 'not_found');
}

async function laneSubmit(request, response, { store, session }) {
  const body = await readJsonBody(request);

  if (!body || typeof body !== 'object' || Array.isArray(body)) {
    throw new HttpError(400, 'request body must be a JSON object', 'invalid_request_body');
  }

  const projectId = body.projectId;
  const sourceLane = body.sourceLane;
  const sourceSnapshot = body.sourceSnapshot;
  const title = body.title;
  const syncRepoId = body.syncRepoId ?? body.repositoryId;

  if (typeof projectId !== 'string' || !projectId.trim()) {
    throw new HttpError(400, 'projectId is required', 'invalid_request_body');
  }
  if (typeof sourceLane !== 'string' || !sourceLane.trim()) {
    throw new HttpError(400, 'sourceLane is required', 'invalid_request_body');
  }
  if (typeof sourceSnapshot !== 'string' || !sourceSnapshot.trim()) {
    throw new HttpError(400, 'sourceSnapshot is required', 'invalid_request_body');
  }
  if (typeof title !== 'string' || !title.trim()) {
    throw new HttpError(400, 'title is required', 'invalid_request_body');
  }

  // Prefer reusing an open/draft proposal for the same lane tip.
  const existing = store
    .listProposals({
      syncRepoId: typeof syncRepoId === 'string' ? syncRepoId : undefined,
      sourceLane: sourceLane.trim(),
    })
    .find(
      (proposal) =>
        proposal.sourceSnapshot === sourceSnapshot.trim() &&
        (proposal.status === 'open' || proposal.status === 'draft'),
    );

  if (existing) {
    sendJson(response, 200, {
      data: existing,
      reused: true,
    });
    return;
  }

  const open = body.open !== false;
  let proposal;
  try {
    proposal = store.createProposal({
      projectId: projectId.trim(),
      repositoryId: body.repositoryId,
      syncRepoId: typeof syncRepoId === 'string' ? syncRepoId.trim() : undefined,
      title: title.trim(),
      description: body.description,
      authorPrincipal:
        session?.principal ?? body.authorPrincipal ?? { type: 'user', id: 'local' },
      authorRef: body.authorRef,
      sourceLane: sourceLane.trim(),
      targetLane: body.targetLane ?? 'lane_main',
      sourceSnapshot: sourceSnapshot.trim(),
      targetSnapshot: body.targetSnapshot,
      sourceBranch: body.sourceBranch ?? sourceLane.trim(),
      targetBranch: body.targetBranch ?? 'main',
      status: open ? 'open' : 'draft',
      policyRefs: body.policyRefs,
      grantRefs: body.grantRefs,
      policyDecisionRefs: body.policyDecisionRefs,
      auditEventRefs: body.auditEventRefs,
      metadata: {
        ...(body.metadata ?? {}),
        submittedVia: 'collaboration.lane-submit',
      },
    });
  } catch (error) {
    if (error instanceof StoreNotFoundError) {
      throw new HttpError(404, error.message, error.code);
    }
    throw error;
  }

  sendJson(
    response,
    201,
    {
      data: proposal,
      reused: false,
    },
    {
      location: `/admin/proposals/${proposal.id}`,
    },
  );
}

function proposalSummary(response, { store, url }) {
  const projectId = url.searchParams.get('projectId') ?? undefined;
  const syncRepoId = url.searchParams.get('syncRepoId') ?? undefined;
  const proposals = store.listProposals({ projectId, syncRepoId });

  const byStatus = {};
  for (const proposal of proposals) {
    byStatus[proposal.status] = (byStatus[proposal.status] ?? 0) + 1;
  }

  sendJson(response, 200, {
    data: {
      total: proposals.length,
      byStatus,
      open: proposals.filter((p) => p.status === 'open' || p.status === 'draft'),
    },
  });
}
