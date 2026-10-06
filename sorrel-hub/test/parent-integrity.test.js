import assert from 'node:assert/strict';
import fs from 'node:fs';
import http from 'node:http';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';

import { createApp } from '../src/app.js';
import { createFsMetadataStore } from '../src/fs-metadata-store.js';
import { InMemoryStore } from '../src/store.js';

const collections = ['organizations', 'projects', 'repositories', 'proposals', 'reviewComments', 'workflowRuns', 'policies'];
const records = (store) => Object.fromEntries(collections.map((name) => [name, [...store[name].values()].sort((a, b) => a.id.localeCompare(b.id))]));

async function withServer(t, persistent, callback) {
  const dir = persistent ? fs.mkdtempSync(path.join(os.tmpdir(), 'sorrel-parents-')) : undefined;
  if (dir) t.after(() => fs.rmSync(dir, { recursive: true, force: true }));
  const store = persistent ? createFsMetadataStore(dir) : new InMemoryStore();
  const trustedGrantsById = Object.fromEntries([['org', 'org_a'], ['org', 'org_b'], ['project', 'project_a'], ['project', 'missing']]
    .map(([kind, id]) => [`grant_${id}`, { id: `grant_${id}`, source: 'core', principal: { type: 'user', id: 'local' }, action: 'policy.grant', effect: 'allow', resource: { kind, id } }]));
  trustedGrantsById.grant_parent_fixture = {
    schemaVersion: 'sorrel.protocol.v0', kind: 'Grant', id: 'grant_parent_fixture',
    principal: { kind: 'user', id: 'local' }, effect: 'allow',
    resources: [{ kind: 'org', id: '*' }, { kind: 'project', id: '*' }],
    capabilities: ['org.write', 'project.read', 'project.create', 'policy.grant', 'proposal.read',
      'proposal.write', 'proposal.review', 'review.comment.write', 'workflow.run.write'],
  };
  const app = createApp({ store, trustedGrantsById, env: { SORREL_HUB_LOCAL_DEMO: '1' } });
  const server = http.createServer(app.handleRequest);
  await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
  try {
    const baseUrl = `http://127.0.0.1:${server.address().port}`;
    await callback(baseUrl, store, dir);
  } finally {
    await new Promise((resolve, reject) => server.close((error) => error ? reject(error) : resolve()));
  }
}

async function request(baseUrl, route, body, method = 'POST') {
  if (route === '/admin/repositories' || route === '/admin/policies') {
    const scope = route === '/admin/policies' ? body.projectId : body.organizationId;
    body = { ...body, grantRefs: [{ id: `grant_${scope}`, source: 'core' }] };
  }
  return await fetch(`${baseUrl}${route}`, {
    method, headers: { 'content-type': 'application/json', 'x-sorrel-acting-principal': JSON.stringify({ type: 'user', id: 'local' }) }, body: JSON.stringify(body),
  });
}

for (const persistent of [false, true]) {
  const backend = persistent ? 'filesystem' : 'memory';
  test(`${backend}: missing and cross-project parents reject before publication`, async (t) => {
    await withServer(t, persistent, async (baseUrl, store, dir) => {
      store.createProject({ id: 'project_a', organizationId: 'org_a', name: 'A' });
      store.createProject({ id: 'project_b', organizationId: 'org_b', name: 'B' });
      const repository = { provider: 'sorrel', owner: 'local', name: 'Repo' };
      store.createRepository({ ...repository, id: 'repo_a', projectId: 'project_a', organizationId: 'org_a' });
      store.createRepository({ ...repository, id: 'repo_b', projectId: 'project_b', organizationId: 'org_b' });
      const proposal = { title: 'Review', authorRef: 'user:local' };
      store.createProposal({ ...proposal, id: 'prop_a', projectId: 'project_a', repositoryId: 'repo_a' });
      store.createProposal({ ...proposal, id: 'prop_b', projectId: 'project_b' });
      store.createWorkflowRun({ id: 'run_b', projectId: 'project_b', name: 'CI' });
      store.createWorkflowRun({ id: 'run_other_proposal', projectId: 'project_a', proposalId: 'prop_a', name: 'CI' });
      const baseline = structuredClone(records(store));
      const cases = [
        ['/admin/repositories', { ...repository, projectId: 'missing', organizationId: 'org_a' }, 404],
        ['/admin/proposals', { ...proposal, projectId: 'missing' }, 404],
        ['/admin/workflow-runs', { projectId: 'missing', name: 'CI' }, 404],
        ['/admin/policies', { projectId: 'missing', organizationId: 'org_a', name: 'Policy' }, 404],
        ['/collaboration/lane-submit', { projectId: 'missing', title: 'Review', sourceLane: 'lane_feature', sourceSnapshot: 'aa'.repeat(32) }, 404],
        ['/admin/repositories', { ...repository, projectId: 'project_a', organizationId: 'org_b' }, 400],
        ['/admin/policies', { projectId: 'project_a', organizationId: 'org_b', name: 'Policy' }, 400],
        ['/admin/proposals', { ...proposal, projectId: 'project_a', repositoryId: 'missing' }, 404],
        ['/admin/proposals', { ...proposal, projectId: 'project_a', repositoryId: 'repo_b' }, 400],
        ['/admin/workflow-runs', { projectId: 'project_a', proposalId: 'missing', name: 'CI' }, 404],
        ['/admin/workflow-runs', { projectId: 'project_a', proposalId: 'prop_b', name: 'CI' }, 400],
        ['/admin/proposals', { ...proposal, projectId: 'project_a', workflowRunIds: ['missing'] }, 404],
        ['/admin/proposals', { ...proposal, projectId: 'project_a', workflowRunIds: ['run_b'] }, 400],
        ['/admin/proposals', { ...proposal, projectId: 'project_a', workflowRunIds: ['run_other_proposal'] }, 400],
        ['/admin/proposals/prop_a', { repositoryId: 'missing', title: 'Replacement' }, 404, 'PATCH'],
        ['/admin/proposals/prop_a', { repositoryId: 'repo_b', title: 'Replacement' }, 400, 'PATCH'],
        ['/admin/proposals/prop_a', { workflowRunIds: ['run_b'], title: 'Replacement' }, 400, 'PATCH'],
      ];
      for (const [route, body, expected, method] of cases) {
        const response = await request(baseUrl, route, body, method);
        assert.equal(response.status, expected, `${route}: ${JSON.stringify(body)}`);
        assert.equal((await response.json()).error.code, expected === 404 ? 'not_found' : 'model_validation_failed');
        assert.deepEqual(records(store), baseline);
        if (dir) assert.deepEqual(records(createFsMetadataStore(dir)), JSON.parse(JSON.stringify(baseline)));
      }
    });
  });

  test(`${backend}: valid local parents coexist with external Core and sync references`, async (t) => {
    await withServer(t, persistent, async (baseUrl, store, dir) => {
      const project = store.createProject({ id: 'project_a', organizationId: 'org_namespace', name: 'A' });
      const coreRefs = { policyRefs: [{ kind: 'Policy', id: 'external_policy' }], grantRefs: [{ id: 'external_grant', source: 'core' }] };
      const repository = store.createRepository({ projectId: project.id, organizationId: project.organizationId, provider: 'github', owner: 'remote', name: 'Repo', externalId: 'external_repo', ...coreRefs });
      const proposal = store.createProposal({ projectId: project.id, repositoryId: repository.id, syncRepoId: 'external_sync', title: 'Review', authorRef: 'user:local', sourceLane: 'external_lane', sourceSnapshot: 'aa'.repeat(32), ...coreRefs });
      const run = store.createWorkflowRun({ projectId: project.id, proposalId: proposal.id, name: 'CI', providerRunId: 'external_run', ...coreRefs });
      store.updateProposal(proposal.id, { workflowRunIds: [run.id] });
      store.updateWorkflowRun(run.id, { status: 'succeeded' });
      store.createPolicy({ projectId: project.id, organizationId: project.organizationId, name: 'Scoped', ...coreRefs });
      store.createPolicy({ organizationId: 'another_namespace', name: 'Organization only', ...coreRefs });
      assert.deepEqual(store.getProposal(proposal.id).workflowRunIds, [run.id]);
      assert.equal(store.getWorkflowRun(run.id).status, 'succeeded');
      const submitted = await request(baseUrl, '/collaboration/lane-submit', {
        projectId: project.id, repositoryId: 'legacy_sync_id', title: 'Legacy submit',
        sourceLane: 'lane_legacy', sourceSnapshot: 'bb'.repeat(32),
      });
      assert.equal(submitted.status, 201);
      const result = (await submitted.json()).data;
      assert.equal(result.syncRepoId, 'legacy_sync_id');
      assert.equal(result.repositoryId, undefined);
      if (dir) assert.deepEqual(records(createFsMetadataStore(dir)), JSON.parse(JSON.stringify(records(store))));
    });
  });
}
