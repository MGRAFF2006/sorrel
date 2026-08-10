import { HttpError } from './http.js';
import { POLICY_ACTION_GRANT, PolicyEvaluationError, evaluateWithTrustedGrants } from './core-policy.js';

const ACTING_PRINCIPAL_HEADER = 'x-sorrel-acting-principal';

const PRIVILEGED_ADMIN_COLLECTIONS = new Set(['repositories', 'policies']);

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

  const actingPrincipal = parseActingPrincipal(request);
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
