import assert from 'node:assert/strict';
import fs from 'node:fs';
import http from 'node:http';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';

import { createApp } from '../src/app.js';
import { objectId } from '../src/blake3.js';
import {
  createFsRepoSyncStore,
  decodePathSegment,
  encodePathSegment,
  FsRepoSyncStore,
} from '../src/fs-sync-store.js';
import { createInMemoryStore } from '../src/store.js';
import {
  SyncObjectIdMismatchError,
  SyncObjectNotFoundError,
} from '../src/sync-store.js';

const repoId = 'repo_fs_test';

function tempDir(t) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'sorrel-hub-fs-'));
  t.after(() => fs.rmSync(dir, { recursive: true, force: true }));
  return dir;
}

function makeBytes(content) {
  const bytes = Buffer.from(content, 'utf8');
  return { id: objectId(bytes), bytes };
}

test('encodePathSegment/decodePathSegment round-trip arbitrary strings', () => {
  const samples = [
    '',
    'simple',
    'repo_with-dashes',
    'org/team',
    'repo with spaces',
    'path/../escape',
    'percent%2fone',
    'symbols:?&=+#@!',
    'unicode-café-日本',
    'mixed/Name With Spaces & %chars%',
    '\0null\nnewline\ttab',
  ];

  for (const sample of samples) {
    assert.equal(decodePathSegment(encodePathSegment(sample)), sample);
  }

  // Encoded form must escape separators and traversal sequences.
  assert.equal(encodePathSegment('a/b'), 'a%2fb');
  assert.equal(encodePathSegment('../etc'), '%2e%2e%2fetc');
  assert.equal(encodePathSegment(''), '%');
  assert.equal(decodePathSegment('%'), '');
});

test('fs store: listRepos decodes percent-encoded directory names', (t) => {
  const store = createFsRepoSyncStore(tempDir(t));
  const plain = 'repo_plain';
  const special = 'org/team:repo with spaces?';
  const snapshot = objectId(Buffer.from('listed'));

  assert.deepEqual(store.listRepos(), []);

  store.setRef(plain, 'main', snapshot);
  store.setRef(special, 'main', snapshot);

  assert.deepEqual(store.listRepos().sort(), [special, plain].sort());
});

test('fs store: put/get/has round-trip', (t) => {
  const store = createFsRepoSyncStore(tempDir(t));
  const blob = makeBytes('hello disk');

  assert.equal(store.has(repoId, blob.id), false);
  assert.equal(store.put(repoId, blob.bytes, blob.id), blob.id);
  assert.equal(store.has(repoId, blob.id), true);
  assert.deepEqual(store.get(repoId, blob.id), blob.bytes);
});

test('fs store: rejects an expected-id mismatch and never stores the bytes', (t) => {
  const store = createFsRepoSyncStore(tempDir(t));
  const blob = makeBytes('honest bytes');
  const wrongId = objectId(Buffer.from('other bytes'));

  assert.throws(() => store.put(repoId, blob.bytes, wrongId), SyncObjectIdMismatchError);
  assert.equal(store.has(repoId, blob.id), false);
  assert.equal(store.has(repoId, wrongId), false);
});

test('fs store: missing object throws not-found', (t) => {
  const store = createFsRepoSyncStore(tempDir(t));
  const absent = objectId(Buffer.from('never stored'));

  assert.throws(() => store.get(repoId, absent), SyncObjectNotFoundError);
});

test('fs store: corrupted bytes on disk surface as id mismatch on read', (t) => {
  const dir = tempDir(t);
  const store = createFsRepoSyncStore(dir);
  const blob = makeBytes('will be corrupted');
  store.put(repoId, blob.bytes, blob.id);

  const objectPath = path.join(
    dir,
    encodePathSegment(repoId),
    'objects',
    blob.id.slice(0, 2),
    blob.id,
  );
  fs.writeFileSync(objectPath, 'tampered');

  assert.throws(() => store.get(repoId, blob.id), SyncObjectIdMismatchError);
});

test('fs store: refs set/get/list and overwrite', (t) => {
  const store = createFsRepoSyncStore(tempDir(t));
  const first = objectId(Buffer.from('snapshot one'));
  const second = objectId(Buffer.from('snapshot two'));

  assert.equal(store.getRef(repoId, 'main'), undefined);
  assert.deepEqual(store.listRefs(repoId), []);

  store.setRef(repoId, 'main', first);
  store.setRef(repoId, 'lane/feature', second);
  assert.equal(store.getRef(repoId, 'main'), first);
  assert.equal(store.getRef(repoId, 'lane/feature'), second);

  store.setRef(repoId, 'main', second);
  assert.equal(store.getRef(repoId, 'main'), second);

  const names = store.listRefs(repoId).map((ref) => ref.name).sort();
  assert.deepEqual(names, ['lane/feature', 'main']);
});

test('fs store: objects and refs persist across store instances', (t) => {
  const dir = tempDir(t);
  const blob = makeBytes('survives restarts');
  const snapshot = objectId(Buffer.from('snapshot'));

  const first = createFsRepoSyncStore(dir);
  first.put(repoId, blob.bytes, blob.id);
  first.setRef(repoId, 'main', snapshot);

  const second = createFsRepoSyncStore(dir);
  assert.equal(second.has(repoId, blob.id), true);
  assert.deepEqual(second.get(repoId, blob.id), blob.bytes);
  assert.equal(second.getRef(repoId, 'main'), snapshot);
  assert.deepEqual(second.listRefs(repoId), [{ name: 'main', snapshot }]);
});

test('fs store: hostile repo ids and ref names stay inside the data root', (t) => {
  const dir = tempDir(t);
  const store = new FsRepoSyncStore(dir);
  const hostileRepo = '../../etc';
  const hostileRef = '..%2f..%2fpasswd';
  const snapshot = objectId(Buffer.from('trapped'));

  store.setRef(hostileRepo, hostileRef, snapshot);
  assert.equal(store.getRef(hostileRepo, hostileRef), snapshot);

  const entries = fs.readdirSync(dir);
  assert.equal(entries.length, 1);
  assert.ok(!entries[0].includes('..'));
  assert.ok(!fs.existsSync(path.join(path.dirname(dir), 'etc')));
});

test('fs store: distinct ids that sanitize similarly never collide', (t) => {
  const store = createFsRepoSyncStore(tempDir(t));
  const snapshotA = objectId(Buffer.from('a'));
  const snapshotB = objectId(Buffer.from('b'));

  store.setRef('repo/one', 'main', snapshotA);
  store.setRef('repo%2fone', 'main', snapshotB);

  assert.equal(store.getRef('repo/one', 'main'), snapshotA);
  assert.equal(store.getRef('repo%2fone', 'main'), snapshotB);
});

// End-to-end: a full push flow against a Hub app using the fs sync store,
// then a fresh app instance over the same directory (simulated restart) still
// serves the pushed refs and objects.

const refWriteGrant = {
  id: 'grant_repo_ref_write',
  source: 'core',
  principal: { type: 'user', id: 'user_pusher' },
  action: 'repo.ref.write',
  resource: { kind: 'repo', id: repoId },
};

const objectWriteGrant = {
  id: 'grant_repo_object_write',
  source: 'core',
  principal: { type: 'user', id: 'user_pusher' },
  action: 'repo.object.write',
  resource: { kind: 'repo', id: repoId },
};

const trustedGrantsById = {
  [objectWriteGrant.id]: objectWriteGrant,
  [refWriteGrant.id]: refWriteGrant,
};

const principalHeader = {
  'x-sorrel-acting-principal': JSON.stringify(objectWriteGrant.principal),
};

function makeApp(dataDir) {
  return createApp({
    store: createInMemoryStore({ sync: createFsRepoSyncStore(dataDir) }),
    trustedGrantsById,
  });
}

async function withServer(app, callback) {
  const server = http.createServer(app.handleRequest);
  await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
  const address = server.address();
  try {
    return await callback(`http://${address.address}:${address.port}`);
  } finally {
    await new Promise((resolve, reject) => {
      server.close((error) => (error ? reject(error) : resolve()));
    });
  }
}

async function postJson(url, payload, headers = {}) {
  return await fetch(url, {
    method: 'POST',
    headers: { 'content-type': 'application/json', ...headers },
    body: JSON.stringify(payload),
  });
}

test('sync transport over fs store survives a server restart', async (t) => {
  const dataDir = tempDir(t);

  const blob = makeBytes('persistent blob');
  const tree = makeBytes(
    JSON.stringify({ kind: 'Tree', entries: [{ name: 'a.txt', object: blob.id }] }),
  );
  const snapshot = makeBytes(JSON.stringify({ kind: 'Snapshot', tree: tree.id, parents: [] }));

  await withServer(makeApp(dataDir), async (baseUrl) => {
    const upload = await postJson(
      `${baseUrl}/${repoId}/objects`,
      {
        objects: [blob, tree, snapshot].map(({ id, bytes }) => ({
          id,
          data: bytes.toString('base64'),
        })),
        grantRefs: [{ id: objectWriteGrant.id, source: 'core' }],
      },
      principalHeader,
    );
    assert.equal(upload.status, 200);

    const advance = await postJson(
      `${baseUrl}/${repoId}/refs/main`,
      {
        snapshot: snapshot.id,
        expected: null,
        grantRefs: [{ id: refWriteGrant.id, source: 'core' }],
      },
      principalHeader,
    );
    assert.equal(advance.status, 200);
  });

  // "Restart": brand-new app + store over the same data directory.
  await withServer(makeApp(dataDir), async (baseUrl) => {
    const refsResponse = await fetch(`${baseUrl}/${repoId}/refs`);
    const refsBody = await refsResponse.json();
    assert.equal(refsResponse.status, 200);
    assert.deepEqual(refsBody.refs, [{ name: 'main', snapshot: snapshot.id }]);

    const objectResponse = await fetch(`${baseUrl}/${repoId}/objects/${blob.id}`);
    assert.equal(objectResponse.status, 200);
    const objectBody = await objectResponse.json();
    assert.equal(objectBody.id, blob.id);
    assert.deepEqual(Buffer.from(objectBody.bytes, 'base64'), blob.bytes);

    // Pull semantics: with an empty `have`, the server offers the whole
    // stored closure for download — proving all three objects survived.
    const missingResponse = await postJson(`${baseUrl}/${repoId}/objects/missing`, {
      want: [snapshot.id],
      have: [],
    });
    const missingBody = await missingResponse.json();
    assert.equal(missingResponse.status, 200);
    assert.deepEqual(missingBody.missing, [blob.id, tree.id, snapshot.id].sort());
  });
});
