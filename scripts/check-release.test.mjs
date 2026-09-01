import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
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
