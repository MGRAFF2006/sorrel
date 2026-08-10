import assert from 'node:assert/strict';
import fs from 'node:fs';
import http from 'node:http';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';

import { createApp } from '../src/app.js';
import {
  createFsMetadataStore,
  FsMetadataStore,
} from '../src/fs-metadata-store.js';
import { encodePathSegment } from '../src/fs-sync-store.js';
import { StoreConflictError } from '../src/store.js';

function tempDir(t) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'sorrel-hub-meta-'));
  t.after(() => fs.rmSync(dir, { recursive: true, force: true }));
  return dir;
}

function makeApp(metadataDir) {
  return createApp({
    store: createFsMetadataStore(metadataDir),
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

async function postJson(url, payload) {
  return await fetch(url, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify(payload),
  });
}

test('fs metadata store: createProject writes a JSON document under the collection dir', (t) => {
  const dir = tempDir(t);
  const store = createFsMetadataStore(dir);
  const project = store.createProject({
    organizationId: 'org_local',
    name: 'Platform Collaboration',
  });

  const filePath = path.join(
    dir,
    'projects',
    `${encodePathSegment(project.id)}.json`,
  );
  assert.equal(fs.existsSync(filePath), true);
  const onDisk = JSON.parse(fs.readFileSync(filePath, 'utf8'));
  assert.equal(onDisk.id, project.id);
  assert.equal(onDisk.slug, 'platform-collaboration');
});

test('POST /projects survives a server restart over the same metadata directory', async (t) => {
  const metadataDir = tempDir(t);
  let projectId;

  await withServer(makeApp(metadataDir), async (baseUrl) => {
    const createResponse = await postJson(`${baseUrl}/projects`, {
      organizationId: 'org_local',
      name: 'Persistent Project',
      description: 'should survive restart',
    });
    const created = await createResponse.json();

    assert.equal(createResponse.status, 201);
    projectId = created.data.id;
  });

  await withServer(makeApp(metadataDir), async (baseUrl) => {
    const listResponse = await fetch(`${baseUrl}/projects`);
    const listed = await listResponse.json();

    assert.equal(listResponse.status, 200);
    assert.equal(listed.data.length, 1);
    assert.equal(listed.data[0].id, projectId);
    assert.equal(listed.data[0].name, 'Persistent Project');
  });
});

test('POST /admin/organizations survives a server restart', async (t) => {
  const metadataDir = tempDir(t);
  let organizationId;

  await withServer(makeApp(metadataDir), async (baseUrl) => {
    const createResponse = await postJson(`${baseUrl}/admin/organizations`, {
      name: 'Acme Collaboration',
      slug: 'acme',
    });
    const created = await createResponse.json();

    assert.equal(createResponse.status, 201);
    organizationId = created.data.id;
  });

  await withServer(makeApp(metadataDir), async (baseUrl) => {
    const listResponse = await fetch(`${baseUrl}/admin/organizations`);
    const listed = await listResponse.json();

    assert.equal(listResponse.status, 200);
    assert.equal(listed.data.length, 1);
    assert.equal(listed.data[0].id, organizationId);
    assert.equal(listed.data[0].slug, 'acme');
  });
});

test('duplicate project slug conflict is still enforced after reload', async (t) => {
  const metadataDir = tempDir(t);

  await withServer(makeApp(metadataDir), async (baseUrl) => {
    const response = await postJson(`${baseUrl}/projects`, {
      organizationId: 'org_local',
      name: 'Policy Engine',
    });
    assert.equal(response.status, 201);
  });

  await withServer(makeApp(metadataDir), async (baseUrl) => {
    const response = await postJson(`${baseUrl}/projects`, {
      organizationId: 'org_local',
      name: 'Policy Engine',
    });
    const body = await response.json();

    assert.equal(response.status, 409);
    assert.deepEqual(body, {
      error: {
        code: 'store_conflict',
        message: 'project slug already exists for organization',
      },
    });
  });
});

test('fs metadata store: corrupt record files are skipped on load', (t) => {
  const dir = tempDir(t);
  const projectsDir = path.join(dir, 'projects');
  fs.mkdirSync(projectsDir, { recursive: true });

  const good = {
    id: 'proj_good',
    organizationId: 'org_local',
    name: 'Good Project',
    slug: 'good-project',
    status: 'active',
    repositoryIds: [],
    policyIds: [],
    principalRefs: [],
    policyRefs: [],
    grantRefs: [],
    policyDecisionRefs: [],
    auditEventRefs: [],
    createdAt: '2026-01-01T00:00:00.000Z',
    updatedAt: '2026-01-01T00:00:00.000Z',
  };
  fs.writeFileSync(path.join(projectsDir, 'proj_good.json'), `${JSON.stringify(good)}\n`);
  fs.writeFileSync(path.join(projectsDir, 'proj_corrupt.json'), '{not valid json');
  fs.writeFileSync(path.join(projectsDir, 'proj_noid.json'), `${JSON.stringify({ name: 'no id' })}\n`);

  const warnings = [];
  const originalWarn = console.warn;
  console.warn = (...args) => {
    warnings.push(args.join(' '));
  };

  let store;
  try {
    store = new FsMetadataStore(dir);
  } finally {
    console.warn = originalWarn;
  }

  assert.equal(store.listProjects().length, 1);
  assert.equal(store.listProjects()[0].id, 'proj_good');
  assert.ok(warnings.some((line) => line.includes('proj_corrupt.json')));
  assert.ok(warnings.some((line) => line.includes('proj_noid.json')));
});

test('fs metadata store: StoreConflictError does not leave an extra file on disk', (t) => {
  const dir = tempDir(t);
  const store = createFsMetadataStore(dir);
  store.createProject({ organizationId: 'org_local', name: 'Unique Slug' });

  assert.throws(
    () => store.createProject({ organizationId: 'org_local', name: 'Unique Slug' }),
    StoreConflictError,
  );

  const files = fs.readdirSync(path.join(dir, 'projects'));
  assert.equal(files.filter((name) => name.endsWith('.json')).length, 1);
});