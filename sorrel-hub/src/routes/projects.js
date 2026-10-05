import { HttpError, readJsonBody, sendJson, sendMethodNotAllowed } from '../http.js';
import { bindSessionPrincipal } from '../policy-guard.js';

export async function handleProjectsRoute(request, response, context) {
  const { url, store } = context;
  const segments = url.pathname.split('/').filter(Boolean);
  // /projects or /projects/:id
  const projectId = segments.length >= 2 ? decodeURIComponent(segments[1]) : null;

  if (segments.length === 3 && segments[2] === 'repositories') {
    if (request.method !== 'POST') return sendMethodNotAllowed(response, ['POST']);
    const body = await readJsonBody(request);
    if (!body || typeof body !== 'object' || typeof body.syncRepoId !== 'string' || !body.syncRepoId.trim()) {
      throw new HttpError(400, 'syncRepoId is required', 'invalid_request_body');
    }
    const syncRepoId = body.syncRepoId.trim();
    if (!store.getProject(projectId)) {
      throw new HttpError(404, `project ${projectId} not found`, 'not_found');
    }
    if (!store.sync.listRepos().includes(syncRepoId)) {
      throw new HttpError(404, `synchronized repository ${syncRepoId} not found`, 'not_found');
    }
    return sendJson(response, 200, { data: store.linkProjectRepository(projectId, syncRepoId) });
  }
  if (segments.length > 2) throw new HttpError(404, 'project route not found', 'not_found');

  if (projectId) {
    if (request.method === 'GET') {
      const project = store.getProject(projectId);
      if (!project) {
        throw new HttpError(404, `project ${projectId} not found`, 'not_found');
      }
      return sendJson(response, 200, { data: project });
    }
    return sendMethodNotAllowed(response, ['GET']);
  }

  if (request.method === 'GET') {
    return listProjects(response, context);
  }

  if (request.method === 'POST') {
    return createProject(request, response, context);
  }

  return sendMethodNotAllowed(response, ['GET', 'POST']);
}

function listProjects(response, { store, url }) {
  const organizationId = url.searchParams.get('organizationId') ?? undefined;

  sendJson(response, 200, {
    data: store.listProjects({ organizationId }),
  });
}

async function createProject(request, response, context) {
  const body = await readJsonBody(request);

  if (!body || typeof body !== 'object' || Array.isArray(body)) {
    throw new HttpError(400, 'request body must be a JSON object', 'invalid_request_body');
  }

  const project = context.store.createProject(bindSessionPrincipal(body, context, 'createdByPrincipal'));

  sendJson(
    response,
    201,
    {
      data: project,
    },
    {
      location: `/projects/${project.id}`,
    },
  );
}
