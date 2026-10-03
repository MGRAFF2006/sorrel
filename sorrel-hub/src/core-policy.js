import { randomUUID } from 'node:crypto';

export const POLICY_ACTION_GRANT = 'policy.grant';

export class PolicyEvaluationError extends Error {
  constructor(message) {
    super(message);
    this.name = 'PolicyEvaluationError';
    this.code = 'policy_evaluation_failed';
  }
}

export class PolicyDeniedError extends Error {
  constructor(message, decision) {
    super(message);
    this.name = 'PolicyDeniedError';
    this.code = 'policy_denied';
    this.decision = decision;
  }
}

function isPlainObject(value) {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}

function principalKey(principal) {
  return `${principal.type ?? principal.kind}:${principal.id}`;
}

function resourceKey(resource) {
  if (!resource) {
    return undefined;
  }
  return `${resource.kind}:${resource.id}`;
}

function principalsMatch(grantPrincipal, actingPrincipal) {
  return principalKey(grantPrincipal) === principalKey(actingPrincipal);
}

function resourcesMatch(grantResource, targetResource) {
  if (!grantResource) {
    return true;
  }
  if (!targetResource) {
    return false;
  }
  return grantResource.kind === targetResource.kind && (grantResource.id === '*' || resourceKey(grantResource) === resourceKey(targetResource));
}

/**
 * Hydrate Core grant records referenced by Hub grantRefs.
 * Hub stores only references; evaluation requires trusted Core grant payloads.
 *
 * @param {import('./models.js').CoreRecordRef[]} grantRefs
 * @param {Record<string, CoreGrant>} trustedGrantsById
 * @returns {CoreGrant[]}
 */
export function hydrateTrustedGrants(grantRefs, trustedGrantsById = {}) {
  return grantRefs.map((grantRef) => {
    const grant = trustedGrantsById[grantRef.id];
    if (!grant) {
      throw new PolicyEvaluationError(
        `grant ${grantRef.id} is not available for headless Core evaluation`,
      );
    }

    return grant;
  });
}

/**
 * Evaluate authorization through Core policy semantics.
 * This skeleton mirrors sorrel-core evaluate() until the package is linked.
 *
 * @param {Object} request
 * @param {import('./models.js').Principal} request.principal
 * @param {string} request.action
 * @param {{ kind: string, id: string } | undefined} [request.resource]
 * @param {CoreGrant[]} request.grants
 * @param {{ kind: string, id: string } | undefined} [request.policyRef]
 * @param {{ kind: string, id: string } | undefined} [request.authorityRootRef]
 * @param {{ kind: string, id: string }[]} [request.policyRefs]
 * @returns {{ allowed: boolean, decision: CorePolicyDecision }}
 */
export function evaluate(request) {
  const {
    principal,
    action,
    resource,
    grants = [],
    policyRef,
    authorityRootRef,
    policyRefs = [],
  } = request;

  if (!isPlainObject(principal) || typeof (principal.type ?? principal.kind) !== 'string' || typeof principal.id !== 'string') {
    throw new PolicyEvaluationError('principal is required for Core evaluation');
  }

  if (typeof action !== 'string' || action.trim() === '') {
    throw new PolicyEvaluationError('action is required for Core evaluation');
  }

  const now = request.now ?? Date.now();
  const matching = grants.filter((grant) => {
    if (!isPlainObject(grant) || !isPlainObject(grant.principal)) return false;
    if (grant.status !== undefined && !['active', 'expired', 'revoked'].includes(grant.status)) {
      throw new PolicyEvaluationError('unsupported trusted grant status');
    }
    if (grant.status !== undefined && grant.status !== 'active') return false;
    if (grant.revokedAt !== undefined) return false;
    for (const field of ['issuedAt', 'expiresAt']) {
      if (grant[field] !== undefined && !Number.isFinite(Date.parse(grant[field]))) {
        throw new PolicyEvaluationError(`invalid trusted grant ${field}`);
      }
    }
    if (grant.issuedAt && Date.parse(grant.issuedAt) > now) return false;
    if (grant.expiresAt && Date.parse(grant.expiresAt) <= now) return false;
    // Conditional grants require Core evaluation; this adapter cannot safely interpret them.
    if (grant.schemaVersion !== undefined && grant.schemaVersion !== 'sorrel.protocol.v0') {
      throw new PolicyEvaluationError('unsupported trusted grant schema version');
    }
    if (grant.conditions && Object.keys(grant.conditions).length && (grant.effect ?? 'allow') === 'allow') return false;
    const actions = grant.capabilities ?? [grant.action ?? grant.capability];
    const resources = grant.resources ?? [grant.resource];
    return actions.some((value) => value === action || value === '*') &&
      principalsMatch(grant.principal, principal) &&
      resources.some((value) => resourcesMatch(value, resource));
  }).sort((a, b) => a.id.localeCompare(b.id));
  const effects = matching.map((grant) => {
    const effect = grant.effect ?? (grant.schemaVersion ? 'deny' : 'allow');
    return ['allow', 'deny', 'redact', 'review'].includes(effect) ? effect : 'deny';
  });
  const effect = ['deny', 'redact', 'review', 'allow'].find((value) => effects.includes(value));
  const outcome = effect === 'review' ? 'needs_review' : effect ?? 'deny';
  return {
    allowed: outcome === 'allow',
    decision: createDecision(outcome, effect ? `matched Core ${effect} grant` : 'no matching Core grant for action', {
      grantId: matching.find((grant) => (grant.effect ?? 'allow') === effect)?.id,
      grantIds: matching.map((grant) => grant.id),
      action, principal: principalKey(principal), resource: resourceKey(resource),
      policyRef, authorityRootRef, policyRefs,
    }),
  };
}

/**
 * @param {import('./models.js').Principal} principal
 * @param {string} action
 * @param {{ kind: string, id: string } | undefined} resource
 * @param {import('./models.js').CoreRecordRef[]} grantRefs
 * @param {Record<string, CoreGrant>} trustedGrantsById
 * @param {Object} [policyContext]
 * @param {{ kind: string, id: string } | undefined} [policyContext.policyRef]
 * @param {{ kind: string, id: string } | undefined} [policyContext.authorityRootRef]
 * @param {{ kind: string, id: string }[]} [policyContext.policyRefs]
 */
export function evaluateWithTrustedGrants(
  principal,
  action,
  resource,
  grantRefs,
  trustedGrantsById,
  policyContext = {},
) {
  // References select allow authority, but cannot hide a trusted restrictive grant.
  const selected = hydrateTrustedGrants(grantRefs, trustedGrantsById);
  const grants = [...new Map([...selected, ...Object.values(trustedGrantsById).filter(
    (grant) => grant.effect && grant.effect !== 'allow',
  )].map((grant) => [grant.id, grant])).values()];
  const result = evaluate({
    principal,
    action,
    resource,
    grants,
    ...policyContext,
  });

  if (!result.allowed) {
    throw new PolicyDeniedError('Core policy denied request', result.decision);
  }

  return result;
}

function createDecision(outcome, reason, metadata = {}) {
  return {
    id: `decision_${randomUUID()}`,
    source: 'core',
    outcome,
    reason,
    metadata,
  };
}

/**
 * @typedef {Object} CoreGrant
 * @property {string} id
 * @property {string} [source]
 * @property {import('./models.js').Principal} principal
 * @property {string} action
 * @property {{ kind: string, id: string } | undefined} [resource]
 */

/**
 * @typedef {Object} CorePolicyDecision
 * @property {string} id
 * @property {string} source
 * @property {'allow' | 'deny' | 'redact' | 'needs_review'} outcome
 * @property {string} reason
 * @property {Record<string, unknown>} metadata
 */
