import assert from 'node:assert/strict';
import { mkdtemp, mkdir, readFile, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { collectArtifacts } from '../scripts/collect-artifacts.mjs';

test('desktop host mounts the shared UI with native transport and platform adapters', async () => {
  const source = await readFile(new URL('../src/main.tsx', import.meta.url), 'utf8');

  assert.match(source, /createDesktopPlatform/);
  assert.match(source, /mountHubApp/);
  assert.match(source, /apiBase: 'http:\/\/127\.0\.0\.1:3000'/);
  assert.match(source, /fetch: tauriFetch/);
  assert.match(source, /openExternal: openUrl/);
  assert.match(source, /sendNotification/);
});

test('Tauri capability limits Hub HTTP access to loopback', async () => {
  const capability = JSON.parse(
    await readFile(
      new URL('../src-tauri/capabilities/default.json', import.meta.url),
      'utf8',
    ),
  );
  const http = capability.permissions.find(
    (permission) => typeof permission === 'object' && permission.identifier === 'http:default',
  );

  assert.ok(http);
  assert.deepEqual(http.allow, [
    { url: 'http://127.0.0.1:*' },
    { url: 'http://localhost:*' },
  ]);
});

test('release collector flattens bundles and writes adjacent SHA-256 files', async () => {
  const root = await mkdtemp(join(tmpdir(), 'sorrel-desktop-artifacts-'));
  const bundles = join(root, 'bundle', 'msi');
  const output = join(root, 'release');
  await mkdir(bundles, { recursive: true });
  await writeFile(join(bundles, 'Sorrel Hub_0.1.0_arm64_en-US.msi'), 'desktop bundle');

  const names = await collectArtifacts(join(root, 'bundle'), output);

  assert.deepEqual(names, ['Sorrel Hub_0.1.0_arm64_en-US.msi']);
  assert.equal(await readFile(join(output, names[0]), 'utf8'), 'desktop bundle');
  assert.match(
    await readFile(join(output, `${names[0]}.sha256`), 'utf8'),
    /^[a-f0-9]{64}  Sorrel Hub_0\.1\.0_arm64_en-US\.msi\n$/,
  );
});
