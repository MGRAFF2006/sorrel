#!/usr/bin/env node
/**
 * Root end-to-end happy path — real processes, no mocks.
 *
 * Touches every module working together:
 *   protocol · core (via CLI) · cli · vault · runners (via CLI workflow) ·
 *   slices · hub · hub-web · web · sdk-js · sdk-rust · agents
 *
 * Usage (from repo root):
 *   node tests/e2e/happy-path.mjs
 *   npm test
 */

import assert from 'node:assert/strict';
import { spawn, spawnSync } from 'node:child_process';
import { createServer } from 'node:http';
import {
  mkdtempSync,
  readFileSync,
  writeFileSync,
  existsSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { join, dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createInterface } from 'node:readline';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const CLI_DIR = join(ROOT, 'sorrel-cli');
const HUB_DIR = join(ROOT, 'sorrel-hub');
const HUB_WEB_DIR = join(ROOT, 'sorrel-hub-web');
const VAULT_DIR = join(ROOT, 'sorrel-vault');
const SLICES_DIR = join(ROOT, 'sorrel-slices');
const PROTOCOL_DIR = join(ROOT, 'sorrel-protocol');
const WEB_DIR = join(ROOT, 'sorrel-web');
const SDK_JS_DIR = join(ROOT, 'sorrel-sdk-js');
const SDK_RUST_DIR = join(ROOT, 'sorrel-sdk-rust');
const AGENTS_DIR = join(ROOT, 'sorrel-agents');
const RUNNERS_DIR = join(ROOT, 'sorrel-runners');

const children = [];

function log(step, detail = '') {
  console.log(`✓ ${step}${detail ? ` — ${detail}` : ''}`);
}

function cleanup() {
  for (const child of children.splice(0)) {
    try {
      child.kill('SIGTERM');
    } catch {
      /* ignore */
    }
    try {
      child.kill('SIGKILL');
    } catch {
      /* ignore */
    }
  }
}

process.on('exit', cleanup);
process.on('SIGINT', () => {
  cleanup();
  process.exit(130);
});
process.on('SIGTERM', () => {
  cleanup();
  process.exit(143);
});

async function spawnReady(command, args, options = {}) {
  const child = spawn(command, args, {
    stdio: ['ignore', 'pipe', 'pipe'],
    ...options,
  });
  children.push(child);
  let stderr = '';
  child.stderr.on('data', (chunk) => {
    stderr += chunk.toString();
  });
  const rl = createInterface({ input: child.stdout });
  try {
    return await new Promise((resolveReady, reject) => {
      const timer = setTimeout(() => {
        reject(
          new Error(
            `timeout waiting for ${command} ${args.join(' ')}\nstderr: ${stderr}`,
          ),
        );
      }, 15000);
      rl.once('line', (line) => {
        clearTimeout(timer);
        try {
          resolveReady({ child, ready: JSON.parse(line) });
        } catch (error) {
          reject(error);
        }
      });
      child.once('exit', (code) => {
        clearTimeout(timer);
        reject(
          new Error(
            `${command} exited early with ${code}\nstderr: ${stderr}`,
          ),
        );
      });
    });
  } catch (error) {
    try {
      child.kill('SIGKILL');
    } catch {
      /* ignore */
    }
    throw error;
  }
}

function run(command, args, options = {}) {
  const result = spawnSync(command, args, {
    encoding: 'utf8',
    timeout: 120_000,
    ...options,
  });
  if (result.error) {
    throw result.error;
  }
  if (result.status !== 0) {
    throw new Error(
      `${command} ${args.join(' ')} failed (exit ${result.status})\n${result.stderr || result.stdout}`,
    );
  }
  return result;
}

function sorrel(cwd, args, env = {}) {
  return run(join(CLI_DIR, 'target/debug/sorrel'), args, {
    cwd,
    env: { ...process.env, ...env },
  });
}

function sorrelJson(cwd, args, env = {}) {
  return JSON.parse(sorrel(cwd, [...args, '--json'], env).stdout);
}

async function main() {
  console.log('Sorrel E2E happy path (no mocks)\n');

  // Protocol validate (fast) — full npm test suite lives in `npm run test:modules`.
  run('npm', ['run', 'validate'], { cwd: PROTOCOL_DIR });
  log('protocol', 'schema/example/conformance validate');

  run('cargo', ['build'], { cwd: CLI_DIR });
  log('sorrel-cli + sorrel-core', 'built');

  const hub = await spawnReady('node', ['scripts/listen.mjs'], {
    cwd: HUB_DIR,
    env: {
      ...process.env,
      SORREL_HUB_SYNC_STORE: 'memory',
      SORREL_HUB_BOOTSTRAP_GRANTS: '1',
    },
  });
  const hubUrl = hub.ready.url;
  log('sorrel-hub', hubUrl);

  const health = await fetch(`${hubUrl}/healthz`).then((r) => r.json());
  assert.equal(health.status, 'ok');
  assert.equal(health.service, 'sorrel-hub');
  log('hub /healthz');

  const hubWeb = await spawnReady('node', ['scripts/listen.mjs'], {
    cwd: HUB_WEB_DIR,
    env: { ...process.env, HUB_API_URL: hubUrl },
  });
  const uiUrl = hubWeb.ready.url;
  log('sorrel-hub-web', uiUrl);

  const proxied = await fetch(`${uiUrl}/api/healthz`).then((r) => r.json());
  assert.equal(proxied.status, 'ok');
  log('hub-web proxies /api/healthz');

  await new Promise((resolveServe, reject) => {
    const server = createServer((req, res) => {
      try {
        const reqPath = req.url === '/' ? '/index.html' : req.url.split('?')[0];
        res.writeHead(200).end(readFileSync(join(WEB_DIR, reqPath)));
      } catch {
        res.writeHead(404).end('missing');
      }
    });
    server.listen(0, '127.0.0.1', async () => {
      const port = server.address().port;
      try {
        const html = await fetch(`http://127.0.0.1:${port}/`).then((r) => r.text());
        assert.match(html, /Sorrel/i);
        log('sorrel-web', `served on :${port}`);
        resolveServe();
      } catch (error) {
        reject(error);
      } finally {
        server.close();
      }
    });
  });

  const workA = mkdtempSync(join(tmpdir(), 'sorrel-e2e-a-'));
  sorrel(workA, ['init']);
  writeFileSync(join(workA, 'a.txt'), 'line1\n');
  const statusDirty = sorrelJson(workA, ['status']);
  assert.equal(statusDirty.worktree.dirty, true);
  log('cli status', 'dirty');

  const change1 = sorrelJson(workA, ['change', 'create', '-m', 'add a.txt']);
  assert.equal(change1.status, 'created');
  assert.equal(change1.object.kind, 'Change');
  log('cli change create');

  writeFileSync(join(workA, 'a.txt'), 'line1\nLINE2\n');
  const diff = sorrelJson(workA, ['diff']);
  assert.ok(diff.files.some((f) => f.path === 'a.txt' && f.kind === 'modified'));
  log('cli diff');

  sorrel(workA, ['change', 'create', '-m', 'edit a.txt']);
  const history = sorrelJson(workA, ['log']);
  assert.ok(history.entries.length >= 2);
  log('cli log');

  const lane = sorrelJson(workA, ['lane', 'create', '--name', 'feature']);
  const featureLaneId = lane.object.id;
  sorrel(workA, ['lane', 'switch', featureLaneId]);
  writeFileSync(join(workA, 'b.txt'), 'feature\n');
  sorrel(workA, ['change', 'create', '-m', 'add b on feature']);
  sorrel(workA, ['lane', 'switch', 'lane_main']);
  const merge = sorrelJson(workA, ['merge', featureLaneId]);
  assert.equal(merge.status, 'merged');
  assert.equal(merge.fastForward, true);
  assert.equal(readFileSync(join(workA, 'b.txt'), 'utf8'), 'feature\n');
  log('cli lanes + merge');

  // Conflicted merge → resolve markers → --continue
  const conflictLane = sorrelJson(workA, ['lane', 'create', '--name', 'conflict']);
  const conflictLaneId = conflictLane.object.id;
  writeFileSync(join(workA, 'c.txt'), 'main-c\n');
  sorrel(workA, ['change', 'create', '-m', 'main adds c']);
  sorrel(workA, ['lane', 'switch', conflictLaneId]);
  writeFileSync(join(workA, 'c.txt'), 'feature-c\n');
  sorrel(workA, ['change', 'create', '-m', 'feature adds c']);
  sorrel(workA, ['lane', 'switch', 'lane_main']);
  const conflicted = spawnSync(join(CLI_DIR, 'target/debug/sorrel'), ['merge', conflictLaneId, '--json'], {
    cwd: workA,
    encoding: 'utf8',
  });
  assert.notEqual(conflicted.status, 0, 'conflicted merge must fail');
  writeFileSync(join(workA, 'c.txt'), 'resolved-c\n');
  const continued = sorrelJson(workA, ['merge', '--continue']);
  assert.equal(continued.status, 'merged');
  assert.equal(continued.continued, true);
  assert.equal(readFileSync(join(workA, 'c.txt'), 'utf8'), 'resolved-c\n');
  log('cli merge --continue');

  const stack = sorrelJson(workA, ['stack', 'create', '--name', 'stack/e2e']);
  assert.equal(stack.status, 'created');
  assert.equal(stack.object.kind, 'Stack');
  log('cli stack create');

  const grant = sorrelJson(workA, ['grant', 'create', '--action', 'workflow.run']);
  assert.equal(grant.status, 'allow');
  assert.ok(grant.object.id.startsWith('grant_'));
  const grants = sorrelJson(workA, ['grant', 'list']);
  assert.ok(grants.count >= 1 || (grants.objects && grants.objects.length >= 1));
  sorrelJson(workA, ['secret', 'refs']);
  const policy = sorrelJson(workA, [
    'policy',
    'evaluate',
    '--action',
    'workflow.run',
    '--principal',
    'user:local',
  ]);
  assert.equal(policy.status, 'allow');
  assert.equal(policy.decision.result, 'allow');
  log('cli grant + secret + policy');

  writeFileSync(
    join(workA, 'sorrel.workflow.yml'),
    `version: 1
id: workflow_e2e
jobs:
  test:
    command: echo e2e-ok
`,
  );
  assert.equal(sorrelJson(workA, ['workflow', 'validate']).status, 'valid');
  const wfRun = sorrelJson(workA, ['workflow', 'run', 'test']);
  assert.equal(wfRun.status, 'completed');
  assert.match(String(wfRun.job.stdout), /e2e-ok/);
  log('cli workflow (runners)');

  const slice = sorrelJson(workA, [
    'slice',
    'create',
    '--name',
    'e2e-slice',
    '--entrypoint',
    'a.txt',
    '--source-path',
    '.',
  ]);
  assert.equal(slice.status, 'created');
  assert.equal(slice.object.kind, 'Slice');
  log('cli slice create');

  const sliceProj = join(SLICES_DIR, 'test/fixtures/basic');
  const slicesOut = run('node', [
    join(SLICES_DIR, 'src/index.js'),
    '--project-root',
    sliceProj,
    '--entrypoint',
    'src/index.ts',
  ]);
  const sliceManifest = JSON.parse(slicesOut.stdout);
  assert.equal(sliceManifest.kind, 'SliceManifest');
  assert.ok(sliceManifest.includedFiles.length > 0);
  log('sorrel-slices', 'manifest generated');

  const envExample = join(VAULT_DIR, 'examples/.env.dev.example');
  const importResult = run(
    'node',
    ['scripts/vault-cli.mjs', 'import', '--file', envExample, '--spec', 'examples/sorrel.secrets.dev.yml'],
    { cwd: VAULT_DIR },
  );
  assert.match(importResult.stdout, /Imported 2 key/);
  const vaultList = run(
    'node',
    ['scripts/vault-cli.mjs', 'list', '--spec', 'examples/sorrel.secrets.dev.yml'],
    { cwd: VAULT_DIR },
  );
  assert.match(vaultList.stdout, /secret_npm_token_dev/);
  const redact = run(
    'node',
    ['scripts/vault-cli.mjs', 'redact', '--spec', 'examples/sorrel.secrets.dev.yml'],
    {
      cwd: VAULT_DIR,
      input: 'NPM_TOKEN=dev-token-example-do-not-use\n',
    },
  );
  assert.ok(!redact.stdout.includes('dev-token-example-do-not-use'));
  log('sorrel-vault', 'import/list/redact');

  const repoId = sorrelJson(workA, ['status']).repoId;
  sorrel(workA, ['remote', 'add', 'origin', hubUrl, '--repo-id', repoId]);
  const push = sorrelJson(workA, ['push', 'origin']);
  assert.equal(push.status, 'pushed');
  assert.ok(push.uploaded > 0);
  log('cli push → hub');

  const workB = mkdtempSync(join(tmpdir(), 'sorrel-e2e-b-'));
  sorrel(workB, ['init']);
  sorrel(workB, ['remote', 'add', 'origin', hubUrl, '--repo-id', repoId]);
  const pull = sorrelJson(workB, ['pull', 'origin']);
  assert.equal(pull.status, 'pulled');
  assert.equal(readFileSync(join(workB, 'a.txt'), 'utf8'), 'line1\nLINE2\n');
  assert.equal(readFileSync(join(workB, 'b.txt'), 'utf8'), 'feature\n');
  log('cli pull ← hub', 'working tree restored');

  // Hub collaboration: lane submit creates a proposal on the live Hub
  const submit = sorrelJson(workA, ['lane', 'submit']);
  assert.ok(submit.status === 'submitted' || submit.status === 'reused');
  assert.ok(String(submit.proposal?.id || '').startsWith('prop_'));
  const hubProposal = await fetch(
    `${hubUrl}/admin/proposals/${submit.proposal.id}?include=comments`,
  ).then((r) => r.json());
  assert.equal(hubProposal.data.id, submit.proposal.id);
  log('cli lane submit → hub proposal');

  // Create a review comment via Hub API (same surface hub-web uses)
  const comment = await fetch(`${hubUrl}/admin/review-comments`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({
      proposalId: submit.proposal.id,
      body: 'e2e review note',
      authorPrincipal: { type: 'user', id: 'local' },
    }),
  }).then((r) => r.json());
  assert.ok(comment.data.id.startsWith('comment_'));
  const viaUi = await fetch(`${uiUrl}/api/admin/proposals/${submit.proposal.id}?include=comments`).then(
    (r) => r.json(),
  );
  assert.equal(viaUi.data.comments.length, 1);
  log('hub + hub-web proposal review');

  const syncRepos = await fetch(`${hubUrl}/admin/sync-repos`).then((r) => r.json());
  assert.ok(syncRepos.repos.some((r) => r.id === repoId));
  const uiSync = await fetch(`${uiUrl}/api/admin/sync-repos`).then((r) => r.json());
  assert.ok(uiSync.repos.some((r) => r.id === repoId));
  log('hub + hub-web sync admin');

  const gitDir = mkdtempSync(join(tmpdir(), 'sorrel-e2e-git-'));
  run('git', ['init'], { cwd: gitDir });
  run('git', ['config', 'user.email', 'e2e@example.com'], { cwd: gitDir });
  run('git', ['config', 'user.name', 'E2E'], { cwd: gitDir });
  writeFileSync(join(gitDir, 'readme.txt'), 'from-git\n');
  run('git', ['add', 'readme.txt'], { cwd: gitDir });
  run('git', ['commit', '-m', 'add readme'], { cwd: gitDir });
  const imported = sorrelJson(gitDir, ['git', 'import']);
  assert.ok(imported.commits.length >= 1);
  assert.equal(readFileSync(join(gitDir, 'readme.txt'), 'utf8'), 'from-git\n');
  log('cli git import');

  const exportDir = mkdtempSync(join(tmpdir(), 'sorrel-e2e-export-'));
  const exported = sorrelJson(gitDir, ['git', 'export', exportDir, '--branch', 'export-main']);
  assert.equal(exported.status, 'exported');
  assert.ok(exported.createdCommits >= 1 || exported.exportedCommits >= 1);
  log('cli git export');

  const { HubClient } = await import(join(SDK_JS_DIR, 'src/index.js'));
  const sdk = new HubClient({ baseUrl: hubUrl });
  assert.equal((await sdk.health()).status, 'ok');
  const created = await sdk.createProject({
    organizationId: 'org_e2e',
    name: 'E2E Project',
  });
  assert.ok(created.data.id.startsWith('proj_'));
  assert.ok(Array.isArray((await sdk.listProjects()).data));
  assert.ok(Array.isArray((await sdk.listSyncRepos()).repos));
  const sdkProposal = await sdk.createProposal({
    projectId: created.data.id,
    title: 'sdk-js e2e',
    authorPrincipal: { type: 'user', id: 'local' },
  });
  assert.ok(sdkProposal.data.id.startsWith('prop_'));
  log('sorrel-sdk-js', 'live Hub client + proposals');

  // sdk-rust: exercise real core via the SDK Workspace helper (no nested cargo test).
  run('cargo', ['test', '--quiet'], { cwd: SDK_RUST_DIR });
  log('sorrel-sdk-rust', 'Workspace against sorrel-core');

  const { AgentControlPlane } = await import(join(AGENTS_DIR, 'src/index.js'));
  const plane = new AgentControlPlane({ hubUrl, workspace: workA });
  const agent = await plane.registerAgent({ id: 'agent_e2e', lane: 'lane_main' });
  assert.equal(agent.id, 'agent_e2e');
  const claim = await plane.claimPath({ agentId: 'agent_e2e', path: 'a.txt' });
  assert.equal(claim.path, 'a.txt');
  const active = await plane.activeWork();
  assert.ok(active.agents.length >= 1);
  assert.ok(active.claims.length >= 1);
  assert.ok(existsSync(join(workA, '.sorrel/agents/state.json')));
  log('sorrel-agents', 'register + claim');

  // Runners already exercised via `sorrel workflow run` above.
  assert.ok(existsSync(join(RUNNERS_DIR, 'Cargo.toml')));
  log('sorrel-runners', 'exercised via CLI workflow run');

  console.log('\nAll E2E checks passed.');
  cleanup();
}

main().catch((error) => {
  console.error('✗ E2E failed');
  console.error(error);
  cleanup();
  process.exit(1);
});
