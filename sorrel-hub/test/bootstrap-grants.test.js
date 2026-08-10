import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import http from 'node:http';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';

import { createApp } from '../src/app.js';
import {
  BOOTSTRAP_OBJECT_WRITE_GRANT_ID,
  BOOTSTRAP_REF_WRITE_GRANT_ID,
  createLocalBootstrapGrants,
  resolveTrustedGrants,
} from '../src/bootstrap-grants.js';
import { objectId } from '../src/blake3.js';

const repoId = 'repo_bootstrap_sync';
const principalHeader = {
  'x-sorrel-acting-principal': JSON.stringify({ type: 'user', id: 'local' }),
};

test('resolveTrustedGrants defaults to no local bootstrap grants', () => {
  const grants = resolveTrustedGrants({});
  assert.deepEqual(grants, {});
});

test('resolveTrustedGrants enables local bootstrap grants only with explicit 1', () => {
  const grants = resolveTrustedGrants({ SORREL_HUB_BOOTSTRAP_GRANTS: '1' });
  assert.equal(Object.keys(grants).length, 2);
  assert.equal(grants[BOOTSTRAP_OBJECT_WRITE_GRANT_ID].action, 'repo.object.write');
  assert.equal(grants[BOOTSTRAP_REF_WRITE_GRANT_ID].action, 'repo.ref.write');
});

test('resolveTrustedGrants does not enable bootstrap for other values', () => {
  for (const value of ['0', 'true', 'yes', '']) {
    assert.deepEqual(
      resolveTrustedGrants({ SORREL_HUB_BOOTSTRAP_GRANTS: value }),
      {},
    );
  }
});

test('bootstrap grants allow CLI-shaped push for user:local', async () => {
  const app = createApp({ trustedGrantsById: createLocalBootstrapGrants() });
  const server = http.createServer(app.handleRequest);
  await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
  const { port } = server.address();
  const baseUrl = `http://127.0.0.1:${port}`;

  try {
    const blobBytes = Buffer.from('bootstrap hello', 'utf8');
    const blob = { id: objectId(blobBytes), bytes: blobBytes };
    const treeBytes = Buffer.from(
      JSON.stringify({ kind: 'Tree', entries: [{ name: 'hello.txt', object: blob.id }] }),
    );
    const tree = { id: objectId(treeBytes), bytes: treeBytes };
    const snapshotBytes = Buffer.from(
      JSON.stringify({ kind: 'Snapshot', tree: tree.id, parents: [] }),
    );
    const snapshot = { id: objectId(snapshotBytes), bytes: snapshotBytes };

    const upload = await fetch(`${baseUrl}/${repoId}/objects`, {
      method: 'POST',
      headers: { 'content-type': 'application/json', ...principalHeader },
      body: JSON.stringify({
        objects: [
          { id: blob.id, data: blob.bytes.toString('base64') },
          { id: tree.id, data: tree.bytes.toString('base64') },
          { id: snapshot.id, data: snapshot.bytes.toString('base64') },
        ],
        grantRefs: [{ id: BOOTSTRAP_OBJECT_WRITE_GRANT_ID, source: 'core' }],
      }),
    });
    assert.equal(upload.status, 200, await upload.text());

    const advance = await fetch(`${baseUrl}/${repoId}/refs/HEAD`, {
      method: 'POST',
      headers: { 'content-type': 'application/json', ...principalHeader },
      body: JSON.stringify({
        snapshot: snapshot.id,
        force: false,
        grantRefs: [{ id: BOOTSTRAP_REF_WRITE_GRANT_ID, source: 'core' }],
      }),
    });
    assert.equal(advance.status, 200, await advance.text());

    const refs = await fetch(`${baseUrl}/${repoId}/refs`);
    const body = await refs.json();
    const head = (body.refs ?? []).find((entry) => entry.name === 'HEAD');
    assert.ok(head, `expected HEAD ref in ${JSON.stringify(body)}`);
    assert.equal(head.snapshot, snapshot.id);
  } finally {
    await new Promise((resolve, reject) => {
      server.close((error) => (error ? reject(error) : resolve()));
    });
  }
});

test('server.js defaults to localhost and accepts a push with bootstrap opt-in', async () => {
  const dataRoot = await mkdtemp(join(tmpdir(), 'sorrel-hub-boot-'));
  const serverPath = fileURLToPath(new URL('../src/server.js', import.meta.url));
  const child = spawn(process.execPath, [serverPath], {
    env: {
      ...process.env,
      PORT: '0',
      SORREL_HUB_BOOTSTRAP_GRANTS: '1',
      SORREL_HUB_DATA_DIR: join(dataRoot, 'sync'),
      SORREL_HUB_METADATA_DIR: join(dataRoot, 'metadata'),
    },
    stdio: ['ignore', 'pipe', 'pipe'],
  });

  let stdout = '';
  child.stdout.setEncoding('utf8');
  child.stdout.on('data', (chunk) => {
    stdout += chunk;
  });

  try {
    const baseUrl = await waitForListen(child, () => stdout);
    const health = await fetch(`${baseUrl}/healthz`);
    assert.equal(health.status, 200);

    const blobBytes = Buffer.from('server bootstrap', 'utf8');
    const blobId = objectId(blobBytes);
    const treeBytes = Buffer.from(
      JSON.stringify({ kind: 'Tree', entries: [{ name: 'note.txt', object: blobId }] }),
    );
    const treeId = objectId(treeBytes);
    const snapshotBytes = Buffer.from(
      JSON.stringify({ kind: 'Snapshot', tree: treeId, parents: [] }),
    );
    const snapshotId = objectId(snapshotBytes);

    const upload = await fetch(`${baseUrl}/repo_server_boot/objects`, {
      method: 'POST',
      headers: { 'content-type': 'application/json', ...principalHeader },
      body: JSON.stringify({
        objects: [
          { id: blobId, data: blobBytes.toString('base64') },
          { id: treeId, data: treeBytes.toString('base64') },
          { id: snapshotId, data: snapshotBytes.toString('base64') },
        ],
        grantRefs: [{ id: BOOTSTRAP_OBJECT_WRITE_GRANT_ID, source: 'core' }],
      }),
    });
    assert.equal(upload.status, 200, await upload.text());
  } finally {
    child.kill('SIGTERM');
    await new Promise((resolve) => child.on('exit', resolve));
    await rm(dataRoot, { recursive: true, force: true });
  }
});

test('SORREL_HUB_TRUSTED_GRANTS_FILE merges additional grants', async () => {
  const dir = await mkdtemp(join(tmpdir(), 'sorrel-grants-'));
  const file = join(dir, 'grants.json');
  await writeFile(
    file,
    JSON.stringify({
      grant_extra: {
        id: 'grant_extra',
        source: 'core',
        principal: { type: 'user', id: 'other' },
        action: 'repo.object.write',
      },
    }),
  );

  try {
    const grants = resolveTrustedGrants({
      SORREL_HUB_BOOTSTRAP_GRANTS: '1',
      SORREL_HUB_TRUSTED_GRANTS_FILE: file,
    });
    assert.ok(grants[BOOTSTRAP_OBJECT_WRITE_GRANT_ID]);
    assert.ok(grants.grant_extra);
  } finally {
    await rm(dir, { recursive: true, force: true });
  }
});

async function waitForListen(child, getStdout) {
  const deadline = Date.now() + 5000;
  while (Date.now() < deadline) {
    if (child.exitCode != null) {
      throw new Error(`server exited early: ${child.exitCode}`);
    }
    const match = getStdout().match(/listening on (http:\/\/[^\s]+)/);
    if (match) {
      return match[1];
    }
    await new Promise((resolve) => setTimeout(resolve, 50));
  }
  throw new Error(`server did not print listen address; stdout=${getStdout()}`);
}
