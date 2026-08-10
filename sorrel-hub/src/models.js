import { randomUUID } from 'node:crypto';

export const PROJECT_STATUSES = ['active', 'archived'];
export const PROPOSAL_STATUSES = ['draft', 'open', 'approved', 'rejected', 'merged', 'closed'];
/** Allowed proposal status transitions (from → to[]). */
export const PROPOSAL_STATUS_TRANSITIONS = Object.freeze({
  draft: ['open', 'closed'],
  open: ['approved', 'rejected', 'merged', 'closed', 'draft'],
  approved: ['merged', 'open', 'closed'],
  rejected: ['open', 'closed'],
  merged: ['closed'],
  closed: ['draft', 'open'],
});
export const REVIEW_COMMENT_STATES = ['open', 'resolved'];
export const WORKFLOW_RUN_STATUSES = ['queued', 'in_progress', 'succeeded', 'failed', 'cancelled'];
export const CORE_PRINCIPAL_TYPES = ['user', 'agent', 'team', 'service', 'runner', 'system'];
export const POLICY_REF_KINDS = ['Policy', 'AgentPolicy'];
export const AUTHORITY_ROOT_REF_KINDS = ['AuthorityRoot'];
const HUB_LOCAL_PERMISSION_FIELDS = ['permissions', 'allowedActions', 'roles', 'capabilities', 'rules'];

function nowIso() {
  return new Date().toISOString();
}

function id(prefix) {
  return `${prefix}_${randomUUID()}`;
}

function requiredString(attributes, fieldName) {
  const value = attributes[fieldName];
  if (typeof value !== 'string' || value.trim() === '') {
    throw new ModelValidationError(`${fieldName} is required`);
  }
  return value.trim();
}

function optionalString(attributes, fieldName) {
  const value = attributes[fieldName];
  if (value === undefined || value === null) {
    return undefined;
  }
  if (typeof value !== 'string') {
    throw new ModelValidationError(`${fieldName} must be a string`);
  }
  return value.trim();
}

function arrayOfStrings(attributes, fieldName) {
  const value = attributes[fieldName] ?? [];
  if (!Array.isArray(value) || value.some((entry) => typeof entry !== 'string')) {
    throw new ModelValidationError(`${fieldName} must be an array of strings`);
  }
  return value;
}

function optionalObject(attributes, fieldName) {
  const value = attributes[fieldName];
  if (value === undefined || value === null) {
    return undefined;
  }
  if (!isPlainObject(value)) {
    throw new ModelValidationError(`${fieldName} must be an object`);
  }
  return value;
}

function enumValue(attributes, fieldName, allowedValues, fallback) {
  const value = attributes[fieldName] ?? fallback;
  if (!allowedValues.includes(value)) {
    throw new ModelValidationError(`${fieldName} must be one of: ${allowedValues.join(', ')}`);
  }
  return value;
}

function isPlainObject(value) {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}

function normalizePrincipal(value, fieldName) {
  if (!isPlainObject(value)) {
    throw new ModelValidationError(`${fieldName} must be a Core principal object`);
  }

  const type = enumValue(value, 'type', CORE_PRINCIPAL_TYPES);
  const idValue = requiredString(value, 'id');
  const displayName = optionalString(value, 'displayName');

  return {
    type,
    id: idValue,
    ...(displayName ? { displayName } : {}),
  };
}

function optionalPrincipal(attributes, fieldName) {
  const value = attributes[fieldName];
  if (value === undefined || value === null) {
    return undefined;
  }
  return normalizePrincipal(value, fieldName);
}

function arrayOfPrincipals(attributes, fieldName) {
  const value = attributes[fieldName] ?? [];
  if (!Array.isArray(value)) {
    throw new ModelValidationError(`${fieldName} must be an array of Core principals`);
  }
  return value.map((entry, index) => normalizePrincipal(entry, `${fieldName}[${index}]`));
}

function normalizeProtocolObjectRef(value, fieldName, allowedKinds) {
  if (!isPlainObject(value)) {
    throw new ModelValidationError(`${fieldName} must be a protocol object reference`);
  }

  const kind = enumValue(value, 'kind', allowedKinds);
  return {
    kind,
    id: requiredString(value, 'id'),
  };
}

function optionalProtocolObjectRef(attributes, fieldName, allowedKinds) {
  const value = optionalObject(attributes, fieldName);
  if (!value) {
    return undefined;
  }
  return normalizeProtocolObjectRef(value, fieldName, allowedKinds);
}

function arrayOfProtocolObjectRefs(attributes, fieldName, allowedKinds) {
  const value = attributes[fieldName] ?? [];
  if (!Array.isArray(value)) {
    throw new ModelValidationError(`${fieldName} must be an array of protocol object references`);
  }
  return value.map((entry, index) => normalizeProtocolObjectRef(entry, `${fieldName}[${index}]`, allowedKinds));
}

function normalizeCoreRecordRef(value, fieldName) {
  if (!isPlainObject(value)) {
    throw new ModelValidationError(`${fieldName} must be a Core record reference`);
  }

  const source = optionalString(value, 'source') ?? 'core';
  const objectId = optionalString(value, 'objectId');
  const uri = optionalString(value, 'uri');

  return {
    id: requiredString(value, 'id'),
    source,
    ...(objectId ? { objectId } : {}),
    ...(uri ? { uri } : {}),
  };
}

function arrayOfCoreRecordRefs(attributes, fieldName) {
  const value = attributes[fieldName] ?? [];
  if (!Array.isArray(value)) {
    throw new ModelValidationError(`${fieldName} must be an array of Core record references`);
  }
  return value.map((entry, index) => normalizeCoreRecordRef(entry, `${fieldName}[${index}]`));
}

function rejectHubLocalPermissions(attributes, entityName) {
  for (const fieldName of HUB_LOCAL_PERMISSION_FIELDS) {
    if (attributes[fieldName] !== undefined) {
      throw new ModelValidationError(
        `${fieldName} on ${entityName} is owned by Core/protocol; reference Core policy and grant records instead`,
      );
    }
  }
}

function corePolicyRefs(attributes) {
  return {
    policyRefs: arrayOfProtocolObjectRefs(attributes, 'policyRefs', POLICY_REF_KINDS),
    grantRefs: arrayOfCoreRecordRefs(attributes, 'grantRefs'),
    policyDecisionRefs: arrayOfCoreRecordRefs(attributes, 'policyDecisionRefs'),
    auditEventRefs: arrayOfCoreRecordRefs(attributes, 'auditEventRefs'),
  };
}

function repositoryPolicyRefs(attributes) {
  return {
    policyRef: optionalProtocolObjectRef(attributes, 'policyRef', POLICY_REF_KINDS),
    authorityRootRef: optionalProtocolObjectRef(attributes, 'authorityRootRef', AUTHORITY_ROOT_REF_KINDS),
    ...corePolicyRefs(attributes),
  };
}

export class ModelValidationError extends Error {
  constructor(message) {
    super(message);
    this.name = 'ModelValidationError';
    this.code = 'model_validation_failed';
  }
}

export function slugify(value) {
  const slug = value
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+|-+$/g, '');

  if (slug === '') {
    throw new ModelValidationError('slug must contain at least one letter or number');
  }

  return slug;
}

/**
 * @typedef {Object} Organization
 * @property {string} id
 * @property {string} name
 * @property {string} slug
 * @property {Principal | undefined} ownerPrincipal
 * @property {Principal[]} principalRefs
 * @property {ObjectRef[]} policyRefs
 * @property {CoreRecordRef[]} grantRefs
 * @property {CoreRecordRef[]} policyDecisionRefs
 * @property {CoreRecordRef[]} auditEventRefs
 * @property {Record<string, unknown>} metadata
 * @property {string} createdAt
 * @property {string} updatedAt
 */
export function createOrganization(attributes) {
  rejectHubLocalPermissions(attributes, 'organization');
  const name = requiredString(attributes, 'name');
  const timestamp = nowIso();

  return {
    id: attributes.id ?? id('org'),
    name,
    slug: attributes.slug ? slugify(attributes.slug) : slugify(name),
    ownerPrincipal: optionalPrincipal(attributes, 'ownerPrincipal'),
    principalRefs: arrayOfPrincipals(attributes, 'principalRefs'),
    ...corePolicyRefs(attributes),
    metadata: attributes.metadata ?? {},
    createdAt: attributes.createdAt ?? timestamp,
    updatedAt: attributes.updatedAt ?? timestamp,
  };
}

/**
 * @typedef {Object} Project
 * @property {string} id
 * @property {string} organizationId
 * @property {string} name
 * @property {string} slug
 * @property {string | undefined} description
 * @property {'active' | 'archived'} status
 * @property {string[]} repositoryIds
 * @property {string[]} policyIds
 * @property {Principal | undefined} createdByPrincipal
 * @property {Principal[]} principalRefs
 * @property {ObjectRef[]} policyRefs
 * @property {CoreRecordRef[]} grantRefs
 * @property {CoreRecordRef[]} policyDecisionRefs
 * @property {CoreRecordRef[]} auditEventRefs
 * @property {Record<string, unknown>} metadata
 * @property {string} createdAt
 * @property {string} updatedAt
 */
export function createProject(attributes) {
  rejectHubLocalPermissions(attributes, 'project');
  const name = requiredString(attributes, 'name');
  const timestamp = nowIso();

  return {
    id: attributes.id ?? id('proj'),
    organizationId: requiredString(attributes, 'organizationId'),
    name,
    slug: attributes.slug ? slugify(attributes.slug) : slugify(name),
    description: optionalString(attributes, 'description'),
    status: enumValue(attributes, 'status', PROJECT_STATUSES, 'active'),
    repositoryIds: arrayOfStrings(attributes, 'repositoryIds'),
    policyIds: arrayOfStrings(attributes, 'policyIds'),
    createdByPrincipal: optionalPrincipal(attributes, 'createdByPrincipal'),
    principalRefs: arrayOfPrincipals(attributes, 'principalRefs'),
    ...corePolicyRefs(attributes),
    metadata: attributes.metadata ?? {},
    createdAt: attributes.createdAt ?? timestamp,
    updatedAt: attributes.updatedAt ?? timestamp,
  };
}

/**
 * @typedef {Object} Repository
 * @property {string} id
 * @property {string} organizationId
 * @property {string} projectId
 * @property {string} provider
 * @property {string} owner
 * @property {string} name
 * @property {string} defaultBranch
 * @property {string | undefined} url
 * @property {string | undefined} externalId
 * @property {Principal | undefined} linkedByPrincipal
 * @property {Principal[]} principalRefs
 * @property {ObjectRef | undefined} policyRef
 * @property {ObjectRef | undefined} authorityRootRef
 * @property {ObjectRef[]} policyRefs
 * @property {CoreRecordRef[]} grantRefs
 * @property {CoreRecordRef[]} policyDecisionRefs
 * @property {CoreRecordRef[]} auditEventRefs
 * @property {string} createdAt
 * @property {string} updatedAt
 */
export function createRepository(attributes) {
  rejectHubLocalPermissions(attributes, 'repository');
  const timestamp = nowIso();

  return {
    id: attributes.id ?? id('repo'),
    organizationId: requiredString(attributes, 'organizationId'),
    projectId: requiredString(attributes, 'projectId'),
    provider: requiredString(attributes, 'provider'),
    owner: requiredString(attributes, 'owner'),
    name: requiredString(attributes, 'name'),
    defaultBranch: optionalString(attributes, 'defaultBranch') ?? 'main',
    url: optionalString(attributes, 'url'),
    externalId: optionalString(attributes, 'externalId'),
    linkedByPrincipal: optionalPrincipal(attributes, 'linkedByPrincipal'),
    principalRefs: arrayOfPrincipals(attributes, 'principalRefs'),
    ...repositoryPolicyRefs(attributes),
    createdAt: attributes.createdAt ?? timestamp,
    updatedAt: attributes.updatedAt ?? timestamp,
  };
}

/**
 * @typedef {Object} Proposal
 * @property {string} id
 * @property {string} projectId
 * @property {string | undefined} repositoryId
 * @property {string | undefined} syncRepoId
 * @property {string} title
 * @property {string | undefined} description
 * @property {string} authorRef
 * @property {Principal | undefined} authorPrincipal
 * @property {string | undefined} sourceBranch
 * @property {string | undefined} targetBranch
 * @property {string | undefined} sourceLane
 * @property {string | undefined} targetLane
 * @property {string | undefined} sourceSnapshot
 * @property {string | undefined} targetSnapshot
 * @property {'draft' | 'open' | 'approved' | 'rejected' | 'merged' | 'closed'} status
 * @property {string[]} workflowRunIds
 * @property {Principal[]} principalRefs
 * @property {ObjectRef[]} policyRefs
 * @property {CoreRecordRef[]} grantRefs
 * @property {CoreRecordRef[]} policyDecisionRefs
 * @property {CoreRecordRef[]} auditEventRefs
 * @property {Record<string, unknown>} metadata
 * @property {string} createdAt
 * @property {string} updatedAt
 */
export function createProposal(attributes) {
  rejectHubLocalPermissions(attributes, 'proposal');
  const timestamp = nowIso();
  const authorPrincipal = optionalPrincipal(attributes, 'authorPrincipal');
  const authorRef = optionalString(attributes, 'authorRef') ?? principalRef(authorPrincipal);

  if (!authorRef) {
    throw new ModelValidationError('authorPrincipal or authorRef is required');
  }

  return {
    id: attributes.id ?? id('prop'),
    projectId: requiredString(attributes, 'projectId'),
    repositoryId: optionalString(attributes, 'repositoryId'),
    syncRepoId: optionalString(attributes, 'syncRepoId'),
    title: requiredString(attributes, 'title'),
    description: optionalString(attributes, 'description'),
    authorRef,
    authorPrincipal,
    sourceBranch: optionalString(attributes, 'sourceBranch'),
    targetBranch: optionalString(attributes, 'targetBranch'),
    sourceLane: optionalString(attributes, 'sourceLane'),
    targetLane: optionalString(attributes, 'targetLane'),
    sourceSnapshot: optionalString(attributes, 'sourceSnapshot'),
    targetSnapshot: optionalString(attributes, 'targetSnapshot'),
    status: enumValue(attributes, 'status', PROPOSAL_STATUSES, 'draft'),
    workflowRunIds: arrayOfStrings(attributes, 'workflowRunIds'),
    principalRefs: arrayOfPrincipals(attributes, 'principalRefs'),
    ...corePolicyRefs(attributes),
    metadata: attributes.metadata ?? {},
    createdAt: attributes.createdAt ?? timestamp,
    updatedAt: attributes.updatedAt ?? timestamp,
  };
}

/**
 * Apply a partial update to an existing proposal (status transitions + editable fields).
 * @param {Proposal} proposal
 * @param {Record<string, unknown>} attributes
 * @returns {Proposal}
 */
export function updateProposal(proposal, attributes) {
  rejectHubLocalPermissions(attributes, 'proposal');
  if (!isPlainObject(attributes)) {
    throw new ModelValidationError('proposal update body must be an object');
  }

  const next = { ...proposal, updatedAt: nowIso() };

  if (attributes.status !== undefined) {
    const status = enumValue(attributes, 'status', PROPOSAL_STATUSES);
    const allowed = PROPOSAL_STATUS_TRANSITIONS[proposal.status] ?? [];
    if (status !== proposal.status && !allowed.includes(status)) {
      throw new ModelValidationError(
        `cannot transition proposal status from ${proposal.status} to ${status}`,
      );
    }
    next.status = status;
  }

  for (const field of [
    'description',
    'repositoryId',
    'syncRepoId',
    'sourceBranch',
    'targetBranch',
    'sourceLane',
    'targetLane',
    'sourceSnapshot',
    'targetSnapshot',
  ]) {
    if (attributes[field] !== undefined) {
      next[field] = optionalString(attributes, field);
    }
  }

  if (attributes.title !== undefined) {
    next.title = requiredString(attributes, 'title');
  }

  if (attributes.metadata !== undefined) {
    if (!isPlainObject(attributes.metadata)) {
      throw new ModelValidationError('metadata must be an object');
    }
    next.metadata = { ...proposal.metadata, ...attributes.metadata };
  }

  if (attributes.workflowRunIds !== undefined) {
    next.workflowRunIds = arrayOfStrings(attributes, 'workflowRunIds');
  }

  return next;
}

/**
 * @typedef {Object} ReviewComment
 * @property {string} id
 * @property {string} proposalId
 * @property {string} authorRef
 * @property {Principal | undefined} authorPrincipal
 * @property {string} body
 * @property {string | undefined} path
 * @property {number | undefined} line
 * @property {'open' | 'resolved'} state
 * @property {Principal[]} principalRefs
 * @property {ObjectRef[]} policyRefs
 * @property {CoreRecordRef[]} grantRefs
 * @property {CoreRecordRef[]} policyDecisionRefs
 * @property {CoreRecordRef[]} auditEventRefs
 * @property {Record<string, unknown>} metadata
 * @property {string} createdAt
 * @property {string} updatedAt
 */
export function createReviewComment(attributes) {
  rejectHubLocalPermissions(attributes, 'review comment');
  const timestamp = nowIso();
  const authorPrincipal = optionalPrincipal(attributes, 'authorPrincipal');
  const authorRef = optionalString(attributes, 'authorRef') ?? principalRef(authorPrincipal);

  if (!authorRef) {
    throw new ModelValidationError('authorPrincipal or authorRef is required');
  }

  return {
    id: attributes.id ?? id('comment'),
    proposalId: requiredString(attributes, 'proposalId'),
    authorRef,
    authorPrincipal,
    body: requiredString(attributes, 'body'),
    path: optionalString(attributes, 'path'),
    line: attributes.line,
    state: enumValue(attributes, 'state', REVIEW_COMMENT_STATES, 'open'),
    principalRefs: arrayOfPrincipals(attributes, 'principalRefs'),
    ...corePolicyRefs(attributes),
    metadata: attributes.metadata ?? {},
    createdAt: attributes.createdAt ?? timestamp,
    updatedAt: attributes.updatedAt ?? timestamp,
  };
}

/**
 * @param {import('./models.js').ReviewComment | object} comment
 * @param {Record<string, unknown>} attributes
 */
export function updateReviewComment(comment, attributes) {
  rejectHubLocalPermissions(attributes, 'review comment');
  if (!isPlainObject(attributes)) {
    throw new ModelValidationError('review comment update body must be an object');
  }

  const next = { ...comment, updatedAt: nowIso() };

  if (attributes.body !== undefined) {
    next.body = requiredString(attributes, 'body');
  }
  if (attributes.state !== undefined) {
    next.state = enumValue(attributes, 'state', REVIEW_COMMENT_STATES);
  }
  if (attributes.path !== undefined) {
    next.path = optionalString(attributes, 'path');
  }
  if (attributes.line !== undefined) {
    next.line = attributes.line;
  }
  if (attributes.metadata !== undefined) {
    if (!isPlainObject(attributes.metadata)) {
      throw new ModelValidationError('metadata must be an object');
    }
    next.metadata = { ...comment.metadata, ...attributes.metadata };
  }

  return next;
}

/**
 * @typedef {Object} WorkflowRun
 * @property {string} id
 * @property {string} projectId
 * @property {string | undefined} proposalId
 * @property {string} name
 * @property {string | undefined} providerRunId
 * @property {'queued' | 'in_progress' | 'succeeded' | 'failed' | 'cancelled'} status
 * @property {Principal | undefined} requestedByPrincipal
 * @property {Principal | undefined} runnerPrincipal
 * @property {Principal[]} principalRefs
 * @property {ObjectRef[]} policyRefs
 * @property {CoreRecordRef[]} grantRefs
 * @property {CoreRecordRef[]} policyDecisionRefs
 * @property {CoreRecordRef[]} auditEventRefs
 * @property {Record<string, unknown>} metadata
 * @property {string | undefined} startedAt
 * @property {string | undefined} completedAt
 * @property {string} createdAt
 * @property {string} updatedAt
 */
export function createWorkflowRun(attributes) {
  rejectHubLocalPermissions(attributes, 'workflow run');
  const timestamp = nowIso();

  return {
    id: attributes.id ?? id('run'),
    projectId: requiredString(attributes, 'projectId'),
    proposalId: optionalString(attributes, 'proposalId'),
    name: requiredString(attributes, 'name'),
    providerRunId: optionalString(attributes, 'providerRunId'),
    status: enumValue(attributes, 'status', WORKFLOW_RUN_STATUSES, 'queued'),
    requestedByPrincipal: optionalPrincipal(attributes, 'requestedByPrincipal'),
    runnerPrincipal: optionalPrincipal(attributes, 'runnerPrincipal'),
    principalRefs: arrayOfPrincipals(attributes, 'principalRefs'),
    ...corePolicyRefs(attributes),
    metadata: attributes.metadata ?? {},
    startedAt: optionalString(attributes, 'startedAt'),
    completedAt: optionalString(attributes, 'completedAt'),
    createdAt: attributes.createdAt ?? timestamp,
    updatedAt: attributes.updatedAt ?? timestamp,
  };
}

/**
 * @param {object} run
 * @param {Record<string, unknown>} attributes
 */
export function updateWorkflowRun(run, attributes) {
  rejectHubLocalPermissions(attributes, 'workflow run');
  if (!isPlainObject(attributes)) {
    throw new ModelValidationError('workflow run update body must be an object');
  }

  const next = { ...run, updatedAt: nowIso() };
  if (attributes.status !== undefined) {
    next.status = enumValue(attributes, 'status', WORKFLOW_RUN_STATUSES);
    if (['in_progress', 'succeeded', 'failed', 'cancelled'].includes(next.status) && !next.startedAt) {
      next.startedAt = next.startedAt ?? nowIso();
    }
    if (['succeeded', 'failed', 'cancelled'].includes(next.status)) {
      next.completedAt = optionalString(attributes, 'completedAt') ?? nowIso();
    }
  }
  if (attributes.providerRunId !== undefined) {
    next.providerRunId = optionalString(attributes, 'providerRunId');
  }
  if (attributes.metadata !== undefined) {
    if (!isPlainObject(attributes.metadata)) {
      throw new ModelValidationError('metadata must be an object');
    }
    next.metadata = { ...run.metadata, ...attributes.metadata };
  }
  return next;
}

/**
 * @typedef {Object} Policy
 * @property {string} id
 * @property {string} organizationId
 * @property {string | undefined} projectId
 * @property {string} name
 * @property {string | undefined} description
 * @property {boolean} enabled
 * @property {ObjectRef | undefined} policyRef
 * @property {Principal[]} principalRefs
 * @property {ObjectRef[]} policyRefs
 * @property {CoreRecordRef[]} grantRefs
 * @property {CoreRecordRef[]} policyDecisionRefs
 * @property {CoreRecordRef[]} auditEventRefs
 * @property {Record<string, unknown>} metadata
 * @property {string} createdAt
 * @property {string} updatedAt
 */
export function createPolicy(attributes) {
  rejectHubLocalPermissions(attributes, 'policy');

  const timestamp = nowIso();

  return {
    id: attributes.id ?? id('policy'),
    organizationId: requiredString(attributes, 'organizationId'),
    projectId: optionalString(attributes, 'projectId'),
    name: requiredString(attributes, 'name'),
    description: optionalString(attributes, 'description'),
    enabled: attributes.enabled ?? true,
    policyRef: optionalProtocolObjectRef(attributes, 'policyRef', POLICY_REF_KINDS),
    principalRefs: arrayOfPrincipals(attributes, 'principalRefs'),
    ...corePolicyRefs(attributes),
    metadata: attributes.metadata ?? {},
    createdAt: attributes.createdAt ?? timestamp,
    updatedAt: attributes.updatedAt ?? timestamp,
  };
}

function principalRef(principal) {
  if (!principal) {
    return undefined;
  }
  return `${principal.type}:${principal.id}`;
}
