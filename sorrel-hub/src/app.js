import { URL } from 'node:url';

import { createAuthAdapterFromEnv } from './auth/adapter.js';
import { resolveCapabilities } from './capabilities.js';
import { createConvexMirror } from './convex-mirror.js';
import { PolicyDeniedError, PolicyEvaluationError } from './core-policy.js';
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
  const authAdapter = options.authAdapter ?? createAuthAdapterFromEnv(options.env);
  const convexMirror = options.convexMirror ?? createConvexMirror(options.env);
  const capabilities =
    options.capabilities ??
    resolveCapabilities({
      authMode: authAdapter.mode,
      env: options.env,
    });

  return {
    store,
    trustedGrantsById,
    authAdapter,
    convexMirror,
    capabilities,
    async handleRequest(request, response) {
      const url = new URL(request.url ?? '/', 'http://localhost');

      try {
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
        const session = await authAdapter.resolveSession(request);

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

        const routeContext = {
          store,
          url,
          trustedGrantsById,
          authAdapter,
          session,
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
  return resource === 'refs' || resource === 'objects';
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
    return sendJson(response, 403, {
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
