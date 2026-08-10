import assert from 'node:assert/strict';
import { mkdtempSync } from 'node:fs';
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
