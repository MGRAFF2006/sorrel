import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { createInterface } from 'node:readline';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { test } from 'node:test';

import { HubClient } from '../src/index.js';

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '../..');
const HUB_DIR = join(ROOT, 'sorrel-hub');

async function withLiveHub(callback) {
  const child = spawn('node', ['scripts/listen.mjs'], {
    cwd: HUB_DIR,
    env: {
      ...process.env,
      SORREL_HUB_SYNC_STORE: 'memory',
      SORREL_HUB_BOOTSTRAP_GRANTS: '1',
    },
    stdio: ['ignore', 'pipe', 'inherit'],
  });
  const rl = createInterface({ input: child.stdout });
  const ready = await new Promise((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error('hub ready timeout')), 10000);
    rl.once('line', (line) => {
      clearTimeout(timer);
      resolve(JSON.parse(line));
    });
    child.once('exit', (code) => {
      clearTimeout(timer);
      reject(new Error(`hub exited ${code}`));
    });
  });
  try {
    return await callback(ready.url);
  } finally {
    child.kill('SIGTERM');
  }
}

test('HubClient health and projects against live hub', async () => {
  await withLiveHub(async (baseUrl) => {
    const client = new HubClient({ baseUrl });
    const health = await client.health();
    assert.equal(health.status, 'ok');

    const projects = await client.listProjects();
    assert.ok(Array.isArray(projects.data));

    const created = await client.createProject({
      name: 'sdk-js-e2e',
      organizationId: 'org_sdk',
    });
    assert.ok(created.data.id.startsWith('proj_'));

    const sync = await client.listSyncRepos();
    assert.ok(Array.isArray(sync.repos));

    const proposal = await client.createProposal({
      projectId: created.data.id,
      title: 'SDK proposal',
      authorPrincipal: { type: 'user', id: 'local' },
      sourceLane: 'lane_feature',
      status: 'open',
    });
    assert.ok(proposal.data.id.startsWith('prop_'));

    const comment = await client.createReviewComment({
      proposalId: proposal.data.id,
      body: 'from sdk-js',
      authorPrincipal: { type: 'user', id: 'local' },
    });
    assert.ok(comment.data.id.startsWith('comment_'));

    const detail = await client.getProposal(proposal.data.id, { includeComments: true });
    assert.equal(detail.data.comments.length, 1);

    const submitted = await client.laneSubmit({
      projectId: created.data.id,
      title: 'Lane tip',
      sourceLane: 'lane_sdk',
      sourceSnapshot: 'cc'.repeat(32),
      syncRepoId: 'repo_sdk',
    });
    assert.equal(submitted.reused, false);
    assert.equal(submitted.data.status, 'open');
  });
});
