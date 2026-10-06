import assert from 'node:assert/strict';
import { mkdtemp, mkdir, readFile, writeFile, copyFile, chmod, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { spawnSync } from 'node:child_process';
import { test } from 'node:test';

for (const target of ['default', 'absolute', 'relative']) {
  test(`API docs builder publishes Cargo output with ${target} target directory`, async () => {
    const root = await mkdtemp(join(tmpdir(), 'sorrel-rustdoc-'));
    try {
      const site = join(root, 'sorrel-web');
      const core = join(root, 'sorrel-core');
      const bin = join(root, 'bin');
      await Promise.all([mkdir(join(site, 'scripts'), { recursive: true }), mkdir(core), mkdir(bin)]);
      await writeFile(join(core, 'Cargo.toml'), '[package]\nname = "sorrel-core"\n');
      await copyFile(new URL('../scripts/build-api-docs.sh', import.meta.url), join(site, 'scripts/build-api-docs.sh'));
      // Model Cargo's workspace output without compiling a crate in a unit test.
      await writeFile(join(bin, 'cargo'), `#!/usr/bin/env bash
set -euo pipefail
target_dir="\${CARGO_TARGET_DIR:-$TEST_WORKSPACE/target}"
while (( $# )); do
  if [[ "$1" == --target-dir ]]; then target_dir="$2"; shift 2; else shift; fi
done
mkdir -p "$target_dir/doc/sorrel_core" "$target_dir/doc/static.files"
printf '<html>generated engine reference</html>' > "$target_dir/doc/sorrel_core/index.html"
printf 'shared rustdoc asset' > "$target_dir/doc/static.files/style.css"
`);
      await chmod(join(bin, 'cargo'), 0o755);
      const env = { ...process.env, PATH: `${bin}:${process.env.PATH}`, TEST_WORKSPACE: root };
      delete env.CARGO_TARGET_DIR;
      if (target === 'absolute') env.CARGO_TARGET_DIR = join(root, 'custom-target');
      if (target === 'relative') env.CARGO_TARGET_DIR = 'relative-target';
      const result = spawnSync('bash', [join(site, 'scripts/build-api-docs.sh')], { cwd: root, env, encoding: 'utf8' });
      assert.equal(result.status, 0, result.stderr);
      const output = join(site, 'api/sorrel-core');
      assert.match(await readFile(join(output, 'sorrel_core/index.html'), 'utf8'), /generated engine reference/);
      assert.equal(await readFile(join(output, 'static.files/style.css'), 'utf8'), 'shared rustdoc asset');
    } finally {
      await rm(root, { recursive: true, force: true });
    }
  });
}
