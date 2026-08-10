import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';

// Drift guard for the vendored policy conformance manifest.
//
// test/conformance/policy-conformance.json is a vendored copy of the canonical
// sorrel-protocol/conformance/policy-conformance.json. The protocol publishes a
// sidecar (policy-conformance.meta.json) recording the manifest version and a
// SHA-256 over the canonical bytes; we vendor it too. This test recomputes the
// SHA-256 of the vendored manifest and asserts it matches the sidecar, so a
// hand-edited or stale vendored manifest fails `npm test`. It complements
// policy-conformance.test.js, which proves the Hub guard agrees with the
// manifest decisions.
//
// Refresh both files with sorrel-protocol's
//   npm run export:conformance -- <this dir>
// (or the root scripts/sync-conformance.sh) instead of editing by hand.

const here = path.dirname(fileURLToPath(import.meta.url));
const conformanceDir = path.join(here, 'conformance');

async function readRaw(name) {
  return readFile(path.join(conformanceDir, name), 'utf8');
}

test('vendored manifest matches sidecar checksum', async () => {
  const manifestRaw = await readRaw('policy-conformance.json');
  const meta = JSON.parse(await readRaw('policy-conformance.meta.json'));
  const manifest = JSON.parse(manifestRaw);

  assert.equal(meta.kind, 'PolicyConformanceMeta');

  const actual = createHash('sha256').update(manifestRaw).digest('hex');
  assert.equal(
    actual,
    meta.sha256,
    'vendored manifest SHA-256 does not match sidecar; re-export from sorrel-protocol instead of hand-editing the manifest',
  );

  assert.equal(meta.manifestVersion, manifest.id, 'sidecar manifestVersion must equal manifest id');
  assert.equal(
    meta.schemaVersion,
    manifest.schemaVersion,
    'sidecar schemaVersion must equal manifest schemaVersion',
  );
});
