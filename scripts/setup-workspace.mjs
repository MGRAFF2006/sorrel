#!/usr/bin/env node

import { spawnSync } from 'node:child_process';
import { existsSync, readFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const manifest = JSON.parse(
  readFileSync(join(ROOT, 'release/manifest.json'), 'utf8'),
);

function run(command, args, cwd = ROOT) {
  const result = spawnSync(command, args, {
    cwd,
    stdio: 'inherit',
    env: process.env,
  });
  if (result.status !== 0) process.exit(result.status ?? 1);
}

const nodePackages = Object.keys(manifest.modules).filter((module) => {
  const packagePath = join(ROOT, module, 'package.json');
  if (!existsSync(packagePath)) return false;
  const pkg = JSON.parse(readFileSync(packagePath, 'utf8'));
  return [pkg.dependencies, pkg.devDependencies, pkg.optionalDependencies].some(
    (dependencies) => Object.keys(dependencies ?? {}).length > 0,
  );
});

for (const module of nodePackages) {
  const packageRoot = join(ROOT, module);
  const install = existsSync(join(packageRoot, 'package-lock.json')) ? 'ci' : 'install';
  console.log(`\n=== ${module}: npm ${install} ===`);
  run('npm', [install], packageRoot);
}

console.log('\n=== Rust workspace: cargo fetch ===');
run('cargo', ['fetch']);

console.log('\nWorkspace dependencies are ready.');
