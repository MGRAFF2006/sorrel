import assert from 'node:assert/strict';
import fs from 'node:fs';
import http from 'node:http';
import os from 'node:os';
import path from 'node:path';
import { test } from 'node:test';

import { createApp } from '../src/app.js';
import { createFsMetadataStore } from '../src/fs-metadata-store.js';
import { atomicWrite, createFsRepoSyncStore, encodePathSegment, FilesystemNameTooLongError } from '../src/fs-sync-store.js';
import { mapSyncStoreError } from '../src/routes/sync.js';
import { createInMemoryStore } from '../src/store.js';

function tempDir(t) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'sorrel-name-limit-'));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  return root;
}

async function withServer(store, run) {
  const principal = { type: 'user', id: 'local' };
  const legacy = { id: 'grant_ref', source: 'core', principal, action: 'repo.ref.write',
    resource: { kind: 'repo', id: 'repo_fixture' }, effect: 'allow' };
  const grants = Object.fromEntries(['org', 'project', 'repo'].map(kind => [`grant_${kind}`, {
    id: `grant_${kind}`, kind: 'Grant', schemaVersion: 'sorrel.protocol.v0', principal: { kind: 'user', id: 'local' },
    capabilities: ['org.read', 'project.create', 'project.read', 'repo.read', 'repo.ref.write'],
    resource: { kind, id: '*' }, effect: 'allow',
  }]));
  const app = createApp({ store, env: { SORREL_HUB_AUTH: 'dev', SORREL_HUB_LOCAL_DEMO: '1' },
    trustedGrantsById: { ...grants, grant_ref: legacy } });
  const server = http.createServer(app.handleRequest);
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  const url = `http://127.0.0.1:${server.address().port}`;
  const post = (pathname, body) => fetch(`${url}${pathname}`, { method: 'POST',
    headers: { 'content-type': 'application/json', 'x-sorrel-acting-principal': JSON.stringify(principal) },
    body: JSON.stringify(body) });
  try { await run({ url, post }); }
  finally { await new Promise(resolve => server.close(resolve)); }
}

test('sync storage accepts complete 255-byte components and rejects larger encoded names before writes', (t) => {
  const root = tempDir(t);
  const sync = createFsRepoSyncStore(root);
  const snapshot = 'a'.repeat(64);
  for (const name of ['a'.repeat(255), 'a' + 'é'.repeat(42) + 'bc', 'a' + '%'.repeat(84) + 'bc']) {
    assert.equal(encodePathSegment(name).length, 255);
    sync.setRef(name, name, snapshot);
    assert.equal(sync.getRef(name, name), snapshot);
  }
  const before = fs.readdirSync(root).sort();
  for (const name of ['a'.repeat(256), 'a' + 'é'.repeat(43), 'a' + '%'.repeat(85)]) {
    assert.throws(() => sync.put(name, Buffer.from('fixture')), FilesystemNameTooLongError);
    assert.throws(() => sync.setRef('repo_fixture', name, snapshot), FilesystemNameTooLongError);
    assert.throws(() => sync.getRef('repo_fixture', name), FilesystemNameTooLongError);
  }
  assert.deepEqual(fs.readdirSync(root).sort(), before);
});

test('metadata includes its suffix in bounds and never publishes invalid records', (t) => {
  const root = tempDir(t);
  const store = createFsMetadataStore(root);
  let index = 0;
  for (const id of ['a'.repeat(250), 'é'.repeat(41) + 'abcd', '%'.repeat(83) + 'a']) {
    assert.equal(encodePathSegment(id).length + 5, 255);
    store.createProject({ id, organizationId: 'org_local', name: `Accepted ${index++}` });
    assert.equal(fs.existsSync(path.join(root, 'projects', `${encodePathSegment(id)}.json`)), true);
  }
  const before = fs.readdirSync(path.join(root, 'projects')).sort();
  for (const id of ['a'.repeat(251), 'é'.repeat(42), '%'.repeat(84)]) {
    assert.throws(() => store.createProject({ id, organizationId: 'org_local', name: `Rejected ${index++}` }), FilesystemNameTooLongError);
    assert.equal(store.getProject(id), null);
  }
  assert.deepEqual(fs.readdirSync(path.join(root, 'projects')).sort(), before);
});

test('atomic writes validate full destination names before creating directories and use bounded temp names', (t) => {
  const root = tempDir(t);
  const targetDir = path.join(root, 'must-not-exist');
  assert.throws(() => atomicWrite(path.join(targetDir, 'a'.repeat(256)), 'fixture'), FilesystemNameTooLongError);
  assert.equal(fs.existsSync(targetDir), false);
  const write = fs.writeFileSync;
  const filenames = [];
  t.mock.method(fs, 'writeFileSync', (filename, ...args) => {
    filenames.push(path.basename(filename));
    return write(filename, ...args);
  });
  const target = path.join(root, 'a'.repeat(255));
  atomicWrite(target, 'fixture');
  assert.equal(fs.readFileSync(target, 'utf8'), 'fixture');
  assert.equal(filenames.length, 1);
  assert.match(filenames[0], /^\.tmp-\d+-[0-9a-f]{12}$/);
  assert.ok(Buffer.byteLength(filenames[0]) <= 255);
  assert.deepEqual(fs.readdirSync(root), ['a'.repeat(255)]);
});

test('HTTP filesystem bounds return clear 400 while memory keeps accepting long IDs and names', async (t) => {
  const root = tempDir(t);
  const id = 'é'.repeat(42);
  const longRepo = 'repo_' + 'a'.repeat(251);
  const longRef = 'a'.repeat(256);
  for (const [backend, store] of [
    ['fs', createFsMetadataStore(path.join(root, 'metadata'), { sync: createFsRepoSyncStore(path.join(root, 'sync')) })],
    ['memory', createInMemoryStore()],
  ]) {
    const tree = store.sync.put('repo_fixture', Buffer.from(JSON.stringify({ kind: 'Tree', schemaVersion: 'sorrel.protocol.v0', entries: [] })));
    const snapshot = store.sync.put('repo_fixture', Buffer.from(JSON.stringify({ kind: 'Snapshot', schemaVersion: 'sorrel.protocol.v0', rootTree: { kind: 'Tree', id: tree }, parents: [] })));
    await withServer(store, async ({ url, post }) => {
      const creation = await post('/projects', { id, name: 'Fixture', organizationId: 'org_local' });
      const refs = await fetch(`${url}/${longRepo}/refs`);
      const advance = await post(`/repo_fixture/refs/${longRef}`, { snapshot, grantRefs: [{ id: 'grant_ref', source: 'core' }] });
      if (backend === 'fs') {
        for (const response of [creation, refs, advance]) {
          assert.equal(response.status, 400);
          assert.deepEqual((await response.json()).error, { code: 'filesystem_name_too_long',
            message: 'identifier or ref name exceeds filesystem component limits' });
        }
        assert.equal(store.getProject(id), null);
        assert.deepEqual(store.sync.listRefs('repo_fixture'), []);
        assert.equal(fs.existsSync(path.join(root, 'metadata', 'projects')), false);
      } else {
        assert.equal(creation.status, 201);
        assert.equal(refs.status, 200);
        assert.equal(advance.status, 200);
        assert.equal(store.getProject(id).id, id);
        assert.equal(store.sync.getRef('repo_fixture', longRef), snapshot);
      }
    });
  }
});

test('actual platform ENAMETOOLONG errors map to 400 without path disclosure or partial publication', async (t) => {
  const root = tempDir(t);
  const store = createFsMetadataStore(root);
  const error = Object.assign(new Error('private fixture path /operator/data'), { code: 'ENAMETOOLONG' });
  t.mock.method(fs, 'renameSync', () => { throw error; });
  await withServer(store, async ({ post }) => {
    const response = await post('/projects', { id: 'safe', name: 'Fixture', organizationId: 'org_local' });
    assert.equal(response.status, 400);
    const body = await response.json();
    assert.equal(body.error.code, 'filesystem_name_too_long');
    assert.equal(JSON.stringify(body).includes('private fixture path'), false);
    assert.equal(JSON.stringify(body).includes('/operator/data'), false);
    assert.equal(store.getProject('safe'), null);
    assert.deepEqual(fs.readdirSync(path.join(root, 'projects')), []);
  });
  assert.equal(mapSyncStoreError(error).statusCode, 400);
  const other = Object.assign(new Error('disk full'), { code: 'ENOSPC' });
  assert.equal(mapSyncStoreError(other), other);
});
