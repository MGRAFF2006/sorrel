import { HttpError, readJsonBody, sendJson, sendMethodNotAllowed } from '../http.js';

export async function handleProjectsRoute(request, response, context) {
  const { url, store } = context;
  const segments = url.pathname.split('/').filter(Boolean);
  // /projects or /projects/:id
  const projectId = segments.length >= 2 ? decodeURIComponent(segments[1]) : null;

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

async function createProject(request, response, { store }) {
  const body = await readJsonBody(request);

  if (!body || typeof body !== 'object' || Array.isArray(body)) {
    throw new HttpError(400, 'request body must be a JSON object', 'invalid_request_body');
  }

  const project = store.createProject(body);

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
