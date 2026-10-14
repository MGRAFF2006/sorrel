export function assertHostedConfiguration(env, trustedGrantsById) {
  if (env.SORREL_HUB_DEPLOY !== 'saas') return;
  if (env.SORREL_HUB_AUTH !== 'workos' || !env.SORREL_HUB_PUBLIC_URL || !env.WORKOS_COOKIE_PASSWORD) {
    throw new Error('Hosted Hub requires WorkOS browser login and an explicit HTTPS public origin');
  }
  if (env.SORREL_HUB_LOCAL_DEMO === '1' || ['1', 'true'].includes(env.SORREL_HUB_BOOTSTRAP_GRANTS) || ['1', 'true'].includes(env.SORREL_HUB_ALLOW_INSECURE_DEV_AUTH)) {
    throw new Error('Hosted Hub cannot enable development identities, bootstrap grants or insecure demo auth');
  }
  if (!env.SORREL_HUB_TRUSTED_GRANTS_FILE || Object.keys(trustedGrantsById).length === 0) {
    throw new Error('Hosted Hub requires operator-provisioned Core grants before startup');
  }
}
