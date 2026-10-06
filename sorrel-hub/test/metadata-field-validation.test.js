import assert from 'node:assert/strict';
import http from 'node:http';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { test } from 'node:test';
import { createApp } from '../src/app.js';
import { createFsMetadataStore } from '../src/fs-metadata-store.js';
import { createPolicy, createReviewComment, ModelValidationError } from '../src/models.js';

const principal = { type: 'user', id: 'validation_fixture' };
const grant = { id: 'grant_validation', source: 'core', principal, action: 'policy.grant' };
// The native record keeps these real HTTP fixtures valid with the private-Hub stack.
const nativeGrant = { id: 'grant_validation_native', schemaVersion: 'sorrel.protocol.v0', kind: 'Grant',
  principal: { kind: 'user', id: principal.id }, effect: 'allow',
  capabilities: ['org.write', 'project.create', 'project.read', 'policy.grant', 'proposal.read',
    'proposal.write', 'review.comment.write', 'workflow.run.write'] };
const fixtures = [
  ['/admin/organizations', 'organizations', { id: 'org_candidate', name: 'Candidate' }],
  ['/projects', 'projects', { id: 'project_candidate', organizationId: 'org_validation', name: 'Candidate' }],
  ['/admin/repositories', 'repositories', { id: 'repository_candidate', organizationId: 'org_validation', projectId: 'project_validation', provider: 'sorrel', owner: 'validation', name: 'Candidate', grantRefs: [{ id: grant.id }] }],
  ['/admin/proposals', 'proposals', { id: 'proposal_candidate', projectId: 'project_validation', title: 'Candidate', authorRef: 'user:validation_fixture' }],
  ['/admin/review-comments', 'reviewComments', { id: 'comment_candidate', proposalId: 'proposal_validation', body: 'Candidate', authorRef: 'user:validation_fixture' }],
  ['/admin/workflow-runs', 'workflowRuns', { id: 'run_candidate', projectId: 'project_validation', name: 'Candidate' }],
  ['/admin/policies', 'policies', { id: 'policy_candidate', organizationId: 'org_validation', name: 'Candidate', grantRefs: [{ id: grant.id }] }],
];

async function withServer(t) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'sorrel-field-validation-'));
  t.after(() => fs.rmSync(dir, { recursive: true, force: true }));
  const store = createFsMetadataStore(dir);
  store.createProject({ id: 'project_validation', organizationId: 'org_validation', name: 'Validation' });
  store.createProposal({ id: 'proposal_validation', projectId: 'project_validation', title: 'Validation', authorRef: 'user:validation_fixture' });
  store.createReviewComment({ id: 'comment_validation', proposalId: 'proposal_validation', body: 'Validation', authorRef: 'user:validation_fixture', path: 'src/main.rs', line: 10, metadata: { original: true } });
  store.createWorkflowRun({ id: 'run_validation', projectId: 'project_validation', name: 'Validation', metadata: { original: true } });
  const app = createApp({ store, trustedGrantsById: { [grant.id]: grant, [nativeGrant.id]: nativeGrant }, env: { SORREL_HUB_LOCAL_DEMO: '1' } });
  const server = http.createServer(app.handleRequest);
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  t.after(() => new Promise(resolve => server.close(resolve)));
  const base = `http://127.0.0.1:${server.address().port}`;
  return { store, dir, async request(method, route, payload) {
    const response = await fetch(base + route, { method, headers: { 'content-type': 'application/json', 'x-sorrel-acting-principal': JSON.stringify(principal) }, body: JSON.stringify(payload) });
    return { status: response.status, body: await response.json() };
  } };
}

function diskSnapshot(dir) {
  const files = {};
  for (const entry of fs.readdirSync(dir, { recursive: true, withFileTypes: true })) {
    if (entry.isFile()) {
      const filename = path.join(entry.parentPath, entry.name);
      files[path.relative(dir, filename)] = fs.readFileSync(filename, 'utf8');
    }
  }
  return files;
}

function assertRejected(result, field) {
  assert.equal(result.status, 400, JSON.stringify(result.body));
  assert.equal(result.body.error.code, 'model_validation_failed');
  assert.match(result.body.error.message, new RegExp(field));
}

test('every collection rejects non-string creation timestamps before memory or disk writes', async (t) => {
  const { store, dir, request } = await withServer(t);
  const before = diskSnapshot(dir);
  for (const [route, collection, payload] of fixtures) {
    for (const field of ['createdAt', 'updatedAt']) {
      for (const value of [{ toString: null, valueOf: null }, [], 42, false]) {
        assertRejected(await request('POST', route, { ...payload, [field]: value }), field);
        assert.equal(store[collection].has(payload.id), false);
      }
    }
  }
  assert.deepEqual(diskSnapshot(dir), before);
});

test('metadata accepts arbitrary object extensions but never primitives or arrays', async (t) => {
  const { store, dir, request } = await withServer(t);
  const before = diskSnapshot(dir);
  for (const [route, collection, payload] of fixtures.filter(([, collection]) => collection !== 'repositories')) {
    for (const metadata of ['wrong', [], 42, false]) {
      assertRejected(await request('POST', route, { ...payload, metadata }), 'metadata');
      assert.equal(store[collection].has(payload.id), false);
    }
  }
  assert.deepEqual(diskSnapshot(dir), before);
  const metadata = { pluginUnknown: { anything: [null, false, 42, { nested: 'value' }] }, extensionString: 'text', extensionNull: null };
  for (const [route, collection, payload] of fixtures.filter(([, collection]) => collection !== 'repositories')) {
    const response = await request('POST', route, { ...payload, metadata, createdAt: '2026-10-06T10:00:00Z', updatedAt: '2026-10-06T11:00:00Z' });
    assert.equal(response.status, 201, JSON.stringify(response.body));
    assert.deepEqual(response.body.data.metadata, metadata);
    assert.deepEqual(createFsMetadataStore(dir)[collection].get(payload.id).metadata, metadata);
    assert.equal(response.body.data.createdAt, '2026-10-06T10:00:00Z');
    assert.equal(response.body.data.updatedAt, '2026-10-06T11:00:00Z');
  }
});

test('comment lines reject malformed POST and PATCH without changing persisted records', async (t) => {
  const { store, dir, request } = await withServer(t);
  const before = diskSnapshot(dir);
  const original = structuredClone(store.getReviewComment('comment_validation'));
  const comment = fixtures.find(([, collection]) => collection === 'reviewComments')[2];
  for (const line of ['10', {}, [], true, 0, -1, 1.5, Number.MAX_SAFE_INTEGER + 1]) {
    assertRejected(await request('POST', '/admin/review-comments', { ...comment, line }), 'line');
    assertRejected(await request('PATCH', '/admin/review-comments/comment_validation', { line, body: 'must not persist' }), 'line');
    assert.deepEqual(store.getReviewComment(original.id), original);
  }
  assert.deepEqual(diskSnapshot(dir), before);
  assert.equal((await request('PATCH', '/admin/review-comments/comment_validation', { line: 20 })).body.data.line, 20);
  assert.equal((await request('PATCH', '/admin/review-comments/comment_validation', { line: null })).status, 200);
  assert.equal(createFsMetadataStore(dir).getReviewComment(original.id).line, undefined);
});

test('malformed PATCH metadata is atomic for proposals, comments and workflow runs', async (t) => {
  const { store, dir, request } = await withServer(t);
  const before = diskSnapshot(dir);
  for (const [route, get, id, patch] of [
    ['/admin/proposals', 'getProposal', 'proposal_validation', { title: 'must not persist' }],
    ['/admin/review-comments', 'getReviewComment', 'comment_validation', { body: 'must not persist' }],
    ['/admin/workflow-runs', 'getWorkflowRun', 'run_validation', { providerRunId: 'must not persist' }],
  ]) {
    const original = structuredClone(store[get](id));
    for (const metadata of ['wrong', [], 42, false, null]) {
      assertRejected(await request('PATCH', `${route}/${id}`, { ...patch, metadata }), 'metadata');
      assert.deepEqual(store[get](id), original);
    }
  }
  assert.deepEqual(diskSnapshot(dir), before);
  const updated = await request('PATCH', '/admin/workflow-runs/run_validation', { metadata: { unknownExtension: [null, 'value'] } });
  assert.deepEqual(updated.body.data.metadata, { original: true, unknownExtension: [null, 'value'] });
});

test('policy enabled rejects type coercion while preserving boolean and null defaults', async (t) => {
  const { store, dir, request } = await withServer(t);
  const before = diskSnapshot(dir);
  const policy = fixtures.find(([, collection]) => collection === 'policies')[2];
  for (const enabled of ['false', 'true', 0, 1, {}, []]) {
    assertRejected(await request('POST', '/admin/policies', { ...policy, enabled }), 'enabled');
    assert.equal(store.getPolicy(policy.id), null);
  }
  assert.deepEqual(diskSnapshot(dir), before);
  assert.equal((await request('POST', '/admin/policies', { ...policy, enabled: false })).body.data.enabled, false);
  assert.equal(createFsMetadataStore(dir).getPolicy(policy.id).enabled, false);
  assert.equal(createPolicy({ organizationId: 'org_validation', name: 'Default', enabled: null }).enabled, true);
});

test('omitted and null optional fields preserve alpha defaults and extension values', () => {
  const comment = createReviewComment({ proposalId: 'proposal', body: 'Body', authorRef: 'user:fixture', metadata: null, line: null, createdAt: null, updatedAt: null });
  assert.deepEqual(comment.metadata, {});
  assert.equal(comment.line, undefined);
  assert.equal(typeof comment.createdAt, 'string');
  assert.equal(typeof comment.updatedAt, 'string');
  assert.equal(createReviewComment({ proposalId: 'proposal', body: 'Body', authorRef: 'user:fixture', createdAt: 'legacy-date-label' }).createdAt, 'legacy-date-label');
  for (const line of [NaN, Infinity, -Infinity]) {
    assert.throws(() => createReviewComment({ proposalId: 'proposal', body: 'Body', authorRef: 'user:fixture', line }), ModelValidationError);
  }
});
