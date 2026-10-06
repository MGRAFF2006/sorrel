import assert from 'node:assert/strict';
import http from 'node:http';
import { test } from 'node:test';
import { createConvexMirror } from '../src/convex-mirror.js';

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
