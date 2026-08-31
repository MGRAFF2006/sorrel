#!/usr/bin/env node
/**
 * Sorrel local test dashboard
 *
 * Builds the CLI, seeds a demo workspace, starts Hub + Hub UI, and opens a
 * small status dashboard with links and copy-paste commands.
 *
 * Usage (from repo root):
 *   node scripts/dev-dashboard.mjs
 *   npm run dev
 *
 * Flags:
 *   --skip-build   reuse existing CLI and Hub UI builds when present
 *   --no-open      do not open a browser
 *   --no-seed      skip demo workspace create/seed
 *   --port <n>     dashboard port (default 5200)
 *
 * Env:
 *   HUB_PORT       default 3000
 *   HUB_WEB_PORT   default 5180
 *   DASHBOARD_PORT default 5200
 */

import { spawn, spawnSync, execFileSync } from 'node:child_process';
import {
  createServer,
} from 'node:http';
import {
  existsSync,
  mkdirSync,
  writeFileSync,
} from 'node:fs';
import { join, dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { platform } from 'node:os';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const HUB_DIR = join(ROOT, 'sorrel-hub');
const HUB_WEB_DIR = join(ROOT, 'sorrel-hub-web');
const HUB_WEB_ENTRY = join(HUB_WEB_DIR, 'dist', 'index.html');
const DEV_DIR = join(ROOT, '.dev');
const HUB_DATA = join(DEV_DIR, 'hub-data');
const WORKSPACE = join(DEV_DIR, 'workspace');
const ENV_FILE = join(DEV_DIR, 'env.sh');

const args = new Set(process.argv.slice(2));
const skipBuild = args.has('--skip-build');
const noOpen = args.has('--no-open');
const noSeed = args.has('--no-seed');

function argValue(flag, fallback) {
  const idx = process.argv.indexOf(flag);
  if (idx >= 0 && process.argv[idx + 1]) return process.argv[idx + 1];
  return fallback;
}

const HUB_PORT = Number(process.env.HUB_PORT ?? 3000);
const HUB_WEB_PORT = Number(process.env.HUB_WEB_PORT ?? 5180);
const DASHBOARD_PORT = Number(
  argValue('--port', process.env.DASHBOARD_PORT ?? 5200),
);

const HUB_URL = `http://127.0.0.1:${HUB_PORT}`;
const HUB_WEB_URL = `http://127.0.0.1:${HUB_WEB_PORT}`;
const DASHBOARD_URL = `http://127.0.0.1:${DASHBOARD_PORT}`;

const children = [];
let shuttingDown = false;

function log(msg) {
  console.log(`▸ ${msg}`);
}

function fail(msg) {
  console.error(`✗ ${msg}`);
  process.exit(1);
}

function sorrelBin() {
  return join(ROOT, 'target/debug/sorrel');
}

function run(command, commandArgs, options = {}) {
  const result = spawnSync(command, commandArgs, {
    encoding: 'utf8',
    stdio: ['ignore', 'pipe', 'pipe'],
    ...options,
  });
  if (result.error) throw result.error;
  if (result.status !== 0) {
    throw new Error(
      `${command} ${commandArgs.join(' ')} failed\n${result.stderr || result.stdout}`,
    );
  }
  return result;
}

function start(command, commandArgs, options = {}) {
  const child = spawn(command, commandArgs, {
    stdio: ['ignore', 'pipe', 'pipe'],
    ...options,
  });
  children.push(child);
  const name = options.name ?? command;
  child.stdout.on('data', (chunk) => {
    for (const line of chunk.toString().split('\n').filter(Boolean)) {
      console.log(`  [${name}] ${line}`);
    }
  });
  child.stderr.on('data', (chunk) => {
    for (const line of chunk.toString().split('\n').filter(Boolean)) {
      console.log(`  [${name}] ${line}`);
    }
  });
  child.on('exit', (code, signal) => {
    if (!shuttingDown) {
      console.error(`✗ ${name} exited (code=${code}, signal=${signal})`);
    }
  });
  return child;
}

async function waitForUrl(url, { timeoutMs = 30_000, label = url } = {}) {
  const startAt = Date.now();
  let lastError = '';
  while (Date.now() - startAt < timeoutMs) {
    try {
      const res = await fetch(url);
      if (res.ok || res.status < 500) return;
      lastError = `HTTP ${res.status}`;
    } catch (error) {
      lastError = error.message;
    }
    await new Promise((r) => setTimeout(r, 250));
  }
  throw new Error(`Timed out waiting for ${label}: ${lastError}`);
}

function ensureDirs() {
  mkdirSync(join(HUB_DATA, 'sync'), { recursive: true });
  mkdirSync(join(HUB_DATA, 'metadata'), { recursive: true });
  mkdirSync(WORKSPACE, { recursive: true });
}

function buildCli() {
  if (skipBuild && existsSync(sorrelBin())) {
    log('Skipping cargo build (--skip-build)');
    return;
  }
  log('Building sorrel CLI…');
  run('cargo', ['build', '-p', 'sorrel-cli'], { cwd: ROOT, stdio: 'inherit' });
}

function buildHubWeb() {
  if (skipBuild && existsSync(HUB_WEB_ENTRY)) {
    log('Skipping Hub UI build (--skip-build)');
    return;
  }
  log('Building Hub UI…');
  run('npm', ['run', 'build'], { cwd: HUB_WEB_DIR, stdio: 'inherit' });
}

function seedWorkspace() {
  if (noSeed) {
    log('Skipping workspace seed (--no-seed)');
    return;
  }

  const bin = sorrelBin();
  if (!existsSync(join(WORKSPACE, '.sorrel'))) {
    log(`Seeding demo workspace at ${WORKSPACE}`);
    run(bin, ['init'], { cwd: WORKSPACE });
    writeFileSync(join(WORKSPACE, 'README.md'), '# Sorrel demo workspace\n\nEdit me, then:\n\n```sh\nsorrel change create -m "update readme"\nsorrel push origin\n```\n');
    run(bin, ['change', 'create', '-m', 'seed readme'], { cwd: WORKSPACE });
  } else {
    log(`Reusing demo workspace at ${WORKSPACE}`);
  }

  // Always refresh remote to the live Hub URL (add_remote replaces).
  const status = JSON.parse(
    run(bin, ['status', '--json'], { cwd: WORKSPACE }).stdout,
  );
  run(bin, ['remote', 'add', 'origin', HUB_URL, '--repo-id', status.repoId], {
    cwd: WORKSPACE,
  });

  writeFileSync(
    ENV_FILE,
    `# Sourced by scripts/dev-dashboard.mjs — local Sorrel test env
export SORREL="${bin}"
export PATH="$(dirname "$SORREL"):$PATH"
export SORREL_WORKSPACE="${WORKSPACE}"
export SORREL_HUB_URL="${HUB_URL}"
export SORREL_HUB_WEB_URL="${HUB_WEB_URL}"
export SORREL_DASHBOARD_URL="${DASHBOARD_URL}"
alias sorrel='"$SORREL"'
cd "$SORREL_WORKSPACE"
`,
  );
}

function openBrowser(url) {
  if (noOpen) return;
  try {
    const os = platform();
    if (os === 'darwin') execFileSync('open', [url], { stdio: 'ignore' });
    else if (os === 'win32') execFileSync('cmd', ['/c', 'start', '', url], { stdio: 'ignore' });
    else execFileSync('xdg-open', [url], { stdio: 'ignore' });
  } catch {
    log(`Open ${url} in your browser`);
  }
}

async function probe(url) {
  try {
    const res = await fetch(url, { signal: AbortSignal.timeout(2000) });
    const text = await res.text();
    let json = null;
    try {
      json = JSON.parse(text);
    } catch {
      /* ignore */
    }
    return { ok: res.ok, status: res.status, json, error: null };
  } catch (error) {
    return { ok: false, status: 0, json: null, error: error.message };
  }
}

function dashboardHtml() {
  const bin = sorrelBin();
  return `<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1" />
  <title>Sorrel · Dev Dashboard</title>
  <style>
    @import url("https://fonts.googleapis.com/css2?family=IBM+Plex+Sans:wght@400;600;700&family=IBM+Plex+Mono:wght@400;500&display=swap");
    :root {
      --bg: #2e3440; --elev: #3b4252; --card: #434c5e;
      --fg: #eceff4; --muted: #9aa5b8; --accent: #88c0d0;
      --ok: #a3be8c; --bad: #bf616a; --warn: #ebcb8b;
      --font: "IBM Plex Sans", "Segoe UI", sans-serif;
      --mono: "IBM Plex Mono", ui-monospace, monospace;
    }
    * { box-sizing: border-box; }
    body {
      margin: 0; min-height: 100vh; color: var(--fg); font-family: var(--font);
      background:
        radial-gradient(900px 480px at 8% -10%, rgba(136,192,208,.18), transparent 55%),
        linear-gradient(180deg, #242933, var(--bg) 50%, #323846);
    }
    header {
      display: flex; justify-content: space-between; align-items: center;
      padding: 1rem 1.5rem; border-bottom: 1px solid #4c566a;
      background: rgba(59,66,82,.85); backdrop-filter: blur(8px);
      position: sticky; top: 0;
    }
    .brand { font-weight: 700; font-size: 1.25rem; color: var(--accent); }
    .brand span { color: var(--muted); font-weight: 500; margin-left: .35rem; }
    main { max-width: 960px; margin: 0 auto; padding: 1.5rem; display: grid; gap: 1.1rem; }
    .grid { display: grid; gap: 1rem; grid-template-columns: repeat(auto-fit, minmax(240px, 1fr)); }
    .card {
      background: rgba(59,66,82,.75); border: 1px solid #4c566a; border-radius: 12px;
      padding: 1.1rem 1.2rem; box-shadow: 0 12px 36px rgba(0,0,0,.25);
    }
    h1 { margin: 0 0 .35rem; font-size: 1.5rem; letter-spacing: -.02em; }
    h2 { margin: 0 0 .75rem; font-size: 1rem; }
    p { margin: .35rem 0; color: var(--muted); }
    a.btn {
      display: inline-block; margin: .25rem .35rem .25rem 0; padding: .45rem .85rem;
      border-radius: 8px; background: var(--accent); color: #2e3440;
      text-decoration: none; font-weight: 700; font-size: .9rem;
    }
    a.btn.ghost { background: transparent; color: var(--fg); border: 1px solid #4c566a; }
    .pill {
      display: inline-block; padding: .15rem .55rem; border-radius: 999px;
      font-size: .72rem; font-weight: 700; text-transform: uppercase; letter-spacing: .04em;
    }
    .pill.ok { background: var(--ok); color: #2e3440; }
    .pill.bad { background: var(--bad); color: var(--fg); }
    .pill.wait { background: var(--warn); color: #2e3440; }
    pre, code { font-family: var(--mono); }
    pre {
      margin: .5rem 0 0; padding: .85rem; background: #242933; border-radius: 8px;
      overflow: auto; font-size: .8rem; line-height: 1.45; border: 1px solid #4c566a;
    }
    .row { display: flex; gap: .5rem; flex-wrap: wrap; align-items: center; }
    button.copy {
      background: transparent; border: 1px solid #4c566a; color: var(--muted);
      border-radius: 6px; padding: .25rem .5rem; cursor: pointer; font: inherit;
    }
    button.copy:hover { color: var(--fg); border-color: var(--accent); }
    footer { text-align: center; color: var(--muted); padding: 1.5rem; font-size: .85rem; }
  </style>
</head>
<body>
  <header>
    <div class="brand">Sorrel<span>Dev Dashboard</span></div>
    <div class="row" id="clock"></div>
  </header>
  <main>
    <section class="card">
      <h1>Local test stack</h1>
      <p>Hub API, Hub UI, CLI binary, and a seeded workspace — ready to poke.</p>
      <div class="row" style="margin-top:.75rem">
        <a class="btn" href="${HUB_WEB_URL}" target="_blank" rel="noreferrer">Open Hub UI</a>
        <a class="btn ghost" href="${HUB_URL}/healthz" target="_blank" rel="noreferrer">Hub /healthz</a>
        <a class="btn ghost" href="${HUB_URL}/admin/projects" target="_blank" rel="noreferrer">Projects API</a>
      </div>
    </section>

    <section class="grid">
      <article class="card">
        <h2>Hub API</h2>
        <div class="row"><span id="hub-pill" class="pill wait">checking</span><code>${HUB_URL}</code></div>
        <p id="hub-detail"></p>
      </article>
      <article class="card">
        <h2>Hub UI</h2>
        <div class="row"><span id="ui-pill" class="pill wait">checking</span><code>${HUB_WEB_URL}</code></div>
        <p id="ui-detail"></p>
      </article>
      <article class="card">
        <h2>CLI</h2>
        <div class="row"><span id="cli-pill" class="pill wait">checking</span></div>
        <p class="mono" id="cli-detail" style="word-break:break-all"></p>
      </article>
    </section>

    <section class="card">
      <div class="row" style="justify-content:space-between">
        <h2>Shell setup</h2>
        <button class="copy" type="button" data-copy="source ${ENV_FILE}">Copy source</button>
      </div>
      <pre>source ${ENV_FILE}
# or:
export SORREL=${bin}
cd ${WORKSPACE}</pre>
    </section>

    <section class="card">
      <div class="row" style="justify-content:space-between">
        <h2>Try these</h2>
        <button class="copy" type="button" data-copy-block="true">Copy all</button>
      </div>
      <pre id="cmds">$SORREL status --json
$SORREL change create -m "dashboard edit"
$SORREL push origin
$SORREL lane create --name feature
$SORREL lane submit
# then open Hub UI → Reviews</pre>
    </section>

    <section class="card">
      <h2>Paths</h2>
      <p><strong>Workspace</strong> <code>${WORKSPACE}</code></p>
      <p><strong>Hub data</strong> <code>${HUB_DATA}</code></p>
      <p><strong>Repo root</strong> <code>${ROOT}</code></p>
      <p style="margin-top:.75rem">Stop the stack with <code>Ctrl+C</code> in the terminal that launched this dashboard.</p>
    </section>
  </main>
  <footer>Sorrel local dashboard · proxies nothing · companion to hub + hub-web</footer>
  <script>
    async function refresh() {
      const res = await fetch('/api/status');
      const data = await res.json();
      const set = (id, ok, text) => {
        const pill = document.getElementById(id);
        pill.className = 'pill ' + (ok ? 'ok' : 'bad');
        pill.textContent = ok ? 'up' : 'down';
        if (text) document.getElementById(id.replace('-pill','-detail')).textContent = text;
      };
      set('hub-pill', data.hub.ok, data.hub.ok
        ? \`status=\${data.hub.json?.status ?? 'ok'} · grants ready\`
        : (data.hub.error || 'unreachable'));
      set('ui-pill', data.hubWeb.ok, data.hubWeb.ok ? 'static + /api proxy' : (data.hubWeb.error || 'unreachable'));
      set('cli-pill', data.cli.ok, data.cli.path + (data.cli.ok ? '' : ' (missing — rebuild)'));
      document.getElementById('clock').textContent = new Date().toLocaleTimeString();
    }
    document.querySelectorAll('button.copy').forEach((btn) => {
      btn.addEventListener('click', async () => {
        const text = btn.dataset.copy || document.getElementById('cmds').textContent;
        await navigator.clipboard.writeText(text.trim());
        btn.textContent = 'Copied';
        setTimeout(() => { btn.textContent = btn.dataset.copy ? 'Copy source' : 'Copy all'; }, 1200);
      });
    });
    refresh();
    setInterval(refresh, 2500);
  </script>
</body>
</html>`;
}

function startDashboard() {
  const server = createServer(async (req, res) => {
    const url = new URL(req.url ?? '/', DASHBOARD_URL);
    if (url.pathname === '/api/status') {
      const [hub, hubWeb] = await Promise.all([
        probe(`${HUB_URL}/healthz`),
        probe(HUB_WEB_URL),
      ]);
      const cliOk = existsSync(sorrelBin());
      res.writeHead(200, { 'content-type': 'application/json' });
      res.end(
        JSON.stringify({
          hub,
          hubWeb,
          cli: { ok: cliOk, path: sorrelBin() },
          workspace: WORKSPACE,
          urls: {
            hub: HUB_URL,
            hubWeb: HUB_WEB_URL,
            dashboard: DASHBOARD_URL,
          },
        }),
      );
      return;
    }
    res.writeHead(200, { 'content-type': 'text/html; charset=utf-8' });
    res.end(dashboardHtml());
  });

  server.listen(DASHBOARD_PORT, '127.0.0.1', () => {
    log(`Dashboard → ${DASHBOARD_URL}`);
  });
  children.push({ kill() { server.close(); } });
  return server;
}

function shutdown() {
  if (shuttingDown) return;
  shuttingDown = true;
  log('Shutting down…');
  for (const child of children.splice(0)) {
    try {
      if (typeof child.kill === 'function') child.kill('SIGTERM');
    } catch {
      /* ignore */
    }
  }
  setTimeout(() => process.exit(0), 400);
}

process.on('SIGINT', shutdown);
process.on('SIGTERM', shutdown);

async function main() {
  console.log('\nSorrel local test dashboard\n');
  ensureDirs();
  buildCli();
  buildHubWeb();
  if (!existsSync(sorrelBin())) fail(`CLI binary missing at ${sorrelBin()}`);

  log(`Starting Hub on :${HUB_PORT}`);
  start('npm', ['start'], {
    cwd: HUB_DIR,
    name: 'hub',
    env: {
      ...process.env,
      HOST: '127.0.0.1',
      PORT: String(HUB_PORT),
      SORREL_HUB_SYNC_STORE: 'fs',
      SORREL_HUB_DATA_DIR: join(HUB_DATA, 'sync'),
      SORREL_HUB_METADATA_DIR: join(HUB_DATA, 'metadata'),
      SORREL_HUB_BOOTSTRAP_GRANTS: '1',
    },
  });

  await waitForUrl(`${HUB_URL}/healthz`, { label: 'Hub /healthz' });
  log('Hub is healthy');

  seedWorkspace();

  log(`Starting Hub UI on :${HUB_WEB_PORT}`);
  start('npm', ['start'], {
    cwd: HUB_WEB_DIR,
    name: 'hub-web',
    env: {
      ...process.env,
      HOST: '127.0.0.1',
      PORT: String(HUB_WEB_PORT),
      HUB_API_URL: HUB_URL,
    },
  });
  await waitForUrl(HUB_WEB_URL, { label: 'Hub UI' });
  log('Hub UI is up');

  startDashboard();
  openBrowser(DASHBOARD_URL);

  console.log(`
Ready.
  Dashboard  ${DASHBOARD_URL}
  Hub UI     ${HUB_WEB_URL}
  Hub API    ${HUB_URL}
  Workspace  ${WORKSPACE}
  Env file   source ${ENV_FILE}

Press Ctrl+C to stop.
`);
}

main().catch((error) => {
  console.error(error);
  shutdown();
  process.exit(1);
});
