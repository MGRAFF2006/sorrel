import { HttpError, readJsonBody, sendJson, sendMethodNotAllowed } from '../http.js';
import { assertPrivilegedAdminAccess } from '../policy-guard.js';
import { StoreNotFoundError } from '../store.js';

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
  return {
    collectionName: segments[0] ?? '',
    itemId: segments[1] ? decodeURIComponent(segments[1]) : null,
    subResource: segments[2] ? decodeURIComponent(segments[2]) : null,
  };
}

export async function handleAdminRoute(request, response, context) {
  const { collectionName, itemId, subResource } = parseAdminPath(context.url.pathname);

  if (collectionName === 'sync-repos') {
    if (itemId || subResource) {
      throw new HttpError(404, 'admin collection not found', 'not_found');
    }
    if (request.method === 'GET') {
      return listSyncRepos(response, context);
    }
    return sendMethodNotAllowed(response, ['GET']);
  }

  const collection = COLLECTIONS[collectionName];

  if (!collection) {
    throw new HttpError(404, 'admin collection not found', 'not_found');
  }

  // GET /admin/proposals/:id/comments — nested review comments for a proposal
  if (
    collectionName === 'proposals' &&
    itemId &&
    subResource === 'comments' &&
    request.method === 'GET'
  ) {
    return getProposalComments(response, context, itemId);
  }

  if (subResource) {
    throw new HttpError(404, 'admin collection not found', 'not_found');
  }

  if (itemId) {
    if (request.method === 'GET') {
      return getCollectionItem(response, context, collection, itemId, collectionName);
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
    return listCollection(response, context, collection);
  }

  if (request.method === 'POST') {
    return await createCollectionItem(request, response, context, collection, collectionName);
  }

  return sendMethodNotAllowed(response, ['GET', 'POST']);
}

function listSyncRepos(response, { store }) {
  const repos = store.sync
    .listRepos()
    .slice()
    .sort()
    .map((id) => ({
      id,
      refCount: store.sync.listRefs(id).length,
    }));

  sendJson(response, 200, { repos });
}

function listCollection(response, { store, url }, collection) {
  const filters = Object.fromEntries(
    (collection.filters ?? [])
      .map((filterName) => [filterName, url.searchParams.get(filterName) ?? undefined])
      .filter(([, value]) => value !== undefined),
  );

  sendJson(response, 200, {
    data: store[collection.list](filters),
  });
}

function getCollectionItem(response, { store, url }, collection, itemId, collectionName) {
  const item = store[collection.get](itemId);
  if (!item) {
    throw new HttpError(404, `${singular(collectionName)} ${itemId} not found`, 'not_found');
  }

  if (collectionName === 'proposals' && url.searchParams.get('include') === 'comments') {
    sendJson(response, 200, {
      data: {
        ...item,
        comments: store.listReviewComments({ proposalId: itemId }),
      },
    });
    return;
  }

  sendJson(response, 200, { data: item });
}

function getProposalComments(response, { store }, proposalId) {
  if (!store.getProposal(proposalId)) {
    throw new HttpError(404, `proposal ${proposalId} not found`, 'not_found');
  }
  sendJson(response, 200, {
    data: store.listReviewComments({ proposalId }),
  });
}

async function createCollectionItem(request, response, context, collection, collectionName) {
  const body = await readJsonBody(request);

  if (!body || typeof body !== 'object' || Array.isArray(body)) {
    throw new HttpError(400, 'request body must be a JSON object', 'invalid_request_body');
  }

  assertPrivilegedAdminAccess(request, body, collectionName, context);

  let item;
  try {
    item = context.store[collection.create](body);
  } catch (error) {
    if (error instanceof StoreNotFoundError) {
      throw new HttpError(404, error.message, error.code);
    }
    throw error;
  }

  if (collectionName === 'proposals' && context.convexMirror) {
    void context.convexMirror.upsertProposal(item);
  }

  sendJson(
    response,
    201,
    {
      data: item,
    },
    {
      location: `${collection.locationPrefix}/${item.id}`,
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
  const body = await readJsonBody(request);

  if (!body || typeof body !== 'object' || Array.isArray(body)) {
    throw new HttpError(400, 'request body must be a JSON object', 'invalid_request_body');
  }

  assertPrivilegedAdminAccess(request, body, collectionName, context);

  let item;
  try {
    item = context.store[collection.update](itemId, body);
  } catch (error) {
    if (error instanceof StoreNotFoundError) {
      throw new HttpError(404, error.message, error.code);
    }
    throw error;
  }

  if (collectionName === 'proposals' && context.convexMirror) {
    void context.convexMirror.upsertProposal(item);
  }

  sendJson(response, 200, { data: item });
}

function singular(collectionName) {
  if (collectionName === 'review-comments') {
    return 'review comment';
  }
  if (collectionName === 'workflow-runs') {
    return 'workflow run';
  }
  if (collectionName.endsWith('ies')) {
    return `${collectionName.slice(0, -3)}y`;
  }
  if (collectionName.endsWith('s')) {
    return collectionName.slice(0, -1);
  }
  return collectionName;
}
