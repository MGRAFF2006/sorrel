#!/usr/bin/env node
/** Local process/OIDC protocol proof, NOT live WorkOS or hosted pilot evidence.
 * Run after npm run setup: node tests/e2e/pilot-workflow.mjs
 * Uses only disposable data, real CLI uploads/downloads, and actual candidate tests.
 */
import assert from 'node:assert/strict';
import { execFile, spawn } from 'node:child_process';
import { createSign, generateKeyPairSync, randomUUID } from 'node:crypto';
import { once } from 'node:events';
import { mkdtemp, mkdir, readFile, rm, writeFile } from 'node:fs/promises';
import http from 'node:http';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { createInterface } from 'node:readline';
import { fileURLToPath } from 'node:url';
import { promisify } from 'node:util';

const ROOT = fileURLToPath(new URL('../../', import.meta.url));
const executeFile = promisify(execFile);
const principal = id => ({ type: 'user', id: `oidc:${id}` });
const log = message => console.log(`✓ pilot workflow — ${message}`);
let stopActiveWorkflow = async () => {};
export const stopPilotWorkflow = () => stopActiveWorkflow();

// Never inherit real bearer tokens, operator grants, WorkOS/Convex config or Node hooks.
const cleanEnv = () => Object.fromEntries(Object.entries(process.env).filter(([key]) =>
  !/^(SORREL_|WORKOS_|CONVEX_|NODE_OPTIONS$|NODE_PATH$|PORT$|HOST$)/.test(key)));

async function startHub(env, signal) {
  const child = spawn(process.execPath, ['tests/e2e/pilot-hub.mjs'], {
    cwd: ROOT, env, signal, stdio: ['ignore', 'pipe', 'pipe'],
  });
  const closed = new Promise(resolve => child.once('close', (code, signal) => resolve({ code, signal })));
  let stderr = '';
  child.stderr.on('data', chunk => { stderr = (stderr + chunk).slice(-8000); });
  const lines = createInterface({ input: child.stdout });
  try {
    const url = await new Promise((resolve, reject) => {
      const timer = setTimeout(() => reject(new Error('Hub readiness timed out')), 15000);
      const fail = error => { clearTimeout(timer); reject(error); };
      child.once('error', fail);
      child.once('exit', () => fail(new Error(`Hub exited before readiness: ${stderr}`)));
      lines.once('line', line => {
        clearTimeout(timer);
        try { resolve(JSON.parse(line).url); } catch (error) { reject(error); }
      });
    });
    return { url, pid: child.pid, closed, async stop() {
      if (child.exitCode !== null || child.signalCode !== null) return closed;
      child.kill('SIGTERM');
      const timer = setTimeout(() => child.kill('SIGKILL'), 5000);
      try { return await closed; } finally { clearTimeout(timer); lines.close(); }
    } };
  } catch (error) { child.kill('SIGKILL'); await closed; lines.close(); throw error; }
}

export async function runPilotWorkflow() {
  const env = cleanEnv();
  const target = path.resolve(ROOT, env.CARGO_TARGET_DIR ?? 'target');
  const binary = path.join(target, 'debug', process.platform === 'win32' ? 'sorrel.exe' : 'sorrel');
  const aborter = new AbortController();
  const finished = Promise.withResolvers();
  const stop = async () => { aborter.abort(); await finished.promise; };
  stopActiveWorkflow = stop;
  const execute = (command, args, options) => executeFile(command, args, { ...options, signal: aborter.signal });
  let root;
  let hub;
  let jwks;
  try {
    await execute('cargo', ['build', '--locked', '-p', 'sorrel-cli', '-p', 'sorrel-core', '--bins'], {
      cwd: ROOT, env, timeout: 240000, maxBuffer: 4 * 1024 * 1024,
    });
    root = await mkdtemp(path.join(tmpdir(), 'sorrel-pilot-workflow-'));
    const work = path.join(root, 'author');
    const privateWork = path.join(root, 'other-team');
    await Promise.all([mkdir(work), mkdir(privateWork)]);
    const cli = async (cwd, args, actor = 'alice') => JSON.parse((await execute(binary, [...args, '--json'], {
      cwd, env: { ...env, SORREL_HUB_TOKEN: tokens[actor] }, timeout: 30000, maxBuffer: 1024 * 1024,
    })).stdout);
    // No network calls occur during init, before local test tokens exist.
    const tokens = {};
    const alpha = (await cli(work, ['init'])).repoId;
    const beta = (await cli(privateWork, ['init'])).repoId;

    const { privateKey, publicKey } = generateKeyPairSync('rsa', { modulusLength: 2048 });
    const key = { ...publicKey.export({ format: 'jwk' }), kid: 'local-pilot', alg: 'RS256', use: 'sig' };
    jwks = http.createServer((req, res) => {
      if (req.url !== '/jwks') return res.writeHead(404).end();
      res.writeHead(200, { 'content-type': 'application/json' }).end(JSON.stringify({ keys: [key] }));
    });
    jwks.listen(0, '127.0.0.1');
    await once(jwks, 'listening');
    const issuer = `http://127.0.0.1:${jwks.address().port}`;
    for (const id of ['alice', 'bob', 'ci', 'eve']) {
      const payload = [ { alg: 'RS256', kid: key.kid }, {
        sub: id, iss: issuer, aud: 'pilot-test', exp: Math.floor(Date.now() / 1000) + 1800,
      } ].map(value => Buffer.from(JSON.stringify(value)).toString('base64url')).join('.');
      tokens[id] = `${payload}.${createSign('RSA-SHA256').update(payload).sign(privateKey).toString('base64url')}`;
    }
    const grants = {};
    function grant(actor, kind, id, capabilities) {
      const name = `${actor}_${kind}`;
      grants[name] = { id: name, schemaVersion: 'sorrel.protocol.v0', kind: 'Grant',
        principal: { kind: 'user', id: principal(actor).id }, resource: { kind, id }, capabilities, effect: 'allow' };
    }
    for (const [actor, team, repo] of [['alice', 'alpha', alpha], ['bob', 'alpha', alpha], ['ci', 'alpha', alpha], ['eve', 'beta', beta]]) {
      if (actor !== 'ci') grant(actor, 'org', `org_${team}`, ['org.read', 'org.write', 'project.create']);
      grant(actor, 'project', `project_${team}`, actor === 'ci' ? ['proposal.read'] : [
        'project.read', 'project.write', 'proposal.read', 'proposal.write', 'proposal.review',
        'review.comment.read', 'review.comment.write', 'workflow.run.read', 'workflow.run.write',
      ]);
      grant(actor, 'repo', repo, actor === 'ci' ? ['repo.read', 'proposal.check.report'] : [
        'repo.read', 'repo.object.write', 'repo.ref.write', 'proposal.integrate',
      ]);
    }
    const grantsFile = path.join(root, 'grants.json');
    await writeFile(grantsFile, JSON.stringify(grants));
    const hubEnv = { ...env, SORREL_PILOT_TEST_ROOT: root, SORREL_HUB_AUTH: 'oidc',
      SORREL_OIDC_ISSUER: issuer, SORREL_OIDC_AUDIENCE: 'pilot-test', SORREL_OIDC_JWKS_URI: `${issuer}/jwks`,
      SORREL_HUB_TRUSTED_GRANTS_FILE: grantsFile,
      SORREL_HUB_REQUIRED_CHECKS_JSON: JSON.stringify({ [alpha]: { version: 1, checks: [{ id: 'test', reporter: principal('ci') }] } }),
    };
    hub = await startHub(hubEnv, aborter.signal);
    log(`disposable Hub ready ${JSON.stringify({ root, pid: hub.pid })}`);
    async function call(route, body, actor = 'alice', method, headers = {}) {
      const response = await fetch(`${hub.url}${route}`, {
        method: method ?? (body === undefined ? 'GET' : 'POST'), signal: AbortSignal.any([aborter.signal, AbortSignal.timeout(15000)]),
        headers: { 'content-type': 'application/json', ...(actor ? { authorization: `Bearer ${tokens[actor]}` } : {}), ...headers },
        ...(body === undefined ? {} : { body: JSON.stringify(body) }),
      });
      return { status: response.status, ...await response.json() };
    }
    function expect(result, status) { assert.equal(result.status, status, JSON.stringify(result)); return result; }
    const refs = async repo => (await call(`/${repo}/refs`, undefined, repo === beta ? 'eve' : 'alice')).refs;
    const tip = async () => (await refs(alpha)).find(ref => ref.name === 'main').snapshot;
    const project = async (team, actor, repo) => {
      expect(await call('/admin/organizations', { id: `org_${team}`, name: team }, actor), 201);
      expect(await call('/projects', { id: `project_${team}`, organizationId: `org_${team}`, name: team, repositoryIds: [repo] }, actor), 201);
    };
    await project('alpha', 'alice', alpha);
    await project('beta', 'eve', beta);
    expect(await call('/session', undefined, null), 200);
    expect(await call('/projects', undefined, null), 401);
    expect(await call('/projects', undefined, null, undefined, { authorization: 'Bearer invalid' }), 401);
    assert.deepEqual(expect(await call('/session', undefined, 'alice', undefined, {
      'x-sorrel-acting-principal': JSON.stringify(principal('eve')),
    }), 200).data.session.principal, principal('alice'));

    for (const [cwd, repo, actor] of [[work, alpha, 'alice'], [privateWork, beta, 'eve']]) {
      await cli(cwd, ['remote', 'add', 'origin', hub.url, '--repo-id', repo], actor);
    }
    await writeFile(path.join(work, 'value.mjs'), 'export const value = 0;\n');
    await writeFile(path.join(work, 'verify.mjs'), [
      "import assert from 'node:assert/strict';", "import { readFileSync } from 'node:fs';",
      "import { value } from './value.mjs';", 'assert.equal(value, 2);',
      "assert.equal(readFileSync('target.txt', 'utf8'), 'target\\n');",
      "assert.equal(readFileSync('source.txt', 'utf8'), 'source\\n');",
    ].join('\n'));
    const base = (await cli(work, ['change', 'create', '-m', 'base test fixture'])).object.resultingSnapshot.id;
    await cli(work, ['push', 'origin', '--ref', 'main']);
    const lane = (await cli(work, ['lane', 'create', '--name', 'source'])).object.id;
    await cli(work, ['lane', 'switch', lane]);
    await writeFile(path.join(work, 'source.txt'), 'source\n');
    let source = (await cli(work, ['change', 'create', '-m', 'source fixture'])).object.resultingSnapshot.id;
    await cli(work, ['push', 'origin', '--ref', 'source']);
    await cli(work, ['lane', 'switch', 'lane_main']);
    await writeFile(path.join(work, 'target.txt'), 'target\n');
    let targetSnapshot = (await cli(work, ['change', 'create', '-m', 'target fixture'])).object.resultingSnapshot.id;
    await cli(work, ['push', 'origin', '--ref', 'main']);
    assert.notEqual(base, targetSnapshot);
    await writeFile(path.join(privateWork, 'private.txt'), 'other-team-private\n');
    const privateSnapshot = (await cli(privateWork, ['change', 'create', '-m', 'private fixture'], 'eve')).object.resultingSnapshot.id;
    await cli(privateWork, ['push', 'origin', '--ref', 'main'], 'eve');
    const privateProposal = expect(await call('/admin/proposals', { id: 'proposal_beta', projectId: 'project_beta',
      syncRepoId: beta, sourceLane: 'main', targetLane: 'main', title: 'Private', status: 'open' }, 'eve'), 201).data;

    const proposal = expect(await call('/admin/proposals', { id: 'proposal_alpha', projectId: 'project_alpha', syncRepoId: alpha,
      title: 'Process workflow', sourceLane: 'source', targetLane: 'main', sourceSnapshot: source, targetSnapshot,
      authorPrincipal: principal('bob'), status: 'approved', metadata: { tests: 'succeeded', approvals: ['bob'] } }), 201).data;
    assert.deepEqual(proposal.authorPrincipal, principal('alice'));
    const endpoint = `/admin/proposals/${proposal.id}`;
    const prepare = async () => expect(await call(`${endpoint}/integration`, {}), 201).data.binding;
    const bindingEndpoint = b => `/admin/proposals/${b.inputs.proposalId}`;
    const approve = b => call(`${bindingEndpoint(b)}/approvals`, { bindingId: b.id, candidateSnapshot: b.candidateSnapshot }, 'bob');
    const reportBody = (b, outcome, attempt = 1, runId = randomUUID()) => ({ bindingId: b.id, candidateSnapshot: b.candidateSnapshot,
      sourceSnapshot: b.inputs.sourceSnapshot, targetSnapshot: b.inputs.targetSnapshot, requirementsDigest: b.requirementsDigest,
      checkId: 'test', attempt, runId, outcome });
    const integrateBody = (b, idempotencyKey = randomUUID()) => ({ bindingId: b.id, expectedTargetSnapshot: b.inputs.targetSnapshot, idempotencyKey });
    const integrate = (b, key) => call(`${endpoint}/integrate`, integrateBody(b, key));
    async function assertUnpublished(expectedTip) {
      assert.equal(await tip(), expectedTip);
      const readiness = expect(await call(`${endpoint}/integration`), 200).data;
      assert.notEqual(readiness.state, 'integrated');
      assert.equal(readiness.receipt, undefined);
      assert.notEqual((await call(endpoint)).data.status, 'merged');
    }
    async function isolation() {
      for (const [actor, foreignRepo, foreignProject, foreignProposal, snapshot] of [
        ['alice', beta, 'project_beta', privateProposal.id, privateSnapshot],
        ['eve', alpha, 'project_alpha', proposal.id, source],
      ]) {
        const projects = expect(await call('/projects', undefined, actor), 200).data;
        assert.ok(!projects.some(p => p.id === foreignProject));
        assert.ok(!expect(await call('/admin/proposals', undefined, actor), 200).data.some(p => p.id === foreignProposal));
        for (const suffix of ['', '/integration', '/integration-changes?bindingId=unknown']) {
          expect(await call(`/admin/proposals/${foreignProposal}${suffix}`, undefined, actor), 404);
        }
        for (const suffix of ['approvals', 'check-reports', 'integrate']) {
          expect(await call(`/admin/proposals/${foreignProposal}/${suffix}`, {}, actor), 404);
        }
        expect(await call(`/${foreignRepo}/refs`, undefined, actor), 403);
        expect(await call(`/${foreignRepo}/objects/${snapshot}`, undefined, actor), 403);
        expect(await call(`/${foreignRepo}/objects/missing`, { want: [snapshot], have: [] }, actor), 403);
        expect(await call(`/${foreignRepo}/refs/main`, { snapshot, grantRefs: [] }, actor), 403);
      }
    }
    await isolation();
    let binding = await prepare();
    assert.equal(binding.inputs.sourceSnapshot, source);
    assert.equal(binding.inputs.targetSnapshot, targetSnapshot);
    assert.equal(binding.mergeBase, base);
    assert.equal(binding.conflicts.length, 0);
    expect(await integrate(binding), 409);
    expect(await call(`${endpoint}/approvals`, { bindingId: binding.id, candidateSnapshot: binding.candidateSnapshot }), 409);
    expect(await approve(binding), 201);
    expect(await call(`${endpoint}/check-reports`, reportBody(binding, 'succeeded'), 'alice'), 409);
    for (const field of ['candidateSnapshot', 'sourceSnapshot', 'targetSnapshot', 'requirementsDigest']) {
      expect(await call(`${endpoint}/check-reports`, { ...reportBody(binding, 'succeeded'), [field]: 'wrong' }, 'ci'), 409);
    }
    await assertUnpublished(targetSnapshot);

    async function testCandidate(b, expectedExit) {
      const candidateEndpoint = bindingEndpoint(b);
      const changes = expect(await call(`${candidateEndpoint}/integration-changes?bindingId=${b.id}`, undefined, 'bob'), 200).data;
      assert.equal(changes.sourceSnapshot, b.candidateSnapshot);
      assert.equal(changes.targetSnapshot, b.inputs.targetSnapshot);
      assert.ok(changes.changes.some(change => change.path === 'source.txt' && change.after.content === 'source\n'));
      const object = expect(await call(`/${alpha}/objects/${b.candidateSnapshot}`, undefined, 'ci'), 200);
      const snapshot = JSON.parse(Buffer.from(object.bytes, 'base64'));
      assert.deepEqual(snapshot.parents.map(parent => parent.id), [b.inputs.targetSnapshot, b.inputs.sourceSnapshot]);
      // A separate immutable ref lets the real CLI validate/download the candidate closure.
      const ref = `check-${randomUUID()}`;
      expect(await call(`/${alpha}/refs/${ref}`, { snapshot: b.candidateSnapshot, grantRefs: [] }), 200);
      const runner = await mkdtemp(path.join(root, 'runner-'));
      await cli(runner, ['init'], 'ci');
      await cli(runner, ['remote', 'add', 'origin', hub.url, '--repo-id', alpha], 'ci');
      assert.equal((await cli(runner, ['pull', 'origin', '--ref', ref], 'ci')).snapshot.id, b.candidateSnapshot);
      const body = reportBody(b, 'pending');
      assert.deepEqual(expect(await call(`${candidateEndpoint}/check-reports`, body, 'ci'), 201).data.principal, principal('ci'));
      let exit = 0;
      try {
        // Candidate processes receive neither reporter credentials nor operator config.
        await execute(process.execPath, ['verify.mjs'], { cwd: runner, env: { PATH: env.PATH }, timeout: 10000 });
      } catch (error) {
        if (error.killed || error.signal || !Number.isInteger(error.code)) throw error;
        exit = error.code;
      }
      assert.equal(exit, expectedExit);
      assert.equal((await cli(runner, ['status'], 'ci')).worktree.dirty, false);
      const terminal = { ...body, outcome: exit === 0 ? 'succeeded' : 'failed' };
      assert.deepEqual(expect(await call(`${candidateEndpoint}/check-reports`, terminal, 'ci'), 201).data.principal, principal('ci'));
      return terminal;
    }
    const failed = await testCandidate(binding, 1);
    expect(await integrate(binding), 409);
    expect(await call(`${endpoint}/check-reports`, { ...failed, outcome: 'succeeded' }, 'ci'), 409);
    await assertUnpublished(targetSnapshot);
    log('actual failed candidate blocks publication; forged evidence and cross-team access rejected');

    await cli(work, ['lane', 'switch', lane]);
    await writeFile(path.join(work, 'value.mjs'), 'export const value = 2;\n');
    source = (await cli(work, ['change', 'create', '-m', 'fix actual failing candidate'])).object.resultingSnapshot.id;
    await cli(work, ['push', 'origin', '--ref', 'source']);
    expect(await integrate(binding), 409);
    binding = await prepare();
    assert.equal(expect(await call(`${endpoint}/integration`), 200).data.approved, false);
    await testCandidate(binding, 0);
    assert.deepEqual(expect(await approve(binding), 201).data.principal, principal('bob'));
    assert.equal(expect(await call(`${endpoint}/integration`), 200).data.ready, true);

    // Public target movement invalidates tested evidence; it must never overwrite the new tip.
    await cli(work, ['lane', 'switch', 'lane_main']);
    await writeFile(path.join(work, 'target-extra.txt'), 'concurrent target change\n');
    targetSnapshot = (await cli(work, ['change', 'create', '-m', 'concurrent target advance'])).object.resultingSnapshot.id;
    await cli(work, ['push', 'origin', '--ref', 'main']);
    expect(await integrate(binding), 409);
    await assertUnpublished(targetSnapshot);
    log('source and target movement invalidate tested/approved bindings without overwriting target');

    // Compete through public HTTP requests against a separate target, leaving main for crash recovery.
    expect(await call(`/${alpha}/refs/race-main`, { snapshot: targetSnapshot, grantRefs: [] }), 200);
    const raceEndpoint = '/admin/proposals/proposal_race';
    expect(await call('/admin/proposals', { id: 'proposal_race', projectId: 'project_alpha', syncRepoId: alpha,
      title: 'CAS race', sourceLane: 'source', targetLane: 'race-main', status: 'open' }), 201);
    const raceBinding = expect(await call(`${raceEndpoint}/integration`, {}), 201).data.binding;
    await testCandidate(raceBinding, 0);
    expect(await approve(raceBinding), 201);
    await writeFile(path.join(work, 'race.txt'), 'competing public writer\n');
    const competitor = (await cli(work, ['change', 'create', '-m', 'competing CAS writer'])).object.resultingSnapshot.id;
    await cli(work, ['push', 'origin', '--ref', 'race-upload']);
    const raceBody = integrateBody(raceBinding);
    const [publication, push] = await Promise.all([
      call(`${raceEndpoint}/integrate`, raceBody),
      call(`/${alpha}/refs/race-main`, { snapshot: competitor, expected: targetSnapshot, grantRefs: [] }),
    ]);
    assert.equal(Number(publication.status === 200) + Number(push.status === 200), 1, JSON.stringify({ publication, push }));
    let raceWinner;
    if (publication.status === 200) {
      expect(push, 400); // Ordinary push reports an expected-tip mismatch as 400.
      assert.equal(publication.data.state, 'integrated');
      raceWinner = raceBinding.candidateSnapshot;
      assert.equal(publication.data.receipt.previousSnapshot, targetSnapshot);
      assert.equal(publication.data.receipt.candidateSnapshot, raceWinner);
      assert.deepEqual(expect(await call(`${raceEndpoint}/integrate`, raceBody), 200).data, publication.data);
    } else {
      expect(publication, 409);
      raceWinner = competitor;
      assert.notEqual(expect(await call(`${raceEndpoint}/integration`), 200).data.state, 'integrated');
    }
    assert.equal((await refs(alpha)).find(ref => ref.name === 'race-main').snapshot, raceWinner);
    assert.equal(await tip(), targetSnapshot);
    log('public push/integration CAS race has exactly one winner and preserves its target');

    // Kill a real Hub process on both sides of the publication boundary. No disk records are fabricated.
    for (const stage of ['journal', 'ref']) {
      await hub.stop();
      hub = await startHub({ ...hubEnv, SORREL_PILOT_TEST_CRASH: stage }, aborter.signal);
      binding = await prepare();
      await testCandidate(binding, 0);
      expect(await approve(binding), 201);
      const body = integrateBody(binding);
      await assert.rejects(call(`${endpoint}/integrate`, body), /fetch failed|terminated|socket/i);
      assert.deepEqual(await hub.closed, { code: null, signal: 'SIGKILL' });
      hub = await startHub(hubEnv, aborter.signal);
      const recovered = expect(await call(`${endpoint}/integrate`, body), 200).data;
      if (stage === 'journal') {
        assert.equal(recovered.state, 'not_published');
        await assertUnpublished(targetSnapshot);
      } else {
        assert.equal(recovered.state, 'integrated');
        assert.equal(await tip(), binding.candidateSnapshot);
        assert.equal(recovered.receipt.bindingId, binding.id);
        assert.equal(recovered.receipt.previousSnapshot, targetSnapshot);
        assert.equal(recovered.receipt.sourceSnapshot, source);
        assert.equal(recovered.receipt.candidateSnapshot, binding.candidateSnapshot);
        assert.deepEqual(recovered.receipt.publishedBy, principal('alice'));
        assert.deepEqual(expect(await call(`${endpoint}/integration`), 200).data.receipt, recovered.receipt);
        const results = await Promise.all([call(`${endpoint}/integrate`, body), call(`${endpoint}/integrate`, body)]);
        for (const result of results) assert.deepEqual(expect(result, 200).data, recovered);
        expect(await call(`${endpoint}/integrate`, { ...body, expectedTargetSnapshot: base }), 409);
        expect(await call(`${endpoint}/integrate`, body, 'bob'), 409);
        await hub.stop();
        hub = await startHub(hubEnv, aborter.signal);
        assert.deepEqual(expect(await call(`${endpoint}/integrate`, body), 200).data, recovered);
      }
      log(`${stage} SIGKILL → restart → ${recovered.state} from durable evidence`);
    }
    const recoveredWork = await mkdtemp(path.join(root, 'recovered-'));
    await cli(recoveredWork, ['init'], 'bob');
    await cli(recoveredWork, ['remote', 'add', 'origin', hub.url, '--repo-id', alpha], 'bob');
    assert.equal((await cli(recoveredWork, ['pull', 'origin', '--ref', 'main'], 'bob')).snapshot.id, binding.candidateSnapshot);
    for (const [name, content] of [['value.mjs', 'export const value = 2;\n'], ['source.txt', 'source\n'], ['target.txt', 'target\n'], ['target-extra.txt', 'concurrent target change\n']]) {
      assert.equal(await readFile(path.join(recoveredWork, name), 'utf8'), content);
    }
    await execute(process.execPath, ['verify.mjs'], { cwd: recoveredWork, env: { PATH: env.PATH }, timeout: 10000 });
    await isolation();
    log('fresh reviewer CLI pull matches tested candidate after recovery; isolation persists');
    log('LOCAL protocol/OIDC proof passed; live WorkOS, public edge and hosted workflow remain unproven');
  } finally {
    try {
      if (hub) await hub.stop();
    } finally {
      try {
        if (jwks) await new Promise(resolve => jwks.close(resolve));
        if (root) await rm(root, { recursive: true, force: true });
      } finally {
        if (stopActiveWorkflow === stop) stopActiveWorkflow = async () => {};
        finished.resolve();
      }
    }
  }
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  let interrupted = false;
  for (const [signal, code] of [['SIGINT', 130], ['SIGTERM', 143]]) process.on(signal, async () => {
    if (interrupted) return;
    interrupted = true;
    await stopPilotWorkflow();
    process.exit(code);
  });
  runPilotWorkflow().catch(error => { if (!interrupted) { console.error(error); process.exitCode = 1; } });
}
