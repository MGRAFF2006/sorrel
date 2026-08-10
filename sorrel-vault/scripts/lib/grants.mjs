export function refKey(ref) {
  if (!ref || typeof ref.kind !== "string" || typeof ref.id !== "string") {
    throw new Error(`Invalid object reference: ${JSON.stringify(ref)}`);
  }

  return `${ref.kind}:${ref.id}`;
}

export function refsEqual(left, right) {
  return refKey(left) === refKey(right);
}

export const CORE_SECRET_CAPABILITIES = Object.freeze({
  READ: "secret.read",
  INJECT: "secret.inject"
});

export const POLICY_DECISION_STATUS = Object.freeze({
  ALLOW: "allow",
  DENY: "deny",
  NEEDS_GRANT: "needs_grant"
});

export function coreSecretCapabilityForAction(action) {
  switch (action) {
    case "materialize":
    case "import":
      return CORE_SECRET_CAPABILITIES.INJECT;
    case "read":
    case "redact":
    default:
      return CORE_SECRET_CAPABILITIES.READ;
  }
}

export function isGrantAllowed(grant, request) {
  return (
    refsEqual(grant.secret, request.secret) &&
    grant.environment === request.environment &&
    grant.actions.includes(request.action) &&
    matchesAccess(grant.access, request.actor)
  );
}

export function findGrant(spec, request) {
  return spec.grants.find((grant) => isGrantAllowed(grant, request));
}

export function assertGrant(spec, request) {
  const grant = findGrant(spec, request);

  if (!grant) {
    throw new AccessDeniedError(request, needsGrantDecision(request));
  }

  return grant;
}

export function buildCorePolicyRequest(request) {
  return {
    schemaVersion: "sorrel.protocol.v0",
    kind: "PolicyRequest",
    capability: coreSecretCapabilityForAction(request.action),
    action: request.action,
    resource: request.secret,
    environment: request.environment,
    subject: request.actor ?? {}
  };
}

export function evaluateCorePolicyFromGrants(spec, request) {
  const grant = findGrant(spec, request);

  if (!grant) {
    return needsGrantDecision(request);
  }

  return {
    schemaVersion: "sorrel.protocol.v0",
    kind: "PolicyDecision",
    status: POLICY_DECISION_STATUS.ALLOW,
    capability: coreSecretCapabilityForAction(request.action),
    action: request.action,
    resource: request.secret,
    environment: request.environment,
    subject: request.actor ?? {},
    grant: { kind: "SecretGrant", id: grant.id },
    policy: grant.policy,
    reason: grant.reason ?? "Matching sorrel-vault grant"
  };
}

export function createLocalDevCorePolicy(spec) {
  return {
    evaluate(policyRequest) {
      return evaluateCorePolicyFromGrants(spec, policyRequestToVaultRequest(policyRequest));
    }
  };
}

export function decideCorePolicy(spec, request, corePolicy) {
  const policyRequest = buildCorePolicyRequest(request);

  if (!corePolicy) {
    return missingCorePolicyDecision(request);
  }

  const decision = invokeCorePolicy(corePolicy, policyRequest);

  return normalizePolicyDecision(decision, request);
}

export function assertCorePolicyAllowed(spec, request, corePolicy) {
  const decision = decideCorePolicy(spec, request, corePolicy);

  if (decision.status !== POLICY_DECISION_STATUS.ALLOW) {
    throw new AccessDeniedError(request, decision);
  }

  return {
    decision,
    grant: findGrant(spec, request)
  };
}

export class AccessDeniedError extends Error {
  constructor(request, decision = needsGrantDecision(request)) {
    super(
      `Core policy ${decision.status} for ${decision.capability} (${request.action}) on ${refKey(request.secret)} in ${request.environment} for ${describeActor(request.actor)}`
    );
    this.name = "AccessDeniedError";
    this.request = request;
    this.decision = decision;
  }
}

function needsGrantDecision(request) {
  return {
    schemaVersion: "sorrel.protocol.v0",
    kind: "PolicyDecision",
    status: POLICY_DECISION_STATUS.NEEDS_GRANT,
    capability: coreSecretCapabilityForAction(request.action),
    action: request.action,
    resource: request.secret,
    environment: request.environment,
    subject: request.actor ?? {},
    reason: `No grant permits ${request.action} on ${refKey(request.secret)} in ${request.environment}`
  };
}

function missingCorePolicyDecision(request) {
  const capability = coreSecretCapabilityForAction(request.action);

  return {
    schemaVersion: "sorrel.protocol.v0",
    kind: "PolicyDecision",
    status: POLICY_DECISION_STATUS.NEEDS_GRANT,
    capability,
    action: request.action,
    resource: request.secret,
    environment: request.environment,
    subject: request.actor ?? {},
    reason: `Trusted Core policy decision required for ${capability} on ${refKey(request.secret)} in ${request.environment}; local grant YAML does not bypass Core`
  };
}

function policyRequestToVaultRequest(policyRequest) {
  return {
    secret: policyRequest.resource,
    environment: policyRequest.environment,
    action: policyRequest.action,
    actor: policyRequest.subject ?? {}
  };
}

function normalizePolicyDecision(decision, request) {
  if (!decision || typeof decision !== "object") {
    return {
      ...needsGrantDecision(request),
      status: POLICY_DECISION_STATUS.DENY,
      reason: "Core policy returned no decision"
    };
  }

  const status = decision.status ?? decision.effect;
  if (!Object.values(POLICY_DECISION_STATUS).includes(status)) {
    return {
      ...needsGrantDecision(request),
      status: POLICY_DECISION_STATUS.DENY,
      reason: `Core policy returned unsupported decision status: ${status}`
    };
  }

  return {
    schemaVersion: decision.schemaVersion ?? "sorrel.protocol.v0",
    kind: decision.kind ?? "PolicyDecision",
    capability: decision.capability ?? coreSecretCapabilityForAction(request.action),
    action: decision.action ?? request.action,
    resource: decision.resource ?? request.secret,
    environment: decision.environment ?? request.environment,
    subject: decision.subject ?? request.actor ?? {},
    status,
    reason: decision.reason,
    grant: decision.grant,
    policy: decision.policy,
    metadata: decision.metadata
  };
}

function invokeCorePolicy(corePolicy, policyRequest) {
  if (typeof corePolicy === "function") {
    return corePolicy(policyRequest);
  }

  if (typeof corePolicy?.evaluate === "function") {
    return corePolicy.evaluate(policyRequest);
  }

  if (typeof corePolicy?.decide === "function") {
    return corePolicy.decide(policyRequest);
  }

  throw new Error("corePolicy must be a function or expose evaluate(request) or decide(request)");
}

function matchesAccess(access, actor = {}) {
  return (
    matchesOptionalRefList(access.agents, actor.agent) &&
    matchesOptionalRefList(access.workflows, actor.workflow) &&
    matchesOptionalRefList(access.runners, actor.runner)
  );
}

function matchesOptionalRefList(allowedRefs, actorRef) {
  if (!allowedRefs || allowedRefs.length === 0) {
    return true;
  }

  if (!actorRef) {
    return false;
  }

  return allowedRefs.some((allowedRef) => refsEqual(allowedRef, actorRef));
}

function describeActor(actor = {}) {
  const parts = [];

  if (actor.agent) {
    parts.push(refKey(actor.agent));
  }

  if (actor.workflow) {
    parts.push(refKey(actor.workflow));
  }

  if (actor.runner) {
    parts.push(refKey(actor.runner));
  }

  return parts.length === 0 ? "anonymous actor" : parts.join(", ");
}
