import assert from 'node:assert/strict';
import fs from 'node:fs';
import http from 'node:http';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';

import { createApp } from '../src/app.js';
import { createFsRepoSyncStore } from '../src/fs-sync-store.js';
import { createInMemoryStore } from '../src/store.js';
import { createRepoSyncStore, SyncObjectNotFoundError } from '../src/sync-store.js';

const absentId = 'a'.repeat(64);

test('in-memory sync lookups do not register absent repos; writes do', () => {
  const store = createRepoSyncStore();
  assert.equal(store.has('repo_absent_has', absentId), false);
  assert.throws(() => store.get('repo_absent_get', absentId), SyncObjectNotFoundError);
  assert.equal(store.getRef('repo_absent_ref', 'main'), undefined);
  assert.deepEqual(store.listRefs('repo_absent_refs'), []);
  assert.deepEqual(store.listRepos(), []);

  const bytes = Buffer.from('synthetic object');
  const id = store.put('repo_object_write', bytes);
  store.setRef('repo_ref_write', 'main', id);
  assert.deepEqual(store.listRepos().sort(), ['repo_object_write', 'repo_ref_write']);
  assert.deepEqual(store.get('repo_object_write', id), bytes);
  assert.equal(store.getRef('repo_ref_write', 'main'), id);
});

for (const kind of ['memory', 'fs']) {
  test(`HTTP unknown-repo reads keep ${kind} registry and disk unchanged`, async (t) => {
    const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'sorrel-hub-read-side-effects-'));
    t.after(() => fs.rmSync(dir, { recursive: true, force: true }));
    const sync = kind === 'fs' ? createFsRepoSyncStore(dir) : createRepoSyncStore();
    const before = fs.readdirSync(dir, { recursive: true });
    const app = createApp({
      store: createInMemoryStore({ sync }),
      env: { SORREL_HUB_LOCAL_DEMO: '1' },
    });
    const server = http.createServer(app.handleRequest);
    await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
    t.after(() => new Promise((resolve, reject) => server.close(error => error ? reject(error) : resolve())));
    const baseUrl = `http://127.0.0.1:${server.address().port}`;
    const listing = async () => {
      const response = await fetch(`${baseUrl}/admin/sync-repos`);
      assert.equal(response.status, 200);
      assert.deepEqual(await response.json(), { repos: [] });
      assert.deepEqual(sync.listRepos(), []);
      assert.deepEqual(fs.readdirSync(dir, { recursive: true }), before);
    };
    await listing();
    for (const [route, status] of [
      ['repo_unknown_refs/refs', 200],
      [`repo_unknown_object/objects/${absentId}`, 404],
      ['repo_unknown_tree/tree', 404],
      ['repo_unknown_file/files?path=fixture.txt', 404],
    ]) {
      const response = await fetch(`${baseUrl}/${route}`);
      assert.equal(response.status, status);
      await response.json();
      await listing();
    }
    const missing = await fetch(`${baseUrl}/repo_unknown_negotiation/objects/missing`, {
      method: 'POST', headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ want: [absentId], have: [] }),
    });
    assert.equal(missing.status, 200);
    assert.deepEqual(await missing.json(), { missing: [absentId] });
    await listing();
  });
}
