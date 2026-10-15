// Test-only real Hub process. Crash hooks interrupt actual durable publication.
import http from 'node:http';
import path from 'node:path';
import { createApp } from '../../sorrel-hub/src/app.js';
import { resolveTrustedGrants } from '../../sorrel-hub/src/bootstrap-grants.js';
import { acquireDatasetWriter } from '../../sorrel-hub/src/core-integration.js';
import { createFsMetadataStore } from '../../sorrel-hub/src/fs-metadata-store.js';
import { createFsRepoSyncStore } from '../../sorrel-hub/src/fs-sync-store.js';

const root = process.env.SORREL_PILOT_TEST_ROOT;
if (!root) throw new Error('SORREL_PILOT_TEST_ROOT is required');
const sync = path.join(root, 'sync');
const metadata = path.join(root, 'metadata');
const datasetLease = await acquireDatasetWriter([sync, metadata]);
const app = createApp({
  datasetLease,
  store: createFsMetadataStore(metadata, { sync: createFsRepoSyncStore(sync) }),
  trustedGrantsById: resolveTrustedGrants(),
  browserAuth: null,
  integrationFault(stage) {
    if (stage === process.env.SORREL_PILOT_TEST_CRASH) process.kill(process.pid, 'SIGKILL');
  },
});
await app.ready;
const server = http.createServer(app.handleRequest);
server.listen(0, '127.0.0.1', () => {
  console.log(JSON.stringify({ url: `http://127.0.0.1:${server.address().port}` }));
});
process.on('SIGTERM', () => server.close(async () => { await app.close(); process.exit(0); }));
