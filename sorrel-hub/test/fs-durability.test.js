import assert from 'node:assert/strict';
import fs from 'node:fs';
import http from 'node:http';
import os from 'node:os';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import test from 'node:test';
import { createApp } from '../src/app.js';
import * as bootstrap from '../src/bootstrap-grants.js';
import { createFsMetadataStore } from '../src/fs-metadata-store.js';
import * as syncStore from '../src/fs-sync-store.js';
import { objectId } from '../src/blake3.js';

const { atomicWrite, createFsRepoSyncStore, PublishedWriteDurabilityError } = syncStore;

const unix = process.platform !== 'win32';
function tempDir(t) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'sorrel-durability-'));
  t.after(() => fs.rmSync(dir, { recursive: true, force: true }));
  return dir;
}
function trackIo(t) {
  const descriptors = new Map();
  const events = [];
  const original = { open: fs.openSync, close: fs.closeSync, sync: fs.fsyncSync, rename: fs.renameSync };
  let fail = () => false;
  t.mock.method(fs, 'openSync', (...args) => {
    const fd = original.open(...args);
    descriptors.set(fd, { path: path.resolve(args[0]), directory: fs.fstatSync(fd).isDirectory() });
    return fd;
  });
  t.mock.method(fs, 'closeSync', fd => {
    original.close(fd);
    descriptors.delete(fd);
  });
  t.mock.method(fs, 'fsyncSync', fd => {
    const event = { kind: 'sync', ...descriptors.get(fd) };
    events.push(event);
    if (fail(event)) throw new Error('synthetic fsync failure');
    original.sync(fd);
  });
  t.mock.method(fs, 'renameSync', (source, target) => {
    events.push({ kind: 'rename', path: path.resolve(target) });
    original.rename(source, target);
  });
  return { events, descriptors, fail: callback => { fail = callback; } };
}

test('atomic writes flush file before rename and directory entries child first within root', { skip: !unix }, t => {
  const root = tempDir(t);
  const io = trackIo(t);
  const target = path.join(root, 'new', 'nested', 'record');
  atomicWrite(target, 'fixture', root);
  assert.deepEqual(io.events.map(event => event.kind), ['sync', 'rename', 'sync', 'sync', 'sync']);
  assert.equal(io.events[0].directory, false);
  assert.deepEqual(io.events.slice(2).map(event => event.path), [path.dirname(target), path.join(root, 'new'), root]);
  assert.equal(io.descriptors.size, 0);
  assert.equal(fs.readFileSync(target, 'utf8'), 'fixture');
});

test('exclusive temporary collision preserves the file owned by another writer', t => {
  const root = tempDir(t);
  const original = fs.openSync;
  let collision;
  t.mock.method(fs, 'openSync', (name, flags, ...rest) => {
    if (flags !== 'wx') return original(name, flags, ...rest);
    collision = name;
    const fd = original(name, 'wx');
    try { fs.writeFileSync(fd, 'other writer fixture'); } finally { fs.closeSync(fd); }
    throw Object.assign(new Error('synthetic temporary collision'), { code: 'EEXIST' });
  });
  assert.throws(() => atomicWrite(path.join(root, 'record'), 'new fixture', root), /synthetic temporary collision/);
  assert.equal(fs.readFileSync(collision, 'utf8'), 'other writer fixture');
  assert.equal(fs.existsSync(path.join(root, 'record')), false);
});

test('file flush failure before rename preserves old bytes and memory and closes temporary file', { skip: !unix }, t => {
  const root = tempDir(t);
  const store = createFsMetadataStore(root);
  store.createOrganization({ id: 'org_local', name: 'Fixture organization' });
  const original = store.createProject({ id: 'project', organizationId: 'org_local', name: 'Fixture' });
  const target = path.join(root, 'projects', 'project.json');
  const oldBytes = fs.readFileSync(target);
  const io = trackIo(t);
  io.fail(event => !event.directory);
  assert.throws(() => store.linkProjectRepository(original.id, 'repo'), /synthetic fsync failure/);
  assert.equal(store.getProject(original.id), original);
  assert.deepEqual(fs.readFileSync(target), oldBytes);
  assert.equal(io.events.some(event => event.kind === 'rename'), false);
  assert.deepEqual(fs.readdirSync(path.dirname(target)), ['project.json']);
  assert.equal(io.descriptors.size, 0);
});

test('post-rename directory failure adopts published create and update while reporting uncertainty', { skip: !unix }, t => {
  const root = tempDir(t);
  const store = createFsMetadataStore(root);
  store.createOrganization({ id: 'org_local', name: 'Fixture organization' });
  const io = trackIo(t);
  io.fail(event => event.directory);
  assert.throws(() => store.createProject({ id: 'project', organizationId: 'org_local', name: 'Fixture' }), PublishedWriteDurabilityError);
  assert.equal(store.getProject('project').name, 'Fixture');
  assert.throws(() => store.linkProjectRepository('project', 'repo'), PublishedWriteDurabilityError);
  const published = JSON.parse(fs.readFileSync(path.join(root, 'projects', 'project.json'), 'utf8'));
  assert.deepEqual(JSON.parse(JSON.stringify(store.getProject('project'))), published);
  assert.deepEqual(published.repositoryIds, ['repo']);
  assert.equal(io.descriptors.size, 0);
  io.fail(() => false);
  assert.deepEqual(createFsMetadataStore(root).getProject('project'), published);
  store.linkProjectRepository('project', 'repo');
  assert.deepEqual(store.getProject('project').repositoryIds, ['repo']);
});

test('reopened stores retry published object durability before publishing a ref', { skip: !unix }, t => {
  const root = tempDir(t);
  const store = createFsRepoSyncStore(root);
  const io = trackIo(t);
  io.fail(event => event.directory);
  assert.throws(() => store.put('repo', Buffer.from('fixture')), PublishedWriteDurabilityError);
  io.fail(() => false);
  const reopened = createFsRepoSyncStore(root);
  io.events.length = 0;
  const id = reopened.put('repo', Buffer.from('fixture'));
  assert.equal(io.events.some(event => event.kind === 'rename'), false);
  assert.equal(io.events[0].directory, false);
  assert.equal(io.events.at(-1).path, root);
  reopened.setRef('repo', 'main', id);
  const refRename = io.events.findIndex(event => event.kind === 'rename');
  assert.ok(refRename > 0);
  assert.equal(io.events[refRename - 1].directory, false);
  assert.equal(io.events.at(-1).path, root);
  assert.equal(reopened.getRef('repo', 'main'), id);
  assert.equal(io.descriptors.size, 0);
});

test('fresh process reconciles nested root ancestor entries after failed initialization', { skip: process.platform !== 'linux' }, t => {
  const base = tempDir(t);
  const root = path.join(base, 'new', 'nested', 'metadata');
  const io = trackIo(t);
  io.fail(event => event.directory && event.path === path.dirname(root));
  assert.throws(() => createFsMetadataStore(root), /synthetic fsync failure/);
  assert.equal(fs.statSync(root).isDirectory(), true);
  assert.equal(io.descriptors.size, 0);
  const child = spawnSync(process.execPath, ['--input-type=module', '-e', `
    import fs from 'node:fs';
    import { createFsMetadataStore } from ${JSON.stringify(new URL('../src/fs-metadata-store.js', import.meta.url).href)};
    const root = ${JSON.stringify(root)};
    const sync = fs.fsyncSync;
    const paths = [];
    fs.fsyncSync = fd => { paths.push(fs.readlinkSync('/proc/self/fd/' + fd)); sync(fd); };
    createFsMetadataStore(root);
    process.stdout.write(JSON.stringify(paths));
  `], { encoding: 'utf8' });
  // /proc supplies independent descriptor-path evidence on Linux only.
  assert.equal(child.status, 0, child.stderr);
  assert.deepEqual(JSON.parse(child.stdout).slice(0, 4), [root, path.dirname(root), path.dirname(path.dirname(root)), base]);
});

test('HTTP reports post-publication failure without exposing its cause and live reads match disk', { skip: !unix }, async t => {
  const root = tempDir(t);
  const store = createFsMetadataStore(root);
  store.createOrganization({ id: 'org_local', name: 'Fixture organization' });
  const grants = { ...bootstrap.createLocalBootstrapGrants(), ...(bootstrap.createLocalDemoGrants?.() ?? {}) };
  const app = createApp({ store, trustedGrantsById: grants,
    env: { SORREL_HUB_AUTH: 'dev', SORREL_HUB_LOCAL_DEMO: '1' } });
  const server = http.createServer(app.handleRequest);
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  t.after(() => new Promise(resolve => server.close(resolve)));
  const io = trackIo(t);
  io.fail(event => event.directory);
  const base = `http://127.0.0.1:${server.address().port}`;
  const headers = { 'content-type': 'application/json', 'x-sorrel-acting-principal': JSON.stringify({ type: 'user', id: 'local' }) };
  const created = await fetch(`${base}/projects`, { method: 'POST', headers,
    body: JSON.stringify({ id: 'project', organizationId: 'org_local', name: 'Fixture' }) });
  assert.equal(created.status, 500);
  assert.deepEqual(await created.json(), { error: { code: 'internal_server_error', message: 'internal server error' } });
  const read = await fetch(`${base}/projects/project`, { headers });
  assert.equal(read.status, 200);
  assert.deepEqual((await read.json()).data, JSON.parse(fs.readFileSync(path.join(root, 'projects', 'project.json'), 'utf8')));
  assert.equal(io.descriptors.size, 0);
});

test('HTTP refs flush the validated closure, including a failed-upload terminal blob, before publication', { skip: !unix }, async t => {
  const root = tempDir(t);
  const sync = createFsRepoSyncStore(root);
  const io = trackIo(t);
  const repoId = 'repo_durability';
  const blob = Buffer.from('terminal blob fixture');
  const blobId = objectId(blob);
  const blobPath = path.join(root, repoId, 'objects', blobId.slice(0, 2), blobId);
  io.fail(event => event.directory && event.path === path.dirname(blobPath));
  assert.throws(() => sync.put(repoId, blob), PublishedWriteDurabilityError);
  io.fail(() => false);
  const treeId = sync.put(repoId, Buffer.from(JSON.stringify({ schemaVersion: 'sorrel.protocol.v0', kind: 'Tree', entries: [{ type: 'file', name: 'fixture', id: blobId }] })));
  const snapshot = sync.put(repoId, Buffer.from(JSON.stringify({ schemaVersion: 'sorrel.protocol.v0', kind: 'Snapshot', tree: treeId, parents: [] })));
  const grants = { ...bootstrap.createLocalBootstrapGrants(), ...(bootstrap.createLocalDemoGrants?.() ?? {}) };
  const app = createApp({ trustedGrantsById: grants, env: { SORREL_HUB_AUTH: 'dev', SORREL_HUB_LOCAL_DEMO: '1' } });
  app.store.sync = sync;
  const server = http.createServer(app.handleRequest);
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  t.after(() => new Promise(resolve => server.close(resolve)));
  const base = `http://127.0.0.1:${server.address().port}`;
  const request = () => fetch(`${base}/${repoId}/refs/main`, { method: 'POST',
    headers: { 'content-type': 'application/json', 'x-sorrel-acting-principal': JSON.stringify({ type: 'user', id: 'local' }) },
    body: JSON.stringify({ snapshot, expected: null, grantRefs: [{ id: bootstrap.BOOTSTRAP_REF_WRITE_GRANT_ID, source: 'core' }] }) });
  io.events.length = 0;
  io.fail(event => event.directory && event.path === path.dirname(blobPath));
  const failed = await request();
  assert.equal(failed.status, 500, await failed.text());
  assert.equal(sync.getRef(repoId, 'main'), undefined);
  assert.equal(io.events.some(event => event.kind === 'rename'), false);
  assert.equal(io.descriptors.size, 0);
  io.events.length = 0;
  io.fail(() => false);
  const accepted = await request();
  assert.equal(accepted.status, 200, await accepted.text());
  const rename = io.events.findIndex(event => event.kind === 'rename');
  assert.ok(rename > 0);
  for (const id of [snapshot, treeId, blobId]) {
    assert.ok(io.events.slice(0, rename).some(event => !event.directory && event.path.endsWith(`${path.sep}${id}`)), `missing object barrier for ${id}`);
  }
  assert.ok(io.events.slice(0, rename).some(event => event.directory && event.path === path.dirname(blobPath)));
  assert.equal(sync.getRef(repoId, 'main'), snapshot);
  assert.equal(io.descriptors.size, 0);
});
