/**
 * Local single-user bootstrap grants for the CLI default acting principal
 * (`{"type":"user","id":"local"}`).
 *
 * These are real Core-shaped grant records used by Hub's trusted-grant
 * evaluation — not stubs. They intentionally omit `resource` so they match any
 * repo id (see `resourcesMatch` in core-policy.js). They are disabled by
 * default and intended only for local development. Enable explicitly with
 * `SORREL_HUB_BOOTSTRAP_GRANTS=1`.
 */

import { readFileSync } from 'node:fs';

export const BOOTSTRAP_OBJECT_WRITE_GRANT_ID = 'grant_local_object_write';
export const BOOTSTRAP_REF_WRITE_GRANT_ID = 'grant_local_ref_write';

export const LOCAL_BOOTSTRAP_PRINCIPAL = Object.freeze({
  type: 'user',
  id: 'local',
});

/**
 * @returns {Record<string, object>}
 */
export function createLocalBootstrapGrants() {
  return {
    [BOOTSTRAP_OBJECT_WRITE_GRANT_ID]: {
      id: BOOTSTRAP_OBJECT_WRITE_GRANT_ID,
      source: 'core',
      principal: { type: 'user', id: 'local' },
      action: 'repo.object.write',
    },
    [BOOTSTRAP_REF_WRITE_GRANT_ID]: {
      id: BOOTSTRAP_REF_WRITE_GRANT_ID,
      source: 'core',
      principal: { type: 'user', id: 'local' },
      action: 'repo.ref.write',
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
  const grants = bootstrapEnabled ? createLocalBootstrapGrants() : {};

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
