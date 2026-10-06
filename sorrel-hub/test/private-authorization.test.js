import assert from 'node:assert/strict';
import http from 'node:http';
import test from 'node:test';
import { HubClient } from '../../sorrel-sdk-js/src/index.js';
import { createApp } from '../src/app.js';
import { createInMemoryStore } from '../src/store.js';
import { createLocalBootstrapGrants, createLocalDemoGrants } from '../src/bootstrap-grants.js';

const member = { type: 'user', id: 'member' };
const headers = { authorization: 'Bearer member', 'content-type': 'application/json' };
function grant(id, kind, resourceId, capabilities, extras = {}) {
  return { schemaVersion: 'sorrel.protocol.v0', kind: 'Grant', id,
    principal: { kind: 'user', id: member.id }, resource: { kind, id: resourceId }, capabilities, effect: 'allow', ...extras };
}
function grants() {
  return Object.fromEntries([
    grant('org_a', 'org', 'org_a', ['org.read', 'org.write', 'project.create', 'policy.read', 'policy.grant']),
    grant('project_a', 'project', 'proj_a', ['project.read', 'project.write', 'proposal.read', 'proposal.write', 'proposal.review',
      'review.comment.read', 'review.comment.write', 'workflow.run.read', 'workflow.run.write', 'policy.read', 'policy.grant']),
    grant('repo_meta_a', 'repo', 'repo_meta_a', ['repo.read']),
    grant('repo_a', 'repo', 'repo_a', ['repo.read', 'repo.object.write', 'repo.ref.write']),
  ].map(g => [g.id, g]));
}
function fixture() {
  const store = createInMemoryStore();
  const snapshots = {};
  for (const suffix of ['a', 'b']) {
    store.createOrganization({ id: `org_${suffix}`, name: `Organization ${suffix}` });
    store.createProject({ id: `proj_${suffix}`, organizationId: `org_${suffix}`, name: `Project ${suffix}` });
    store.createRepository({ id: `repo_meta_${suffix}`, projectId: `proj_${suffix}`, organizationId: `org_${suffix}`, provider: 'sorrel', owner: 'local', name: `Repo ${suffix}` });
    const blob = store.sync.put(`repo_${suffix}`, Buffer.from(`sorrel.blob.v0\nprivate ${suffix}`));
    const tree = store.sync.put(`repo_${suffix}`, Buffer.from(JSON.stringify({ schemaVersion: 'sorrel.protocol.v0', kind: 'Tree', entries: [{ name: 'private.txt', object: { kind: 'Blob', id: blob } }] })));
    const snapshot = store.sync.put(`repo_${suffix}`, Buffer.from(JSON.stringify({ schemaVersion: 'sorrel.protocol.v0', kind: 'Snapshot', rootTree: { kind: 'Tree', id: tree }, parents: [] })));
    store.sync.setRef(`repo_${suffix}`, 'main', snapshot);
    snapshots[suffix] = { blob, snapshot };
    store.createProposal({ id: `prop_${suffix}`, projectId: `proj_${suffix}`, repositoryId: `repo_meta_${suffix}`, syncRepoId: `repo_${suffix}`, title: `Proposal ${suffix}`, authorRef: 'user:local', status: 'open', sourceSnapshot: snapshot, targetSnapshot: snapshot });
    store.createReviewComment({ id: `comment_${suffix}`, proposalId: `prop_${suffix}`, body: `Comment ${suffix}`, authorRef: 'user:local' });
    store.createWorkflowRun({ id: `run_${suffix}`, projectId: `proj_${suffix}`, proposalId: `prop_${suffix}`, name: `Run ${suffix}` });
    store.createPolicy({ id: `policy_${suffix}`, organizationId: `org_${suffix}`, projectId: `proj_${suffix}`, name: `Policy ${suffix}` });
  }
  return { store, snapshots };
}
const authAdapter = { mode: 'oidc', async resolveSession(request) {
  return request.headers.authorization === headers.authorization ? { principal: member, authMode: 'oidc', sessionId: 'fixture' } : null;
} };
async function withHub(t, options = {}) {
  const state = fixture();
  const app = createApp({ ...state, authAdapter, trustedGrantsById: grants(), ...options });
  const server = http.createServer(app.handleRequest);
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  t.after(() => new Promise(resolve => server.close(resolve)));
  const url = `http://127.0.0.1:${server.address().port}`;
  return { ...state, app, url, async json(path, method = 'GET', body, requestHeaders = headers) {
    const response = await fetch(url + path, { method, headers: requestHeaders, ...(body === undefined ? {} : { body: JSON.stringify(body) }) });
    return { status: response.status, body: await response.json() };
  } };
}

test('default private mode refuses anonymous and forged dev identities on every private route', async t => {
  const { json, snapshots } = await withHub(t, { authAdapter: undefined });
  for (const [path, method] of [
    ['/projects', 'GET'], ['/projects/proj_a', 'GET'], ['/projects', 'POST'], ['/projects/proj_a/repositories', 'POST'],
    ...['organizations', 'repositories', 'proposals', 'review-comments', 'workflow-runs', 'policies', 'sync-repos'].map(c => [`/admin/${c}`, 'GET']),
    ['/admin/proposals/prop_a', 'GET'], ['/admin/proposals/prop_a/comments', 'GET'], ['/admin/proposals/prop_a/changes', 'GET'],
    ['/admin/proposals/prop_a', 'PATCH'], ['/admin/review-comments', 'POST'], ['/admin/workflow-runs', 'POST'],
    ['/collaboration/lane-submit', 'POST'], ['/collaboration/proposal-summary', 'GET'],
    ['/repo_a/refs', 'GET'], [`/repo_a/objects/${snapshots.a.blob}`, 'GET'], ['/repo_a/tree', 'GET'], ['/repo_a/files?path=private.txt', 'GET'],
    ['/repo_a/objects/missing', 'POST'], ['/repo_a/objects', 'POST'], ['/repo_a/refs/main', 'POST'],
  ]) {
    const result = await json(path, method, method === 'GET' ? undefined : {}, { 'x-sorrel-acting-principal': JSON.stringify(member) });
    assert.equal(result.status, 401, `${method} ${path}`);
    assert.equal(result.body.error.code, 'authentication_required');
  }
  for (const path of ['/healthz', '/capabilities', '/session']) assert.equal((await json(path, 'GET', undefined, {})).status, 200);
  assert.equal((await json('/session', 'GET', undefined, { 'x-sorrel-acting-principal': JSON.stringify(member) })).body.data.session, null);
});

test('verified sessions filter all metadata lists, nested comments and summary counts by actual scopes', async t => {
  const { json } = await withHub(t);
  for (const [path, ids] of [
    ['/projects', ['proj_a']], ['/admin/organizations', ['org_a']], ['/admin/repositories', ['repo_meta_a']],
    ['/admin/proposals', ['prop_a']], ['/admin/review-comments', ['comment_a']], ['/admin/workflow-runs', ['run_a']], ['/admin/policies', ['policy_a']],
    ['/admin/proposals/prop_a/comments', ['comment_a']],
  ]) {
    const result = await json(path);
    assert.equal(result.status, 200, path);
    assert.deepEqual(result.body.data.map(item => item.id), ids, path);
  }
  assert.deepEqual((await json('/projects?organizationId=org_b')).body.data, []);
  assert.deepEqual((await json('/projects?organizationId=org_absent')).body.data, []);
  assert.deepEqual((await json('/admin/proposals?projectId=proj_b')).body.data, []);
  assert.deepEqual((await json('/admin/sync-repos')).body.repos, [{ id: 'repo_a', refCount: 1 }]);
  const summary = (await json('/collaboration/proposal-summary')).body.data;
  assert.equal(summary.total, 1);
  assert.deepEqual(summary.byStatus, { open: 1 });
  assert.deepEqual(summary.open.map(p => p.id), ['prop_a']);
  assert.equal((await json('/admin/proposals/prop_a?include=comments')).body.data.comments[0].id, 'comment_a');
});

test('private individual records and orphans return the same response as missing records', async t => {
  const { json, store } = await withHub(t);
  for (const path of ['/projects/proj_b', '/admin/organizations/org_b', '/admin/repositories/repo_meta_b',
    '/admin/proposals/prop_b', '/admin/proposals/prop_b/comments', '/admin/proposals/prop_b/changes',
    '/admin/review-comments/comment_b', '/admin/workflow-runs/run_b', '/admin/policies/policy_b']) {
    const existing = await json(path);
    const absent = await json(path.replace(/_b(?=\/?|$)/, '_absent'));
    assert.equal(existing.status, 404, path);
    assert.deepEqual(existing, absent, path);
  }
  // Legacy persisted records can predate parent-validation fixes.
  store.proposals.set('orphan', { ...store.getProposal('prop_a'), id: 'orphan', projectId: 'proj_absent' });
  store.workflowRuns.set('cross_run', { ...store.getWorkflowRun('run_a'), id: 'cross_run', proposalId: 'prop_b' });
  store.proposals.set('cross_repo', { ...store.getProposal('prop_a'), id: 'cross_repo', repositoryId: 'repo_meta_b' });
  store.reviewComments.set('orphan_comment', { ...store.getReviewComment('comment_a'), id: 'orphan_comment', proposalId: 'cross_repo' });
  assert.equal((await json('/admin/proposals/orphan')).status, 404);
  assert.equal((await json('/admin/workflow-runs/cross_run')).status, 404);
  assert.equal((await json('/admin/review-comments/orphan_comment')).status, 404);
  assert.deepEqual((await json('/admin/review-comments')).body.data.map(c => c.id), ['comment_a']);
  assert.deepEqual((await json('/admin/workflow-runs')).body.data.map(r => r.id), ['run_a']);
});

test('all whole-object, ref, browser and missing negotiation reads require repo.read', async t => {
  const { json, snapshots } = await withHub(t);
  for (const suffix of ['a', 'b']) {
    for (const path of [`/repo_${suffix}/refs`, `/repo_${suffix}/objects/${snapshots[suffix].blob}`, `/repo_${suffix}/tree?ref=main`, `/repo_${suffix}/files?ref=main&path=private.txt`]) {
      const result = await json(path);
      assert.equal(result.status, suffix === 'a' ? 200 : 403, path);
    }
    assert.equal((await json(`/repo_${suffix}/objects/missing`, 'POST', { want: [snapshots[suffix].snapshot] })).status, suffix === 'a' ? 200 : 403);
  }
  assert.equal((await json('/admin/proposals/prop_a/changes')).status, 200);
  const narrowed = grants(); delete narrowed.repo_a;
  const other = await withHub(t, { trustedGrantsById: narrowed });
  assert.equal((await other.json('/admin/proposals/prop_a')).status, 200);
  assert.equal((await other.json('/admin/proposals/prop_a/changes')).status, 403);
});

test('writes use stored parents and reject forged identity, parent moves and cross-scope links', async t => {
  const { json, store, snapshots } = await withHub(t);
  const spoof = { ...headers, 'x-sorrel-acting-principal': JSON.stringify({ type: 'user', id: 'administrator' }) };
  const created = await json('/admin/proposals', 'POST', { projectId: 'proj_a', title: 'New', authorPrincipal: { type: 'user', id: 'forged' } }, spoof);
  assert.equal(created.status, 201);
  assert.deepEqual(created.body.data.authorPrincipal, member);
  assert.equal(created.body.data.authorRef, 'user:member');
  assert.equal((await json('/admin/proposals/prop_b', 'PATCH', { projectId: 'proj_a', id: 'prop_a', title: 'Stolen' }, spoof)).status, 404);
  assert.equal(store.getProposal('prop_b').title, 'Proposal b');
  assert.equal((await json('/admin/proposals/prop_a', 'PATCH', { projectId: 'proj_b' })).body.error.code, 'immutable_parent');
  assert.equal((await json('/admin/proposals/prop_a', 'PATCH', { repositoryId: 'repo_meta_b' })).status, 404);
  assert.equal((await json('/admin/proposals', 'POST', { projectId: 'proj_a', repositoryId: 'repo_meta_b', title: 'Cross repo' })).status, 404);
  assert.equal((await json('/admin/workflow-runs', 'POST', { projectId: 'proj_a', proposalId: 'prop_b', name: 'Cross proposal' })).status, 404);
  assert.equal((await json('/projects/proj_a/repositories', 'POST', { syncRepoId: 'repo_b' })).status, 403);
  assert.deepEqual(store.getProject('proj_a').repositoryIds, []);
  const wrongOrganization = await json('/admin/repositories', 'POST', { projectId: 'proj_a', organizationId: 'org_b', provider: 'sorrel', owner: 'local', name: 'Wrong parent' });
  assert.equal(wrongOrganization.status, 400);
  assert.equal(wrongOrganization.body.error.code, 'model_validation_failed');
  assert.equal((await json('/projects/proj_b/repositories', 'POST', { syncRepoId: 'repo_a' })).status, 404);
  assert.equal((await json('/repo_b/refs/main', 'POST', { snapshot: snapshots.a.snapshot, grantRefs: [] })).status, 403);
  assert.equal(store.sync.getRef('repo_b', 'main'), snapshots.b.snapshot);
  assert.equal((await json('/admin/proposals/prop_a', 'PATCH', { status: 'approved' })).status, 200);
  assert.equal((await json('/admin/review-comments/comment_a', 'PATCH', { body: 'Edited' })).status, 200);
  assert.equal((await json('/admin/workflow-runs/run_a', 'PATCH', { status: 'in_progress' })).status, 200);
  assert.equal((await json('/admin/workflow-runs/run_a', 'PATCH', { status: 'succeeded' })).status, 200);
});

test('deny, expiry, revocation and missing capabilities block reads and writes independently', async t => {
  for (const extra of [{ effect: 'deny' }, { expiresAt: '2000-01-01T00:00:00Z' }, { status: 'revoked' }]) {
    const trusted = grants();
    trusted.project_a = grant('project_a', 'project', 'proj_a', ['project.read', 'proposal.read', 'proposal.write'], extra);
    const { json, store } = await withHub(t, { trustedGrantsById: trusted });
    assert.deepEqual((await json('/projects')).body.data, []);
    assert.equal((await json('/admin/proposals/prop_a')).status, 404);
    assert.equal((await json('/admin/proposals/prop_a', 'PATCH', { title: 'Denied' })).status, 404);
    assert.equal(store.getProposal('prop_a').title, 'Proposal a');
  }
  const trusted = grants();
  trusted.project_a.capabilities = ['project.read', 'proposal.read'];
  const { json, store } = await withHub(t, { trustedGrantsById: trusted });
  assert.equal((await json('/admin/proposals/prop_a')).status, 200);
  assert.equal((await json('/admin/proposals/prop_a', 'PATCH', { title: 'Denied' })).status, 403);
  assert.equal(store.getProposal('prop_a').title, 'Proposal a');
  trusted.project_a.capabilities = ['project.read', 'proposal.read', 'proposal.write'];
  assert.equal((await json('/admin/proposals/prop_a', 'PATCH', { status: 'approved' })).status, 403);
  assert.equal(store.getProposal('prop_a').status, 'open');
});

test('list filtering propagates evaluator failures and ignores forged authorization payloads', async t => {
  const invalid = grants(); invalid.project_a.conditions = { unsupported: true };
  const { json } = await withHub(t, { trustedGrantsById: invalid });
  assert.equal((await json('/projects')).body.error.code, 'policy_evaluation_failed');
  const none = await withHub(t, { trustedGrantsById: {} });
  const injected = await none.json('/admin/proposals', 'POST', { projectId: 'proj_a', title: 'Self grant', grants: Object.values(grants()),
    policies: [{ schemaVersion: 'sorrel.protocol.v0', kind: 'Policy', id: 'self', resource: { kind: 'project', id: '*' }, rules: [], defaultDecision: 'allow' }] });
  assert.equal(injected.status, 404);
  assert.equal(none.store.listProposals().length, 2);
});

test('local demo is explicit and still requires actual grants; non-dev sessions remain authoritative', async t => {
  const demo = await withHub(t, { authAdapter: undefined, env: { SORREL_HUB_LOCAL_DEMO: '1' }, trustedGrantsById: { ...createLocalDemoGrants(), ...createLocalBootstrapGrants() } });
  assert.equal((await demo.json('/projects', 'GET', undefined, {})).body.data.length, 2);
  assert.equal((await demo.json('/session', 'GET', undefined, {})).body.data.session.principal.id, 'local');
  const unprovisioned = await withHub(t, { authAdapter: undefined, env: { SORREL_HUB_LOCAL_DEMO: '1' }, trustedGrantsById: {} });
  assert.deepEqual((await unprovisioned.json('/projects', 'GET', undefined, {})).body.data, []);
  assert.equal((await unprovisioned.json('/projects', 'POST', { organizationId: 'org_a', name: 'Ungrantable' }, {})).status, 403);
  const verified = await withHub(t, { env: { SORREL_HUB_LOCAL_DEMO: '1' } });
  assert.equal((await verified.json('/projects', 'GET', undefined, {})).status, 401);
});


test('verified creation and SDK calls use exact Core scopes and server attribution', async t => {
  const trusted = grants();
  trusted.org_new = grant('org_new', 'org', 'org_new', ['org.read', 'org.write']);
  trusted.repo_new = grant('repo_new', 'repo', 'repo_new', ['repo.read']);
  const { json, url } = await withHub(t, { trustedGrantsById: trusted });
  const organization = await json('/admin/organizations', 'POST', { id: 'org_new', name: 'New', ownerPrincipal: { type: 'user', id: 'forged' } });
  assert.equal(organization.status, 201);
  assert.deepEqual(organization.body.data.ownerPrincipal, member);
  const project = await json('/projects', 'POST', { id: 'proj_new', organizationId: 'org_a', name: 'New', createdByPrincipal: { type: 'user', id: 'forged' } });
  assert.equal(project.status, 201);
  assert.deepEqual(project.body.data.createdByPrincipal, member);
  const repository = await json('/admin/repositories', 'POST', { id: 'repo_new', projectId: 'proj_a', organizationId: 'org_a', provider: 'sorrel', owner: 'local', name: 'New' });
  assert.equal(repository.status, 201);
  assert.deepEqual(repository.body.data.linkedByPrincipal, member);
  assert.equal((await json('/admin/repositories/repo_new')).status, 200);
  const policy = await json('/admin/policies', 'POST', { projectId: 'proj_a', organizationId: 'org_a', name: 'New' });
  assert.equal(policy.status, 201);
  assert.equal((await json('/admin/policies/' + policy.body.data.id)).status, 200);
  const client = new HubClient({ baseUrl: url, accessToken: 'member', principal: { type: 'user', id: 'forged' } });
  assert.deepEqual((await client.listProjects()).data.map(p => p.id), ['proj_a']);
  const comment = await client.createReviewComment({ proposalId: 'prop_a', body: 'Verified SDK', authorRef: 'user:forged' });
  assert.deepEqual(comment.data.authorPrincipal, member);
  const run = await json('/admin/workflow-runs', 'POST', { projectId: 'proj_a', proposalId: 'prop_a', name: 'Verified runner' });
  assert.equal(run.status, 201);
  assert.deepEqual(run.body.data.requestedByPrincipal, member);
  const lane = await client.laneSubmit({ projectId: 'proj_a', syncRepoId: 'repo_a', title: 'Verified lane', sourceLane: 'feature', sourceSnapshot: 'aa'.repeat(32), authorRef: 'user:forged' });
  assert.equal(lane.data.authorRef, 'user:member');
  assert.deepEqual(lane.data.authorPrincipal, member);
  await assert.rejects(client.getProject('proj_b'), error => error.status === 404);
  await assert.rejects(client.listRefs('repo_b'), error => error.status === 403);
});

test('a missing Core process fails list requests with 503 instead of a partial empty success', async t => {
  const original = process.env.SORREL_HUB_CORE_POLICY_BIN;
  t.after(() => { if (original === undefined) delete process.env.SORREL_HUB_CORE_POLICY_BIN; else process.env.SORREL_HUB_CORE_POLICY_BIN = original; });
  process.env.SORREL_HUB_CORE_POLICY_BIN = '/nonexistent-sorrel-core-policy-fixture';
  const { json } = await withHub(t);
  const result = await json('/projects');
  assert.equal(result.status, 503);
  assert.equal(result.body.error.code, 'policy_evaluation_failed');
  assert.equal(result.body.data, undefined);
});
