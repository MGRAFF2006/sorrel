#!/usr/bin/env node
// Root E2E for managed CLI login, against a real local Hub with a stubbed
// browser session (tests/e2e/cli-login-hub.mjs):
//   sorrel login → browser approval → push → pull → owner revoke → 401.
//
// Usage: node tests/e2e/cli-login.mjs   (expects target/debug/sorrel to be built)
import assert from 'node:assert/strict';
import { spawn, spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { createInterface } from 'node:readline';
import { fileURLToPath } from 'node:url';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const SORREL_BIN = join(ROOT, 'target/debug/sorrel');
const REPO_ID = 'repo_cli_login_e2e';

function log(step, detail = '') {
  console.log(`✓ ${step}${detail ? ` — ${detail}` : ''}`);
}

async function firstLine(stream, match, timeoutMs = 15_000) {
  const lines = createInterface({ input: stream });
  const timer = setTimeout(() => lines.close(), timeoutMs);
  try {
    for await (const line of lines) {
      const found = match(line);
      if (found) return found;
    }
  } finally {
    clearTimeout(timer);
  }
  throw new Error('expected output line not seen');
}

export async function runCliLogin() {
  const temp = mkdtempSync(join(tmpdir(), 'sorrel-cli-login-'));
  const hub = spawn(process.execPath, [join(ROOT, 'tests/e2e/cli-login-hub.mjs'), '--repo', REPO_ID],
    { cwd: ROOT, stdio: ['ignore', 'pipe', 'inherit'] });
  try {
    const ready = await firstLine(hub.stdout, line => { try { return JSON.parse(line); } catch { return null; } });
    const { url: hubUrl, cookie, principal } = ready;
    hub.stdout.resume(); // drain Hub audit lines
    const env = { ...process.env, SORREL_CONFIG_DIR: join(temp, 'config'), HOME: temp };
    delete env.SORREL_HUB_TOKEN;
    const sorrel = (cwd, args, extra = {}) => spawnSync(SORREL_BIN, args, { cwd, env: { ...env, ...extra }, encoding: 'utf8' });
    const sorrelJson = (cwd, args) => {
      const result = sorrel(cwd, [...args, '--json']);
      assert.equal(result.status, 0, `sorrel ${args.join(' ')} failed: ${result.stderr}`);
      return JSON.parse(result.stdout);
    };

    // 1. `sorrel login` prints a code; the signed-in browser approves it.
    const login = spawn(SORREL_BIN, ['login', hubUrl, '--label', 'e2e laptop', '--json'], { cwd: temp, env, stdio: ['ignore', 'pipe', 'pipe'] });
    let loginStdout = '';
    let loginStderr = '';
    login.stdout.on('data', chunk => { loginStdout += chunk; });
    const exited = new Promise(done => login.on('close', done));
    const userCode = await firstLine(login.stderr, line => {
      loginStderr += `${line}\n`;
      return /confirm the code ([A-Z]{4}-[A-Z]{4})/.exec(line)?.[1];
    });
    login.stderr.on('data', chunk => { loginStderr += chunk; });
    const approve = await fetch(`${hubUrl}/auth/cli/approve`, {
      method: 'POST',
      headers: { 'content-type': 'application/json', cookie, origin: hubUrl },
      body: JSON.stringify({ user_code: userCode, decision: 'approve' }),
    });
    assert.equal(approve.status, 200, await approve.clone().text());
    assert.equal(await exited, 0, `login failed: ${loginStderr}`);
    const loggedIn = JSON.parse(loginStdout);
    assert.deepEqual(loggedIn.principal, principal);
    assert.equal(loggedIn.hub, hubUrl);
    assert.equal(loggedIn.label, 'e2e laptop');

    const credentialsPath = join(temp, 'config', 'credentials.json');
    const stored = JSON.parse(readFileSync(credentialsPath, 'utf8'));
    const token = stored.hubs[hubUrl].token;
    assert.match(token, /^srl_cli_/);
    assert.ok(!loginStdout.includes(token) && !loginStderr.includes(token), 'login never prints the token');
    if (process.platform !== 'win32') {
      assert.equal(statSync(credentialsPath).mode & 0o777, 0o600);
      assert.equal(statSync(join(temp, 'config')).mode & 0o777, 0o700);
    }
    const status = sorrelJson(temp, ['auth', 'status']);
    assert.deepEqual(status.hubs.map(entry => entry.hub), [hubUrl]);
    assert.ok(!JSON.stringify(status).includes(token));
    log('cli login (device flow)', `${principal.type}:${principal.id}`);

    // 2. Push and pull use the stored credential for this Hub automatically.
    const workA = join(temp, 'a');
    const workB = join(temp, 'b');
    for (const dir of [workA, workB]) {
      mkdirSync(dir, { recursive: true });
      assert.equal(sorrel(dir, ['init']).status, 0);
      assert.equal(sorrel(dir, ['remote', 'add', 'origin', hubUrl, '--repo-id', REPO_ID]).status, 0);
    }
    writeFileSync(join(workA, 'notes.txt'), 'signed in\n');
    assert.equal(sorrel(workA, ['change', 'create', '--message', 'login e2e']).status, 0);
    const push = sorrelJson(workA, ['push', 'origin']);
    assert.equal(push.status, 'pushed');
    const pull = sorrelJson(workB, ['pull', 'origin']);
    assert.equal(pull.status, 'pulled');
    assert.equal(readFileSync(join(workB, 'notes.txt'), 'utf8'), 'signed in\n');
    log('cli push/pull with stored login');

    // 3. The owner revokes the token in the web UI (cookie session) → 401.
    const listed = await fetch(`${hubUrl}/auth/cli/tokens`, { headers: { cookie } });
    const tokens = (await listed.json()).data;
    assert.deepEqual(tokens.map(entry => entry.id), [loggedIn.tokenId]);
    const revoked = await fetch(`${hubUrl}/auth/cli/tokens/${loggedIn.tokenId}`, { method: 'DELETE', headers: { cookie, origin: hubUrl } });
    assert.equal(revoked.status, 200);
    writeFileSync(join(workA, 'notes.txt'), 'after revoke\n');
    assert.equal(sorrel(workA, ['change', 'create', '--message', 'after revoke']).status, 0);
    const denied = sorrel(workA, ['push', 'origin']);
    assert.notEqual(denied.status, 0);
    assert.match(denied.stderr, /authentication required; run `sorrel login/);
    const session = await fetch(`${hubUrl}/session`, { headers: { authorization: `Bearer ${token}` } });
    assert.equal((await session.json()).data.session, null);
    log('cli token revoked in browser', 'push → 401');

    // 4. Logout removes the local credential even when the Hub already revoked it.
    const logout = sorrelJson(temp, ['logout', hubUrl]);
    assert.equal(logout.removed, true);
    assert.deepEqual(sorrelJson(temp, ['auth', 'status']).hubs, []);
    log('cli logout');
  } finally {
    hub.kill('SIGTERM');
    rmSync(temp, { recursive: true, force: true });
  }
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  runCliLogin().catch(error => { console.error(error); process.exit(1); });
}
