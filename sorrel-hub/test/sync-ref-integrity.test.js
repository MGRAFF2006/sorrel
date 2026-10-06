import assert from 'node:assert/strict';
import fs from 'node:fs';
import http from 'node:http';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';

import { createApp } from '../src/app.js';
import { createLocalBootstrapGrants } from '../src/bootstrap-grants.js';
import { createFsRepoSyncStore, encodePathSegment } from '../src/fs-sync-store.js';
import { createInMemoryStore } from '../src/store.js';

const repoId = 'repo_integrity';
const schemaVersion = 'sorrel.protocol.v0';

async function withServer(t, sync) {
  const app = createApp({
    store: createInMemoryStore(sync ? { sync } : {}),
    trustedGrantsById: createLocalBootstrapGrants(),
    env: { SORREL_HUB_LOCAL_DEMO: '1' },
  });
  const server = http.createServer(app.handleRequest);
  await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
  t.after(() => new Promise((resolve, reject) => {
    server.close((error) => error ? reject(error) : resolve());
  }));
  return { app, baseUrl: `http://127.0.0.1:${server.address().port}` };
}

function putJson(sync, object) {
  return sync.put(repoId, Buffer.from(JSON.stringify(object)));
}

function snapshot(rootTreeId, parents = []) {
  return {
    schemaVersion, kind: 'Snapshot', repo: repoId,
    rootTree: { kind: 'Tree', id: rootTreeId },
    parents: parents.map((id) => ({ kind: 'Snapshot', id })),
    createdAt: '2026-10-06T00:00:00Z', author: { type: 'user', id: 'local' },
  };
}

function fixture(sync, entryType = 'file') {
  const emptyTree = putJson(sync, { schemaVersion, kind: 'Tree', entries: [] });
  const previous = putJson(sync, snapshot(emptyTree));
  sync.setRef(repoId, 'main', previous);
  const blob = sync.put(repoId, Buffer.from('sorrel.blob.v0\nhello'));
  const childTree = {
    schemaVersion, kind: 'Tree', entries: [{
      name: 'hello.txt', path: 'hello.txt', type: entryType,
      object: { kind: 'Blob', id: blob }, mode: entryType === 'symlink' ? 'symlink' : 'normal',
    }],
  };
  const child = putJson(sync, childTree);
  const tree = {
    schemaVersion, kind: 'Tree', entries: [{
      name: 'src', path: 'src', type: 'directory',
      object: { kind: 'Tree', id: child }, mode: 'directory',
    }],
  };
  const root = putJson(sync, tree);
  const next = putJson(sync, snapshot(root, [previous]));
  return { previous, blob, child, childTree, tree, root, next };
}

async function advance(baseUrl, snapshotId, previous) {
  return fetch(`${baseUrl}/${repoId}/refs/main`, {
    method: 'POST', headers: {
      'content-type': 'application/json',
      'x-sorrel-acting-principal': JSON.stringify({ type: 'user', id: 'local' }),
    },
    body: JSON.stringify({
      snapshot: snapshotId, expected: previous,
      grantRefs: [{ id: 'grant_local_ref_write', source: 'core' }],
    }),
  });
}

test('Core-encoded nested snapshot closures publish normally', async (t) => {
  const { app, baseUrl } = await withServer(t);
  const objects = fixture(app.store.sync);
  const response = await advance(baseUrl, objects.next, objects.previous);
  assert.equal(response.status, 200);
  assert.equal(app.store.sync.getRef(repoId, 'main'), objects.next);
  const file = await fetch(`${baseUrl}/${repoId}/files?path=src/hello.txt`);
  assert.equal(file.status, 200);
  assert.equal((await file.json()).content, 'hello');
});

test('ref publication rejects corrupt disk bytes anywhere in a typed closure', async (t) => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'sorrel-hub-integrity-'));
  t.after(() => fs.rmSync(dir, { recursive: true, force: true }));
  const sync = createFsRepoSyncStore(dir);
  const { baseUrl } = await withServer(t, sync);
  for (const entryType of ['file', 'symlink']) {
    const objects = fixture(sync, entryType);
    for (const id of [objects.blob, objects.child, objects.root, objects.next, objects.previous]) {
      const file = path.join(dir, encodePathSegment(repoId), 'objects', id.slice(0, 2), id);
      const original = fs.readFileSync(file);
      fs.writeFileSync(file, 'corrupt');
      const response = await advance(baseUrl, objects.next, objects.previous);
      assert.equal(response.status, 400, `${entryType}: ${id}`);
      assert.equal((await response.json()).error.code, 'object_id_mismatch');
      assert.equal(sync.getRef(repoId, 'main'), objects.previous);
      fs.writeFileSync(file, original);
    }
  }
});

test('invalid versions and kinds cannot replace an existing ref', async (t) => {
  const { app, baseUrl } = await withServer(t);
  const sync = app.store.sync;
  const objects = fixture(sync);
  for (const override of [
    { schemaVersion: undefined }, { schemaVersion: null },
    { schemaVersion: 'sorrel.protocol.v999' }, { schemaVersion: 0 },
    { kind: 'snapshot' }, { kind: 'Tree' },
  ]) {
    const invalid = putJson(sync, { ...snapshot(objects.root, [objects.previous]), ...override });
    const invalidParent = putJson(sync, snapshot(objects.root, [objects.previous, invalid]));
    for (const tip of [invalid, invalidParent]) {
      const response = await advance(baseUrl, tip, objects.previous);
      assert.equal(response.status, 422, JSON.stringify(override));
      assert.equal((await response.json()).error.code, 'invalid_sync_object');
      assert.equal(sync.getRef(repoId, 'main'), objects.previous);
    }
  }
  for (const override of [
    { schemaVersion: undefined }, { schemaVersion: 'sorrel.protocol.v999' },
    { kind: 'tree' }, { kind: 'Snapshot' },
  ]) {
    const child = putJson(sync, { ...objects.childTree, ...override });
    const root = putJson(sync, { ...objects.tree, entries: [{
      ...objects.tree.entries[0], object: { kind: 'Tree', id: child },
    }] });
    const invalid = putJson(sync, snapshot(root, [objects.previous]));
    const response = await advance(baseUrl, invalid, objects.previous);
    assert.equal(response.status, 422, JSON.stringify(override));
    assert.equal((await response.json()).error.code, 'invalid_sync_object');
    assert.equal(sync.getRef(repoId, 'main'), objects.previous);
  }
});

test('browsing rejects unsupported persisted typed versions', async (t) => {
  const { app, baseUrl } = await withServer(t);
  const objects = fixture(app.store.sync);
  const invalidTree = putJson(app.store.sync, {
    ...objects.tree, schemaVersion: 'sorrel.protocol.v999',
  });
  const invalidSnapshots = [
    { ...snapshot(objects.root), schemaVersion: 'sorrel.protocol.v999' },
    snapshot(invalidTree),
  ];
  for (const invalid of invalidSnapshots) {
    app.store.sync.setRef(repoId, 'old-import', putJson(app.store.sync, invalid));
    for (const route of ['tree?ref=old-import', 'files?ref=old-import&path=src/hello.txt']) {
      const response = await fetch(`${baseUrl}/${repoId}/${route}`);
      assert.equal(response.status, 422);
      assert.equal((await response.json()).error.code, 'invalid_sync_object');
    }
  }
});
