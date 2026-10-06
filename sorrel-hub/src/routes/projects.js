import { decodePathComponent, HttpError, readJsonBody, sendJson, sendMethodNotAllowed } from '../http.js';
import { createProject as normalizeProject } from '../models.js';
import { assertCoreAccess, bindSessionPrincipal, canAccess, projectScope } from '../policy-guard.js';

export async function handleProjectsRoute(request, response, context) {
  const { url, store } = context;
  const segments = url.pathname.split('/').filter(Boolean);
  // /projects or /projects/:id
  const projectId = segments.length >= 2 ? decodePathComponent(segments[1]) : null;

  if (segments.length === 3 && segments[2] === 'repositories') {
    if (request.method !== 'POST') return sendMethodNotAllowed(response, ['POST']);
    if (!await canAccess(context, 'project.read', { kind: 'project', id: projectId })) {
      throw new HttpError(404, 'resource not found', 'not_found');
    }
    await assertCoreAccess(context, 'project.write', projectScope(context, projectId));
    const body = await readJsonBody(request);
    if (!body || typeof body !== 'object' || Array.isArray(body) || typeof body.syncRepoId !== 'string' || !body.syncRepoId.trim()) {
      throw new HttpError(400, 'syncRepoId is required', 'invalid_request_body');
    }
    const syncRepoId = body.syncRepoId.trim();
    if (!store.getProject(projectId)) {
      throw new HttpError(404, `project ${projectId} not found`, 'not_found');
    }
    await assertCoreAccess(context, 'repo.read', { kind: 'repo', id: syncRepoId });
    if (!store.sync.listRepos().includes(syncRepoId)) {
      throw new HttpError(404, `synchronized repository ${syncRepoId} not found`, 'not_found');
    }
    return sendJson(response, 200, { data: store.linkProjectRepository(projectId, syncRepoId) });
  }
  if (segments.length > 2) throw new HttpError(404, 'project route not found', 'not_found');

  if (projectId) {
    if (request.method === 'GET') {
      const project = store.getProject(projectId);
      if (!project || !await canAccess(context, 'project.read', { kind: 'project', id: projectId })) {
        throw new HttpError(404, 'resource not found', 'not_found');
      }
      return sendJson(response, 200, { data: project });
    }
    return sendMethodNotAllowed(response, ['GET']);
  }

  if (request.method === 'GET') {
    return await listProjects(response, context);
  }

  if (request.method === 'POST') {
    return createProject(request, response, context);
  }

  return sendMethodNotAllowed(response, ['GET', 'POST']);
}

async function listProjects(response, context) {
  const { store, url } = context;
  const organizationId = url.searchParams.get('organizationId') ?? undefined;

  const data = [];
  for (const project of store.listProjects({ organizationId })) {
    if (await canAccess(context, 'project.read', { kind: 'project', id: project.id })) data.push(project);
  }
  sendJson(response, 200, { data });
}

async function createProject(request, response, context) {
  const body = await readJsonBody(request);

  if (!body || typeof body !== 'object' || Array.isArray(body)) {
    throw new HttpError(400, 'request body must be a JSON object', 'invalid_request_body');
  }

  const attributes = normalizeProject(bindSessionPrincipal(body, context, 'createdByPrincipal'));
  await assertCoreAccess(context, 'project.create', { kind: 'org', id: attributes.organizationId });
  if (Array.isArray(attributes.repositoryIds)) {
    for (const id of attributes.repositoryIds) await assertCoreAccess(context, 'repo.read', { kind: 'repo', id });
  }
  const project = context.store.createProject(attributes);

  sendJson(
    response,
    201,
    {
      data: project,
    },
    {
      location: `/projects/${encodeURIComponent(project.id)}`,
    },
  );
}
