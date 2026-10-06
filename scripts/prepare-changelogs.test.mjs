#!/usr/bin/env node

import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, mkdirSync, readFileSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';
import {
  categorizeChange,
  displayTitle,
  generateChangelogs,
  packagesForChange,
  renderChangelogPullRequestBody,
  updateChangelog,
} from './prepare-changelogs.mjs';

test('generated changelog PR preserves the current canonical template and impact checklist', () => {
  const template = readFileSync(new URL('../.github/PULL_REQUEST_TEMPLATE.md', import.meta.url), 'utf8');
  const body = renderChangelogPullRequestBody({ version: '0.2.0-alpha.1', date: '2026-10-06', template });
  for (const line of template.split('\n').filter(line => line.startsWith('## ') || line.startsWith('- [ ]'))) {
    assert.ok(body.split('\n').includes(line), `missing canonical template line: ${line}`);
  }
  assert.match(body, /v0\.2\.0-alpha\.1 \(2026-10-06\)/);
  assert.match(body, /does not publish a release or update package versions/);
  assert.match(body, /`npm run test:changelogs`/);
  assert.throws(() => renderChangelogPullRequestBody({ version: 'bad\nversion', date: '2026-10-06', template }), /invalid semantic version/);
  assert.throws(() => renderChangelogPullRequestBody({ version: '0.2.0', date: '2026-02-30', template }), /invalid release date/);
});

test('actual changelog publication step passes the generated canonical body through a file', (t) => {
  const root = mkdtempSync(join(tmpdir(), 'sorrel-changelog-pr-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const bin = join(root, 'bin');
  mkdirSync(bin);
  writeFileSync(join(bin, 'git'), '#!/bin/sh\nexit 0\n', { mode: 0o755 });
  writeFileSync(join(bin, 'gh'), `#!/usr/bin/env node
const fs = require('node:fs');
const args = process.argv.slice(2);
const index = args.indexOf('--body-file');
fs.writeFileSync(process.env.PR_CAPTURE, JSON.stringify({ args, body: index < 0 ? null : fs.readFileSync(args[index + 1], 'utf8') }));
`, { mode: 0o755 });
  const workflow = readFileSync(new URL('../.github/workflows/prepare-changelogs.yml', import.meta.url), 'utf8');
  const block = workflow.split('      - name: Open changelog pull request\n')[1].split('        run: |\n')[1];
  const script = block.split('\n').map(line => line.replace(/^          /, '')).join('\n');
  const capture = join(root, 'capture.json');
  const result = spawnSync('bash', ['-e', '-o', 'pipefail', '-c', script], {
    cwd: fileURLToPath(new URL('../', import.meta.url)), encoding: 'utf8',
    env: { ...process.env, PATH: `${bin}:${process.env.PATH}`, NEXT_VERSION: '0.2.0-alpha.1',
      RELEASE_DATE: '2026-10-06', RUNNER_TEMP: root, PR_CAPTURE: capture },
  });
  assert.equal(result.status, 0, result.stderr);
  const actual = JSON.parse(readFileSync(capture, 'utf8'));
  assert.deepEqual(actual.args, ['pr', 'create', '--base', 'main', '--head', 'release/changelogs-v0.2.0-alpha.1',
    '--title', 'chore: prepare v0.2.0-alpha.1 changelogs', '--body-file', join(root, 'changelog-pr-body.md')]);
  const template = readFileSync(new URL('../.github/PULL_REQUEST_TEMPLATE.md', import.meta.url), 'utf8');
  assert.equal(actual.body, renderChangelogPullRequestBody({ version: '0.2.0-alpha.1', date: '2026-10-06', template }));
});

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

test('a malformed package changelog does not partially update earlier files', (t) => {
  const root = mkdtempSync(join(tmpdir(), 'sorrel-changelog-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  mkdirSync(join(root, 'release'));
  mkdirSync(join(root, 'sorrel-cli'));
  writeFileSync(join(root, 'release/manifest.json'), JSON.stringify({ modules: { 'sorrel-cli': true } }));
  writeFileSync(join(root, 'CHANGELOG.md'), TEMPLATE);
  writeFileSync(join(root, 'sorrel-cli/CHANGELOG.md'), '# Missing release sections\n');
  assert.throws(() => generateChangelogs({
    root, version: '0.2.0', date: '2026-09-01', repository: 'MGRAFF2006/sorrel', changes: []
  }), /no \[Unreleased\] section/);
  assert.equal(readFileSync(join(root, 'CHANGELOG.md'), 'utf8'), TEMPLATE);
});

test('release preparation rejects dates normalized into a different month', () => {
  for (const date of ['2026-02-30', '2025-02-29']) {
    const result = spawnSync(process.execPath, [
      fileURLToPath(new URL('./prepare-changelogs.mjs', import.meta.url)),
      '--version', '0.2.0', '--date', date, '--input', '/nonexistent-sorrel-changes.json'
    ], { encoding: 'utf8' });
    assert.equal(result.status, 1);
    assert.match(result.stderr, /invalid release date/);
  }
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
