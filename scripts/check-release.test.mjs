import assert from 'node:assert/strict';
import { copyFileSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { test } from 'node:test';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const manifest = JSON.parse(
  readFileSync(join(ROOT, 'release/manifest.json'), 'utf8'),
);

function runCheck(tag) {
  return spawnSync(process.execPath, ['scripts/check-release.mjs', tag], {
    cwd: ROOT,
    encoding: 'utf8',
  });
}

test('release validation accepts the coordinated manifest tag', () => {
  const result = runCheck(manifest.release);
  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, new RegExp(`Release ${manifest.release}:`));
});

test('release validation rejects a publication event for another tag', () => {
  const wrongTag = `${manifest.release}-wrong`;
  const result = runCheck(wrongTag);
  assert.notEqual(result.status, 0);
  assert.match(
    result.stderr,
    new RegExp(`requested release tag ${wrongTag} != manifest release ${manifest.release}`),
  );
});


test('release validation rejects an inconsistent inherited Cargo workspace version', (t) => {
  const fixture = mkdtempSync(join(tmpdir(), 'sorrel-release-version-'));
  t.after(() => rmSync(fixture, { recursive: true, force: true }));
  const tracked = spawnSync('git', ['ls-files', '-z'], { cwd: ROOT, encoding: 'utf8' });
  assert.equal(tracked.status, 0, tracked.stderr);
  for (const file of tracked.stdout.split('\0').filter(Boolean)) {
    const destination = join(fixture, file);
    mkdirSync(dirname(destination), { recursive: true });
    copyFileSync(join(ROOT, file), destination);
  }
  const cargo = join(fixture, 'Cargo.toml');
  writeFileSync(cargo, readFileSync(cargo, 'utf8').replace(
    /(^\[workspace\.package\][\s\S]*?^version\s*=\s*)"[^"]+"/m,
    '$1"9.8.7"',
  ));
  const result = spawnSync(process.execPath, ['scripts/check-release.mjs'], { cwd: fixture, encoding: 'utf8' });
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /root Cargo workspace version 9\.8\.7 !=/);
  assert.match(result.stderr, /sorrel-core version 9\.8\.7 !=/);
});
