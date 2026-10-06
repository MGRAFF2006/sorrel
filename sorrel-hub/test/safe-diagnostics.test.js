import assert from 'node:assert/strict';
import fs from 'node:fs';
import http from 'node:http';
import os from 'node:os';
import path from 'node:path';
import { test } from 'node:test';

import { createApp } from '../src/app.js';
import { PolicyEvaluationError } from '../src/core-policy.js';
import { createFsMetadataStore } from '../src/fs-metadata-store.js';

async function withServer(app, run, methodOverride) {
  const server = http.createServer((request, response) => {
    if (methodOverride) request.method = methodOverride;
    void app.handleRequest(request, response);
  });
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  try { await run(`http://127.0.0.1:${server.address().port}`); }
  finally { await new Promise(resolve => server.close(resolve)); }
}

test('a real metadata filesystem failure logs useful sanitized diagnostics and preserves HTTP redaction', async (t) => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'operator-private-path-'));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  const store = createFsMetadataStore(root);
  fs.writeFileSync(path.join(root, 'projects'), 'blocked collection fixture');
  const logs = [];
  t.mock.method(console, 'error', (...args) => logs.push(args));
  const grant = { id: 'grant_org', schemaVersion: 'sorrel.protocol.v0', kind: 'Grant',
    principal: { kind: 'user', id: 'local' }, resource: { kind: 'org', id: '*' },
    effect: 'allow', capabilities: ['project.create'] };
  const app = createApp({ store, env: { SORREL_HUB_LOCAL_DEMO: '1' }, trustedGrantsById: { grant_org: grant } });
  await withServer(app, async url => {
    const response = await fetch(`${url}/projects?privateQuery=private-sentinel`, { method: 'POST',
      headers: { 'content-type': 'application/json', authorization: 'Bearer fixture-credential-sentinel',
        'x-sorrel-acting-principal': '{"type":"user","id":"local"}' },
      body: JSON.stringify({ name: 'Private body sentinel', description: 'private-sentinel', organizationId: 'org_local' }) });
    assert.equal(response.status, 500);
    assert.deepEqual(await response.json(), { error: { code: 'internal_server_error', message: 'internal server error' } });
  });
  assert.deepEqual(logs, [['[sorrel-hub] unexpected request failure', { method: 'POST', category: 'filesystem', code: 'EEXIST' }]]);
  for (const sentinel of [root, 'private-sentinel', 'fixture-credential-sentinel', 'Private body sentinel', 'local', 'projects']) {
    assert.equal(JSON.stringify(logs).includes(sentinel), false);
  }
  assert.deepEqual(store.listProjects(), []);
});

test('unknown errors and methods cannot inject arbitrary error fields or request metadata into logs', async (t) => {
  const logs = [];
  t.mock.method(console, 'error', (...args) => logs.push(args));
  const app = createApp({ authAdapter: { mode: 'oidc', async resolveSession() {
    throw Object.assign(new Error('fixture-secret exception message'), { code: 'fixture-secret-code' });
  } } });
  await withServer(app, async url => {
    assert.equal((await fetch(`${url}/private-sentinel`)).status, 500);
  }, 'fixture-secret-method');
  assert.deepEqual(logs, [['[sorrel-hub] unexpected request failure', { method: 'UNKNOWN', category: 'internal' }]]);
});

test('known client and policy evaluation errors do not produce unexpected-error diagnostics', async (t) => {
  const logs = [];
  t.mock.method(console, 'error', (...args) => logs.push(args));
  await withServer(createApp(), async url => {
    assert.equal((await fetch(`${url}/not-found`)).status, 404);
    assert.equal((await fetch(`${url}/projects`, { method: 'POST', headers: { 'content-type': 'application/json' }, body: '{' })).status, 400);
  });
  const policyError = new PolicyEvaluationError('known policy failure');
  const app = createApp({ authAdapter: { mode: 'oidc', async resolveSession() { throw policyError; } } });
  await withServer(app, async url => {
    const response = await fetch(`${url}/projects`);
    assert.equal(response.status, 403);
    assert.equal((await response.json()).error.code, 'policy_evaluation_failed');
  });
  assert.deepEqual(logs, []);
});
