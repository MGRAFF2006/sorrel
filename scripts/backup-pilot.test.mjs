import assert from 'node:assert/strict';
import { copyFileSync, existsSync, mkdirSync, mkdtempSync, readFileSync, realpathSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { spawnSync } from 'node:child_process';
import test from 'node:test';

const skip = process.platform !== 'linux' && 'Pilot backup requires Linux flock and GNU realpath';
const restic = spawnSync('which', ['restic'], { encoding: 'utf8' }).stdout?.trim();

function fixture(t) {
  const root = mkdtempSync(join(tmpdir(), 'sorrel-backup-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  for (const path of ['deploy', 'bin', 'volume/_data', 'bind', 'unrelated']) mkdirSync(join(root, path), { recursive: true });
  copyFileSync(new URL('../deploy/backup-pilot.sh', import.meta.url), join(root, 'deploy/backup-pilot.sh'));
  writeFileSync(join(root, 'password'), 'test-placeholder');
  writeFileSync(join(root, 'bin/docker'), `#!/usr/bin/env bash
set -euo pipefail
printf '%s\\n' "$*" >> "$MOCK_CALLS"
if [[ $1 == inspect ]]; then printf '%s\\n' "$MOCK_DATA_SOURCE"; exit; fi
while [[ $# -gt 0 && $1 != config && $1 != ps && $1 != stop && $1 != up ]]; do shift; done
case $1 in
  config|stop|up) ;;
  ps) if [[ $2 == -q ]]; then printf 'test-hub\\n'; else printf 'hub\\nhub-web\\n'; fi ;;
  *) exit 99 ;;
esac
`, { mode: 0o700 });
  writeFileSync(join(root, 'bin/restic'), `#!/usr/bin/env bash
set -euo pipefail
printf '%s\\n' "$@" > "$MOCK_RESTIC_ARGS"
exit "${'${MOCK_RESTIC_EXIT:-0}'}"
`, { mode: 0o700 });
  const run = (source, configured, extra = {}) => {
    const env = {
      ...process.env, PATH: `${join(root, 'bin')}:${process.env.PATH}`,
      RESTIC_REPOSITORY: join(root, 'repository'), RESTIC_PASSWORD_FILE: join(root, 'password'),
      SORREL_BACKUP_LOCK: join(root, 'backup.lock'), SORREL_PILOT_TUNNEL: '0',
      MOCK_CALLS: join(root, 'calls'), MOCK_RESTIC_ARGS: join(root, 'restic-args'),
      MOCK_DATA_SOURCE: source, ...extra,
    };
    delete env.SORREL_HUB_DATA_PATH;
    if (configured !== undefined) env.SORREL_HUB_DATA_PATH = configured;
    return spawnSync('bash', [join(root, 'deploy/backup-pilot.sh')], { env, encoding: 'utf8' });
  };
  return { root, run };
}

test('backs up the actual named volume without a supplied data path', { skip }, t => {
  const { root, run } = fixture(t);
  const data = join(root, 'volume/_data');
  const result = run(data);
  assert.equal(result.status, 0, result.stderr);
  assert.equal(readFileSync(join(root, 'restic-args'), 'utf8').split('\n')[2], realpathSync(data));
});

test('accepts an equivalent bind-mount path and resumes after Restic failure', { skip }, t => {
  const { root, run } = fixture(t);
  const data = join(root, 'bind');
  const alias = join(root, 'bind-alias');
  symlinkSync(data, alias);
  const result = run(data, alias, { SORREL_PILOT_TUNNEL: '1', MOCK_RESTIC_EXIT: '42' });
  assert.equal(result.status, 42, result.stderr);
  assert.equal(readFileSync(join(root, 'restic-args'), 'utf8').split('\n')[2], realpathSync(data));
  assert.match(readFileSync(join(root, 'calls'), 'utf8'), /up -d --no-build --wait/);
});

test('rejects an unrelated existing data directory before stopping services', { skip }, t => {
  const { root, run } = fixture(t);
  const result = run(join(root, 'volume/_data'), join(root, 'unrelated'));
  assert.equal(result.status, 1);
  assert.match(result.stderr, /does not match the running Hub data mount/);
  assert.doesNotMatch(readFileSync(join(root, 'calls'), 'utf8'), /stop hub-web hub/);
  assert.equal(existsSync(join(root, 'restic-args')), false);
});

test('real Restic restores cache-like repository/ref names while excluding checkout artifacts', {
  skip: skip || (!restic && process.env.SORREL_REQUIRE_RESTIC !== '1' && 'Install Restic to verify saved and restored backup content'),
}, t => {
  assert.ok(restic, 'Restic is required when SORREL_REQUIRE_RESTIC=1');
  const { root, run } = fixture(t);
  const storage = mkdtempSync(join(tmpdir(), 'sorrel-restic-'));
  t.after(() => rmSync(storage, { recursive: true, force: true }));
  // Use the actual Restic binary; Docker remains a disposable lifecycle fixture.
  rmSync(join(root, 'bin/restic'));
  symlinkSync(restic, join(root, 'bin/restic'));
  const backupEnv = {
    RESTIC_REPOSITORY: join(storage, 'repository'),
    RESTIC_PASSWORD_FILE: join(root, 'password'),
  };
  const env = { ...process.env, ...backupEnv };
  function saved(command, ...args) {
    const result = spawnSync(restic, ['--no-cache', command, ...args], { env, encoding: 'utf8' });
    assert.equal(result.status, 0, result.stderr);
    return result;
  }
  saved('init');
  for (const location of ['bind', 'target/hub-data', 'sorrel-hub/node_modules/hub-data']) {
    const data = join(root, location);
    const files = [
      'sync/repos/target/refs/main', 'sync/repos/node_modules/refs/main',
      'sync/repos/pilot/refs/target', 'sync/repos/pilot/refs/node_modules',
      'sync/repos/pilot/objects/.git/blob', 'metadata/projects/pilot.json',
    ];
    for (const file of files) {
      mkdirSync(dirname(join(data, file)), { recursive: true });
      writeFileSync(join(data, file), `authoritative ${file}`);
    }
    const artifacts = ['.git/HEAD', 'node_modules/cache', 'target/build-output',
      'sorrel-hub/node_modules/cache', 'sorrel-core/target/build-output', 'deploy/backups/old-copy'];
    for (const file of artifacts) {
      mkdirSync(dirname(join(root, file)), { recursive: true });
      writeFileSync(join(root, file), 'generated');
    }
    writeFileSync(join(root, 'deploy/.env'), 'PRIVATE_TEST_PLACEHOLDER=preserve');
    mkdirSync(join(root, 'deploy/config'), { recursive: true });
    writeFileSync(join(root, 'deploy/config/grants.json'), '{"grants":[]}');
    const result = run(data, undefined, backupEnv);
    assert.equal(result.status, 0, result.stderr);
    const destination = join(storage, 'restore', location);
    saved('restore', 'latest', '--target', destination, '--verify');
    for (const file of files) assert.equal(readFileSync(join(destination, data, file), 'utf8'), `authoritative ${file}`);
    for (const file of ['deploy/.env', 'deploy/config/grants.json']) {
      assert.equal(readFileSync(join(destination, root, file), 'utf8'), readFileSync(join(root, file), 'utf8'));
    }
    for (const file of artifacts) {
      // Overlapping artifact roots must be retained rather than risk data loss.
      const artifactRoot = file.startsWith('target/') ? 'target/'
        : file.startsWith('sorrel-hub/node_modules/') ? 'sorrel-hub/node_modules/' : null;
      if (artifactRoot && location.startsWith(artifactRoot)) continue;
      assert.equal(existsSync(join(destination, root, file)), false, `${location}: ${file}`);
    }
  }
});
