import assert from 'node:assert/strict';
import test from 'node:test';

import { createRepoSyncStore } from '../src/sync-store.js';
import { missingObjects, walkClosure } from '../src/sync-closure.js';

const repoId = 'repo_work_bounds';
const schemaVersion = 'sorrel.protocol.v0';

function history(count) {
  const store = createRepoSyncStore();
  const tree = store.put(repoId, Buffer.from(JSON.stringify({ schemaVersion, kind: 'Tree', entries: [] })));
  const snapshots = [];
  for (let index = 0; index < count; index++) {
    snapshots.push(store.put(repoId, Buffer.from(JSON.stringify({
      schemaVersion, kind: 'Snapshot', rootTree: { kind: 'Tree', id: tree },
      parents: snapshots.length ? [{ kind: 'Snapshot', id: snapshots.at(-1) }] : [], index,
    }))));
  }
  let reads = 0;
  const get = store.get.bind(store);
  store.get = (...args) => { reads++; return get(...args); };
  return { store, tree, snapshots, reads: () => reads };
}

test('duplicate negotiation roots read each nonterminal object once', () => {
  const fixture = history(300);
  const result = missingObjects(Array(200).fill(fixture.snapshots.at(-1)), [], repoId, fixture.store);
  assert.deepEqual(result, [fixture.tree, ...fixture.snapshots].sort());
  assert.equal(fixture.reads(), 301);
});

test('overlapping roots share traversal while absent descendants remain missing', () => {
  const fixture = history(100);
  const absent = 'a'.repeat(64);
  const result = missingObjects([...fixture.snapshots, absent, absent], [fixture.tree], repoId, fixture.store);
  assert.deepEqual(result, [...fixture.snapshots, absent].sort());
  assert.equal(fixture.reads(), 101);
});

test('cached objects still reject conflicting expected kinds', () => {
  const fixture = history(1);
  const invalidTree = fixture.store.put(repoId, Buffer.from(JSON.stringify({
    schemaVersion, kind: 'Tree', entries: [{
      name: 'invalid', type: 'directory', object: { kind: 'Tree', id: fixture.snapshots[0] },
    }],
  })));
  assert.throws(() => walkClosure(repoId, [invalidTree, fixture.snapshots[0]], fixture.store), {
    statusCode: 422, code: 'invalid_sync_object',
  });
});
