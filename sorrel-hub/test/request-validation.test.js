import assert from 'node:assert/strict';
import http from 'node:http';
import test from 'node:test';

import { createDemoApp as createApp } from '../test-support/demo-app.js';

async function withServer(t) {
  const app = createApp();
  const server = http.createServer(app.handleRequest);
  await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
  t.after(() => new Promise((resolve, reject) => {
    server.close((error) => error ? reject(error) : resolve());
  }));
  return { app, baseUrl: `http://127.0.0.1:${server.address().port}` };
}

test('malformed encoded path parameters return client errors', async (t) => {
  const { baseUrl } = await withServer(t);
  for (const path of ['/projects/%zz', '/admin/proposals/%zz', '/repo_%zz/refs',
    '/repo_valid/refs/lane%zz']) {
    const response = await fetch(`${baseUrl}${path}`, {
      method: path.includes('/refs/lane') ? 'POST' : 'GET',
    });
    assert.equal(response.status, 400, path);
    assert.equal((await response.json()).error.code, 'invalid_request');
  }
});

test('extra route segments and prototype names cannot resolve existing resources', async (t) => {
  const { app, baseUrl } = await withServer(t);
  const project = app.store.createProject({ organizationId: 'org_test', name: 'Project' });
  const proposal = app.store.createProposal({ projectId: project.id, title: 'Tip', authorRef: 'user:local' });
  for (const path of [`/projects/${project.id}/extra`,
    `/admin/proposals/${proposal.id}/comments/extra`, '/admin/constructor', '/admin/__proto__']) {
    const response = await fetch(`${baseUrl}${path}`);
    assert.equal(response.status, 404, path);
  }
});

test('non-object JSON bodies return client errors on every request parser', async (t) => {
  const { baseUrl } = await withServer(t);
  for (const path of ['/projects', '/admin/organizations', '/collaboration/lane-submit',
    '/repo_valid/objects/missing', '/repo_valid/objects', '/repo_valid/refs/main']) {
    for (const body of [null, [], 42, 'wrong']) {
      const response = await fetch(`${baseUrl}${path}`, {
        method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify(body),
      });
      assert.equal(response.status, 400, `${path}: ${JSON.stringify(body)}`);
      assert.equal((await response.json()).error.code, 'invalid_request_body');
    }
  }
});

test('explicit invalid metadata IDs are rejected before persistence', async (t) => {
  const { app, baseUrl } = await withServer(t);
  for (const id of ['', ' ', 42, {}, '.', '..', '\ud800', '\udc00']) {
    const response = await fetch(`${baseUrl}/projects`, {
      method: 'POST', headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ id, organizationId: 'org_test', name: 'Project' }),
    });
    assert.equal(response.status, 400);
    assert.equal((await response.json()).error.code, 'model_validation_failed');
  }
  assert.deepEqual(app.store.listProjects(), []);
});


test('creation locations round-trip arbitrary string IDs', async (t) => {
  const { baseUrl } = await withServer(t);
  for (const [route, payload] of [
    ['/projects', { id: 'proj/ü?part#one', organizationId: 'org_test', name: 'Unicode Project' }],
    ['/admin/organizations', { id: 'org/组织?part#one', name: 'Unicode Organization' }],
  ]) {
    const response = await fetch(`${baseUrl}${route}`, {
      method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify(payload),
    });
    assert.equal(response.status, 201);
    const location = response.headers.get('location');
    assert.equal(location, `${route}/${encodeURIComponent(payload.id)}`);
    const get = await fetch(`${baseUrl}${location}`);
    assert.equal(get.status, 200);
    assert.equal((await get.json()).data.id, payload.id);
  }
});


test('malformed grant references produce validation errors instead of server exceptions', async (t) => {
  const { app, baseUrl } = await withServer(t);
  app.store.createProject({ id: 'proj_valid', organizationId: 'org_valid', name: 'Fixture' });
  for (const path of ['/repo_valid/objects', '/repo_valid/refs/main', '/admin/repositories']) {
    for (const grantRefs of [[null], [42], [{}], [{ id: '' }]]) {
      const response = await fetch(`${baseUrl}${path}`, {
        method: 'POST', headers: {
          'content-type': 'application/json',
          'x-sorrel-acting-principal': JSON.stringify({ type: 'user', id: 'local' }),
        },
        body: JSON.stringify({ snapshot: 'aa'.repeat(32), grantRefs, projectId: 'proj_valid', organizationId: 'org_valid', provider: 'sorrel', owner: 'local', name: 'Malformed grants' }),
      });
      assert.equal(response.status, path.startsWith('/admin/') ? 400 : 403);
      assert.equal((await response.json()).error.code, path.startsWith('/admin/') ? 'model_validation_failed' : 'policy_evaluation_failed');
    }
  }
});


test('malformed absolute request targets return 400 and leave the server alive', async (t) => {
  const { baseUrl } = await withServer(t);
  const status = await new Promise((resolve, reject) => {
    const request = http.request(baseUrl, { path: 'http://[invalid', method: 'GET' }, (response) => {
      response.resume();
      response.on('end', () => resolve(response.statusCode));
    });
    request.on('error', reject);
    request.setTimeout(2000, () => request.destroy(new Error('request target was not handled')));
    request.end();
  });
  assert.equal(status, 400);
  assert.equal((await fetch(`${baseUrl}/healthz`)).status, 200);
});
