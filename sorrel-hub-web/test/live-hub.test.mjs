import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { createInterface } from 'node:readline';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { test } from 'node:test';

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..');
const HUB_DIR = join(ROOT, '..', 'sorrel-hub');

async function spawnReady(cwd, script, env = {}) {
  const child = spawn('node', [script], {
    cwd,
    env: { ...process.env, ...env },
    stdio: ['ignore', 'pipe', 'inherit'],
  });
  const rl = createInterface({ input: child.stdout });
  const ready = await new Promise((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error('ready timeout')), 15000);
    rl.once('line', (line) => {
      clearTimeout(timer);
      resolve(JSON.parse(line));
    });
    child.once('exit', (code) => {
      clearTimeout(timer);
      reject(new Error(`process exited ${code}`));
    });
  });
  return { child, url: ready.url };
}

test('hub-web proxies live hub health and sync-repos', async () => {
  const hub = await spawnReady(HUB_DIR, 'scripts/listen.mjs', {
    SORREL_HUB_SYNC_STORE: 'memory',
    SORREL_HUB_BOOTSTRAP_GRANTS: '1',
  });
  try {
    const ui = await spawnReady(ROOT, 'scripts/listen.mjs', {
      HUB_API_URL: hub.url,
    });
    try {
      const health = await fetch(`${ui.url}/api/healthz`).then((r) => r.json());
      assert.equal(health.status, 'ok');
      assert.equal(health.service, 'sorrel-hub');

      const page = await fetch(`${ui.url}/`).then((r) => r.text());
      assert.match(page, /Sorrel Hub/);
      assert.match(page, /proposal-form/);

      const sync = await fetch(`${ui.url}/api/admin/sync-repos`).then((r) => r.json());
      assert.ok(Array.isArray(sync.repos));

      const project = await fetch(`${ui.url}/api/projects`, {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({
          organizationId: 'org_ui',
          name: 'UI Collab',
        }),
      }).then(async (r) => ({ status: r.status, body: await r.json() }));
      assert.equal(project.status, 201);

      const proposal = await fetch(`${ui.url}/api/admin/proposals`, {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({
          projectId: project.body.data.id,
          title: 'From hub-web proxy',
          authorPrincipal: { type: 'user', id: 'local' },
          status: 'open',
        }),
      }).then(async (r) => ({ status: r.status, body: await r.json() }));
      assert.equal(proposal.status, 201);

      const listed = await fetch(`${ui.url}/api/admin/proposals`).then((r) => r.json());
      assert.ok(listed.data.some((p) => p.id === proposal.body.data.id));
    } finally {
      ui.child.kill('SIGTERM');
    }
  } finally {
    hub.child.kill('SIGTERM');
  }
});
