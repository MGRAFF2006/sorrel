#!/usr/bin/env node
/**
 * Run each package's own test suite (no mocks where those modules already
 * spawn real processes). Used by `npm run test:modules`.
 */

import { spawnSync } from 'node:child_process';
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
