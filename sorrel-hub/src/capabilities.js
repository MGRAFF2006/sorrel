/**
 * Modular install capabilities advertised to clients.
 * `GET /capabilities` hides dead nav when modules aren’t installed.
 */

/**
 * @typedef {{
 *   modules: {
 *     core: true,
 *     actions: boolean,
 *     agents: boolean,
 *     secrets: boolean,
 *     objectStorage: 'fs' | 'memory',
 *   },
 *   auth: { mode: 'dev' | 'workos' | 'oidc', session: 'cookie' | 'bearer' | 'none' },
 *   convex: { enabled: boolean, url?: string },
 *   deploy: 'saas' | 'selfhost' | 'dev',
 * }} HubCapabilities
 */

/**
 * @param {{
 *   authMode?: 'dev' | 'workos' | 'oidc',
 *   env?: NodeJS.ProcessEnv,
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
  const objectStorage = env.SORREL_HUB_SYNC_STORE === 'memory' ? 'memory' : 'fs';

  // Browsers cannot use an internal Compose/service URL. Operators can expose
  // a separate public origin while the Hub mirror keeps using CONVEX_URL.
  const convexUrl =
    env.CONVEX_PUBLIC_URL || env.VITE_CONVEX_URL || env.CONVEX_URL || undefined;
  const convexEnabled =
    env.SORREL_HUB_CONVEX === '0' || env.SORREL_HUB_CONVEX === 'false'
      ? false
      : Boolean(convexUrl) || env.SORREL_HUB_CONVEX === '1' || env.SORREL_HUB_CONVEX === 'true';

  let deploy = /** @type {'saas'|'selfhost'|'dev'} */ ('dev');
  if (env.SORREL_HUB_DEPLOY === 'saas' || env.SORREL_HUB_DEPLOY === 'selfhost') {
    deploy = env.SORREL_HUB_DEPLOY;
  } else if (authMode === 'workos') {
    deploy = 'saas';
  } else if (authMode === 'oidc') {
    deploy = 'selfhost';
  }

  return {
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
      enabled: convexEnabled,
      ...(convexUrl ? { url: convexUrl } : {}),
    },
    deploy,
  };
}
