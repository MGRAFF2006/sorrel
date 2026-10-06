import assert from 'node:assert/strict';
import fs from 'node:fs';
import http from 'node:http';
import os from 'node:os';
import path from 'node:path';
import { test } from 'node:test';
import { createApp } from '../src/app.js';
import { createFsMetadataStore } from '../src/fs-metadata-store.js';
import { WORKFLOW_RUN_STATUSES } from '../src/models.js';

const principal = { type: 'user', id: 'workflow_fixture' };
const grant = { id: 'grant_workflow_fixture', schemaVersion: 'sorrel.protocol.v0', kind: 'Grant',
  principal: { kind: 'user', id: principal.id }, effect: 'allow',
  capabilities: ['project.read', 'workflow.run.write'], resource: { kind: 'project', id: '*' } };
const startedAt = '2026-01-01T10:00:00Z';
const completedAt = '2026-01-01T10:01:00Z';

async function withServer(t) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'sorrel-run-lifecycle-'));
  t.after(() => fs.rmSync(dir, { recursive: true, force: true }));
  const store = createFsMetadataStore(dir);
  store.createProject({ id: 'project_workflow_fixture', organizationId: 'org_workflow_fixture', name: 'Workflow' });
  const app = createApp({ store, trustedGrantsById: { [grant.id]: grant }, env: { SORREL_HUB_LOCAL_DEMO: '1' } });
  const server = http.createServer(app.handleRequest);
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  t.after(() => new Promise(resolve => server.close(resolve)));
  const base = `http://127.0.0.1:${server.address().port}`;
  return { store, dir, async request(method, route, body) {
    const response = await fetch(base + route, { method, headers: { 'content-type': 'application/json', 'x-sorrel-acting-principal': JSON.stringify(principal) }, body: JSON.stringify(body) });
    return { status: response.status, body: await response.json() };
  } };
}

// Expected product flow, independent of the production transition constant.
const expectedNext = {
  queued: ['in_progress', 'failed', 'cancelled'],
  in_progress: ['succeeded', 'failed', 'cancelled'],
  succeeded: [], failed: [], cancelled: [],
};

test('HTTP rejects every impossible run transition without memory or persisted-byte mutation', async (t) => {
  const { store, dir, request } = await withServer(t);
  for (const from of WORKFLOW_RUN_STATUSES) {
    for (const to of WORKFLOW_RUN_STATUSES) {
      if (from === to || expectedNext[from].includes(to)) continue;
      const run = store.createWorkflowRun({ id: `run_${from}_${to}`, projectId: 'project_workflow_fixture', name: 'Attempt', status: from,
        ...(from !== 'queued' ? { startedAt } : {}),
        ...(['succeeded', 'failed', 'cancelled'].includes(from) ? { completedAt } : {}), metadata: { original: true } });
      const filename = path.join(dir, 'workflowRuns', `${run.id}.json`);
      const before = fs.readFileSync(filename, 'utf8');
      const response = await request('PATCH', `/admin/workflow-runs/${run.id}`, { status: to, providerRunId: 'must-not-persist', metadata: { changed: true } });
      assert.equal(response.status, 400, `${from}→${to}`);
      assert.equal(response.body.error.code, 'model_validation_failed');
      assert.match(response.body.error.message, new RegExp(`from ${from} to ${to}`));
      assert.deepEqual(store.getWorkflowRun(run.id), run);
      assert.equal(fs.readFileSync(filename, 'utf8'), before);
      assert.deepEqual(createFsMetadataStore(dir).getWorkflowRun(run.id), JSON.parse(before));
    }
  }
});

test('HTTP permits established forward, failure and cancellation flows with stable terminal times', async (t) => {
  const { store, dir, request } = await withServer(t);
  for (const [from, nextStatuses] of Object.entries(expectedNext)) {
    for (const to of nextStatuses) {
      const run = store.createWorkflowRun({ id: `run_${from}_${to}`, projectId: 'project_workflow_fixture', name: 'Attempt', status: from,
        ...(from === 'in_progress' ? { startedAt } : {}) });
      const response = await request('PATCH', `/admin/workflow-runs/${run.id}`, { status: to });
      assert.equal(response.status, 200, `${from}→${to}`);
      const updated = response.body.data;
      assert.equal(updated.status, to);
      assert.equal(typeof updated.startedAt, 'string');
      assert.ok(Number.isFinite(Date.parse(updated.startedAt)));
      if (from === 'in_progress') assert.equal(updated.startedAt, startedAt);
      if (to !== 'in_progress') {
        assert.equal(typeof updated.completedAt, 'string');
        assert.ok(Date.parse(updated.completedAt) >= Date.parse(updated.startedAt));
        const replay = await request('PATCH', `/admin/workflow-runs/${run.id}`, { status: to, metadata: { providerDetail: 'finished' } });
        assert.equal(replay.status, 200);
        assert.equal(replay.body.data.startedAt, updated.startedAt);
        assert.equal(replay.body.data.completedAt, updated.completedAt);
        assert.deepEqual(replay.body.data.metadata, { providerDetail: 'finished' });
      } else {
        assert.equal(updated.completedAt, undefined);
      }
      assert.equal(createFsMetadataStore(dir).getWorkflowRun(run.id).status, to);
    }
  }
});

test('same-status terminal retries preserve original times and explicit correction remains supported', async (t) => {
  const { store, dir, request } = await withServer(t);
  for (const status of ['succeeded', 'failed', 'cancelled']) {
    const run = store.createWorkflowRun({ id: `run_${status}`, projectId: 'project_workflow_fixture', name: 'Imported attempt', status, startedAt, completedAt });
    const repeated = await request('PATCH', `/admin/workflow-runs/${run.id}`, { status, providerRunId: 'provider-run', metadata: { externalDetail: [null, 'value'] } });
    assert.equal(repeated.status, 200);
    assert.equal(repeated.body.data.completedAt, completedAt);
    assert.equal(repeated.body.data.startedAt, startedAt);
    const correction = '2026-01-01T10:02:00Z';
    const corrected = await request('PATCH', `/admin/workflow-runs/${run.id}`, { status, completedAt: correction });
    assert.equal(corrected.status, 200);
    assert.equal(corrected.body.data.completedAt, correction);
    assert.equal(createFsMetadataStore(dir).getWorkflowRun(run.id).completedAt, correction);
    const edit = await request('PATCH', `/admin/workflow-runs/${run.id}`, { metadata: { note: 'metadata only' } });
    assert.equal(edit.status, 200);
    assert.equal(edit.body.data.status, status);
    assert.equal(edit.body.data.completedAt, correction);
  }
});

test('POST retains completed-run imports and queued defaults', async (t) => {
  const { dir, request } = await withServer(t);
  for (const status of WORKFLOW_RUN_STATUSES) {
    const response = await request('POST', '/admin/workflow-runs', { id: `import_${status}`, projectId: 'project_workflow_fixture', name: 'Imported attempt', status, startedAt, completedAt });
    assert.equal(response.status, 201);
    assert.equal(response.body.data.status, status);
    assert.equal(response.body.data.completedAt, completedAt);
    assert.equal(createFsMetadataStore(dir).getWorkflowRun(`import_${status}`).startedAt, startedAt);
  }
  const response = await request('POST', '/admin/workflow-runs', { projectId: 'project_workflow_fixture', name: 'New attempt' });
  assert.equal(response.status, 201);
  assert.equal(response.body.data.status, 'queued');
  assert.equal(response.body.data.startedAt, undefined);
  assert.equal(response.body.data.completedAt, undefined);
});
