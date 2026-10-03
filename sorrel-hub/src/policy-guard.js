import { HttpError } from './http.js';
import { POLICY_ACTION_GRANT, PolicyEvaluationError, evaluateWithTrustedGrants } from './core-policy.js';

const ACTING_PRINCIPAL_HEADER = 'x-sorrel-acting-principal';

const PRIVILEGED_ADMIN_COLLECTIONS = new Set(['repositories', 'policies']);

/**
 * Prefer AuthAdapter session principal; allow acting-principal headers only
 * with the development adapter. Authorization still happens via grants.
 *
 * @param {import('node:http').IncomingMessage} request
 * @param {{ authAdapter?: { mode: string }, session?: { principal?: { type: string, id: string } } | null }} [context]
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

  if (context.authAdapter?.mode === 'dev') {
    return parseActingPrincipal(request);
  }

  throw new HttpError(403, 'authenticated session is required for privileged actions', 'policy_denied');
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

/** Guard authenticated deployments with trusted Core capability records. Dev stays local and explicit. */
export function assertCoreAccess(context, action, resource) {
  if (context.authAdapter?.mode === 'dev') return;
  const principal = resolveActingPrincipal(context.request, context);
  const trusted = context.trustedGrantsById ?? {};
  return evaluateWithTrustedGrants(principal, action, resource,
    Object.keys(trusted).map((id) => ({ id, source: 'core' })), trusted);
}

export function canCoreAccess(context, action, resource) {
  try {
    assertCoreAccess(context, action, resource);
    return true;
  } catch (error) {
    if (error.code === 'policy_denied' || error instanceof PolicyEvaluationError) return false;
    throw error;
  }
}

export function metadataResource(context, item, collectionName) {
  if (collectionName === 'organizations') return { kind: 'org', id: item.id };
  if (collectionName === 'repositories') return item.id ? { kind: 'repo', id: item.id } : { kind: 'org', id: item.organizationId };
  if (collectionName === 'projects') return { kind: 'project', id: item.id };
  if (item.projectId && collectionName !== 'review-comments') return { kind: 'project', id: item.projectId };
  if (item.proposalId) {
    const proposal = context.store.getProposal(item.proposalId);
    return proposal?.projectId ? { kind: 'project', id: proposal.projectId } : { kind: 'proposal', id: item.proposalId };
  }
  return { kind: 'org', id: item.organizationId ?? item.id };
}

export function metadataCapability(collectionName, write = false) {
  if (write && ['organizations', 'repositories', 'policies', 'projects'].includes(collectionName)) return 'policy.grant';
  if (collectionName === 'workflow-runs') return write ? 'workflow.run' : 'workflow.read';
  if (collectionName === 'review-comments') return write ? 'proposal.write' : 'proposal.read';
  return `${({ organizations: 'org', repositories: 'repo', projects: 'project', proposals: 'proposal', policies: 'policy' })[collectionName]}.${write ? 'write' : 'read'}`;
}
