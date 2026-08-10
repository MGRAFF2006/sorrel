#!/usr/bin/env node
/**
 * Start sorrel-hub on an ephemeral port and print one JSON line with the URL.
 *
 * Used by integration / E2E tests that need a real Hub process (no mocks).
 *
 * Env:
 *   SORREL_HUB_SYNC_STORE   default "memory" for tests
 *   SORREL_HUB_DATA_DIR     optional FS sync root when store is fs
 *   SORREL_HUB_METADATA_DIR optional FS metadata root
 *   SORREL_HUB_BOOTSTRAP_GRANTS  set to "1" to enable dev-only local grants
 *
 * Stdout (single line): {"url":"http://127.0.0.1:<port>","pid":N}
 * Ready when that line is flushed. Stops on SIGTERM/SIGINT.
 */

import http from 'node:http';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { createApp } from '../src/app.js';
import { resolveTrustedGrants } from '../src/bootstrap-grants.js';
import { createFsMetadataStore } from '../src/fs-metadata-store.js';
import { createFsRepoSyncStore } from '../src/fs-sync-store.js';
import { createInMemoryStore } from '../src/store.js';

const root = path.dirname(fileURLToPath(import.meta.url));
void root;

const syncStoreKind = process.env.SORREL_HUB_SYNC_STORE ?? 'memory';
const dataDir = process.env.SORREL_HUB_DATA_DIR ?? './data/sync';
const metadataDir =
  process.env.SORREL_HUB_METADATA_DIR ?? path.join(dataDir, '..', 'metadata');

const store =
  syncStoreKind === 'memory'
    ? createInMemoryStore()
    : createFsMetadataStore(metadataDir, { sync: createFsRepoSyncStore(dataDir) });

const trustedGrantsById = resolveTrustedGrants();
const app = createApp({ store, trustedGrantsById });
const server = http.createServer(app.handleRequest);

server.listen(0, '127.0.0.1', () => {
  const address = server.address();
  const port = typeof address === 'object' && address ? address.port : 0;
  const payload = JSON.stringify({
    url: `http://127.0.0.1:${port}`,
    pid: process.pid,
  });
  process.stdout.write(`${payload}\n`);
});

function shutdown() {
  server.close(() => process.exit(0));
}

process.on('SIGTERM', shutdown);
process.on('SIGINT', shutdown);
