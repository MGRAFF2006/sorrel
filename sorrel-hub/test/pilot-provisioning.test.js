import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { pilotGrants, provisionPilot } from '../scripts/provision-pilot.mjs';
import { createFsMetadataStore } from '../src/fs-metadata-store.js';
import { evaluate } from '../src/core-policy.js';

test('pilot provisioning uses exact Core scopes, persists and safely repeats', t => {
  const root = mkdtempSync(join(tmpdir(), 'sorrel-pilot-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const options = { subject: 'user_alice', organization: 'org_team', project: 'proj_team', repository: 'repo_team',
    metadataDir: join(root, 'metadata'), grantsFile: join(root, 'config/grants.json') };
  assert.equal(provisionPilot(options).written, false);
  const result = provisionPilot({ ...options, write: true });
  assert.deepEqual(Object.values(result.grants).map(g => g.resource.id), ['org_team', 'proj_team', 'repo_team']);
  assert.ok(Object.values(result.grants).every(g => g.principal.id === 'workos:user_alice' && !g.capabilities.includes('policy.grant')));
  const before = readFileSync(options.grantsFile);
  provisionPilot({ ...options, write: true });
  assert.deepEqual(readFileSync(options.grantsFile), before);
  const store = createFsMetadataStore(options.metadataDir);
  assert.deepEqual(store.projects.get('proj_team').repositoryIds, ['repo_team']);
  provisionPilot({ ...options, subject: 'user_bob', write: true });
  assert.equal(Object.keys(JSON.parse(readFileSync(options.grantsFile))).length, 6);
  assert.throws(() => provisionPilot({ ...options, subject: 'user_charlie', organization: 'org_other', write: true }), /another workspace/);
  assert.throws(() => pilotGrants({ ...options, repository: '*' }), /concrete identifier/);
});

test('pilot grants authorize the invited workspace through native Core and exclude policy administration', async () => {
  const options = { subject: 'user_alice', organization: 'org_team', project: 'proj_team', repository: 'repo_team' };
  const grants = Object.values(pilotGrants(options));
  const principal = { kind: 'user', id: 'workos:user_alice' };
  for (const [action, resource, allowed, actingPrincipal = principal] of [
    ['org.read', { kind: 'org', id: 'org_team' }, true],
    ['proposal.review', { kind: 'project', id: 'proj_team' }, true],
    ['repo.ref.write', { kind: 'repo', id: 'repo_team' }, true],
    ['policy.grant', { kind: 'project', id: 'proj_team' }, false],
    ['project.create', { kind: 'org', id: 'org_team' }, false],
    ['repo.read', { kind: 'repo', id: 'repo_other' }, false],
    ['repo.read', { kind: 'repo', id: 'repo_team' }, false, { kind: 'user', id: 'workos:user_bob' }],
  ]) {
    assert.equal((await evaluate({ principal: actingPrincipal, action, resource, grants })).allowed, allowed, action);
  }
});
