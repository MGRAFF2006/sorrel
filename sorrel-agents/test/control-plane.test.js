import assert from 'node:assert/strict';
import fs, { mkdtempSync } from 'node:fs';
import { syncBuiltinESMExports } from 'node:module';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';

import { AgentControlPlane, DEFAULT_CLAIM_TTL_MS } from '../src/index.js';

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

test('claims with colons in agent ids and paths do not overwrite each other', async (t) => {
  const workspace = mkdtempSync(join(tmpdir(), 'sorrel-agents-'));
  t.after(() => fs.rmSync(workspace, { recursive: true, force: true }));
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
  t.after(() => fs.rmSync(stateDir, { recursive: true, force: true }));
  const plane = new AgentControlPlane({ stateDir });
  fs.mkdirSync(join(stateDir, 'state.json'));
  await assert.rejects(plane.registerAgent({ id: 'rejected' }));
  assert.deepEqual([...plane.agents.values()], []);
  fs.rmSync(join(stateDir, 'state.json'), { recursive: true });
  assert.deepEqual((await plane.activeWork()).agents, []);
  assert.deepEqual(fs.readdirSync(stateDir), []);
});

function leasedPlane(t) {
  const stateDir = mkdtempSync(join(tmpdir(), 'sorrel-agents-leases-'));
  t.after(() => fs.rmSync(stateDir, { recursive: true, force: true }));
  let time = Date.parse('2026-10-15T12:00:00.000Z');
  const options = { stateDir, now: () => time };
  return { plane: new AgentControlPlane(options), options, advance: (ms) => { time += ms; } };
}

test('release is owner-specific and idempotent, and heartbeat never revives inactive claims', async (t) => {
  const { plane, options, advance } = leasedPlane(t);
  for (const id of ['first', 'second']) await plane.registerAgent({ id });
  await plane.claimPath({ agentId: 'first', path: 'shared', ttlMs: 100 });
  await plane.claimPath({ agentId: 'second', path: 'shared', ttlMs: 200 });
  await plane.claimPath({ agentId: 'first', path: 'expired', ttlMs: 50 });
  const original = await plane.claimPath({ agentId: 'first', path: 'active', mode: 'blocking' });
  assert.equal(Date.parse(original.expiresAt) - Date.parse(original.claimedAt), DEFAULT_CLAIM_TTL_MS);
  assert.equal(await plane.releaseClaim({ agentId: 'first', path: './shared/' }), true);
  assert.equal(await plane.releaseClaim({ agentId: 'first', path: 'shared' }), false);
  assert.equal(await plane.releaseClaim({ agentId: 'stranger', path: 'active' }), false);
  advance(50); // Exactly at expiry is inactive.
  const renewed = await plane.heartbeat({ agentId: 'first', ttlMs: 300 });
  assert.deepEqual(renewed.map((claim) => claim.path), ['active']);
  assert.equal(renewed[0].claimedAt, original.claimedAt);
  assert.equal(renewed[0].mode, 'blocking');
  assert.equal(Date.parse(renewed[0].expiresAt) - Date.parse(renewed[0].renewedAt), 300);
  const restored = new AgentControlPlane(options);
  assert.deepEqual((await restored.activeWork()).claims, [
    plane.claims.get(JSON.stringify(['second', 'shared'])), renewed[0],
  ]);
  advance(300);
  assert.deepEqual(await restored.heartbeat({ agentId: 'first' }), []);
  const active = await restored.activeWork();
  assert.deepEqual(active.claims, []);
  assert.equal(active.agents.length, 2);
  assert.equal(restored.claims.has(JSON.stringify(['first', 'expired'])), true);
  const reclaimed = await restored.claimPath({ agentId: 'first', path: 'expired' });
  assert.notEqual(reclaimed.claimedAt, original.claimedAt);
  assert.deepEqual((await restored.activeWork()).claims, [reclaimed]);
  await restored.releaseClaim({ agentId: 'first', path: 'expired' });
  const replaced = await restored.claimPath({ agentId: 'first', path: 'expired', ttlMs: 1 });
  assert.deepEqual((await restored.activeWork()).claims, [replaced]);
  await assert.rejects(restored.heartbeat({ agentId: 'missing' }), /unknown agent/);
  await assert.rejects(restored.heartbeat({}), /requires agentId/);
  await assert.rejects(restored.releaseClaim({}), /requires agentId and path/);
});

test('legacy records keep original timestamps and no expiry until explicit renewal or release', async (t) => {
  const { options, advance } = leasedPlane(t);
  const legacy = { agentId: 'old', path: './src/old', claimedAt: '2020-01-01T00:00:00.000Z', mode: 'advisory' };
  const removed = { ...legacy, path: '/legacy/opaque' };
  const state = { agents: [{ id: 'old', registeredAt: legacy.claimedAt }], claims: [legacy, removed] };
  const file = join(options.stateDir, 'state.json');
  fs.writeFileSync(file, JSON.stringify(state));
  advance(365 * 24 * 60 * 60 * 1000);
  const restored = new AgentControlPlane(options);
  assert.deepEqual((await restored.activeWork()).claims, [legacy, removed]);
  assert.deepEqual(JSON.parse(fs.readFileSync(file, 'utf8')), state);
  assert.equal(await restored.releaseClaim({ agentId: 'old', path: removed.path }), true);
  assert.equal(await restored.releaseClaim({ agentId: 'old', path: removed.path }), false);
  const renewed = await restored.heartbeat({ agentId: 'old' });
  assert.equal(renewed[0].claimedAt, legacy.claimedAt);
  assert.equal(renewed[0].path, legacy.path);
  assert.ok(renewed[0].expiresAt);
  assert.deepEqual((await new AgentControlPlane(options).activeWork()).claims, renewed);
  assert.equal(await restored.releaseClaim({ agentId: 'old', path: legacy.path }), true);
  advance(DEFAULT_CLAIM_TTL_MS);
  assert.deepEqual((await restored.activeWork()).claims, []);
});

test('active-work warnings cover same, ancestor and descendant paths across agents only', async (t) => {
  const { plane, advance } = leasedPlane(t);
  for (const id of ['first', 'second']) await plane.registerAgent({ id });
  for (const path of ['src/a', 'src/a/deep']) await plane.claimPath({ agentId: 'first', path });
  for (const path of ['src/a', 'src/a/child', 'src/ab']) await plane.claimPath({ agentId: 'second', path });
  await plane.claimPath({ agentId: 'second', path: 'src', ttlMs: 1 });
  let warnings = (await plane.activeWork()).warnings;
  assert.equal(warnings.length, 5);
  advance(1);
  warnings = (await plane.activeWork()).warnings;
  assert.equal(warnings.length, 3);
  assert.deepEqual(warnings.map(({ type, claims }) => [type, claims.map((claim) => claim.path)]), [
    ['overlap', ['src/a', 'src/a']],
    ['overlap', ['src/a', 'src/a/child']],
    ['overlap', ['src/a/deep', 'src/a']],
  ]);
});

test('new paths normalize separators and dots; invalid paths and ttl values cannot mutate state', async () => {
  const plane = new AgentControlPlane();
  await plane.registerAgent({ id: 'agent' });
  const claim = await plane.claimPath({ agentId: 'agent', path: './src//a/./file/' });
  assert.equal(claim.path, 'src/a/file');
  const windows = await plane.claimPath({ agentId: 'agent', path: String.raw`src\a\file` });
  assert.equal(windows.path, claim.path);
  for (const path of ['', ' ', './ ', '././ /', '.', '/', '/abs', '../a', 'a/../b', String.raw`a\..\b`,
    String.raw`C:\file`, 'C:file', './C:/repo', '././c:repo', './C:/repo/file', String.raw`.\C:\repo`, String.raw`\\host\file`, 'a\0b', 'a\nb', 1]) {
    await assert.rejects(plane.claimPath({ agentId: 'agent', path }), /path/);
  }
  for (const ttlMs of [0, -1, 1.5, NaN, Infinity, null, '100', Number.MAX_SAFE_INTEGER + 1]) {
    await assert.rejects(plane.claimPath({ agentId: 'agent', path: 'invalid', ttlMs }), /ttlMs/);
    await assert.rejects(plane.heartbeat({ agentId: 'agent', ttlMs }), /ttlMs/);
  }
  await assert.rejects(plane.heartbeat({ agentId: 'agent', ttlMs: Number.MAX_SAFE_INTEGER }), /Invalid time value/);
  assert.deepEqual((await plane.activeWork()).claims, [windows]);
});

test('failed release and renewal leave persisted state and shallow snapshots unchanged', async (t) => {
  const { plane, options, advance } = leasedPlane(t);
  await plane.registerAgent({ id: 'agent' });
  const claim = await plane.claimPath({ agentId: 'agent', path: 'file' });
  const previous = fs.readFileSync(join(options.stateDir, 'state.json'), 'utf8');
  advance(100);
  const rename = t.mock.method(fs, 'renameSync', () => { throw new Error('simulated disk failure'); });
  syncBuiltinESMExports();
  try {
    for (const update of [() => plane.heartbeat({ agentId: 'agent' }),
      () => plane.releaseClaim({ agentId: 'agent', path: 'file' })]) {
      await assert.rejects(update(), /simulated disk failure/);
      assert.deepEqual([...plane.claims.values()], [claim]);
      assert.equal(fs.readFileSync(join(options.stateDir, 'state.json'), 'utf8'), previous);
      assert.deepEqual(fs.readdirSync(options.stateDir), ['state.json']);
    }
  } finally {
    rename.mock.restore();
    syncBuiltinESMExports();
  }
  assert.equal(claim.renewedAt, undefined);
  assert.equal(await plane.releaseClaim({ agentId: 'agent', path: 'file' }), true);
});

test('release and heartbeat reload latest state across independent processes', async (t) => {
  const { plane, options } = leasedPlane(t);
  for (const id of ['first', 'second']) await plane.registerAgent({ id });
  const moduleUrl = new URL('../src/index.js', import.meta.url).href;
  const script = `
    const { AgentControlPlane } = await import(process.argv[1]);
    const plane = new AgentControlPlane({ stateDir: process.argv[2], now: () => Number(process.argv[4]) });
    const agentId = process.argv[3];
    for (let index = 0; index < 20; index++) {
      await plane.claimPath({ agentId, path: agentId + '/' + index });
      await plane.claimPath({ agentId, path: agentId + '/temporary' });
      await plane.heartbeat({ agentId });
      await plane.releaseClaim({ agentId, path: agentId + '/temporary' });
    }
  `;
  await Promise.all(['first', 'second'].map((id) =>
    promisify(execFile)(process.execPath, ['--input-type=module', '-e', script, moduleUrl, options.stateDir, id, String(options.now())]),
  ));
  const state = await plane.activeWork();
  assert.equal(state.claims.length, 40);
  for (const id of ['first', 'second']) {
    assert.equal(state.claims.filter((claim) => claim.agentId === id).length, 20);
  }
  assert.deepEqual(fs.readdirSync(options.stateDir), ['state.json']);
});

function legacyAliases(options) {
  const state = {
    agents: [{ id: 'old' }, { id: 'other' }],
    claims: [
      { agentId: 'old', path: './src/old', claimedAt: '2020-01-01T00:00:00.000Z' },
      { agentId: 'old', path: 'src//old/', claimedAt: '2020-01-02T00:00:00.000Z' },
      { agentId: 'old', path: 'src/old/child', claimedAt: '2020-01-03T00:00:00.000Z' },
      { agentId: 'other', path: './src/old', claimedAt: '2020-01-04T00:00:00.000Z' },
    ],
  };
  fs.writeFileSync(join(options.stateDir, 'state.json'), JSON.stringify(state));
  return state;
}

test('explicit reclaim consolidates legacy aliases and one release removes the whole owner/path', async (t) => {
  const { plane, options } = leasedPlane(t);
  const legacy = legacyAliases(options);
  assert.deepEqual((await plane.activeWork()).claims, legacy.claims);
  assert.deepEqual(JSON.parse(fs.readFileSync(join(options.stateDir, 'state.json'), 'utf8')), legacy);
  const claimed = await plane.claimPath({ agentId: 'old', path: './src/old' });
  assert.equal(claimed.path, 'src/old');
  const active = await new AgentControlPlane(options).activeWork();
  assert.deepEqual(active.claims, [legacy.claims[2], legacy.claims[3], claimed]);
  assert.equal(await plane.releaseClaim({ agentId: 'old', path: './src/old' }), true);
  assert.equal(await plane.releaseClaim({ agentId: 'old', path: './src/old' }), false);
  assert.equal(await plane.releaseClaim({ agentId: 'old', path: 'src/old' }), false);
  assert.deepEqual((await plane.activeWork()).claims, [legacy.claims[2], legacy.claims[3]]);

  legacyAliases(options);
  assert.equal(await plane.releaseClaim({ agentId: 'old', path: 'src/old' }), true);
  assert.equal(await plane.releaseClaim({ agentId: 'old', path: './src/old' }), false);
  assert.deepEqual((await new AgentControlPlane(options).activeWork()).claims, [legacy.claims[2], legacy.claims[3]]);
});

test('failed legacy consolidation and release preserve all alias records and original timestamps', async (t) => {
  const { plane, options } = leasedPlane(t);
  const legacy = legacyAliases(options);
  const previous = fs.readFileSync(join(options.stateDir, 'state.json'), 'utf8');
  const snapshot = (await plane.activeWork()).claims;
  const rename = t.mock.method(fs, 'renameSync', () => { throw new Error('simulated disk failure'); });
  syncBuiltinESMExports();
  try {
    for (const update of [() => plane.claimPath({ agentId: 'old', path: 'src/old' }),
      () => plane.releaseClaim({ agentId: 'old', path: './src/old' })]) {
      await assert.rejects(update(), /simulated disk failure/);
      assert.deepEqual([...plane.claims.values()], legacy.claims);
      assert.deepEqual(snapshot, legacy.claims);
      assert.equal(fs.readFileSync(join(options.stateDir, 'state.json'), 'utf8'), previous);
      assert.deepEqual(fs.readdirSync(options.stateDir), ['state.json']);
    }
  } finally {
    rename.mock.restore();
    syncBuiltinESMExports();
  }
});
