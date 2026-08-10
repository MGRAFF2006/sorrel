import assert from 'node:assert/strict';
import http from 'node:http';
import test from 'node:test';

import { createApp } from '../src/app.js';
import { objectId } from '../src/blake3.js';

const repoId = 'repo_sync_test';

const objectWriteGrant = {
  id: 'grant_repo_object_write',
  source: 'core',
  principal: { type: 'user', id: 'user_pusher' },
  action: 'repo.object.write',
  resource: { kind: 'repo', id: repoId },
};

const refWriteGrant = {
  id: 'grant_repo_ref_write',
  source: 'core',
  principal: { type: 'user', id: 'user_pusher' },
  action: 'repo.ref.write',
  resource: { kind: 'repo', id: repoId },
};

const trustedGrantsById = {
  [objectWriteGrant.id]: objectWriteGrant,
  [refWriteGrant.id]: refWriteGrant,
};

const principalHeader = {
  'x-sorrel-acting-principal': JSON.stringify(objectWriteGrant.principal),
};

async function withSyncServer(callback) {
  const app = createApp({ trustedGrantsById });
  const server = http.createServer(app.handleRequest);

  await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));

  const address = server.address();
  const baseUrl = `http://${address.address}:${address.port}`;

  try {
    return await callback(baseUrl, app);
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

function encodeObject(bytes) {
  return {
    id: objectId(bytes),
    bytes: Buffer.from(bytes).toString('base64'),
  };
}

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

function grantBody(extra = {}) {
  return {
    grantRefs: [
      { id: objectWriteGrant.id, source: 'core' },
      { id: refWriteGrant.id, source: 'core' },
    ],
    ...extra,
  };
}

test('push flow: missing -> upload -> advance ref', async () => {
  await withSyncServer(async (baseUrl) => {
    const blob = makeBlob('hello sorrel');
    const tree = makeTree([{ name: 'hello.txt', object: blob.id }]);
    const snapshot = makeSnapshot(tree.id);
    const allIds = [snapshot.id, tree.id, blob.id];

    const missingResponse = await postJson(`${baseUrl}/${repoId}/objects/missing`, {
      want: allIds,
      have: allIds,
    });
    const missingBody = await missingResponse.json();

    assert.equal(missingResponse.status, 200);
    assert.deepEqual(missingBody.missing, allIds.sort());

    const uploadResponse = await postJson(
      `${baseUrl}/${repoId}/objects`,
      {
        objects: [encodeObject(blob.bytes), encodeObject(tree.bytes), encodeObject(snapshot.bytes)],
        grantRefs: [{ id: objectWriteGrant.id, source: 'core' }],
      },
      principalHeader,
    );
    const uploadBody = await uploadResponse.json();

    assert.equal(uploadResponse.status, 200);
    assert.deepEqual(uploadBody.stored.sort(), allIds.sort());

    const advanceResponse = await postJson(
      `${baseUrl}/${repoId}/refs/main`,
      {
        snapshot: snapshot.id,
        expected: null,
        ...grantBody({ grantRefs: [{ id: refWriteGrant.id, source: 'core' }] }),
      },
      principalHeader,
    );
    const advanceBody = await advanceResponse.json();

    assert.equal(advanceResponse.status, 200);
    assert.deepEqual(advanceBody, { name: 'main', snapshot: snapshot.id, previous: null });

    const refsResponse = await fetch(`${baseUrl}/${repoId}/refs`);
    const refsBody = await refsResponse.json();

    assert.equal(refsResponse.status, 200);
    assert.equal(refsBody.repoId, repoId);
    assert.deepEqual(refsBody.refs, [{ name: 'main', snapshot: snapshot.id }]);
  });
});

test('pull flow: get refs -> missing -> download objects', async () => {
  await withSyncServer(async (baseUrl, app) => {
    const blob = makeBlob('pull me');
    const tree = makeTree([{ name: 'pull.txt', object: blob.id }]);
    const snapshot = makeSnapshot(tree.id);

    app.store.sync.put(repoId, blob.bytes);
    app.store.sync.put(repoId, tree.bytes);
    app.store.sync.put(repoId, snapshot.bytes);
    app.store.sync.setRef(repoId, 'main', snapshot.id);

    const refsResponse = await fetch(`${baseUrl}/${repoId}/refs`);
    const refsBody = await refsResponse.json();

    assert.equal(refsResponse.status, 200);
    assert.equal(refsBody.refs[0].snapshot, snapshot.id);

    const missingResponse = await postJson(`${baseUrl}/${repoId}/objects/missing`, {
      want: [snapshot.id],
      have: [],
    });
    const missingBody = await missingResponse.json();

    assert.equal(missingResponse.status, 200);
    assert.deepEqual(missingBody.missing.sort(), [blob.id, snapshot.id, tree.id].sort());

    for (const id of missingBody.missing) {
      const objectResponse = await fetch(`${baseUrl}/${repoId}/objects/${id}`);
      assert.equal(objectResponse.status, 200);
      const objectBody = await objectResponse.json();
      assert.equal(objectBody.id, id);
      const bytes = Buffer.from(objectBody.bytes, 'base64');
      assert.equal(objectId(bytes), id);
    }
  });
});

test('POST /objects rejects object_id_mismatch', async () => {
  await withSyncServer(async (baseUrl) => {
    const blob = makeBlob('mismatch');

    const response = await postJson(
      `${baseUrl}/${repoId}/objects`,
      {
        objects: [
          {
            id: '0'.repeat(64),
            bytes: blob.bytes.toString('base64'),
          },
        ],
        grantRefs: [{ id: objectWriteGrant.id, source: 'core' }],
      },
      principalHeader,
    );
    const body = await response.json();

    assert.equal(response.status, 400);
    assert.equal(body.error.code, 'object_id_mismatch');
  });
});

test('POST /refs rejects closure_incomplete when snapshot graph is missing', async () => {
  await withSyncServer(async (baseUrl) => {
    const tree = makeTree([]);
    const snapshot = makeSnapshot(tree.id);

    const response = await postJson(
      `${baseUrl}/${repoId}/refs/main`,
      {
        snapshot: snapshot.id,
        ...grantBody({ grantRefs: [{ id: refWriteGrant.id, source: 'core' }] }),
      },
      principalHeader,
    );
    const body = await response.json();

    assert.equal(response.status, 409);
    assert.equal(body.error.code, 'closure_incomplete');
  });
});

test('POST /refs rejects non_fast_forward without force', async () => {
  await withSyncServer(async (baseUrl, app) => {
    const blobA = makeBlob('base');
    const treeA = makeTree([{ name: 'a.txt', object: blobA.id }]);
    const snapA = makeSnapshot(treeA.id);
    const blobB = makeBlob('branch');
    const treeB = makeTree([{ name: 'b.txt', object: blobB.id }]);
    const snapB = makeSnapshot(treeB.id, []);

    for (const object of [blobA, treeA, snapA, blobB, treeB, snapB]) {
      app.store.sync.put(repoId, object.bytes);
    }
    app.store.sync.setRef(repoId, 'main', snapA.id);

    const response = await postJson(
      `${baseUrl}/${repoId}/refs/main`,
      {
        snapshot: snapB.id,
        expected: snapA.id,
        ...grantBody({ grantRefs: [{ id: refWriteGrant.id, source: 'core' }] }),
      },
      principalHeader,
    );
    const body = await response.json();

    assert.equal(response.status, 409);
    assert.equal(body.error.code, 'non_fast_forward');
  });
});

test('POST /refs denies without acting principal header', async () => {
  await withSyncServer(async (baseUrl, app) => {
    const blob = makeBlob('deny');
    const tree = makeTree([{ name: 'deny.txt', object: blob.id }]);
    const snapshot = makeSnapshot(tree.id);

    for (const object of [blob, tree, snapshot]) {
      app.store.sync.put(repoId, object.bytes);
    }

    const response = await postJson(`${baseUrl}/${repoId}/refs/main`, {
      snapshot: snapshot.id,
      ...grantBody({ grantRefs: [{ id: refWriteGrant.id, source: 'core' }] }),
    });
    const body = await response.json();

    assert.equal(response.status, 403);
    assert.equal(body.error.code, 'policy_denied');
  });
});

test('GET /objects returns object_not_found for unknown id', async () => {
  await withSyncServer(async (baseUrl) => {
    const missingId = 'a'.repeat(64);
    const response = await fetch(`${baseUrl}/${repoId}/objects/${missingId}`);
    const body = await response.json();

    assert.equal(response.status, 404);
    assert.equal(body.error.code, 'object_not_found');
  });
});

test('slash ref names work encoded and literal', async () => {
  await withSyncServer(async (baseUrl, app) => {
    const blob = makeBlob('lane ref');
    const tree = makeTree([{ name: 'l.txt', object: blob.id }]);
    const snapshot = makeSnapshot(tree.id);
    for (const object of [blob, tree, snapshot]) {
      app.store.sync.put(repoId, object.bytes);
    }

    const encoded = await postJson(
      `${baseUrl}/${repoId}/refs/lane%2Fmain`,
      {
        snapshot: snapshot.id,
        expected: null,
        grantRefs: [{ id: refWriteGrant.id, source: 'core' }],
      },
      principalHeader,
    );
    const encodedBody = await encoded.json();
    assert.equal(encoded.status, 200);
    assert.deepEqual(encodedBody, { name: 'lane/main', snapshot: snapshot.id, previous: null });

    const literal = await postJson(
      `${baseUrl}/${repoId}/refs/lane/feature`,
      {
        snapshot: snapshot.id,
        expected: null,
        grantRefs: [{ id: refWriteGrant.id, source: 'core' }],
      },
      principalHeader,
    );
    const literalBody = await literal.json();
    assert.equal(literal.status, 200);
    assert.equal(literalBody.name, 'lane/feature');

    const refsBody = await (await fetch(`${baseUrl}/${repoId}/refs`)).json();
    assert.deepEqual(
      refsBody.refs.map((ref) => ref.name).sort(),
      ['lane/feature', 'lane/main'],
    );
  });
});

test('re-uploading an object reports it as skipped', async () => {
  await withSyncServer(async (baseUrl) => {
    const blob = makeBlob('idempotent');

    const first = await postJson(
      `${baseUrl}/${repoId}/objects`,
      { objects: [encodeObject(blob.bytes)], grantRefs: [{ id: objectWriteGrant.id, source: 'core' }] },
      principalHeader,
    );
    const firstBody = await first.json();
    assert.deepEqual(firstBody, { stored: [blob.id], skipped: [] });

    const second = await postJson(
      `${baseUrl}/${repoId}/objects`,
      { objects: [encodeObject(blob.bytes)], grantRefs: [{ id: objectWriteGrant.id, source: 'core' }] },
      principalHeader,
    );
    const secondBody = await second.json();
    assert.deepEqual(secondBody, { stored: [], skipped: [blob.id] });
  });
});

test('mutating requests without grantRefs are invalid_request', async () => {
  await withSyncServer(async (baseUrl) => {
    const blob = makeBlob('no grants');

    const upload = await postJson(
      `${baseUrl}/${repoId}/objects`,
      { objects: [encodeObject(blob.bytes)] },
      principalHeader,
    );
    const uploadBody = await upload.json();
    assert.equal(upload.status, 400);
    assert.equal(uploadBody.error.code, 'invalid_request');

    const advance = await postJson(
      `${baseUrl}/${repoId}/refs/main`,
      { snapshot: 'b'.repeat(64) },
      principalHeader,
    );
    const advanceBody = await advance.json();
    assert.equal(advance.status, 400);
    assert.equal(advanceBody.error.code, 'invalid_request');
  });
});

test('expected mismatch is invalid_request with current; missing ref is unknown_ref', async () => {
  await withSyncServer(async (baseUrl, app) => {
    const blob = makeBlob('expected');
    const tree = makeTree([{ name: 'e.txt', object: blob.id }]);
    const snapA = makeSnapshot(tree.id);
    const snapB = makeSnapshot(tree.id, [snapA.id]);
    for (const object of [blob, tree, snapA, snapB]) {
      app.store.sync.put(repoId, object.bytes);
    }

    const unknownRef = await postJson(
      `${baseUrl}/${repoId}/refs/main`,
      {
        snapshot: snapB.id,
        expected: snapA.id,
        grantRefs: [{ id: refWriteGrant.id, source: 'core' }],
      },
      principalHeader,
    );
    const unknownRefBody = await unknownRef.json();
    assert.equal(unknownRef.status, 404);
    assert.equal(unknownRefBody.error.code, 'unknown_ref');

    app.store.sync.setRef(repoId, 'main', snapB.id);

    const mismatch = await postJson(
      `${baseUrl}/${repoId}/refs/main`,
      {
        snapshot: snapB.id,
        expected: snapA.id,
        grantRefs: [{ id: refWriteGrant.id, source: 'core' }],
      },
      principalHeader,
    );
    const mismatchBody = await mismatch.json();
    assert.equal(mismatch.status, 400);
    assert.equal(mismatchBody.error.code, 'invalid_request');
    assert.equal(mismatchBody.error.current, snapB.id);
  });
});

test('closure_incomplete lists missing ids; non_fast_forward carries current', async () => {
  await withSyncServer(async (baseUrl, app) => {
    const blob = makeBlob('details');
    const tree = makeTree([{ name: 'd.txt', object: blob.id }]);
    const snapshot = makeSnapshot(tree.id);
    // Store the snapshot and tree but not the blob → incomplete closure.
    app.store.sync.put(repoId, tree.bytes);
    app.store.sync.put(repoId, snapshot.bytes);

    const incomplete = await postJson(
      `${baseUrl}/${repoId}/refs/main`,
      {
        snapshot: snapshot.id,
        grantRefs: [{ id: refWriteGrant.id, source: 'core' }],
      },
      principalHeader,
    );
    const incompleteBody = await incomplete.json();
    assert.equal(incomplete.status, 409);
    assert.equal(incompleteBody.error.code, 'closure_incomplete');
    assert.deepEqual(incompleteBody.error.missing, [blob.id]);

    app.store.sync.put(repoId, blob.bytes);
    app.store.sync.setRef(repoId, 'main', snapshot.id);

    // A divergent snapshot (no parent link to `snapshot`) over the same tree.
    const otherBytes = Buffer.from(JSON.stringify({ kind: 'Snapshot', tree: tree.id, parents: [], marker: 1 }));
    const otherSnapshot = { id: objectId(otherBytes), bytes: otherBytes };
    app.store.sync.put(repoId, otherSnapshot.bytes);
    assert.notEqual(otherSnapshot.id, snapshot.id);

    const diverged = await postJson(
      `${baseUrl}/${repoId}/refs/main`,
      {
        snapshot: otherSnapshot.id,
        grantRefs: [{ id: refWriteGrant.id, source: 'core' }],
      },
      principalHeader,
    );
    const divergedBody = await diverged.json();
    assert.equal(diverged.status, 409);
    assert.equal(divergedBody.error.code, 'non_fast_forward');
    assert.equal(divergedBody.error.current, snapshot.id);
  });
});

test('uppercase object ids and malformed repo ids are rejected', async () => {
  await withSyncServer(async (baseUrl) => {
    const upper = await postJson(`${baseUrl}/${repoId}/objects/missing`, {
      want: ['A'.repeat(64)],
    });
    const upperBody = await upper.json();
    assert.equal(upper.status, 400);
    assert.equal(upperBody.error.code, 'invalid_request');

    const emptyWant = await postJson(`${baseUrl}/${repoId}/objects/missing`, { want: [] });
    const emptyWantBody = await emptyWant.json();
    assert.equal(emptyWant.status, 400);
    assert.equal(emptyWantBody.error.code, 'invalid_request');

    const badRepo = await fetch(`${baseUrl}/not-a-repo-id/refs`);
    const badRepoBody = await badRepo.json();
    assert.equal(badRepo.status, 400);
    assert.equal(badRepoBody.error.code, 'invalid_request');
  });
});
