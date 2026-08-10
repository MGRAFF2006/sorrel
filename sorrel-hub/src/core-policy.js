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
  return `${principal.type}:${principal.id}`;
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
  return resourceKey(grantResource) === resourceKey(targetResource);
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

  if (!isPlainObject(principal) || typeof principal.type !== 'string' || typeof principal.id !== 'string') {
    throw new PolicyEvaluationError('principal is required for Core evaluation');
  }

  if (typeof action !== 'string' || action.trim() === '') {
    throw new PolicyEvaluationError('action is required for Core evaluation');
  }

  const matchingGrant = grants.find(
    (grant) =>
      grant.action === action &&
      principalsMatch(grant.principal, principal) &&
      resourcesMatch(grant.resource, resource),
  );

  if (matchingGrant) {
    return {
      allowed: true,
      decision: createDecision('allow', `matched Core grant ${matchingGrant.id}`, {
        grantId: matchingGrant.id,
        policyRef,
        authorityRootRef,
        policyRefs,
      }),
    };
  }

  return {
    allowed: false,
    decision: createDecision('deny', 'no matching Core grant for action', {
      action,
      principal: principalKey(principal),
      resource: resourceKey(resource),
      policyRef,
      authorityRootRef,
      policyRefs,
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
  const grants = hydrateTrustedGrants(grantRefs, trustedGrantsById);
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
 * @property {'allow' | 'deny'} outcome
 * @property {string} reason
 * @property {Record<string, unknown>} metadata
 */
