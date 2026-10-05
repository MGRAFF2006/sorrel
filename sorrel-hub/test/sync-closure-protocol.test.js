import assert from 'node:assert/strict';
import test from 'node:test';

import { objectId } from '../src/blake3.js';
import { createRepoSyncStore } from '../src/sync-store.js';
import { missingObjects, refObjectId, walkClosure } from '../src/sync-closure.js';

test('refObjectId accepts string and {kind,id} refs', () => {
  assert.equal(refObjectId('Aa'.repeat(32)), 'aa'.repeat(32));
  assert.equal(refObjectId({ kind: 'Tree', id: 'Bb'.repeat(32) }), 'bb'.repeat(32));
  assert.equal(refObjectId(null), undefined);
});

test('walkClosure follows Core protocol Snapshot/Tree shapes', () => {
  const repoId = 'repo_protocol_shape';
  const store = createRepoSyncStore();

  const blobBytes = Buffer.from('hello protocol', 'utf8');
  const blobId = objectId(blobBytes);
  const treeBytes = Buffer.from(
    JSON.stringify({
      kind: 'Tree',
      entries: [
        {
          name: 'hello.txt',
          object: { kind: 'Blob', id: blobId },
        },
      ],
    }),
  );
  const treeId = objectId(treeBytes);
  const snapshotBytes = Buffer.from(
    JSON.stringify({
      kind: 'Snapshot',
      rootTree: { kind: 'Tree', id: treeId },
      parents: [],
    }),
  );
  const snapshotId = objectId(snapshotBytes);

  store.put(repoId, blobBytes, blobId);
  store.put(repoId, treeBytes, treeId);
  store.put(repoId, snapshotBytes, snapshotId);

  const { closure, incomplete } = walkClosure(repoId, [snapshotId], store);
  assert.equal(incomplete, false);
  assert.deepEqual([...closure].sort(), [blobId, snapshotId, treeId].sort());

  const missing = missingObjects([snapshotId], [], repoId, store);
  assert.deepEqual(missing, [blobId, snapshotId, treeId].sort());
});


test('missingObjects includes absent descendants even when the root was uploaded', () => {
  const store = createRepoSyncStore();
  const childId = 'aa'.repeat(32);
  const treeId = store.put('repo_missing', Buffer.from(JSON.stringify({
    kind: 'Tree', entries: [{ name: 'missing.txt', object: childId }],
  })));
  const snapshotId = store.put('repo_missing', Buffer.from(JSON.stringify({
    kind: 'Snapshot', tree: treeId, parents: [],
  })));
  assert.deepEqual(
    missingObjects([snapshotId], [snapshotId, treeId, childId], 'repo_missing', store),
    [childId],
  );
});

test('walkClosure handles a long snapshot history without overflowing the stack', () => {
  const repoId = 'repo_long';
  const store = createRepoSyncStore();
  const treeId = store.put(repoId, Buffer.from(JSON.stringify({ kind: 'Tree', entries: [] })));
  let tip;
  for (let index = 0; index < 12000; index++) {
    tip = store.put(repoId, Buffer.from(JSON.stringify({
      kind: 'Snapshot', tree: treeId, parents: tip ? [tip] : [], index,
    })));
  }
  const result = walkClosure(repoId, [tip], store, 'snapshot');
  assert.equal(result.incomplete, false);
  assert.equal(result.closure.size, 12001);
});

test('in-memory object reads cannot mutate content-addressed stored bytes', () => {
  const store = createRepoSyncStore();
  const original = Buffer.from('immutable');
  const id = store.put('repo_immutable', original);
  store.get('repo_immutable', id).fill(0);
  assert.deepEqual(store.get('repo_immutable', id), original);
});
