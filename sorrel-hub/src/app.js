import { URL } from 'node:url';

import { createAuthAdapterFromEnv } from './auth/adapter.js';
import { resolveCapabilities } from './capabilities.js';
import { createConvexMirror } from './convex-mirror.js';
import { PolicyDeniedError, PolicyEvaluationError } from './core-policy.js';
import { FsRepoSyncStore } from './fs-sync-store.js';
import { HttpError, sendJson, sendNotFound } from './http.js';
import { ModelValidationError } from './models.js';
import { handleAdminRoute } from './routes/admin.js';
import { handleCollaborationRoute } from './routes/collaboration.js';
import { handleProjectsRoute } from './routes/projects.js';
import { handleSyncRoute, mapSyncStoreError } from './routes/sync.js';
import { createInMemoryStore, StoreConflictError, StoreNotFoundError } from './store.js';
import {
  SyncObjectIdMismatchError,
  SyncObjectNotFoundError,
} from './sync-store.js';

export function createApp(options = {}) {
  const store = options.store ?? createInMemoryStore();
  const trustedGrantsById = options.trustedGrantsById ?? {};
  const trustedPoliciesById = options.trustedPoliciesById ?? {};
  const authAdapter = options.authAdapter ?? createAuthAdapterFromEnv(options.env);
  const localDemo = authAdapter.mode === 'dev' && (options.env ?? process.env).SORREL_HUB_LOCAL_DEMO === '1';
  const convexMirror = options.convexMirror ?? createConvexMirror(options.env);
  const capabilities =
    options.capabilities ??
    resolveCapabilities({
      authMode: authAdapter.mode,
      env: options.env,
      objectStorage: store.sync instanceof FsRepoSyncStore ? 'fs' : 'memory',
      convexEnabled: convexMirror.enabled === true,
    });

  return {
    store,
    trustedGrantsById,
    trustedPoliciesById,
    authAdapter,
    convexMirror,
    capabilities,
    async handleRequest(request, response) {
      try {
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

        if (request.method === 'GET' && url.pathname === '/capabilities') {
          return sendJson(response, 200, { data: capabilities });
        }

        // Resolve session once per request (auth off the hot object path).
        const session = authAdapter.mode === 'dev' && !localDemo ? null :
          await authAdapter.resolveSession(request) ?? (localDemo && request.headers['x-sorrel-acting-principal'] === undefined ? {
            principal: { type: 'user', id: 'local' }, sessionId: 'dev:user:local', authMode: 'dev',
          } : null);

        if (request.method === 'GET' && url.pathname === '/session') {
          return sendJson(response, 200, {
            data: {
              auth: {
                mode: authAdapter.mode,
                session: capabilities.auth.session,
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

        const routeContext = {
          store,
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
      } catch (error) {
        return sendError(response, error);
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

function sendError(response, error) {
  const mapped = mapSyncStoreError(error);
  if (mapped !== error) {
    return sendError(response, mapped);
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

  return sendJson(response, 500, {
    error: {
      code: 'internal_server_error',
      message: 'internal server error',
    },
  });
}
