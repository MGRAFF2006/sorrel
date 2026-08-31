/**
 * Refuse unsafe binds for development auth / bootstrap grants.
 *
 * Dev auth trusts `x-sorrel-acting-principal`. Binding that to a non-loopback
 * interface without an explicit override is a foot-gun — fail closed at startup.
 */

/**
 * @param {string} host
 * @returns {boolean}
 */
export function isLoopbackHost(host) {
  const normalized = String(host ?? '').trim().toLowerCase();
  return (
    normalized === '127.0.0.1' ||
    normalized === '::1' ||
    normalized === 'localhost' ||
    normalized === '::ffff:127.0.0.1'
  );
}

/**
 * @param {{
 *   host: string,
 *   authMode: string,
 *   bootstrapGrantsEnabled?: boolean,
 *   env?: NodeJS.ProcessEnv,
 * }} options
 * @returns {{ ok: true } | { ok: false, message: string }}
 */
export function evaluateBindSafety(options) {
  const env = options.env ?? process.env;
  const allowInsecure =
    env.SORREL_HUB_ALLOW_INSECURE_DEV_AUTH === '1' ||
    env.SORREL_HUB_ALLOW_INSECURE_DEV_AUTH === 'true';

  if (isLoopbackHost(options.host) || allowInsecure) {
    return { ok: true };
  }

  if (options.authMode === 'dev') {
    return {
      ok: false,
      message:
        `Refusing to bind auth=dev on non-loopback host "${options.host}". ` +
        `Use SORREL_HUB_AUTH=workos|oidc, bind HOST=127.0.0.1, or set ` +
        `SORREL_HUB_ALLOW_INSECURE_DEV_AUTH=1 for an isolated demo only.`,
    };
  }

  if (options.bootstrapGrantsEnabled) {
    return {
      ok: false,
      message:
        `Refusing to enable SORREL_HUB_BOOTSTRAP_GRANTS on non-loopback host ` +
        `"${options.host}". Disable bootstrap grants or bind to loopback ` +
        `(override with SORREL_HUB_ALLOW_INSECURE_DEV_AUTH=1 for demos).`,
    };
  }

  return { ok: true };
}

/**
 * @param {{
 *   host: string,
 *   authMode: string,
 *   bootstrapGrantsEnabled?: boolean,
 *   env?: NodeJS.ProcessEnv,
 * }} options
 */
export function assertSafeHubBind(options) {
  const result = evaluateBindSafety(options);
  if (!result.ok) {
    console.error(`sorrel-hub: ${result.message}`);
    process.exit(1);
  }
}
