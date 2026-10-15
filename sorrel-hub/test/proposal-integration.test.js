import assert from 'node:assert/strict';
import fs from 'node:fs';
import http from 'node:http';
import os from 'node:os';
import path from 'node:path';
import { spawn, spawnSync } from 'node:child_process';
import { once } from 'node:events';
import test from 'node:test';
import { createApp } from '../src/app.js';
import { acquireDatasetWriter, coreIntegration, integrationBinary } from '../src/core-integration.js';
import { createFsMetadataStore } from '../src/fs-metadata-store.js';
import { createFsRepoSyncStore } from '../src/fs-sync-store.js';

const identity = id => ({ type: id === 'ci' ? 'service' : 'user', id });
function grants() {
  return Object.fromEntries(['author', 'reviewer', 'ci', 'intruder'].flatMap(id => [
    ['project', id === 'intruder' ? ['proposal.read'] : ['project.read', 'proposal.read', 'proposal.write', 'proposal.review', 'workflow.run.read', 'workflow.run.write', 'review.comment.read', 'review.comment.write']],
    ['repo', id === 'intruder' ? ['repo.read'] : ['repo.read', 'repo.object.write', 'repo.ref.write', 'proposal.integrate', ...(id === 'ci' ? ['proposal.check.report'] : [])]],
  ].map(([kind, capabilities]) => {
    const key = `${id}_${kind}`;
    return [key, { id: key, schemaVersion: 'sorrel.protocol.v0', kind: 'Grant', principal: { kind: identity(id).type, id }, capabilities, resource: { kind, id: '*' }, effect: 'allow' }];
  })));
}
const requirements = () => ({ repo_test: { version: 1, checks: [{ id: 'test', reporter: identity('ci') }] } });
const executable = path.join(path.dirname(integrationBinary()), 'examples', 'proposal-integration-fixture');

async function fixture(t, options = {}) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'sorrel-proposal-'));
  const generated = spawnSync(executable, [root], { encoding: 'utf8' });
  assert.equal(generated.status, 0, generated.stderr);
  const ids = JSON.parse(generated.stdout);
  const data = path.join(root, 'sync');
  const metadata = path.join(root, 'metadata');
  const lease = await acquireDatasetWriter([data, metadata]);
  const sync = createFsRepoSyncStore(data);
  for (const shard of fs.readdirSync(path.join(root, 'core/objects'))) {
    for (const rest of fs.readdirSync(path.join(root, 'core/objects', shard))) {
      sync.put('repo_test', fs.readFileSync(path.join(root, 'core/objects', shard, rest)), shard + rest);
    }
  }
  sync.setRef('repo_test', 'source', ids.source);
  sync.setRef('repo_test', 'main', options.target ?? ids.base);
  const store = createFsMetadataStore(metadata, { sync });
  store.createProject({ id: 'project', organizationId: 'org', name: 'Project', repositoryIds: ['repo_test'] });
  const trustedGrantsById = grants();
  const requiredChecks = options.noChecks ? {} : requirements();
  const authAdapter = { mode: 'oidc', async resolveSession(req) {
    const id = req.headers.authorization?.replace('Bearer ', '');
    if (!['author', 'reviewer', 'ci', 'intruder'].includes(id)) return null;
    return { principal: identity(id), sessionId: id, authMode: 'oidc' };
  } };
  let app = createApp({ store, datasetLease: lease, authAdapter, browserAuth: null, trustedGrantsById, requiredChecks, integrationFault: options.integrationFault });
  await app.ready;
  let server = http.createServer(app.handleRequest);
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  let baseUrl = `http://127.0.0.1:${server.address().port}`;
  async function call(route, body, actor = 'author', method) {
    const response = await fetch(`${baseUrl}${route}`, { method: method ?? (body === undefined ? 'GET' : 'POST'), headers: { ...(actor ? { authorization: `Bearer ${actor}` } : {}), 'content-type': 'application/json' }, ...(body === undefined ? {} : { body: JSON.stringify(body) }) });
    const value = await response.json();
    return { status: response.status, ...value };
  }
  const created = await call('/admin/proposals', { id: 'proposal', projectId: 'project', syncRepoId: 'repo_test', title: 'Real integration', sourceLane: 'source', targetLane: 'main', status: 'open', authorPrincipal: identity('reviewer'), metadata: { checks: 'fake' } });
  assert.equal(created.status, 201, JSON.stringify(created));
  assert.deepEqual(created.data.authorPrincipal, identity('author'));
  const endpoint = '/admin/proposals/proposal';
  const prepare = async () => {
    const result = await call(`${endpoint}/integration`, {});
    assert.equal(result.status, 201, JSON.stringify(result));
    return result.data.binding;
  };
  const approve = b => call(`${endpoint}/approvals`, { bindingId: b.id, candidateSnapshot: b.candidateSnapshot }, 'reviewer');
  const report = (b, override = {}, actor = 'ci') => call(`${endpoint}/check-reports`, { bindingId: b.id, candidateSnapshot: b.candidateSnapshot, sourceSnapshot: b.inputs.sourceSnapshot, targetSnapshot: b.inputs.targetSnapshot, requirementsDigest: b.requirementsDigest, checkId: 'test', attempt: 1, runId: 'run-1', outcome: 'succeeded', ...override }, actor);
  const integrate = (b, key = 'operation-1') => call(`${endpoint}/integrate`, { bindingId: b.id, expectedTargetSnapshot: b.inputs.targetSnapshot, idempotencyKey: key });
  const readiness = () => call(`${endpoint}/integration`);
  async function stop() { await new Promise(resolve => server.close(resolve)); await app.close(); }
  async function restart() {
    await stop();
    const nextLease = await acquireDatasetWriter([data, metadata]);
    app = createApp({ store: createFsMetadataStore(metadata, { sync: createFsRepoSyncStore(data) }), datasetLease: nextLease, authAdapter, browserAuth: null, trustedGrantsById, requiredChecks });
    await app.ready;
    server = http.createServer(app.handleRequest);
    await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
    baseUrl = `http://127.0.0.1:${server.address().port}`;
  }
  t.after(async () => { await stop(); fs.rmSync(root, { recursive: true, force: true }); });
  return { root, data, metadata, ids, store, sync, call, prepare, approve, report, integrate, readiness, restart, trustedGrantsById, requiredChecks, endpoint };
}

test('real Core candidate requires explicit non-author approval and trusted exact-candidate tests; publishes atomic receipt', async t => {
  const f = await fixture(t);
  const b = await f.prepare();
  assert.equal(b.inputs.targetSnapshot, f.ids.base);
  assert.notEqual(b.candidateSnapshot, f.ids.source);
  const snapshot = JSON.parse(f.sync.get('repo_test', b.candidateSnapshot));
  assert.deepEqual(snapshot.parents.map(p => p.id), [f.ids.base, f.ids.source]);
  assert.equal((await f.integrate(b)).status, 409);
  assert.equal((await f.call(`${f.endpoint}/approvals`, { bindingId: b.id, candidateSnapshot: b.candidateSnapshot })).status, 409);
  assert.equal((await f.call(`${f.endpoint}/approvals`, { bindingId: b.id, candidateSnapshot: b.candidateSnapshot }, null)).status, 401);
  assert.equal((await f.approve(b)).status, 201);
  assert.equal((await f.report(b, {}, 'intruder')).status, 409);
  assert.equal((await f.report(b, { candidateSnapshot: f.ids.source })).status, 409);
  assert.equal((await f.report(b, { targetSnapshot: f.ids.source })).status, 409);
  assert.equal((await f.report(b, { sourceSnapshot: f.ids.base })).status, 409);
  assert.equal((await f.report(b, { requirementsDigest: 'wrong' })).status, 409);
  assert.equal((await f.report(b)).status, 201);
  assert.equal((await f.readiness()).data.ready, true);
  const result = await f.integrate(b);
  assert.equal(result.status, 200, JSON.stringify(result));
  assert.equal(result.data.state, 'integrated');
  assert.equal(f.sync.getRef('repo_test', 'main'), b.candidateSnapshot);
  assert.deepEqual(f.sync.getRefRecord('repo_test', 'main').receipt, result.data.receipt);
  assert.deepEqual((await f.integrate(b)).data, result.data);
  assert.equal((await f.integrate({ ...b, id: 'wrong' })).status, 409);
  await f.restart();
  assert.deepEqual((await f.integrate(b)).data, result.data);
  assert.equal((await f.readiness()).data.state, 'integrated');
});

test('unconfigured test reporter blocks despite old approved status and fabricated workflow metadata', async t => {
  const f = await fixture(t, { noChecks: true });
  await f.call(f.endpoint, { status: 'approved', metadata: { tests: 'succeeded', approvals: ['reviewer'] } }, 'author', 'PATCH');
  const b = await f.prepare();
  assert.equal((await f.approve(b)).status, 201);
  assert.equal((await f.report(b)).status, 409);
  const state = (await f.readiness()).data;
  assert.ok(state.blockers.includes('trusted_required_checks_not_configured'));
  assert.equal((await f.integrate(b)).status, 409);
  assert.equal(f.sync.getRef('repo_test', 'main'), f.ids.base);
});

test('all editable review inputs and requirements revisions invalidate evidence, including revert', async t => {
  const f = await fixture(t);
  for (const edit of [{ title: 'new title' }, { description: 'new description' }, { metadata: { arbitrary: { nested: true } } }, { workflowRunIds: [] }]) {
    const b = await f.prepare(); await f.approve(b); await f.report(b);
    assert.equal((await f.call(f.endpoint, edit, 'author', 'PATCH')).status, 200);
    assert.equal((await f.integrate(b, JSON.stringify(edit))).status, 409);
  }
  const b = await f.prepare(); await f.approve(b); await f.report(b);
  f.requiredChecks.repo_test.version += 1;
  assert.ok((await f.readiness()).data.blockers.includes('requirements_changed'));
  assert.equal((await f.integrate(b)).status, 409);
  assert.equal((await f.call(f.endpoint, { authorPrincipal: identity('reviewer') }, 'author', 'PATCH')).status, 400);
  assert.equal((await f.call(f.endpoint, { integrationReceipt: {} }, 'author', 'PATCH')).status, 400);
});

test('source/target moves and current reviewer/reporter/integrator revocation and denies stay blocking', async t => {
  const f = await fixture(t);
  let b = await f.prepare(); await f.approve(b); await f.report(b);
  f.sync.setRef('repo_test', 'source', f.ids.target);
  assert.ok((await f.readiness()).data.blockers.includes('superseded_source'));
  assert.equal((await f.integrate(b)).status, 409);
  f.sync.setRef('repo_test', 'source', f.ids.source);
  f.sync.setRef('repo_test', 'main', f.ids.target);
  assert.ok((await f.readiness()).data.blockers.includes('stale_target'));
  b = await f.prepare();
  assert.notEqual(b.id, (await f.prepare()).id);
  b = await f.prepare(); await f.approve(b); await f.report(b);
  for (const key of ['reviewer_project', 'ci_repo', 'author_repo']) {
    const old = f.trustedGrantsById[key]; delete f.trustedGrantsById[key];
    assert.ok([403, 409].includes((await f.integrate(b, `revoked-${key}`)).status));
    f.trustedGrantsById[key] = old;
  }
  f.trustedGrantsById.deny = { ...f.trustedGrantsById.author_repo, id: 'deny', effect: 'deny', capabilities: ['proposal.integrate'] };
  assert.equal((await f.integrate(b, 'effective-deny')).status, 409);
  f.trustedGrantsById.deny.effect = 'review';
  assert.equal((await f.integrate(b, 'needs-review')).status, 409);
});

test('new attempts supersede old success; terminal failure cannot be overwritten and run identity cannot be reused', async t => {
  const f = await fixture(t); const b = await f.prepare(); await f.approve(b);
  assert.equal((await f.report(b)).status, 201);
  assert.equal((await f.report(b, { attempt: 2, runId: 'run-2', outcome: 'pending' })).status, 201);
  assert.equal((await f.integrate(b)).status, 409);
  assert.equal((await f.report(b, { attempt: 1 })).status, 409);
  assert.equal((await f.report(b, { attempt: 2, runId: 'run-2', outcome: 'failed' })).status, 201);
  assert.equal((await f.report(b, { attempt: 2, runId: 'run-2', outcome: 'succeeded' })).status, 409);
  assert.equal((await f.report(b, { attempt: 3, runId: 'run-1' })).status, 409);
  assert.equal((await f.report(b, { attempt: 3, runId: 'run-3' })).status, 201);
  assert.equal((await f.integrate(b)).data.state, 'integrated');
});

test('real Core divergent merge publishes both parents; content/add-add/delete/binary/file-directory conflicts never advance refs', async t => {
  const f = await fixture(t);
  f.sync.setRef('repo_test', 'main', f.ids.target);
  let b = await f.prepare(); assert.equal(b.conflicts.length, 0);
  await f.approve(b); await f.report(b);
  assert.equal((await f.integrate(b)).data.state, 'integrated');
  const snapshot = JSON.parse(f.sync.get('repo_test', b.candidateSnapshot));
  assert.deepEqual(snapshot.parents.map(p => p.id), [f.ids.target, f.ids.source]);
  for (const [source, target, type] of [['source', 'conflict', 'content'], ['add1', 'add2', 'add_add'], ['source', 'deleted', 'modify_delete'], ['binary1', 'binary2', 'binary'], ['file', 'directory', 'file_directory']]) {
    const g = await fixture(t);
    g.sync.setRef('repo_test', 'source', g.ids[source]); g.sync.setRef('repo_test', 'main', g.ids[target]);
    b = await g.prepare(); assert.equal(b.candidateSnapshot, null);
    assert.ok(b.conflicts.some(c => c.conflictType === type || c.type === type), JSON.stringify(b.conflicts));
    assert.equal((await g.integrate(b)).status, 409);
    assert.equal(g.sync.getRef('repo_test', 'main'), g.ids[target]);
    assert.equal(g.store.getProposal('proposal').status, 'open');
  }
});

for (const stage of ['journal', 'ref', 'metadata']) test(`restart recovers the ${stage} fault without false integration`, async t => {
  let injected = false;
  const f = await fixture(t, { integrationFault(current) { if (current === stage && !injected) { injected = true; throw new Error('injected fault'); } } });
  const b = await f.prepare(); await f.approve(b); await f.report(b);
  assert.equal((await f.integrate(b)).status, 500);
  await f.restart();
  const retry = await f.integrate(b);
  assert.equal(retry.status, 200, JSON.stringify(retry));
  assert.equal(retry.data.state, stage === 'journal' ? 'not_published' : 'integrated');
  assert.equal(f.sync.getRef('repo_test', 'main'), stage === 'journal' ? f.ids.base : b.candidateSnapshot);
});

test('ordinary push and integration serialize their target CAS; same-key concurrent publication is idempotent', async t => {
  const f = await fixture(t); const b = await f.prepare(); await f.approve(b); await f.report(b);
  const results = await Promise.all([f.integrate(b), f.integrate(b), f.call('/repo_test/refs/main', { snapshot: f.ids.target, expected: f.ids.base, grantRefs: [] })]);
  if (results[0].status === 200) { assert.deepEqual(results[0].data, results[1].data); assert.notEqual(results[2].status, 200); }
  else { assert.equal(results[2].status, 200); assert.equal(f.sync.getRef('repo_test', 'main'), f.ids.target); }
});

test('missing and digest-corrupt terminal blobs and candidate tampering cannot publish', async t => {
  const f = await fixture(t); let b = await f.prepare(); await f.approve(b); await f.report(b);
  const snapshot = JSON.parse(f.sync.get('repo_test', f.ids.source));
  const tree = JSON.parse(f.sync.get('repo_test', snapshot.rootTree.id));
  const id = tree.entries.find(e => e.type === 'file').object.id;
  const file = path.join(f.data, 'repo_test', 'objects', id.slice(0, 2), id);
  const good = fs.readFileSync(file); fs.writeFileSync(file, 'corrupt');
  assert.equal((await f.integrate(b)).status, 409);
  fs.unlinkSync(file);
  assert.equal((await f.call(`${f.endpoint}/integration`, {})).status, 409);
  fs.writeFileSync(file, good);
  const record = f.store.proposalBindings.get('proposal'); record.binding.candidateSnapshot = f.ids.source;
  assert.equal((await f.integrate(record.binding)).status, 409);
  assert.equal(f.sync.getRef('repo_test', 'main'), f.ids.base);
});

test('two real processes exclude canonical aliases and release every root on close and parent death', async t => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'sorrel-lock-')); t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  const first = path.join(root, 'sync'); const second = path.join(root, 'metadata');
  const lease = await acquireDatasetWriter([first, second]);
  const alias = path.join(root, 'alias'); fs.symlinkSync(first, alias);
  await assert.rejects(acquireDatasetWriter([alias]), /already has a Hub writer/);
  await assert.rejects(acquireDatasetWriter([second]), /already has a Hub writer/);
  await lease.close();
  const next = await acquireDatasetWriter([alias]); await next.close();
  const module = new URL('../src/core-integration.js', import.meta.url).href;
  const parent = spawn(process.execPath, ['--input-type=module', '-e', `import { acquireDatasetWriter } from ${JSON.stringify(module)}; await acquireDatasetWriter([${JSON.stringify(first)}]); console.log('ready'); setInterval(()=>{},1000);`], { stdio: ['ignore', 'pipe', 'pipe'] });
  await once(parent.stdout, 'data'); parent.kill('SIGKILL'); await once(parent, 'close');
  let reopened;
  for (let i = 0; i < 20; i++) {
    try { reopened = await acquireDatasetWriter([first]); break; } catch { await new Promise(resolve => setTimeout(resolve, 20)); }
  }
  assert.ok(reopened); await reopened.close();
});

test('malformed/future bridge inputs and wire overflow fail closed', async () => {
  await assert.rejects(coreIntegration({ operation: 'prepare', unknown: true }), /missing field|unknown field/);
  await assert.rejects(coreIntegration({ operation: 'probe', input: 'x'.repeat(2 * 1024 * 1024) }), /safety limit/);
});

test('linked workflow and comment edits invalidate approvals; legacy authors cannot auto-upgrade', async t => {
  const f = await fixture(t);
  const run = await f.call('/admin/workflow-runs', { id: 'run', projectId: 'project', proposalId: 'proposal', name: 'Editable metadata', status: 'succeeded' });
  assert.equal(run.status, 201);
  await f.call(f.endpoint, { workflowRunIds: ['run'] }, 'author', 'PATCH');
  let b = await f.prepare(); await f.approve(b); await f.report(b);
  assert.equal((await f.call('/admin/workflow-runs/run', { metadata: { fakeTests: 'succeeded' } }, 'author', 'PATCH')).status, 200);
  assert.equal((await f.integrate(b)).status, 409);
  b = await f.prepare(); await f.approve(b); await f.report(b);
  assert.equal((await f.call('/admin/review-comments', { proposalId: 'proposal', body: 'New review input' })).status, 201);
  assert.equal((await f.integrate(b, 'comment-change')).status, 409);
  f.store.proposalAuthors.delete('proposal');
  assert.equal((await f.call(`${f.endpoint}/integration`, {})).status, 409);
});

test('moving refs away and back still invalidates their generation-bound evidence', async t => {
  const f = await fixture(t); const b = await f.prepare(); await f.approve(b); await f.report(b);
  f.sync.setRef('repo_test', 'source', f.ids.target); f.sync.setRef('repo_test', 'source', f.ids.source);
  assert.ok((await f.readiness()).data.blockers.includes('superseded_source'));
  f.sync.setRef('repo_test', 'main', f.ids.target); f.sync.setRef('repo_test', 'main', f.ids.base);
  assert.ok((await f.readiness()).data.blockers.includes('stale_target'));
  assert.equal((await f.integrate(b)).status, 409);
});

for (const afterRename of [false, true]) test(`native filesystem barrier failure ${afterRename ? 'after' : 'before'} ref rename recovers from receipt evidence`, async t => {
  let store;
  const f = await fixture(t, { integrationFault(stage) { if (stage === 'journal') store.writer.injectFault(path.join(f.data, 'repo_test/refs/main'), afterRename ? 'after_rename' : 'before_rename'); } });
  store = f.store;
  const b = await f.prepare(); await f.approve(b); await f.report(b);
  assert.equal((await f.integrate(b)).status, 500);
  assert.equal(f.store.getProposal('proposal').status, 'open');
  assert.equal(f.sync.getRef('repo_test', 'main'), afterRename ? b.candidateSnapshot : f.ids.base);
  await f.restart();
  assert.equal((await f.integrate(b)).data.state, afterRename ? 'integrated' : 'not_published');
});

test('a target equal to candidate without the journal receipt is contradictory, never recovered as integrated', async t => {
  const f = await fixture(t, { integrationFault(stage) { if (stage === 'journal') throw new Error('stop before publication'); } });
  const b = await f.prepare(); await f.approve(b); await f.report(b);
  assert.equal((await f.integrate(b)).status, 500);
  f.sync.setRef('repo_test', 'main', b.candidateSnapshot);
  await assert.rejects(f.restart(), /contradictory ref evidence/);
  assert.equal(f.sync.getRef('repo_test', 'main'), b.candidateSnapshot);
  assert.equal(f.store.getProposal('proposal').status, 'open');
});

test('native bridge timeout/output limits terminate the process and fail closed', { timeout: 20000 }, async t => {
  if (process.platform === 'win32') return t.skip('executable fixture uses a POSIX shebang');
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'sorrel-bridge-limits-'));
  const old = process.env.SORREL_HUB_CORE_INTEGRATION_BIN;
  t.after(() => { if (old === undefined) delete process.env.SORREL_HUB_CORE_INTEGRATION_BIN; else process.env.SORREL_HUB_CORE_INTEGRATION_BIN = old; fs.rmSync(root, { recursive: true, force: true }); });
  const binary = path.join(root, 'bridge'); process.env.SORREL_HUB_CORE_INTEGRATION_BIN = binary;
  fs.writeFileSync(binary, '#!/usr/bin/env node\nprocess.stdout.write("x".repeat(3*1024*1024));setInterval(()=>{},1000);\n', { mode: 0o700 });
  await assert.rejects(coreIntegration({ operation: 'probe' }), /output exceeds/);
  fs.writeFileSync(binary, '#!/usr/bin/env node\nsetInterval(()=>{},1000);\n', { mode: 0o700 });
  await assert.rejects(coreIntegration({ operation: 'probe' }), /timed out/);
});

test('actual Hub processes refuse overlapping dataset aliases before announcing readiness', { timeout: 15000 }, async t => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'sorrel-hub-writers-'));
  const children = [];
  t.after(async () => {
    for (const child of children) if (child.exitCode === null && child.signalCode === null) { const exited = once(child, 'close'); child.kill('SIGKILL'); await exited; }
    fs.rmSync(root, { recursive: true, force: true });
  });
  const data = path.join(root, 'sync'); const metadata = path.join(root, 'metadata');
  const script = new URL('../scripts/listen.mjs', import.meta.url).pathname;
  function launch(syncRoot = data) {
    const child = spawn(process.execPath, [script], { env: { ...process.env,
      SORREL_HUB_SYNC_STORE: 'fs', SORREL_HUB_DATA_DIR: syncRoot, SORREL_HUB_METADATA_DIR: metadata,
      SORREL_HUB_AUTH: 'dev', SORREL_HUB_LOCAL_DEMO: '0', SORREL_HUB_BOOTSTRAP_GRANTS: '0',
      SORREL_HUB_TRUSTED_GRANTS_FILE: '', SORREL_HUB_TRUSTED_POLICIES_FILE: '', SORREL_HUB_REQUIRED_CHECKS_JSON: '{}',
    }, stdio: ['ignore', 'pipe', 'pipe'] });
    children.push(child);
    return child;
  }
  async function ready(child) {
    return await new Promise((resolve, reject) => {
      child.once('error', reject);
      child.once('exit', code => reject(new Error(`fixture Hub exited before readiness: ${code}`)));
      child.stdout.once('data', chunk => resolve(JSON.parse(String(chunk))));
    });
  }
  const first = launch(); assert.match((await ready(first)).url, /^http:\/\/127\.0\.0\.1:/);
  const alias = path.join(root, 'alias'); fs.symlinkSync(data, alias);
  const second = launch(alias); let announced = false; second.stdout.on('data', () => { announced = true; }); second.stderr.resume();
  assert.notEqual((await once(second, 'close'))[0], 0);
  assert.equal(announced, false);
  const exited = once(first, 'close'); first.kill('SIGTERM'); assert.equal((await exited)[0], 0);
  const replacement = launch(alias); assert.match((await ready(replacement)).url, /^http:\/\/127\.0\.0\.1:/);
  const replaced = once(replacement, 'close'); replacement.kill('SIGTERM'); assert.equal((await replaced)[0], 0);
});

test('readiness omits private review records while a least-privileged reporter can attest', async t => {
  const f = await fixture(t);
  const comment = await f.call('/admin/review-comments', { proposalId: 'proposal', body: 'private-review-body' });
  assert.equal(comment.status, 201);
  const b = await f.prepare();
  assert.equal(b.inputs.review, undefined);
  const ci = f.trustedGrantsById.ci_project;
  ci.capabilities = ['proposal.read'];
  const state = await f.call(`${f.endpoint}/integration`, undefined, 'ci');
  assert.equal(state.status, 200);
  assert.equal(state.data.binding.inputs.review, undefined);
  assert.equal(JSON.stringify(state).includes('private-review-body'), false);
  assert.deepEqual((await f.call(`${f.endpoint}/comments`, undefined, 'ci')).data, []);
  assert.equal((await f.report(b)).status, 201);
});

test('stale binding readiness and candidate previews authorize the bound repository', async t => {
  for (const published of [false, true]) await t.test(`published=${published}`, async t => {
    const f = await fixture(t); const b = await f.prepare();
    if (published) {
      await f.approve(b); await f.report(b);
      assert.equal((await f.integrate(b)).status, 200);
      assert.equal((await f.call(f.endpoint, { status: 'closed' }, 'author', 'PATCH')).status, 200);
    }
    f.store.linkProjectRepository('project', 'repo_other');
    f.sync.put('repo_other', Buffer.from('other content'));
    f.trustedGrantsById.intruder_repo.resource.id = 'repo_other';
    assert.equal((await f.call(f.endpoint, { syncRepoId: 'repo_other' }, 'author', 'PATCH')).status, 200);
    assert.equal((await f.call('/repo_test/tree?ref=main', undefined, 'intruder')).status, 403);
    for (const route of [`${f.endpoint}/integration`, `${f.endpoint}/integration-changes?bindingId=${b.id}`]) {
      const denied = await f.call(route, undefined, 'intruder');
      assert.ok([403, 404].includes(denied.status));
      assert.equal(denied.data, undefined);
    }
  });
});

test('closing during startup and failing hydration both release the real-process writer lease', async t => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'sorrel-close-startup-')); t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  for (const failure of [false, true]) {
    const dir = path.join(root, String(failure));
    const store = createFsMetadataStore(dir);
    if (failure) store.reload = () => { throw new Error('injected hydration failure'); };
    const app = createApp({ store, browserAuth: null });
    if (failure) { await assert.rejects(app.ready, /injected hydration/); await app.close(); }
    else { await app.close(); await app.ready; }
    const replacement = await acquireDatasetWriter([dir]); await replacement.close();
  }
});

test('helper death cannot leave integration, admin metadata or sync writes running under a new Hub owner', { timeout: 15000 }, async t => {
  let takeover;
  let f;
  t.after(async () => {
    if (takeover && takeover.exitCode === null && takeover.signalCode === null) { const exited = once(takeover, 'close'); takeover.kill('SIGTERM'); await exited; }
  });
  f = await fixture(t, { integrationFault(stage) {
    if (stage !== 'journal') return;
    process.kill(f.store.writer.pid, 'SIGKILL');
    const readyFile = path.join(f.root, 'takeover-ready'); const fd = fs.openSync(readyFile, 'w');
    takeover = spawn(process.execPath, [new URL('../scripts/listen.mjs', import.meta.url).pathname], {
      env: { ...process.env, SORREL_HUB_SYNC_STORE: 'fs', SORREL_HUB_DATA_DIR: f.data, SORREL_HUB_METADATA_DIR: f.metadata,
        SORREL_HUB_AUTH: 'dev', SORREL_HUB_LOCAL_DEMO: '0', SORREL_HUB_BOOTSTRAP_GRANTS: '0',
        SORREL_HUB_TRUSTED_GRANTS_FILE: '', SORREL_HUB_TRUSTED_POLICIES_FILE: '', SORREL_HUB_REQUIRED_CHECKS_JSON: '{}',
      }, stdio: ['ignore', fd, 'pipe'],
    }); fs.closeSync(fd); takeover.stderr.resume();
    const deadline = Date.now() + 5000;
    while (!fs.readFileSync(readyFile, 'utf8').includes('\n') && Date.now() < deadline) Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 10);
    assert.match(fs.readFileSync(readyFile, 'utf8'), /"url":/);
  } });
  const b = await f.prepare(); await f.approve(b); await f.report(b);
  assert.equal((await f.integrate(b)).status, 503);
  assert.throws(() => f.store.createProject({ id: 'forbidden-project', organizationId: 'org', name: 'Never write' }), /writer unavailable|writer lease lost/);
  assert.throws(() => f.sync.put('repo_test', Buffer.from('must-not-publish')), /writer unavailable|writer lease lost/);
  assert.throws(() => f.sync.setRef('repo_test', 'forbidden-ref', f.ids.source), /writer unavailable|writer lease lost/);
  assert.equal(f.sync.getRef('repo_test', 'main'), f.ids.base);
  assert.equal(f.sync.getRef('repo_test', 'forbidden-ref'), undefined);
  assert.equal(f.store.getProposal('proposal').status, 'open');
  assert.equal(fs.existsSync(path.join(f.metadata, 'projects/forbidden-project.json')), false);
  assert.equal(f.sync.has('repo_test', (await import('../src/blake3.js')).objectId(Buffer.from('must-not-publish'))), false);
});

test('abandoned native ref temporary files cannot become refs after restart', async t => {
  const f = await fixture(t);
  fs.writeFileSync(path.join(f.data, 'repo_test/refs/.tmp-native-123-456'), 'unpublished partial bytes');
  await f.restart();
  const refs = await f.call('/repo_test/refs');
  assert.equal(refs.status, 200);
  assert.deepEqual(refs.refs.map(ref => ref.name).sort(), ['main', 'source']);
});
