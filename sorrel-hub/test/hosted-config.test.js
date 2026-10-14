import assert from 'node:assert/strict';
import test from 'node:test';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
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

test('actual SaaS startup rejects invalid authority, credentials and browser origins before listening', t => {
  const root = mkdtempSync(join(tmpdir(), 'sorrel-hosted-startup-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const grantsFile = join(root, 'grants.json');
  writeFileSync(grantsFile, JSON.stringify({ grant_fixture: {} }));
  const env = { SORREL_HUB_DEPLOY: 'saas', SORREL_HUB_AUTH: 'workos', SORREL_HUB_SYNC_STORE: 'memory',
    SORREL_HUB_PUBLIC_URL: 'https://hub.example', WORKOS_API_KEY: 'sk_test_fixture', WORKOS_CLIENT_ID: 'client_fixture',
    WORKOS_COOKIE_PASSWORD: 'fixture-password-of-at-least-32-characters', SORREL_HUB_TRUSTED_GRANTS_FILE: grantsFile,
    HOST: '127.0.0.1', PORT: '0' };
  for (const [overrides, message] of [
    [{ WORKOS_API_KEY: '' }, /WorkOS browser login requires/],
    [{ WORKOS_COOKIE_PASSWORD: 'short' }, /32 characters/],
    [{ SORREL_HUB_PUBLIC_URL: 'http://hub.example' }, /HTTPS origin/],
    [{ SORREL_HUB_TRUSTED_GRANTS_FILE: '' }, /Core grants/],
    [{ SORREL_HUB_BOOTSTRAP_GRANTS: 'true' }, /development/],
  ]) {
    const result = spawnSync(process.execPath, [fileURLToPath(new URL('../src/server.js', import.meta.url))], {
      env: { ...env, ...overrides }, timeout: 5_000, encoding: 'utf8',
    });
    assert.notEqual(result.status, null, 'startup must exit rather than listen');
    assert.notEqual(result.status, 0);
    assert.match(result.stderr, message);
    assert.doesNotMatch(result.stdout, /listening/);
  }
});
