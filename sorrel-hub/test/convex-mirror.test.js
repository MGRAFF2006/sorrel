import assert from 'node:assert/strict';
import http from 'node:http';
import { setImmediate } from 'node:timers/promises';
import { test } from 'node:test';
import { createConvexMirror } from '../src/convex-mirror.js';

function deferred() {
  let resolve;
  let reject;
  const promise = new Promise((done, fail) => { resolve = done; reject = fail; });
  return { promise, resolve, reject };
}

function controlledMirror(t) {
  const requests = [];
  const rows = new Map();
  t.mock.method(globalThis, 'fetch', async (_url, options) => {
    const body = JSON.parse(options.body);
    const gate = deferred();
    requests.push({ ...body, gate });
    await gate.promise;
    if (body.path === 'proposals:remove') rows.delete(body.args.hubId);
    else rows.set(body.args.hubId, body.args);
    return new Response('{}');
  });
  const mirror = createConvexMirror({ CONVEX_URL: 'http://local-fixture.invalid', CONVEX_DEPLOY_KEY: 'fixture-admin' });
  return { mirror, requests, rows };
}

test('same-proposal upserts dispatch in invocation order with captured input', async (t) => {
  const { mirror, requests, rows } = controlledMirror(t);
  const first = mirror.upsertProposal({ id: 'proposal-a', status: 'open', updatedAt: 'later-client-time' });
  const input = { id: 'proposal-a', status: 'approved', updatedAt: 'earlier-client-time' };
  const second = mirror.upsertProposal(input);
  input.status = 'open';
  await setImmediate();
  assert.equal(requests.length, 1, 'newer mutation must wait for old request completion');
  requests[0].gate.resolve();
  await first;
  await setImmediate();
  assert.equal(requests.length, 2);
  assert.equal(requests[1].args.status, 'approved');
  requests[1].gate.resolve();
  await second;
  assert.equal(rows.get('proposal-a').status, 'approved');
});

test('queued remove cannot be overtaken by an earlier upsert, and the ID remains usable', async (t) => {
  const { mirror, requests, rows } = controlledMirror(t);
  const first = mirror.upsertProposal({ id: 'proposal-a', status: 'open' });
  const removal = mirror.removeProposal('proposal-a');
  await setImmediate();
  assert.equal(requests.length, 1);
  requests[0].gate.resolve();
  await first;
  await setImmediate();
  assert.equal(requests[1].path, 'proposals:remove');
  requests[1].gate.resolve();
  await removal;
  assert.equal(rows.has('proposal-a'), false);
  const recreated = mirror.upsertProposal({ id: 'proposal-a', status: 'approved' });
  await setImmediate();
  requests[2].gate.resolve();
  await recreated;
  assert.equal(rows.get('proposal-a').status, 'approved');
});

test('a blocked proposal does not block other proposal IDs', async (t) => {
  const { mirror, requests, rows } = controlledMirror(t);
  const first = mirror.upsertProposal({ id: 'proposal-a', status: 'open' });
  const other = mirror.upsertProposal({ id: 'proposal-b', status: 'approved' });
  await setImmediate();
  assert.deepEqual(requests.map(request => request.args.hubId), ['proposal-a', 'proposal-b']);
  requests[1].gate.resolve();
  await other;
  assert.equal(rows.get('proposal-b').status, 'approved');
  assert.equal(rows.has('proposal-a'), false);
  requests[0].gate.resolve();
  await first;
});

test('transport failure does not poison queued operations or leak backend error text', async (t) => {
  const warnings = [];
  t.mock.method(console, 'warn', message => warnings.push(message));
  const { mirror, requests, rows } = controlledMirror(t);
  const first = mirror.upsertProposal({ id: 'proposal-a', status: 'open' });
  const next = mirror.upsertProposal({ id: 'proposal-a', status: 'approved' });
  await setImmediate();
  requests[0].gate.reject(new Error('fixture-admin private backend failure'));
  await first;
  await setImmediate();
  assert.equal(requests.length, 2);
  requests[1].gate.resolve();
  await next;
  assert.equal(rows.get('proposal-a').status, 'approved');
  assert.deepEqual(warnings, ['[convex-mirror] /api/mutation request failed']);
});

test('mirror stays disabled without both server URL and privileged key', async () => {
  for (const env of [{}, { CONVEX_URL: 'http://127.0.0.1:1' },
    { CONVEX_DEPLOY_KEY: 'fixture-admin' },
    { CONVEX_URL: 'http://127.0.0.1:1', CONVEX_DEPLOY_KEY: 'fixture-admin', SORREL_HUB_CONVEX: 'false' }]) {
    const mirror = createConvexMirror(env);
    assert.equal(mirror.enabled, false);
    await mirror.upsertProposal({ id: 'never-sent' });
    await mirror.removeProposal('never-sent');
  }
});

test('mirror sends internal mutations only with server-held admin auth', async () => {
  const requests = [];
  const server = http.createServer(async (req, res) => {
    let body = '';
    for await (const chunk of req) body += chunk;
    requests.push({ path: req.url, auth: req.headers.authorization, body: JSON.parse(body) });
    res.setHeader('content-type', 'application/json');
    res.end(JSON.stringify({ status: 'success', value: null }));
  });
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  try {
    const mirror = createConvexMirror({ CONVEX_SELF_HOSTED_URL: `http://127.0.0.1:${server.address().port}`,
      CONVEX_SELF_HOSTED_ADMIN_KEY: 'fixture-admin' });
    assert.equal(mirror.enabled, true);
    await mirror.upsertProposal({ id: 'proposal-a', projectId: 'project-a', title: 'A', status: 'open', updatedAt: '2026-10-06T00:00:00Z' });
    await mirror.removeProposal('proposal-a');
    assert.deepEqual(requests, [
      { path: '/api/mutation', auth: 'Convex fixture-admin', body: { path: 'proposals:upsert',
        args: { hubId: 'proposal-a', status: 'open', projectId: 'project-a', title: 'A', updatedAt: '2026-10-06T00:00:00Z' }, format: 'json' } },
      { path: '/api/mutation', auth: 'Convex fixture-admin', body: { path: 'proposals:remove', args: { hubId: 'proposal-a' }, format: 'json' } },
    ]);
  } finally {
    await new Promise(resolve => server.close(resolve));
  }
});

test('best-effort mirror failures never log privileged backend payloads', async (t) => {
  const warnings = [];
  t.mock.method(console, 'warn', message => warnings.push(message));
  const server = http.createServer((_req, res) => {
    res.statusCode = 403;
    res.end('fixture-admin private-backend-payload');
  });
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  try {
    const mirror = createConvexMirror({ CONVEX_URL: `http://127.0.0.1:${server.address().port}`, CONVEX_DEPLOY_KEY: 'fixture-admin' });
    await mirror.removeProposal('proposal-a');
    assert.deepEqual(warnings, ['[convex-mirror] /api/mutation failed: HTTP 403']);
  } finally {
    await new Promise(resolve => server.close(resolve));
  }
});
