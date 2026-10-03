import assert from 'node:assert/strict';
import fs from 'node:fs';
import http from 'node:http';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';

import { createApp } from '../src/app.js';
import { evaluate, evaluateWithTrustedGrants } from '../src/core-policy.js';
import { createFsMetadataStore } from '../src/fs-metadata-store.js';
import { createFsRepoSyncStore, encodePathSegment } from '../src/fs-sync-store.js';
import { createRepoSyncStore } from '../src/sync-store.js';
import { walkClosure } from '../src/sync-closure.js';
import { MAX_JSON_BODY_BYTES, readJsonBody } from '../src/http.js';
import { createConvexMirror } from '../src/convex-mirror.js';

const principal = { type: 'user', id: 'oidc:owner' };
const resource = { kind: 'repo', id: 'repo_private' };
const allow = { id: 'allow', principal, action: 'repo.object.read', resource, effect: 'allow' };

test('restrictive effects override allows independent of order and cannot be omitted by references', () => {
  for (const [effect, outcome] of [['deny', 'deny'], ['redact', 'redact'], ['review', 'needs_review']]) {
    const restrictive = { ...allow, id: 'restrictive', effect };
    for (const grants of [[allow, restrictive], [restrictive, allow]]) {
      const result = evaluate({ principal, resource, action: allow.action, grants });
      assert.equal(result.allowed, false);
      assert.equal(result.decision.outcome, outcome);
    }
    assert.throws(() => evaluateWithTrustedGrants(principal, allow.action, resource,
      [{ id: 'allow' }], { allow, restrictive }), { code: 'policy_denied' });
  }
});

test('grant lifecycle, wildcard capability/resource and Core principal shape', () => {
  for (const fields of [
    { status: 'expired' }, { status: 'revoked' }, { revokedAt: '2026-01-01T00:00:00Z' },
    { expiresAt: '2000-01-01T00:00:00Z' }, { issuedAt: '2100-01-01T00:00:00Z' },
    { conditions: { unsupported: 'yes' } },
  ]) {
    assert.equal(evaluate({ principal, resource, action: allow.action, grants: [{ ...allow, ...fields }] }).allowed, false);
  }
  assert.throws(() => evaluate({ principal, resource, action: allow.action, grants: [{ ...allow, expiresAt: 'invalid' }] }), { code: 'policy_evaluation_failed' });
  assert.equal(evaluate({ principal, resource, action: allow.action, grants: [{
    id: 'core', principal: { kind: 'user', id: principal.id }, capabilities: ['*'],
    resource: { kind: 'repo', id: '*' }, status: 'active', effect: 'allow',
  }] }).allowed, true);
});

test('publishing requires typed Snapshot roots, Tree links, Snapshot parents and supported versions', () => {
  const store = createRepoSyncStore();
  const put = (value) => store.put(resource.id, Buffer.from(JSON.stringify(value)));
  const blob = store.put(resource.id, Buffer.from('opaque blob'));
  const tree = put({ kind: 'Tree', entries: [] });
  const malformed = [
    blob, tree,
    put({ kind: 'Snapshot', parents: [] }),
    put({ kind: 'Snapshot', tree: blob, parents: [] }),
    put({ kind: 'Snapshot', tree, parents: [blob] }),
    put({ kind: 'Snapshot', rootTree: { kind: 'Blob', id: tree }, parents: [] }),
    put({ kind: 'Snapshot', schemaVersion: 'sorrel.protocol.v99', tree, parents: [] }),
    put({ kind: 'Snapshot', tree: put({ kind: 'Tree', entries: [{ name: '..', object: blob }] }), parents: [] }),
    put({ kind: 'Snapshot', tree: put({ kind: 'Tree', entries: [{ name: 'x', object: { kind: 'Snapshot', id: blob } }] }), parents: [] }),
  ];
  for (const id of malformed) assert.throws(() => walkClosure(resource.id, [id], store, { snapshotRoots: true }), { code: 'invalid_closure' });
  const jsonBlob = put({ kind: 'Snapshot', tree: 'not a valid tree' });
  const validTree = put({ kind: 'Tree', entries: [{ name: 'json.txt', object: { kind: 'Blob', id: jsonBlob } }] });
  const valid = put({ kind: 'Snapshot', tree: validTree, parents: [] });
  assert.equal(walkClosure(resource.id, [valid], store, { snapshotRoots: true }).incomplete, false);
});

test('closure traversal is iterative for deep history and still reports missing links', () => {
  const store = createRepoSyncStore();
  const tree = store.put(resource.id, Buffer.from(JSON.stringify({ kind: 'Tree', entries: [] })));
  let tip;
  for (let i = 0; i < 12_000; i++) tip = store.put(resource.id, Buffer.from(JSON.stringify({ kind: 'Snapshot', tree, parents: tip ? [tip] : [] })));
  assert.equal(walkClosure(resource.id, [tip], store, { snapshotRoots: true }).closure.size, 12_001);
  const missing = 'aa'.repeat(32);
  const id = store.put(resource.id, Buffer.from(JSON.stringify({ kind: 'Snapshot', tree, parents: [missing] })));
  assert.deepEqual(walkClosure(resource.id, [id], store, { snapshotRoots: true }).missingIds, [missing]);
});

test('metadata failed writes roll back both create and update; corrupt refs fail closed', (t) => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'sorrel-trust-'));
  t.after(() => fs.rmSync(dir, { recursive: true, force: true }));
  const metadata = createFsMetadataStore(path.join(dir, 'metadata'));
  const proposal = metadata.createProposal({ projectId: 'project_a', title: 'original', authorPrincipal: principal });
  fs.renameSync(path.join(metadata.rootDir, 'proposals'), path.join(metadata.rootDir, 'saved'));
  fs.writeFileSync(path.join(metadata.rootDir, 'proposals'), 'not a directory');
  assert.throws(() => metadata.updateProposal(proposal.id, { title: 'changed' }));
  assert.equal(metadata.getProposal(proposal.id).title, 'original');
  assert.throws(() => metadata.createProposal({ projectId: 'project_a', title: 'new', authorPrincipal: principal }));
  assert.equal(metadata.listProposals().length, 1);
  const sync = createFsRepoSyncStore(path.join(dir, 'sync'));
  sync.setRef(resource.id, 'main', 'aa'.repeat(32));
  fs.writeFileSync(path.join(sync.rootDir, encodePathSegment(resource.id), 'refs', 'main'), '{broken');
  assert.throws(() => sync.getRef(resource.id, 'main'), /corrupt sync ref/);
  assert.throws(() => sync.listRefs(resource.id), /corrupt sync ref/);
});

async function withApp(options, callback) {
  const app = createApp(options);
  const server = http.createServer(app.handleRequest);
  await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
  try { await callback(`http://127.0.0.1:${server.address().port}`, app); }
  finally { await new Promise((resolve) => server.close(resolve)); }
}

const verifiedAdapter = { mode: 'oidc', async resolveSession(request) {
  return request.headers.authorization === 'Bearer verified' ? { principal, sessionId: 'verified', authMode: 'oidc' } : null;
} };

test('guarded deployments protect metadata mutations and all sync read/discovery routes', async () => {
  await withApp({ authAdapter: verifiedAdapter }, async (url, app) => {
    app.store.sync.put(resource.id, Buffer.from('private'));
    app.store.createProject({ id: 'project_private', organizationId: 'org_private', name: 'Private' });
    app.store.createProposal({ id: 'proposal_private', projectId: 'project_private', title: 'Private', authorPrincipal: principal });
    for (const [route, method, body] of [
      ['/projects', 'POST', { organizationId: 'org_private', name: 'Unauthorized' }],
      ['/admin/organizations', 'POST', { name: 'Unauthorized' }],
      ['/admin/proposals', 'POST', { projectId: 'project_private', title: 'Unauthorized', authorPrincipal: principal }],
      ['/admin/proposals/proposal_private', 'PATCH', { title: 'Unauthorized' }],
      ['/collaboration/lane-submit', 'POST', { projectId: 'project_private', title: 'Unauthorized', sourceLane: 'lane_a', sourceSnapshot: 'aa' }],
      ['/repo_private/refs', 'GET'], ['/repo_private/tree', 'GET'], ['/repo_private/files', 'GET'],
      [`/repo_private/objects/${'aa'.repeat(32)}`, 'GET'],
      ['/repo_private/objects/missing', 'POST', { want: ['aa'.repeat(32)], have: [] }],
      ['/projects/project_private', 'GET'], ['/admin/proposals/proposal_private', 'GET'],
    ]) {
      for (const authorization of [undefined, 'Bearer verified']) {
        const response = await fetch(url + route, { method, headers: { 'content-type': 'application/json', ...(authorization ? { authorization } : {}) }, ...(body ? { body: JSON.stringify(body) } : {}) });
        assert.equal(response.status, 403, `${method} ${route} ${authorization}`);
      }
    }
    for (const route of ['/projects', '/admin/sync-repos', '/admin/proposals', '/collaboration/proposal-summary']) {
      assert.equal((await fetch(url + route)).status, 403, route);
      const response = await fetch(url + route, { headers: { authorization: 'Bearer verified' } });
      assert.equal(response.status, 200);
      const body = await response.json();
      if (route.includes('summary')) assert.equal(body.data.total, 0);
      else assert.deepEqual(body.data ?? body.repos, []);
    }
    assert.equal(app.store.getProposal('proposal_private').title, 'Private');
  });
});

test('trusted scoped read grants reveal only their own project and repository', async () => {
  await withApp({ authAdapter: verifiedAdapter, trustedGrantsById: {
    allow, project: { id: 'project', principal, action: 'project.read', resource: { kind: 'project', id: 'project_allowed' } },
  } }, async (url, app) => {
    app.store.createProject({ id: 'project_allowed', organizationId: 'org_private', name: 'Allowed' });
    app.store.createProject({ id: 'project_hidden', organizationId: 'org_private', name: 'Hidden' });
    app.store.sync.put('repo_private', Buffer.from('private'));
    app.store.sync.put('repo_hidden', Buffer.from('hidden'));
    const headers = { authorization: 'Bearer verified' };
    assert.deepEqual((await fetch(url + '/admin/sync-repos', { headers }).then((r) => r.json())).repos, [{ id: 'repo_private', refCount: 0 }]);
    assert.deepEqual((await fetch(url + '/projects', { headers }).then((r) => r.json())).data.map((p) => p.id), ['project_allowed']);
  });
});

test('oversize bodies are rejected with and without Content-Length; mirror requires service credentials', async () => {
  async function* body() { yield Buffer.alloc(MAX_JSON_BODY_BYTES + 1); }
  const request = body(); request.headers = {};
  await assert.rejects(readJsonBody(request), { statusCode: 413 });
  await assert.rejects(readJsonBody({ headers: { 'content-length': String(MAX_JSON_BODY_BYTES + 1) } }), { statusCode: 413 });
  assert.equal(createConvexMirror({ CONVEX_URL: 'https://example.test' }).enabled, false);
});

test('authorized metadata writes bind actor identity and cannot overwrite another record or spoof comment scope', async () => {
  const grants = {
    admin: { id: 'admin', principal, action: 'policy.grant', resource: { kind: 'org', id: 'org_private' } },
    proposal: { id: 'proposal', principal, action: 'proposal.write', resource: { kind: 'project', id: 'project_allowed' } },
  };
  await withApp({ authAdapter: verifiedAdapter, trustedGrantsById: grants }, async (url, app) => {
    app.store.createProposal({ id: 'proposal_hidden', projectId: 'project_hidden', title: 'Hidden', authorPrincipal: principal });
    const post = async (route, body) => fetch(url + route, { method: 'POST', headers: { authorization: 'Bearer verified', 'content-type': 'application/json' }, body: JSON.stringify(body) });
    const created = await post('/admin/proposals', { id: 'proposal_allowed', projectId: 'project_allowed', title: 'Allowed', authorPrincipal: { type: 'user', id: 'spoofed' }, authorRef: 'spoofed-ref' });
    assert.equal(created.status, 201);
    assert.deepEqual((await created.json()).data.authorPrincipal, principal);
    const overwrite = await post('/admin/proposals', { id: 'proposal_hidden', projectId: 'project_allowed', title: 'Overwrite' });
    assert.equal(overwrite.status, 409);
    assert.equal(app.store.getProposal('proposal_hidden').title, 'Hidden');
    const forgedComment = await post('/admin/review-comments', { projectId: 'project_allowed', proposalId: 'proposal_hidden', body: 'Forge' });
    assert.equal(forgedComment.status, 403);
    const allowedComment = await post('/admin/review-comments', { proposalId: 'proposal_allowed', body: 'Allowed' });
    assert.equal(allowedComment.status, 201);
    assert.deepEqual((await allowedComment.json()).data.authorPrincipal, principal);
  });
});

test('lane-submit deduplication is scoped to project', async () => {
  await withApp({}, async (url) => {
    const submit = async (projectId) => fetch(url + '/collaboration/lane-submit', { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ projectId, syncRepoId: 'repo_same', sourceLane: 'lane_same', sourceSnapshot: 'aa'.repeat(32), title: 'same tip' }) }).then((r) => r.json());
    const first = await submit('project_first');
    const second = await submit('project_second');
    assert.equal(first.reused, false);
    assert.equal(second.reused, false);
    assert.notEqual(first.data.id, second.data.id);
    assert.equal((await submit('project_second')).reused, true);
  });
});
