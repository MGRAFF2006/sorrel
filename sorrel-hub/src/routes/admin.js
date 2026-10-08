import { createOrganization, createRepository, createProposal as normalizeProposal, createReviewComment, createWorkflowRun, createPolicy } from '../models.js';
import { decodePathComponent, HttpError, readJsonBody, sendJson, sendMethodNotAllowed } from '../http.js';
import { assertCoreAccess, assertCollectionRead, assertCollectionWrite, assertPrivilegedAdminAccess, bindSessionPrincipal, canAccess, filterCollection } from '../policy-guard.js';
import { createProposal, updateProposal } from '../proposal-mutations.js';
import { browseSnapshotChanges } from '../sync-browser.js';
import { StoreNotFoundError } from '../store.js';

const NORMALIZE = { organizations: createOrganization, repositories: createRepository,
  proposals: normalizeProposal, 'review-comments': createReviewComment,
  'workflow-runs': createWorkflowRun, policies: createPolicy };
const PRINCIPAL_FIELDS = { organizations: 'ownerPrincipal', repositories: 'linkedByPrincipal',
  proposals: 'authorPrincipal', 'review-comments': 'authorPrincipal', 'workflow-runs': 'requestedByPrincipal' };

const COLLECTIONS = {
  organizations: {
    create: 'createOrganization',
    list: 'listOrganizations',
    get: 'getOrganization',
    locationPrefix: '/admin/organizations',
  },
  repositories: {
    create: 'createRepository',
    list: 'listRepositories',
    get: 'getRepository',
    filters: ['organizationId', 'projectId'],
    locationPrefix: '/admin/repositories',
  },
  proposals: {
    create: 'createProposal',
    list: 'listProposals',
    get: 'getProposal',
    update: 'updateProposal',
    filters: ['projectId', 'repositoryId', 'syncRepoId', 'status', 'sourceLane'],
    locationPrefix: '/admin/proposals',
  },
  'review-comments': {
    create: 'createReviewComment',
    list: 'listReviewComments',
    get: 'getReviewComment',
    update: 'updateReviewComment',
    filters: ['proposalId', 'state'],
    locationPrefix: '/admin/review-comments',
  },
  'workflow-runs': {
    create: 'createWorkflowRun',
    list: 'listWorkflowRuns',
    get: 'getWorkflowRun',
    update: 'updateWorkflowRun',
    filters: ['projectId', 'proposalId', 'status'],
    locationPrefix: '/admin/workflow-runs',
  },
  policies: {
    create: 'createPolicy',
    list: 'listPolicies',
    get: 'getPolicy',
    filters: ['organizationId', 'projectId'],
    locationPrefix: '/admin/policies',
  },
};

/**
 * Parse `/admin/<collection>` or `/admin/<collection>/<id>` (+ optional subpath).
 * @param {string} pathname
 */
export function parseAdminPath(pathname) {
  const rest = pathname.slice('/admin/'.length);
  const segments = rest.split('/').filter(Boolean);
  if (segments.length > 3) {
    throw new HttpError(404, 'admin route not found', 'not_found');
  }
  return {
    collectionName: segments[0] ?? '',
    itemId: segments[1] ? decodePathComponent(segments[1]) : null,
    subResource: segments[2] ? decodePathComponent(segments[2]) : null,
  };
}

export async function handleAdminRoute(request, response, context) {
  const { collectionName, itemId, subResource } = parseAdminPath(context.url.pathname);

  if (collectionName === 'sync-repos') {
    if (itemId || subResource) {
      throw new HttpError(404, 'admin collection not found', 'not_found');
    }
    if (request.method === 'GET') {
      return await listSyncRepos(response, context);
    }
    return sendMethodNotAllowed(response, ['GET']);
  }

  const collection = Object.hasOwn(COLLECTIONS, collectionName) ? COLLECTIONS[collectionName] : undefined;

  if (!collection) {
    throw new HttpError(404, 'admin collection not found', 'not_found');
  }

  // GET /admin/proposals/:id/comments — nested review comments for a proposal
  if (collectionName === 'proposals' && itemId && subResource === 'changes' && request.method === 'GET') {
    const proposal = context.store.getProposal(itemId);
    await assertCollectionRead(context, 'proposals', proposal);
    if (proposal.syncRepoId) await assertCoreAccess(context, 'repo.read', { kind: 'repo', id: proposal.syncRepoId });
    return sendJson(response, 200, { data: browseSnapshotChanges(proposal, context.store.sync) });
  }
  if (
    collectionName === 'proposals' &&
    itemId &&
    subResource === 'comments' &&
    request.method === 'GET'
  ) {
    return await getProposalComments(response, context, itemId);
  }

  if (subResource) {
    throw new HttpError(404, 'admin collection not found', 'not_found');
  }

  if (itemId) {
    if (request.method === 'GET') {
      return await getCollectionItem(response, context, collection, itemId, collectionName);
    }
    if (request.method === 'PATCH' && collection.update) {
      return await updateCollectionItem(
        request,
        response,
        context,
        collection,
        itemId,
        collectionName,
      );
    }
    const allowed = collection.update ? ['GET', 'PATCH'] : ['GET'];
    return sendMethodNotAllowed(response, allowed);
  }

  if (request.method === 'GET') {
    return await listCollection(response, context, collection, collectionName);
  }

  if (request.method === 'POST') {
    return await createCollectionItem(request, response, context, collection, collectionName);
  }

  return sendMethodNotAllowed(response, ['GET', 'POST']);
}

async function listSyncRepos(response, context) {
  const repos = [];
  for (const id of context.store.sync.listRepos().slice().sort()) {
    if (await canAccess(context, 'repo.read', { kind: 'repo', id })) {
      repos.push({ id, refCount: context.store.sync.listRefs(id).length });
    }
  }
  sendJson(response, 200, { repos });
}

async function listCollection(response, context, collection, collectionName) {
  const { store, url } = context;
  const filters = Object.fromEntries((collection.filters ?? [])
    .map(name => [name, url.searchParams.get(name) ?? undefined]).filter(([, value]) => value !== undefined));
  sendJson(response, 200, { data: await filterCollection(context, collectionName, store[collection.list](filters)) });
}

async function getCollectionItem(response, context, collection, itemId, collectionName) {
  const item = context.store[collection.get](itemId);
  await assertCollectionRead(context, collectionName, item);
  if (collectionName === 'proposals' && context.url.searchParams.get('include') === 'comments') {
    const comments = await filterCollection(context, 'review-comments', context.store.listReviewComments({ proposalId: itemId }));
    return sendJson(response, 200, { data: { ...item, comments } });
  }
  return sendJson(response, 200, { data: item });
}

async function getProposalComments(response, context, proposalId) {
  await assertCollectionRead(context, 'proposals', context.store.getProposal(proposalId));
  const data = await filterCollection(context, 'review-comments', context.store.listReviewComments({ proposalId }));
  sendJson(response, 200, { data });
}

async function createCollectionItem(request, response, context, collection, collectionName) {
  const body = await readJsonBody(request, context.limits.requestBodyBytes);

  if (!body || typeof body !== 'object' || Array.isArray(body)) {
    throw new HttpError(400, 'request body must be a JSON object', 'invalid_request_body');
  }

  const field = PRINCIPAL_FIELDS[collectionName];
  const attributes = NORMALIZE[collectionName](field ? bindSessionPrincipal(body, context, field) : body);
  await assertCollectionWrite(context, collectionName, attributes);
  await assertPrivilegedAdminAccess(request, body, collectionName, context);

  let item;
  try {
    if (collectionName === 'proposals') {
      item = createProposal(attributes, context);
    } else {
      item = context.store[collection.create](attributes);
    }
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
      data: item,
    },
    {
      location: `${collection.locationPrefix}/${encodeURIComponent(item.id)}`,
    },
  );
}

async function updateCollectionItem(
  request,
  response,
  context,
  collection,
  itemId,
  collectionName,
) {
  const existing = context.store[collection.get](itemId);
  await assertCollectionRead(context, collectionName, existing);
  const body = await readJsonBody(request, context.limits.requestBodyBytes);

  if (!body || typeof body !== 'object' || Array.isArray(body)) {
    throw new HttpError(400, 'request body must be a JSON object', 'invalid_request_body');
  }

  await assertCollectionWrite(context, collectionName, body, existing);

  let item;
  try {
    item = collectionName === 'proposals'
      ? updateProposal(itemId, body, context)
      : context.store[collection.update](itemId, body);
  } catch (error) {
    if (error instanceof StoreNotFoundError) {
      throw new HttpError(404, error.message, error.code);
    }
    throw error;
  }

  sendJson(response, 200, { data: item });
}
