import assert from 'node:assert/strict';
import fs, { mkdtempSync } from 'node:fs';
import { syncBuiltinESMExports } from 'node:module';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';

import { AgentControlPlane } from '../src/index.js';

test('register agent, claim path, list active work', async () => {
  const workspace = mkdtempSync(join(tmpdir(), 'sorrel-agents-'));
  const plane = new AgentControlPlane({ workspace });

  const agent = await plane.registerAgent({ id: 'agent_test', lane: 'lane_main' });
  assert.equal(agent.id, 'agent_test');

  const claim = await plane.claimPath({ agentId: 'agent_test', path: 'src/lib.rs' });
  assert.equal(claim.path, 'src/lib.rs');
  assert.equal(claim.mode, 'advisory');

  const active = await plane.activeWork();
  assert.equal(active.agents.length, 1);
  assert.equal(active.claims.length, 1);

  // Persist across instances
  const again = new AgentControlPlane({ workspace });
  const restored = await again.activeWork();
  assert.equal(restored.agents[0].id, 'agent_test');
  assert.equal(restored.claims[0].path, 'src/lib.rs');
});

test('claimPath rejects unknown agents', async () => {
  const plane = new AgentControlPlane();
  await assert.rejects(
    () => plane.claimPath({ agentId: 'missing', path: 'a.txt' }),
    /unknown agent/,
  );
});

test('independent writers preserve registrations and claims and readers refresh', async (t) => {
  const stateDir = mkdtempSync(join(tmpdir(), 'sorrel-agents-writers-'));
  t.after(() => fs.rmSync(stateDir, { recursive: true, force: true }));
  const first = new AgentControlPlane({ stateDir });
  const second = new AgentControlPlane({ stateDir });
  await Promise.all([
    first.registerAgent({ id: 'first' }),
    second.registerAgent({ id: 'second' }),
  ]);
  await second.claimPath({ agentId: 'first', path: 'shared.txt' });
  await first.claimPath({ agentId: 'second', path: 'other.txt' });
  const state = await second.activeWork();
  assert.deepEqual(state.agents.map((agent) => agent.id).sort(), ['first', 'second']);
  assert.equal(state.claims.length, 2);
});

test('failed atomic replacement leaves both cached and persisted state unchanged', async (t) => {
  const stateDir = mkdtempSync(join(tmpdir(), 'sorrel-agents-failure-'));
  t.after(() => fs.rmSync(stateDir, { recursive: true, force: true }));
  const plane = new AgentControlPlane({ stateDir });
  await plane.registerAgent({ id: 'existing' });
  const previous = fs.readFileSync(join(stateDir, 'state.json'), 'utf8');
  const rename = t.mock.method(fs, 'renameSync', () => { throw new Error('simulated disk failure'); });
  syncBuiltinESMExports();
  try {
    await assert.rejects(plane.registerAgent({ id: 'failed' }), /simulated disk failure/);
  } finally {
    rename.mock.restore();
    syncBuiltinESMExports();
  }
  assert.equal(plane.agents.has('failed'), false);
  assert.equal(fs.readFileSync(join(stateDir, 'state.json'), 'utf8'), previous);
  assert.deepEqual(fs.readdirSync(stateDir), ['state.json']);
  await plane.registerAgent({ id: 'recovered' });
  assert.equal((await plane.activeWork()).agents.length, 2);
});

test('independent processes serialize state updates', async (t) => {
  const stateDir = mkdtempSync(join(tmpdir(), 'sorrel-agents-processes-'));
  t.after(() => fs.rmSync(stateDir, { recursive: true, force: true }));
  const moduleUrl = new URL('../src/index.js', import.meta.url).href;
  const script = `
    const { AgentControlPlane } = await import(process.argv[1]);
    const plane = new AgentControlPlane({ stateDir: process.argv[2] });
    for (let index = 0; index < 20; index++) {
      await plane.registerAgent({ id: process.argv[3] + index });
    }
  `;
  await Promise.all(['first', 'second'].map((prefix) =>
    promisify(execFile)(process.execPath, ['--input-type=module', '-e', script, moduleUrl, stateDir, prefix]),
  ));
  const state = await new AgentControlPlane({ stateDir }).activeWork();
  assert.equal(state.agents.length, 40);
  assert.deepEqual(fs.readdirSync(stateDir), ['state.json']);
});
