import assert from 'node:assert/strict';
import test from 'node:test';
import { assertHostedConfiguration } from '../src/hosted-config.js';

test('hosted mode refuses missing authority and development bypasses', () => {
  const env = { SORREL_HUB_DEPLOY: 'saas', SORREL_HUB_AUTH: 'workos', SORREL_HUB_PUBLIC_URL: 'https://hub.example',
    WORKOS_COOKIE_PASSWORD: 'fixture', SORREL_HUB_TRUSTED_GRANTS_FILE: 'grants.json' };
  assert.doesNotThrow(() => assertHostedConfiguration(env, { grant_fixture: {} }));
  assert.throws(() => assertHostedConfiguration(env, {}), /Core grants/);
  assert.throws(() => assertHostedConfiguration({ ...env, SORREL_HUB_AUTH: 'dev' }, {}), /WorkOS/);
  for (const key of ['SORREL_HUB_LOCAL_DEMO', 'SORREL_HUB_BOOTSTRAP_GRANTS', 'SORREL_HUB_ALLOW_INSECURE_DEV_AUTH']) {
    assert.throws(() => assertHostedConfiguration({ ...env, [key]: '1' }, { grant_fixture: {} }), /development/);
  }
});
