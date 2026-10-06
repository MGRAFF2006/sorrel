import assert from 'node:assert/strict';
import http from 'node:http';
import test from 'node:test';

import { HubClient } from '@sorrel/sdk-js';

import { createApp } from '../../sorrel-hub/src/app.js';
import { createInMemoryStore } from '../../sorrel-hub/src/store.js';
import { connectedSyncRepos, unwrapList } from '../src/lib/domain';
import type { DataResponse, Project, Proposal, SyncRepo } from '../src/lib/types';

test('a linked repository appears through live Hub SDK responses before any proposal exists', async () => {
  const store = createInMemoryStore();
  store.sync.put('repo_linked', Buffer.from('linked fixture'));
  store.sync.put('repo_unrelated', Buffer.from('unrelated fixture'));
  const trustedGrantsById = Object.fromEntries(['org', 'project', 'repo'].map(kind => [`grant_${kind}`, {
    id: `grant_${kind}`, schemaVersion: 'sorrel.protocol.v0', kind: 'Grant',
    principal: { kind: 'user', id: 'local' }, resource: { kind, id: '*' }, effect: 'allow',
    capabilities: ['org.read', 'project.create', 'project.read', 'project.write', 'repo.read', 'proposal.read'],
  }]));
  const app = createApp({ store, trustedGrantsById,
    env: { SORREL_HUB_AUTH: 'dev', SORREL_HUB_LOCAL_DEMO: '1' } });
  const server = http.createServer(app.handleRequest);
  await new Promise<void>(resolve => server.listen(0, '127.0.0.1', resolve));
  const address = server.address();
  assert.ok(address && typeof address === 'object');
  try {
    const client = new HubClient({ baseUrl: `http://127.0.0.1:${address.port}`, principal: { type: 'user', id: 'local' } });
    const created = await client.createProject<DataResponse<Project>>({ name: 'Linked before review', organizationId: 'org_local' });
    await client.linkProjectRepository(created.data.id, 'repo_linked');
    const [project, proposalPayload, syncPayload] = await Promise.all([
      client.getProject<DataResponse<Project>>(created.data.id),
      client.listProposals({ projectId: created.data.id }),
      client.listSyncRepos(),
    ]);
    const proposals = unwrapList<Proposal>(proposalPayload);
    assert.deepEqual(proposals, []);
    assert.deepEqual(project.data.repositoryIds, ['repo_linked']);
    assert.deepEqual(connectedSyncRepos(project.data, proposals, unwrapList<SyncRepo>(syncPayload)),
      [{ id: 'repo_linked', refCount: 0 }]);
  } finally {
    await new Promise<void>((resolve, reject) => server.close(error => error ? reject(error) : resolve()));
  }
});
