import { HttpError, readJsonBody, sendJson, sendMethodNotAllowed } from '../http.js';

import { assertCoreAccess, canCoreAccess } from '../policy-guard.js';

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
      assertCoreAccess(context, 'project.read', { kind: 'project', id: project.id });
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

function listProjects(response, context) {
  const { store, url } = context;
  const organizationId = url.searchParams.get('organizationId') ?? undefined;

  sendJson(response, 200, {
    data: store.listProjects({ organizationId }).filter((project) => canCoreAccess(context, 'project.read', { kind: 'project', id: project.id })),
  });
}

async function createProject(request, response, context) {
  const { store } = context;
  const body = await readJsonBody(request);

  if (!body || typeof body !== 'object' || Array.isArray(body)) {
    throw new HttpError(400, 'request body must be a JSON object', 'invalid_request_body');
  }

  assertCoreAccess(context, 'policy.grant', { kind: 'org', id: body.organizationId });
  if (context.authAdapter.mode !== 'dev') body.createdByPrincipal = context.session.principal;
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
