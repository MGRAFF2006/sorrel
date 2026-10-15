import { URL } from 'node:url';

import { acquireDatasetWriter, coreIntegration } from './core-integration.js';
import { createProposalIntegration } from './proposal-integration.js';
import { createAuthAdapterFromEnv } from './auth/adapter.js';
import { createWorkOsBrowserAuth } from './auth/workos-browser.js';
import { resolveCapabilities } from './capabilities.js';
import { createConvexMirror } from './convex-mirror.js';
import { PolicyDeniedError, PolicyEvaluationError } from './core-policy.js';
import { FsRepoSyncStore } from './fs-sync-store.js';
import { HttpError, sendJson, sendNotFound } from './http.js';
import { ModelValidationError } from './models.js';
import { resolveResourceLimits } from './resource-limits.js';
import { handleAdminRoute } from './routes/admin.js';
import { handleCollaborationRoute } from './routes/collaboration.js';
import { handleProjectsRoute } from './routes/projects.js';
import { handleSyncRoute, mapSyncStoreError } from './routes/sync.js';
import { createInMemoryStore, StoreConflictError, StoreNotFoundError } from './store.js';
import {
  SyncObjectIdMismatchError,
  SyncObjectNotFoundError,
} from './sync-store.js';

const DIAGNOSTIC_METHODS = new Set(['GET', 'HEAD', 'POST', 'PUT', 'PATCH', 'DELETE', 'OPTIONS', 'CONNECT', 'TRACE']);
const FILESYSTEM_ERROR_CODES = new Set([
  'EACCES', 'EPERM', 'ENOSPC', 'EDQUOT', 'EROFS', 'ENOENT', 'EEXIST',
  'ENOTDIR', 'EISDIR', 'EMFILE', 'ENFILE', 'EIO', 'EXDEV', 'EBUSY', 'ELOOP', 'ENAMETOOLONG',
]);

export function createApp(options = {}) {
  const limits = resolveResourceLimits(options.env);
  const store = options.store ?? createInMemoryStore();
  const trustedGrantsById = options.trustedGrantsById ?? {};
  const trustedPoliciesById = options.trustedPoliciesById ?? {};
  const authAdapter = options.authAdapter ?? createAuthAdapterFromEnv(options.env);
  const browserAuth = options.browserAuth === undefined ? createWorkOsBrowserAuth(options.env) : options.browserAuth;
  const localDemo = authAdapter.mode === 'dev' && (options.env ?? process.env).SORREL_HUB_LOCAL_DEMO === '1';
  const convexMirror = options.convexMirror ?? createConvexMirror(options.env);
  const capabilities =
    options.capabilities ??
    resolveCapabilities({
      authMode: authAdapter.mode,
      browserLogin: Boolean(browserAuth),
      env: options.env,
      objectStorage: store.sync instanceof FsRepoSyncStore ? 'fs' : 'memory',
      convexEnabled: convexMirror.enabled === true,
    });

  const integration = createProposalIntegration(store, { ...options, convexMirror });
  capabilities.collaboration ??= {};
  capabilities.collaboration.integration = integration.capabilities;
  const roots = [...new Set([store.rootDir, store.sync instanceof FsRepoSyncStore ? store.sync.rootDir : undefined].filter(Boolean))];
  let datasetLease = options.datasetLease;
  // ponytail: one queue per Hub writer; partition only when storage supports independent writers.
  let tail = Promise.resolve();
  const serialize = work => {
    const result = tail.then(work);
    tail = result.catch(() => {});
    return result;
  };
  const ready = (async () => {
    if (roots.length) {
      datasetLease ??= await acquireDatasetWriter(roots);
      for (const root of roots) if (!datasetLease.covers(root)) throw new Error('writer lease does not cover every filesystem root');
      datasetLease.assert();
      store.reload?.();
    }
    if (integration.capabilities.available) {
      await coreIntegration({ operation: 'probe' });
      integration.recover();
    }
  })().catch(async error => { await datasetLease?.close(); throw error; });
  // A failed startup remains observable through ready and request errors.
  void ready.catch(() => {});
  return {
    ready,
    async close() { await tail; await datasetLease?.close(); },
    store,
    trustedGrantsById,
    trustedPoliciesById,
    authAdapter,
    convexMirror,
    capabilities,
    async handleRequest(request, response) {
      try {
        response.setHeader('cache-control', 'no-store');
        response.setHeader('x-content-type-options', 'nosniff');
        let url;
        try {
          url = new URL(request.url ?? '/', 'http://localhost');
        } catch {
          throw new HttpError(400, 'request URL is invalid', 'invalid_request');
        }
        if (request.method === 'GET' && url.pathname === '/healthz') {
          return sendJson(response, 200, {
            status: 'ok',
            service: 'sorrel-hub',
          });
        }

        await ready;
        datasetLease?.assert();
        if (request.method === 'GET' && url.pathname === '/capabilities') {
          return sendJson(response, 200, { data: capabilities });
        }

        if (browserAuth && await browserAuth.handleRequest(request, response, url)) return;

        // Resolve session once per request (auth off the hot object path).
        const session = authAdapter.mode === 'dev' && !localDemo ? null :
          (request.headers.authorization ? await authAdapter.resolveSession(request) : await browserAuth?.resolveSession(request, response) ?? await authAdapter.resolveSession(request)) ?? (localDemo && request.headers['x-sorrel-acting-principal'] === undefined ? {
            principal: { type: 'user', id: 'local' }, sessionId: 'dev:user:local', authMode: 'dev',
          } : null);

        if (request.method === 'GET' && url.pathname === '/session') {
          return sendJson(response, 200, {
            data: {
              auth: {
                mode: authAdapter.mode,
                session: capabilities.auth.session,
                browserLogin: Boolean(browserAuth),
              },
              session: session
                ? {
                    sessionId: session.sessionId,
                    authMode: session.authMode,
                    principal: session.principal,
                    idpSubject: session.idpSubject ?? null,
                    expiresAt: session.expiresAt ?? null,
                  }
                : null,
            },
          });
        }

        if (!session) {
          throw new HttpError(401, 'a verified Hub session is required', 'authentication_required');
        }

        return await serialize(async () => {
          datasetLease?.assert();
          integration.recover();
          const routeContext = {
            integration,
            datasetLease,
            store,
            limits,
            url,
            trustedGrantsById,
            trustedPoliciesById,
            authAdapter,
            session,
            localDemo,
            convexMirror,
            capabilities,
          };

          if (url.pathname === '/projects' || url.pathname.startsWith('/projects/')) {
            return await handleProjectsRoute(request, response, routeContext);
          }

          if (url.pathname.startsWith('/collaboration/')) {
            return await handleCollaborationRoute(request, response, routeContext);
          }

          if (url.pathname.startsWith('/admin/')) {
            return await handleAdminRoute(request, response, routeContext);
          }

          if (isSyncPath(url.pathname)) {
            return await handleSyncRoute(request, response, routeContext);
          }

          return sendNotFound(response);
        });
      } catch (error) {
        return sendError(response, error, request.method);
      }
    },
  };
}

function isSyncPath(pathname) {
  const segments = pathname.split('/').filter(Boolean);
  if (segments.length < 2) {
    return false;
  }

  const resource = segments[1];
  return resource === 'refs' || resource === 'objects' || resource === 'tree' || resource === 'files';
}

function sendError(response, error, method) {
  const mapped = mapSyncStoreError(error);
  if (mapped !== error) {
    return sendError(response, mapped, method);
  }

  if (error instanceof SyncObjectIdMismatchError) {
    return sendJson(response, 400, {
      error: {
        code: error.code,
        message: error.message,
      },
    });
  }

  if (error instanceof SyncObjectNotFoundError) {
    return sendJson(response, 404, {
      error: {
        code: error.code,
        message: error.message,
      },
    });
  }

  if (error instanceof HttpError) {
    if (error.statusCode === 413) response.setHeader('connection', 'close');
    return sendJson(response, error.statusCode, {
      error: {
        code: error.code,
        message: error.message,
        ...(error.details ?? {}),
      },
    });
  }

  if (error instanceof ModelValidationError) {
    return sendJson(response, 400, {
      error: {
        code: error.code,
        message: error.message,
      },
    });
  }

  if (error instanceof StoreConflictError) {
    return sendJson(response, 409, {
      error: {
        code: error.code,
        message: error.message,
      },
    });
  }

  if (error instanceof StoreNotFoundError) {
    return sendJson(response, 404, {
      error: {
        code: error.code,
        message: error.message,
      },
    });
  }

  if (error instanceof PolicyDeniedError) {
    return sendJson(response, 403, {
      error: {
        code: error.code,
        message: error.message,
        decision: error.decision,
      },
    });
  }

  if (error instanceof PolicyEvaluationError) {
    return sendJson(response, error.statusCode ?? 403, {
      error: {
        code: error.code,
        message: error.message,
      },
    });
  }

  const code = FILESYSTEM_ERROR_CODES.has(error?.code) ? error.code : undefined;
  console.error('[sorrel-hub] unexpected request failure', {
    method: DIAGNOSTIC_METHODS.has(method) ? method : 'UNKNOWN',
    category: code ? 'filesystem' : 'internal',
    ...(code ? { code } : {}),
  });
  return sendJson(response, 500, {
    error: {
      code: 'internal_server_error',
      message: 'internal server error',
    },
  });
}
