/**
 * Local single-user bootstrap grants for the CLI default acting principal
 * (`{"type":"user","id":"local"}`).
 *
 * These are native Core Grant records with explicit allow effects and repo
 * wildcards. Disabled by default; enable only for local development with
 * `SORREL_HUB_BOOTSTRAP_GRANTS=1`.
 */

import { readFileSync } from 'node:fs';

export const BOOTSTRAP_OBJECT_WRITE_GRANT_ID = 'grant_local_object_write';
export const BOOTSTRAP_REF_WRITE_GRANT_ID = 'grant_local_ref_write';
export const BOOTSTRAP_REPO_READ_GRANT_ID = 'grant_local_repo_read';

/**
 * @returns {Record<string, object>}
 */
export function createLocalBootstrapGrants() {
  return {
    [BOOTSTRAP_REPO_READ_GRANT_ID]: {
      id: BOOTSTRAP_REPO_READ_GRANT_ID, schemaVersion: 'sorrel.protocol.v0', kind: 'Grant',
      principal: { kind: 'user', id: 'local' }, capabilities: ['repo.read'],
      resource: { kind: 'repo', id: '*' }, effect: 'allow',
    },
    [BOOTSTRAP_OBJECT_WRITE_GRANT_ID]: {
      id: BOOTSTRAP_OBJECT_WRITE_GRANT_ID,
      schemaVersion: 'sorrel.protocol.v0',
      kind: 'Grant',
      principal: { kind: 'user', id: 'local' },
      capabilities: ['repo.object.write'],
      resource: { kind: 'repo', id: '*' },
      effect: 'allow',
    },
    [BOOTSTRAP_REF_WRITE_GRANT_ID]: {
      id: BOOTSTRAP_REF_WRITE_GRANT_ID,
      schemaVersion: 'sorrel.protocol.v0',
      kind: 'Grant',
      principal: { kind: 'user', id: 'local' },
      capabilities: ['repo.ref.write'],
      resource: { kind: 'repo', id: '*' },
      effect: 'allow',
    },
  };
}

/**
 * Resolve the trusted-grant map for a running Hub server.
 *
 * @param {NodeJS.ProcessEnv} [env]
 * @returns {Record<string, object>}
 */
export function resolveTrustedGrants(env = process.env) {
  const bootstrapEnabled = env.SORREL_HUB_BOOTSTRAP_GRANTS === '1';
  const grants = bootstrapEnabled ? { ...createLocalBootstrapGrants(),
    ...(env.SORREL_HUB_LOCAL_DEMO === '1' ? createLocalDemoGrants() : {}),
  } : {};

  const grantsFile = env.SORREL_HUB_TRUSTED_GRANTS_FILE;
  if (!grantsFile) {
    return grants;
  }

  const parsed = JSON.parse(readFileSync(grantsFile, 'utf8'));
  if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) {
    throw new Error(
      'SORREL_HUB_TRUSTED_GRANTS_FILE must contain a JSON object of id → grant',
    );
  }
  return { ...grants, ...parsed };
}

/** Operator-supplied native Core policies, evaluated alongside all grants. */
export function resolveTrustedPolicies(env = process.env) {
  if (!env.SORREL_HUB_TRUSTED_POLICIES_FILE) return {};
  const parsed = JSON.parse(readFileSync(env.SORREL_HUB_TRUSTED_POLICIES_FILE, 'utf8'));
  if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) {
    throw new Error('SORREL_HUB_TRUSTED_POLICIES_FILE must contain a JSON object of id → policy');
  }
  return parsed;
}

/** Explicit local demo provisioning; these are ordinary native Core grants. */
export function createLocalDemoGrants() {
  return Object.fromEntries([
    ['org', ['org.read', 'org.write', 'project.create', 'policy.read', 'policy.grant']],
    ['project', ['project.read', 'project.write', 'proposal.read', 'proposal.write', 'proposal.review',
      'review.comment.read', 'review.comment.write', 'workflow.run.read', 'workflow.run.write', 'policy.read', 'policy.grant']],
  ].map(([kind, capabilities]) => {
    const id = `grant_local_demo_${kind}`;
    return [id, { schemaVersion: 'sorrel.protocol.v0', kind: 'Grant', id,
      principal: { kind: 'user', id: 'local' }, capabilities,
      resource: { kind, id: '*' }, effect: 'allow' }];
  }));
}
