import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, readdirSync, rmSync } from 'node:fs';
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

test('claims with colons in agent ids and paths do not overwrite each other', async (t) => {
  const workspace = mkdtempSync(join(tmpdir(), 'sorrel-agents-'));
  t.after(() => rmSync(workspace, { recursive: true, force: true }));
  const plane = new AgentControlPlane({ workspace });
  await plane.registerAgent({ id: 'agent:one' });
  await plane.registerAgent({ id: 'agent' });
  await plane.claimPath({ agentId: 'agent:one', path: 'file' });
  await plane.claimPath({ agentId: 'agent', path: 'one:file' });
  assert.equal((await plane.activeWork()).claims.length, 2);
  const restored = new AgentControlPlane({ workspace });
  assert.equal((await restored.activeWork()).claims.length, 2);
});

test('failed persistence leaves no rejected registration or temporary files', async (t) => {
  const stateDir = mkdtempSync(join(tmpdir(), 'sorrel-agents-write-'));
  t.after(() => rmSync(stateDir, { recursive: true, force: true }));
  const plane = new AgentControlPlane({ stateDir });
  mkdirSync(join(stateDir, 'state.json'));
  await assert.rejects(plane.registerAgent({ id: 'rejected' }));
  assert.deepEqual((await plane.activeWork()).agents, []);
  assert.deepEqual(readdirSync(stateDir), ['state.json']);
});
