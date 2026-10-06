import { createApp } from '../src/app.js';
import { createLocalBootstrapGrants, createLocalDemoGrants } from '../src/bootstrap-grants.js';

/** Behavior fixtures opt into local demo explicitly, with actual native grants. */
export function createDemoApp(options = {}) {
  const { fixturePrincipal, ...appOptions } = options;
  const grants = { ...createLocalBootstrapGrants(), ...createLocalDemoGrants() };
  if (fixturePrincipal) {
    for (const grant of Object.values(grants)) grant.principal = { kind: fixturePrincipal.type, id: fixturePrincipal.id };
  }
  return createApp({ ...appOptions, env: { ...options.env, SORREL_HUB_LOCAL_DEMO: '1' },
    trustedGrantsById: { ...grants, ...options.trustedGrantsById } });
}
