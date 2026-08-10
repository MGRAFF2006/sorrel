import assert from 'node:assert/strict';
import fs from 'node:fs';
import http from 'node:http';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';

import { createApp } from '../src/app.js';
import { objectId } from '../src/blake3.js';
import { createFsRepoSyncStore } from '../src/fs-sync-store.js';
import { createInMemoryStore } from '../src/store.js';

function makeBlob(content) {
  const bytes = Buffer.from(content, 'utf8');
  return { id: objectId(bytes), bytes };
}

function makeTree(entries) {
  const bytes = Buffer.from(JSON.stringify({ kind: 'Tree', entries }));
  return { id: objectId(bytes), bytes };
}

function makeSnapshot(treeId, parents = []) {
  const bytes = Buffer.from(JSON.stringify({ kind: 'Snapshot', tree: treeId, parents }));
  return { id: objectId(bytes), bytes };
}

function grantsFor(repoId) {
  const objectWriteGrant = {
    id: `grant_object_write_${repoId}`,
    source: 'core',
    principal: { type: 'user', id: 'user_pusher' },
    action: 'repo.object.write',
    resource: { kind: 'repo', id: repoId },
  };
  const refWriteGrant = {
    id: `grant_ref_write_${repoId}`,
    source: 'core',
    principal: { type: 'user', id: 'user_pusher' },
    action: 'repo.ref.write',
    resource: { kind: 'repo', id: repoId },
  };
  return { objectWriteGrant, refWriteGrant };
}

async function withServer(app, callback) {
  const server = http.createServer(app.handleRequest);
  await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
  const address = server.address();
  try {
    return await callback(`http://${address.address}:${address.port}`, app);
  } finally {
    await new Promise((resolve, reject) => {
      server.close((error) => (error ? reject(error) : resolve()));
    });
  }
}

async function postJson(url, payload, headers = {}) {
  return await fetch(url, {
    method: 'POST',
    headers: {
      'content-type': 'application/json',
      ...headers,
    },
    body: JSON.stringify(payload),
  });
}

async function pushRepo(baseUrl, repoId, { objectWriteGrant, refWriteGrant }, refNames) {
  const principalHeader = {
    'x-sorrel-acting-principal': JSON.stringify(objectWriteGrant.principal),
  };

  const objects = [];
  const snapshots = [];
  for (const [index, name] of refNames.entries()) {
    const blob = makeBlob(`${repoId}:${name}:${index}`);
    const tree = makeTree([{ name: `${name}.txt`, object: blob.id }]);
    const snapshot = makeSnapshot(tree.id);
    objects.push(blob, tree, snapshot);
    snapshots.push({ name, id: snapshot.id });
  }

  const upload = await postJson(
    `${baseUrl}/${repoId}/objects`,
    {
      objects: objects.map(({ id, bytes }) => ({
        id,
        data: bytes.toString('base64'),
      })),
      grantRefs: [{ id: objectWriteGrant.id, source: 'core' }],
    },
    principalHeader,
  );
  assert.equal(upload.status, 200, await upload.text());

  for (const { name, id } of snapshots) {
    const advance = await postJson(
      `${baseUrl}/${repoId}/refs/${name}`,
      {
        snapshot: id,
        expected: null,
        grantRefs: [{ id: refWriteGrant.id, source: 'core' }],
      },
      principalHeader,
    );
    assert.equal(advance.status, 200, await advance.text());
  }
}

test('GET /admin/sync-repos returns an empty list for an empty store', async () => {
  const app = createApp();
  await withServer(app, async (baseUrl) => {
    const response = await fetch(`${baseUrl}/admin/sync-repos`);
    const body = await response.json();

    assert.equal(response.status, 200);
    assert.deepEqual(body, { repos: [] });
  });
});

test('GET /admin/sync-repos lists pushed repos with refCounts sorted by id', async () => {
  const repoA = 'repo_bravo';
  const repoB = 'repo_alpha';
  const grantsA = grantsFor(repoA);
  const grantsB = grantsFor(repoB);
  const trustedGrantsById = {
    [grantsA.objectWriteGrant.id]: grantsA.objectWriteGrant,
    [grantsA.refWriteGrant.id]: grantsA.refWriteGrant,
    [grantsB.objectWriteGrant.id]: grantsB.objectWriteGrant,
    [grantsB.refWriteGrant.id]: grantsB.refWriteGrant,
  };

  const app = createApp({ trustedGrantsById });
  await withServer(app, async (baseUrl) => {
    await pushRepo(baseUrl, repoA, grantsA, ['main', 'develop']);
    await pushRepo(baseUrl, repoB, grantsB, ['main']);

    const response = await fetch(`${baseUrl}/admin/sync-repos`);
    const body = await response.json();

    assert.equal(response.status, 200);
    assert.deepEqual(body, {
      repos: [
        { id: repoB, refCount: 1 },
        { id: repoA, refCount: 2 },
      ],
    });
  });
});

test('GET /admin/sync-repos round-trips a special-character repo id via fs store', async (t) => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'sorrel-hub-sync-repos-'));
  t.after(() => fs.rmSync(dir, { recursive: true, force: true }));

  const repoId = 'org/team:repo with spaces & symbols?';
  const sync = createFsRepoSyncStore(dir);
  const blob = makeBlob('special-char repo');
  const tree = makeTree([{ name: 'a.txt', object: blob.id }]);
  const snapshot = makeSnapshot(tree.id);

  sync.put(repoId, blob.bytes, blob.id);
  sync.put(repoId, tree.bytes, tree.id);
  sync.put(repoId, snapshot.bytes, snapshot.id);
  sync.setRef(repoId, 'main', snapshot.id);
  sync.setRef(repoId, 'lane/feature', snapshot.id);

  const app = createApp({
    store: createInMemoryStore({ sync }),
  });

  await withServer(app, async (baseUrl) => {
    const response = await fetch(`${baseUrl}/admin/sync-repos`);
    const body = await response.json();

    assert.equal(response.status, 200);
    assert.deepEqual(body, {
      repos: [{ id: repoId, refCount: 2 }],
    });
  });
});

test('GET /admin/sync-repos rejects non-GET methods', async () => {
  const app = createApp();
  await withServer(app, async (baseUrl) => {
    const response = await postJson(`${baseUrl}/admin/sync-repos`, {});
    assert.equal(response.status, 405);
  });
});
