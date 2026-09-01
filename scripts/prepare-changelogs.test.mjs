#!/usr/bin/env node

import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import {
  categorizeChange,
  displayTitle,
  generateChangelogs,
  packagesForChange,
  updateChangelog,
} from './prepare-changelogs.mjs';

const TEMPLATE = `# Changelog

## [Unreleased]

### Changed

- Hand-written pending note.

## [0.1.0] - 2026-01-01

- Initial release.

[Unreleased]: https://github.com/MGRAFF2006/sorrel/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/MGRAFF2006/sorrel/releases/tag/v0.1.0
`;

test('categorizes labels and conventional titles with a safe fallback', () => {
  assert.equal(categorizeChange(change('feat(cli): add lanes')), 'Added');
  assert.equal(categorizeChange(change('fix: restore files')), 'Fixed');
  assert.equal(categorizeChange(change('Unstructured but useful title')), 'Changed');
  assert.equal(categorizeChange(change('feat: auth', ['security'])), 'Security');
  assert.equal(categorizeChange(change('Ship native installers')), 'Added');
  assert.equal(categorizeChange(change('Correct invalid refs')), 'Fixed');
  assert.equal(categorizeChange(change('Remove obsolete endpoint')), 'Removed');
  assert.equal(displayTitle('fix(cli): restore files.'), 'Restore files');
});

test('maps changed paths to affected packages', () => {
  const value = change('feat: sync', [], ['sorrel-cli/src/main.rs', 'docs/STATUS.md']);
  assert.deepEqual(packagesForChange(value, ['sorrel-cli', 'sorrel-hub']), ['sorrel-cli']);
});

test('replaces pending prose with a generated release section and links', () => {
  const updated = updateChangelog(TEMPLATE, {
    version: '0.2.0',
    date: '2026-09-01',
    repository: 'MGRAFF2006/sorrel',
    changes: [change('feat(cli): add lanes', [], ['sorrel-cli/src/main.rs'], 12)],
  });
  assert.match(updated, /## \[Unreleased\]\n\nNo changes yet\./);
  assert.match(updated, /## \[0\.2\.0\] - 2026-09-01\n\n### Added/);
  assert.match(updated, /Add lanes \(\[#12\]\(https:\/\/example\.test\/12\)\)\./);
  assert.doesNotMatch(updated, /Hand-written pending note/);
  assert.match(updated, /\[Unreleased\]: .*compare\/v0\.2\.0\.\.\.HEAD/);
});

test('generates root and package changelogs without contributor fragments', () => {
  const root = mkdtempSync(join(tmpdir(), 'sorrel-changelog-'));
  mkdirSync(join(root, 'release'));
  mkdirSync(join(root, 'sorrel-cli'));
  mkdirSync(join(root, 'sorrel-hub'));
  writeFileSync(
    join(root, 'release/manifest.json'),
    JSON.stringify({ modules: { 'sorrel-cli': true, 'sorrel-hub': true } }),
  );
  writeFileSync(join(root, 'CHANGELOG.md'), TEMPLATE);
  writeFileSync(join(root, 'sorrel-cli/CHANGELOG.md'), TEMPLATE);
  writeFileSync(join(root, 'sorrel-hub/CHANGELOG.md'), TEMPLATE);

  generateChangelogs({
    root,
    version: '0.2.0',
    date: '2026-09-01',
    repository: 'MGRAFF2006/sorrel',
    changes: [
      change('feat(cli): add lanes', [], ['sorrel-cli/src/main.rs'], 12),
      change('fix(hub): validate refs', ['bug'], ['sorrel-hub/server.mjs'], 13),
      change('docs: internal typo', ['skip-changelog'], ['README.md'], 14),
    ],
  });

  assert.match(readFileSync(join(root, 'CHANGELOG.md'), 'utf8'), /Add lanes/);
  assert.match(readFileSync(join(root, 'CHANGELOG.md'), 'utf8'), /Validate refs/);
  assert.doesNotMatch(readFileSync(join(root, 'CHANGELOG.md'), 'utf8'), /Internal typo/);
  assert.match(readFileSync(join(root, 'sorrel-cli/CHANGELOG.md'), 'utf8'), /Add lanes/);
  assert.doesNotMatch(readFileSync(join(root, 'sorrel-cli/CHANGELOG.md'), 'utf8'), /Validate refs/);
  assert.match(readFileSync(join(root, 'sorrel-hub/CHANGELOG.md'), 'utf8'), /Validate refs/);
});

function change(title, labels = [], files = [], number = null) {
  return {
    number,
    title,
    labels,
    files,
    url: number ? `https://example.test/${number}` : 'https://example.test/commit',
  };
}
