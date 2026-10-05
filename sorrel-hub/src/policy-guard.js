import { HttpError } from './http.js';
import { POLICY_ACTION_GRANT, PolicyEvaluationError, evaluateWithTrustedGrants } from './core-policy.js';

const ACTING_PRINCIPAL_HEADER = 'x-sorrel-acting-principal';

const PRIVILEGED_ADMIN_COLLECTIONS = new Set(['repositories', 'policies']);

/**
 * Prefer AuthAdapter session principal; fall back to the acting-principal header
 * (dev / CLI compatibility). Authorization still happens in Core via grants.
 *
 * @param {import('node:http').IncomingMessage} request
 * @param {{ session?: { principal?: { type: string, id: string } } | null }} [context]
 * @returns {{ type: string, id: string }}
 */
export function resolveActingPrincipal(request, context = {}) {
  const fromSession = context.session?.principal;
  if (
    fromSession &&
    typeof fromSession === 'object' &&
    typeof fromSession.type === 'string' &&
    typeof fromSession.id === 'string'
  ) {
    return fromSession;
  }

  if (context.authAdapter && context.authAdapter.mode !== 'dev') {
    throw new HttpError(401, 'a verified Hub session is required', 'authentication_required');
  }
  return parseActingPrincipal(request);
}

/** Bind attribution to verified identity; preserve anonymous local demo callers. */
export function bindSessionPrincipal(attributes, context, field = 'authorPrincipal') {
  if (!context.session?.principal) return attributes;
  const principal = context.session.principal;
  const bound = { ...attributes, [field]: principal };
  if (field === 'authorPrincipal') bound.authorRef = `${principal.type}:${principal.id}`;
  return bound;
}

export function parseActingPrincipal(request) {
  const rawHeader = request.headers[ACTING_PRINCIPAL_HEADER];
  const rawValue = Array.isArray(rawHeader) ? rawHeader[0] : rawHeader;

  if (!rawValue) {
    throw new HttpError(403, 'acting principal is required for privileged admin actions', 'policy_denied');
  }

  try {
    const principal = JSON.parse(rawValue);
    if (!principal || typeof principal !== 'object' || typeof principal.type !== 'string' || typeof principal.id !== 'string') {
      throw new Error('invalid principal shape');
    }
    return principal;
  } catch {
    throw new HttpError(403, 'acting principal header must be valid JSON', 'policy_denied');
  }
}

export function assertPrivilegedAdminAccess(request, body, collectionName, context) {
  if (!PRIVILEGED_ADMIN_COLLECTIONS.has(collectionName)) {
    return undefined;
  }

  const actingPrincipal = resolveActingPrincipal(request, context);
  const grantRefs = body.grantRefs ?? [];
  const resource = resolveAdminResource(collectionName, body);
  const policyContext = {
    policyRef: body.policyRef,
    authorityRootRef: body.authorityRootRef,
    policyRefs: body.policyRefs ?? [],
  };

  try {
    return evaluateWithTrustedGrants(
      actingPrincipal,
      POLICY_ACTION_GRANT,
      resource,
      grantRefs,
      context.trustedGrantsById ?? {},
      policyContext,
    );
  } catch (error) {
    if (error instanceof PolicyEvaluationError) {
      throw new HttpError(400, error.message, error.code);
    }
    throw error;
  }
}

function resolveAdminResource(collectionName, body) {
  if (collectionName === 'repositories') {
    if (body.id) {
      return { kind: 'repo', id: body.id };
    }
    return { kind: 'org', id: body.organizationId };
  }

  if (collectionName === 'policies') {
    if (body.projectId) {
      return { kind: 'project', id: body.projectId };
    }
    return { kind: 'org', id: body.organizationId };
  }

  return undefined;
}
