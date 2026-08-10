#!/usr/bin/env node
/**
 * Run each package's own test suite (no mocks where those modules already
 * spawn real processes). Used by `npm run test:modules`.
 */

import { spawnSync } from 'node:child_process';
import { existsSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '../..');

const JOBS = [
  { name: 'sorrel-protocol', cwd: 'sorrel-protocol', cmd: ['npm', 'test'] },
  { name: 'sorrel-core', cwd: '.', cmd: ['cargo', 'test', '-p', 'sorrel-core'] },
  { name: 'sorrel-cli', cwd: '.', cmd: ['cargo', 'test', '-p', 'sorrel-cli'] },
  { name: 'sorrel-vault', cwd: 'sorrel-vault', cmd: ['npm', 'test'] },
  { name: 'sorrel-runners', cwd: '.', cmd: ['cargo', 'test', '-p', 'sorrel-runners'] },
  { name: 'sorrel-slices', cwd: 'sorrel-slices', cmd: ['npm', 'test'] },
  { name: 'sorrel-hub', cwd: 'sorrel-hub', cmd: ['npm', 'test'] },
  { name: 'sorrel-hub-web', cwd: 'sorrel-hub-web', cmd: ['npm', 'test'] },
  { name: 'sorrel-sdk-js', cwd: 'sorrel-sdk-js', cmd: ['npm', 'test'] },
  { name: 'sorrel-sdk-rust', cwd: '.', cmd: ['cargo', 'test', '-p', 'sorrel-sdk'] },
  { name: 'sorrel-agents', cwd: 'sorrel-agents', cmd: ['npm', 'test'] },
];

function ensureNodeModules(cwd) {
  const abs = join(ROOT, cwd);
  if (existsSync(join(abs, 'node_modules'))) return true;
  if (!existsSync(join(abs, 'package.json'))) return true;
  console.log(`Installing dependencies in ${cwd}…`);
  const install = spawnSync(
    'npm',
    [existsSync(join(abs, 'package-lock.json')) ? 'ci' : 'install'],
    { cwd: abs, stdio: 'inherit', env: process.env },
  );
  return install.status === 0;
}

let failed = 0;
for (const job of JOBS) {
  console.log(`\n=== ${job.name} ===`);
  if (job.cmd[0] === 'npm' && !ensureNodeModules(job.cwd)) {
    console.error(`FAILED: ${job.name} (dependency install)`);
    failed += 1;
    continue;
  }
  const result = spawnSync(job.cmd[0], job.cmd.slice(1), {
    cwd: join(ROOT, job.cwd),
    stdio: 'inherit',
    env: process.env,
  });
  if (result.status !== 0) {
    console.error(`FAILED: ${job.name}`);
    failed += 1;
  }
}

if (failed > 0) {
  console.error(`\n${failed} module suite(s) failed`);
  process.exit(1);
}
console.log('\nAll module suites passed.');
