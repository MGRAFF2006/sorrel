import { isConvexMirrorEnabled } from './convex-mirror.js';
import { PROPOSAL_STATUS_TRANSITIONS } from './models.js';

/**
 * Modular install capabilities advertised to clients.
 * `GET /capabilities` hides dead nav when modules aren’t installed.
 */

/**
 * @typedef {{
 *   collaboration: { proposalTransitions: Record<string, string[]> },
 *   modules: {
 *     core: true,
 *     actions: boolean,
 *     agents: boolean,
 *     secrets: boolean,
 *     objectStorage: 'fs' | 'memory',
 *   },
 *   auth: { mode: 'dev' | 'workos' | 'oidc', session: 'cookie' | 'bearer' | 'none' },
 *   convex: { enabled: boolean },
 *   deploy: 'saas' | 'selfhost' | 'dev',
 * }} HubCapabilities
 */

/**
 * @param {{
 *   authMode?: 'dev' | 'workos' | 'oidc',
 *   env?: NodeJS.ProcessEnv,
 *   objectStorage?: 'fs' | 'memory',
 *   convexEnabled?: boolean,
 * }} [options]
 * @returns {HubCapabilities}
 */
export function resolveCapabilities(options = {}) {
  const env = options.env ?? process.env;
  const authMode = options.authMode ?? /** @type {'dev'|'workos'|'oidc'} */ (
    (env.SORREL_HUB_AUTH ?? 'dev').toLowerCase()
  );

  // Optional module flags are reserved until the Hub actually wires their
  // routes and product surfaces. Never advertise an env toggle as installed.
  const actions = false;
  const agents = false;
  const secrets = false;
  const objectStorage = options.objectStorage ??
    (env.SORREL_HUB_SYNC_STORE === 'memory' ? 'memory' : 'fs');

  let deploy = /** @type {'saas'|'selfhost'|'dev'} */ ('dev');
  if (env.SORREL_HUB_DEPLOY === 'saas' || env.SORREL_HUB_DEPLOY === 'selfhost') {
    deploy = env.SORREL_HUB_DEPLOY;
  } else if (authMode === 'workos') {
    deploy = 'saas';
  } else if (authMode === 'oidc') {
    deploy = 'selfhost';
  }

  return {
    collaboration: { proposalTransitions: PROPOSAL_STATUS_TRANSITIONS },
    modules: {
      core: true,
      actions,
      agents,
      secrets,
      objectStorage,
    },
    auth: {
      mode: authMode === 'workos' || authMode === 'oidc' ? authMode : 'dev',
      session: authMode === 'dev' ? 'none' : 'bearer',
    },
    convex: {
      enabled: options.convexEnabled ?? isConvexMirrorEnabled(env),
    },
    deploy,
  };
}
