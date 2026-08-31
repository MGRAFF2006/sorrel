import http from 'node:http';
import path from 'node:path';

import { createApp } from './app.js';
import { createAuthAdapterFromEnv } from './auth/adapter.js';
import { assertSafeHubBind } from './bind-safety.js';
import { resolveTrustedGrants } from './bootstrap-grants.js';
import { createConvexMirror } from './convex-mirror.js';
import { createFsMetadataStore } from './fs-metadata-store.js';
import { createFsRepoSyncStore } from './fs-sync-store.js';
import { createInMemoryStore } from './store.js';

const port = Number.parseInt(process.env.PORT ?? '3000', 10);
const host = process.env.HOST ?? '127.0.0.1';

// Sync objects/refs and product metadata persist to disk by default so data
// survives restarts. SORREL_HUB_SYNC_STORE=memory restores ephemeral
// in-memory behavior for both sync and metadata (createApp() stays in-memory
// for tests regardless).
const syncStoreKind = process.env.SORREL_HUB_SYNC_STORE ?? 'fs';
const dataDir = process.env.SORREL_HUB_DATA_DIR ?? './data/sync';
const metadataDir =
  process.env.SORREL_HUB_METADATA_DIR ?? path.join(dataDir, '..', 'metadata');

const store =
  syncStoreKind === 'memory'
    ? createInMemoryStore()
    : createFsMetadataStore(metadataDir, { sync: createFsRepoSyncStore(dataDir) });

// Optional local-development bootstrap grants let the CLI's `user:local`
// principal push/pull without a separate grant-distribution service. They are
// disabled unless SORREL_HUB_BOOTSTRAP_GRANTS=1. See bootstrap-grants.js.
const trustedGrantsById = resolveTrustedGrants();
const authAdapter = createAuthAdapterFromEnv();
const bootstrapGrantsEnabled =
  process.env.SORREL_HUB_BOOTSTRAP_GRANTS === '1' ||
  process.env.SORREL_HUB_BOOTSTRAP_GRANTS === 'true';

assertSafeHubBind({
  host,
  authMode: authAdapter.mode,
  bootstrapGrantsEnabled,
});

const convexMirror = createConvexMirror();
const app = createApp({ store, trustedGrantsById, authAdapter, convexMirror });

const server = http.createServer(app.handleRequest);

server.listen(port, host, () => {
  const address = server.address();
  const boundPort = typeof address === 'object' && address ? address.port : port;
  const persistence =
    syncStoreKind === 'memory'
      ? 'in-memory sync and metadata stores'
      : `sync store at ${dataDir}, metadata store at ${metadataDir}`;
  const grantCount = Object.keys(trustedGrantsById).length;
  console.log(
    `sorrel-hub listening on http://${host}:${boundPort} (${persistence}; ${grantCount} trusted grant(s); auth=${authAdapter.mode}; convex=${convexMirror.enabled ? 'on' : 'off'})`,
  );
});

process.on('SIGTERM', () => {
  server.close(() => {
    process.exit(0);
  });
});
