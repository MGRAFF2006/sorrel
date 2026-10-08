import assert from 'node:assert/strict';
import http from 'node:http';
import test from 'node:test';
import { Readable } from 'node:stream';
import { createApp } from '../src/app.js';
import { BOOTSTRAP_REF_WRITE_GRANT_ID, createLocalBootstrapGrants } from '../src/bootstrap-grants.js';
import { readJsonBody } from '../src/http.js';
import { DEFAULT_LIMITS, resolveResourceLimits } from '../src/resource-limits.js';
import { createTraversalBudget, isDescendant, missingObjects, walkClosure } from '../src/sync-closure.js';
import { createRepoSyncStore } from '../src/sync-store.js';

const repo = 'repo_limits';
const tooLarge = (error) => error.statusCode === 413 && error.code === 'sync_traversal_too_large';
const budget = (overrides) => createTraversalBudget({ ...DEFAULT_LIMITS, ...overrides });
function fixture() {
  const store = createRepoSyncStore();
  const blob = store.put(repo, Buffer.from('terminal blob'));
  const tree = store.put(repo, Buffer.from(JSON.stringify({ schemaVersion: 'sorrel.protocol.v0', kind: 'Tree',
    entries: [1, 2].map((i) => ({ name: `${i}.txt`, type: 'file', object: { kind: 'Blob', id: blob } })) })));
  const snapshot = store.put(repo, Buffer.from(JSON.stringify({ schemaVersion: 'sorrel.protocol.v0', kind: 'Snapshot', tree, parents: [] })));
  const bytes = [snapshot, tree, blob].reduce((sum, id) => sum + store.get(repo, id).length, 0);
  return { store, blob, tree, snapshot, bytes };
}

test('resource limit configuration requires positive safe integer byte/count limits', () => {
  assert.deepEqual(resolveResourceLimits({}), DEFAULT_LIMITS);
  assert.equal(resolveResourceLimits({ SORREL_HUB_MAX_BODY_BYTES: '123' }).requestBodyBytes, 123);
  for (const name of ['SORREL_HUB_MAX_BODY_BYTES', 'SORREL_HUB_MAX_TRAVERSAL_LINKS', 'SORREL_HUB_MAX_TRAVERSAL_OBJECTS', 'SORREL_HUB_MAX_TRAVERSAL_BYTES']) {
    for (const value of ['', '0', '-1', '1.5', '12x', 'Infinity', '9007199254740992']) {
      assert.throws(() => createApp({ env: { [name]: value } }), /positive safe integer/);
    }
    assert.equal(Object.values(resolveResourceLimits({ [name]: '1' })).includes(1), true);
  }
});

test('JSON body limit counts bytes across chunks before parsing', async () => {
  const raw = Buffer.from('{"text":"é"}');
  assert.deepEqual(await readJsonBody(Readable.from([raw.subarray(0, 5), raw.subarray(5)]), raw.length), { text: 'é' });
  await assert.rejects(readJsonBody(Readable.from([raw.subarray(0, 5), raw.subarray(5)]), raw.length - 1),
    (error) => error.statusCode === 413 && error.code === 'request_body_too_large');
});

test('HTTP declared and chunked oversized bodies return JSON 413 without mutation', async (t) => {
  const app = createApp({ env: { SORREL_HUB_MAX_BODY_BYTES: '16', SORREL_HUB_LOCAL_DEMO: '1' } });
  const server = http.createServer(app.handleRequest);
  await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
  t.after(() => new Promise((resolve) => server.close(resolve)));
  async function post(headers, chunks) {
    return await new Promise((resolve, reject) => {
      const request = http.request({ host: '127.0.0.1', port: server.address().port, path: '/projects', method: 'POST', headers: { ...headers, 'x-sorrel-acting-principal': JSON.stringify({ type: 'user', id: 'local' }) } }, (response) => {
        let body = '';
        response.on('data', (chunk) => { body += chunk; });
        response.on('end', () => resolve({ status: response.statusCode, body: JSON.parse(body) }));
      });
      request.on('error', reject);
      for (const chunk of chunks) request.write(chunk);
      request.end();
    });
  }
  for (const headers of [{ 'content-length': '17' }, { 'transfer-encoding': 'chunked' }]) {
    const result = await post(headers, ['12345678', '123456789']);
    assert.equal(result.status, 413);
    assert.equal(result.body.error.code, 'request_body_too_large');
  }
  assert.equal(app.store.projects.size, 0);
});

test('closure limits enforce exact link/object/byte boundaries and deduplicate terminal reads', () => {
  const { store, snapshot, bytes } = fixture();
  let reads = 0;
  const counted = { has: (...args) => store.has(...args), get: (...args) => { reads++; return store.get(...args); } };
  assert.equal(walkClosure(repo, [snapshot], counted, 'snapshot', budget({ traversalLinks: 4, traversalObjects: 3, traversalBytes: bytes })).closure.size, 3);
  assert.equal(reads, 3);
  for (const overrides of [{ traversalLinks: 3 }, { traversalObjects: 2 }, { traversalBytes: bytes - 1 }]) {
    assert.throws(() => walkClosure(repo, [snapshot], store, 'snapshot', budget(overrides)), tooLarge);
  }
});

test('duplicate and missing roots consume links and unique object budget', () => {
  const { store, snapshot } = fixture();
  assert.throws(() => missingObjects([snapshot, snapshot], [], repo, store, budget({ traversalLinks: 1 })), tooLarge);
  assert.throws(() => missingObjects(['aa'.repeat(32), 'bb'.repeat(32)], [], repo, store, budget({ traversalObjects: 1 })), tooLarge);
});

test('ancestry is bounded and ref validation shares its request budget', () => {
  const { store, snapshot } = fixture();
  const descendant = store.put(repo, Buffer.from(JSON.stringify({ schemaVersion: 'sorrel.protocol.v0', kind: 'Snapshot', tree: 'aa'.repeat(32), parents: [snapshot] })));
  const limits = budget({ traversalLinks: 2 });
  assert.equal(isDescendant(repo, snapshot, descendant, store, limits), true);
  assert.throws(() => isDescendant(repo, snapshot, descendant, store, budget({ traversalLinks: 1 })), tooLarge);
  assert.throws(() => isDescendant(repo, snapshot, descendant, store, budget({ traversalObjects: 1 })), tooLarge);
  assert.throws(() => isDescendant(repo, 'bb'.repeat(32), descendant, store, budget({ traversalLinks: 1 })), tooLarge);
  assert.throws(() => isDescendant(repo, snapshot, descendant, store, budget({ traversalBytes: 1 })), tooLarge);
  assert.throws(() => walkClosure(repo, [snapshot], store, 'snapshot', limits), tooLarge);
});

test('HTTP traversal limit rejects ref publication and can be raised by operator', async (t) => {
  const { store: sync, snapshot } = fixture();
  const grants = createLocalBootstrapGrants();
  const principal = { type: 'user', id: 'local' };
  for (const limit of ['2', '3']) {
    const app = createApp({ env: { SORREL_HUB_MAX_TRAVERSAL_OBJECTS: limit, SORREL_HUB_LOCAL_DEMO: '1' }, trustedGrantsById: grants });
    app.store.sync = sync;
    const server = http.createServer(app.handleRequest);
    await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
    t.after(() => new Promise((resolve) => server.close(resolve)));
    const response = await fetch(`http://127.0.0.1:${server.address().port}/${repo}/refs/main`, { method: 'POST',
      headers: { 'content-type': 'application/json', 'x-sorrel-acting-principal': JSON.stringify(principal) },
      body: JSON.stringify({ snapshot, grantRefs: [{ id: BOOTSTRAP_REF_WRITE_GRANT_ID }] }) });
    assert.equal(response.status, limit === '2' ? 413 : 200, response.status === 403 ? JSON.stringify(await response.json()) : undefined);
    if (limit === '2') {
      assert.equal((await response.json()).error.code, 'sync_traversal_too_large');
      assert.equal(sync.getRef(repo, 'main'), undefined);
    } else assert.equal(sync.getRef(repo, 'main'), snapshot);
  }
});
