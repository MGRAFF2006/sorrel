#!/usr/bin/env node
/**
 * Run each submodule's own test suite (no mocks where those modules already
 * spawn real processes). Used by `npm run test:modules`.
 */

import { spawnSync } from 'node:child_process';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '../..');

const JOBS = [
  { name: 'sorrel-protocol', cwd: 'sorrel-protocol', cmd: ['npm', 'test'] },
  { name: 'sorrel-core', cwd: 'sorrel-core', cmd: ['cargo', 'test'] },
  { name: 'sorrel-cli', cwd: 'sorrel-cli', cmd: ['cargo', 'test'] },
  { name: 'sorrel-vault', cwd: 'sorrel-vault', cmd: ['npm', 'test'] },
  { name: 'sorrel-runners', cwd: 'sorrel-runners', cmd: ['cargo', 'test'] },
  { name: 'sorrel-slices', cwd: 'sorrel-slices', cmd: ['npm', 'test'] },
  { name: 'sorrel-hub', cwd: 'sorrel-hub', cmd: ['npm', 'test'] },
  { name: 'sorrel-hub-web', cwd: 'sorrel-hub-web', cmd: ['npm', 'test'] },
  { name: 'sorrel-sdk-js', cwd: 'sorrel-sdk-js', cmd: ['npm', 'test'] },
  { name: 'sorrel-sdk-rust', cwd: 'sorrel-sdk-rust', cmd: ['cargo', 'test'] },
  { name: 'sorrel-agents', cwd: 'sorrel-agents', cmd: ['npm', 'test'] },
];

let failed = 0;
for (const job of JOBS) {
  console.log(`\n=== ${job.name} ===`);
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
