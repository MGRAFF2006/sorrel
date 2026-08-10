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
