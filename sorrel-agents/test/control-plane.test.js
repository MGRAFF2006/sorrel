import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, readdirSync, symlinkSync, writeFileSync } from 'node:fs';
import { execFile } from 'node:child_process';
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

test('stale instances reload agents and independently preserve writes', async () => {
  const workspace = mkdtempSync(join(tmpdir(), 'sorrel-agents-'));
  const first = new AgentControlPlane({ workspace });
  const second = new AgentControlPlane({ workspace });
  await first.registerAgent({ id: 'first', workspace: '/tmp/first', task: 'parser' });
  await second.registerAgent({ id: 'second' });
  await second.claimPath({ agentId: 'first', path: 'src/a' });
  await first.claimPath({ agentId: 'second', path: 'src/b' });
  assert.equal((await first.activeWork()).agents.length, 2);
  assert.equal((await second.activeWork()).claims.length, 2);
  assert.equal((await second.activeWork()).agents.find((agent) => agent.id === 'first').task, 'parser');
});

test('independent child processes preserve simultaneous registrations and claims', async () => {
  const workspace = mkdtempSync(join(tmpdir(), 'sorrel-agents-'));
  const source = new URL('../src/index.js', import.meta.url).href;
  const children = Array.from({ length: 8 }, (_, index) => new Promise((resolve, reject) => {
    const script = `
      import { AgentControlPlane } from ${JSON.stringify(source)};
      const plane = new AgentControlPlane({ workspace: ${JSON.stringify(workspace)} });
      await plane.registerAgent({ id: 'agent_${index}' });
      await plane.claimPath({ agentId: 'agent_${index}', path: 'src/${index}' });
    `;
    execFile(process.execPath, ['--input-type=module', '-e', script], (error, stdout, stderr) => {
      if (error) reject(new Error(stderr || error.message));
      else resolve();
    });
  }));
  await Promise.all(children);
  const active = await new AgentControlPlane({ workspace }).activeWork();
  assert.equal(active.agents.length, 8);
  assert.equal(active.claims.length, 8);
});

test('canonical claims show directory overlaps and release across instances', async () => {
  const workspace = mkdtempSync(join(tmpdir(), 'sorrel-agents-'));
  const plane = new AgentControlPlane({ workspace });
  for (const id of ['a', 'b', 'c']) await plane.registerAgent({ id });
  await plane.claimPath({ agentId: 'a', path: './src//lib/' });
  await plane.claimPath({ agentId: 'a', path: 'src\\lib' });
  await plane.claimPath({ agentId: 'b', path: 'src/lib/file.rs' });
  await plane.claimPath({ agentId: 'c', path: 'src/library' });
  const active = await plane.activeWork();
  assert.equal(active.claims.length, 3);
  assert.deepEqual(active.overlaps, [{ path: 'src/lib', agentIds: ['a', 'b'] }]);
  const second = new AgentControlPlane({ workspace });
  assert.equal(await second.releasePath({ agentId: 'a', path: './src/lib' }), true);
  assert.equal(await second.releasePath({ agentId: 'a', path: './src/lib' }), false);
  assert.equal((await plane.activeWork()).claims.length, 2);
  assert.deepEqual((await plane.activeWork()).overlaps, []);
});

test('unsafe IDs, paths and unsupported blocking claims fail without writing', async () => {
  const plane = new AgentControlPlane();
  for (const id of ['../escape', '', 'a/b', 'ä', 'x'.repeat(65)]) {
    await assert.rejects(() => plane.registerAgent({ id }), /agent id/);
  }
  await plane.registerAgent({ id: 'safe' });
  for (const path of ['', '/etc/passwd', '../escape', 'src/../escape', 'C:\\file', '\\server', '.', 'a\0b']) {
    await assert.rejects(() => plane.claimPath({ agentId: 'safe', path }), /claim path/);
  }
  await assert.rejects(() => plane.claimPath({ agentId: 'safe', path: 'src', mode: 'blocking' }), /not implemented/);
  assert.equal((await plane.activeWork()).claims.length, 0);
});

test('legacy migration preserves records and does not resurrect released claims', async () => {
  const stateDir = mkdtempSync(join(tmpdir(), 'sorrel-agents-'));
  const legacy = {
    agents: [{ id: 'legacy', lane: 'lane_main', displayName: 'Legacy', registeredAt: '2026-01-01T00:00:00.000Z' }],
    claims: [{ agentId: 'legacy', path: './src//file', mode: 'advisory', claimedAt: '2026-01-01T00:00:00.000Z' }],
  };
  writeFileSync(join(stateDir, 'state.json'), JSON.stringify(legacy));
  const plane = new AgentControlPlane({ stateDir });
  assert.equal((await plane.activeWork()).claims[0].path, 'src/file');
  assert.deepEqual(JSON.parse(readFileSync(join(stateDir, 'state.migrated.json'), 'utf8')), legacy);
  await plane.releasePath({ agentId: 'legacy', path: 'src/file' });
  assert.equal((await new AgentControlPlane({ stateDir }).activeWork()).claims.length, 0);
});

test('legacy migration never overwrites newer records', async () => {
  const stateDir = mkdtempSync(join(tmpdir(), 'sorrel-agents-'));
  const plane = new AgentControlPlane({ stateDir });
  await plane.registerAgent({ id: 'existing', displayName: 'New' });
  writeFileSync(join(stateDir, 'state.json'), JSON.stringify({
    agents: [{ id: 'existing', lane: 'lane_old', displayName: 'Old', registeredAt: '2026-01-01T00:00:00.000Z' }],
    claims: [],
  }));
  assert.equal((await new AgentControlPlane({ stateDir }).activeWork()).agents[0].displayName, 'New');
});

test('corrupted records fail closed and cannot be hidden by an instance cache', async () => {
  const stateDir = mkdtempSync(join(tmpdir(), 'sorrel-agents-'));
  const plane = new AgentControlPlane({ stateDir });
  await plane.registerAgent({ id: 'safe' });
  writeFileSync(join(stateDir, 'agents', 'safe.json'), '{');
  await assert.rejects(() => plane.activeWork(), /invalid registry record/);
  await assert.rejects(() => plane.registerAgent({ id: 'another' }), /invalid registry record/);
  assert.throws(() => new AgentControlPlane({ stateDir }), /invalid registry record/);
});

test('corrupt legacy state is preserved without partial migration', () => {
  const stateDir = mkdtempSync(join(tmpdir(), 'sorrel-agents-'));
  const legacy = { agents: [{ id: '../unsafe' }], claims: [] };
  writeFileSync(join(stateDir, 'state.json'), JSON.stringify(legacy));
  assert.throws(() => new AgentControlPlane({ stateDir }), /agent id/);
  assert.deepEqual(JSON.parse(readFileSync(join(stateDir, 'state.json'), 'utf8')), legacy);
  assert.deepEqual(readdirSync(join(stateDir, 'agents')), []);
});

test('records and registry directories reject symlinks', async () => {
  const stateDir = mkdtempSync(join(tmpdir(), 'sorrel-agents-'));
  const plane = new AgentControlPlane({ stateDir });
  const external = join(stateDir, 'external');
  writeFileSync(external, '{}');
  symlinkSync(external, join(stateDir, 'agents', 'fake.json'));
  await assert.rejects(() => plane.activeWork(), /regular file/);
  const aliased = join(stateDir, 'alias');
  symlinkSync(join(stateDir, 'claims'), aliased);
  assert.throws(() => new AgentControlPlane({ stateDir: aliased }), /symlink/);
});

test('live readers tolerate concurrent registration and claim release', async () => {
  const workspace = mkdtempSync(join(tmpdir(), 'sorrel-agents-'));
  const plane = new AgentControlPlane({ workspace });
  const source = new URL('../src/index.js', import.meta.url).href;
  const script = `
    import { AgentControlPlane } from ${JSON.stringify(source)};
    const plane = new AgentControlPlane({ workspace: ${JSON.stringify(workspace)} });
    for (let i = 0; i < 32; i++) {
      const agentId = 'live_' + i;
      await plane.registerAgent({ id: agentId });
      await plane.claimPath({ agentId, path: 'src/' + i });
      await plane.releasePath({ agentId, path: 'src/' + i });
    }
  `;
  let done = false;
  const child = new Promise((resolve, reject) => {
    execFile(process.execPath, ['--input-type=module', '-e', script], (error, stdout, stderr) => {
      done = true;
      if (error) reject(new Error(stderr || error.message));
      else resolve();
    });
  });
  while (!done) {
    await plane.activeWork();
    await new Promise((resolve) => setImmediate(resolve));
  }
  await child;
  assert.equal((await plane.activeWork()).agents.length, 32);
  assert.deepEqual((await plane.activeWork()).claims, []);
});
