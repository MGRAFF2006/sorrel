import assert from 'node:assert/strict';
import fs from 'node:fs';
import http from 'node:http';
import os from 'node:os';
import path from 'node:path';
import { test } from 'node:test';

import { createApp } from '../src/app.js';
import { resolveCapabilities } from '../src/capabilities.js';
import { createFsMetadataStore } from '../src/fs-metadata-store.js';
import { createFsRepoSyncStore } from '../src/fs-sync-store.js';
import { createInMemoryStore } from '../src/store.js';

async function readCapabilities(app) {
  const server = http.createServer(app.handleRequest);
  await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
  try {
    const response = await fetch(`http://127.0.0.1:${server.address().port}/capabilities`);
    assert.equal(response.status, 200);
    return (await response.json()).data;
  } finally {
    await new Promise((resolve) => server.close(resolve));
  }
}

test('default createApp advertises its ephemeral object store', async () => {
  const caps = await readCapabilities(createApp({ env: {} }));
  assert.equal(caps.modules.objectStorage, 'memory');
});

test('object storage follows the wired sync store, independent of metadata and environment', async () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'sorrel-capabilities-'));
  try {
    for (const [store, env, expected] of [
      [createInMemoryStore(), { SORREL_HUB_SYNC_STORE: 'fs' }, 'memory'],
      [createInMemoryStore({ sync: createFsRepoSyncStore(path.join(root, 'sync')) }),
        { SORREL_HUB_SYNC_STORE: 'memory' }, 'fs'],
      [createFsMetadataStore(path.join(root, 'metadata')), {}, 'memory'],
    ]) {
      const caps = await readCapabilities(createApp({ store, env }));
      assert.equal(caps.modules.objectStorage, expected);
    }
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});

test('mirror capability follows the injected instance without disclosing privileged configuration', async () => {
  const configured = { CONVEX_URL: 'http://127.0.0.1:1', CONVEX_DEPLOY_KEY: 'fixture-admin-key' };
  for (const [enabled, env] of [[false, configured], [true, {}]]) {
    const caps = await readCapabilities(createApp({
      env,
      convexMirror: { enabled, async upsertProposal() {}, async removeProposal() {} },
    }));
    assert.deepEqual(caps.convex, { enabled });
    assert.equal(JSON.stringify(caps).includes(configured.CONVEX_URL), false);
    assert.equal(JSON.stringify(caps).includes(configured.CONVEX_DEPLOY_KEY), false);
  }
});

test('standalone capability helper preserves its environment fallback and explicit overrides', () => {
  const env = { CONVEX_URL: 'http://127.0.0.1:1', CONVEX_DEPLOY_KEY: 'fixture-admin-key' };
  assert.equal(resolveCapabilities({ env }).modules.objectStorage, 'fs');
  assert.deepEqual(resolveCapabilities({ env }).convex, { enabled: true });
  const caps = resolveCapabilities({ env, objectStorage: 'memory', convexEnabled: false });
  assert.equal(caps.modules.objectStorage, 'memory');
  assert.deepEqual(caps.convex, { enabled: false });
});
