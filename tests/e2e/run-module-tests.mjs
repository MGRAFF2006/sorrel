#!/usr/bin/env node
/**
 * Run each package's own test suite (no mocks where those modules already
 * spawn real processes). Used by `npm run test:modules`.
 */

import { spawnSync } from 'node:child_process';
import { existsSync, readFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const releaseManifest = JSON.parse(
  readFileSync(join(ROOT, 'release/manifest.json'), 'utf8'),
);

const JOB_DEFINITIONS = {
  'sorrel-protocol': { cwd: 'sorrel-protocol', cmd: ['npm', 'run', 'check'] },
  'sorrel-core': { cwd: '.', cmd: ['cargo', 'test', '-p', 'sorrel-core'] },
  'sorrel-cli': { cwd: '.', cmd: ['cargo', 'test', '-p', 'sorrel-cli'] },
  'sorrel-vault': { cwd: 'sorrel-vault', cmd: ['npm', 'run', 'check'] },
  'sorrel-runners': { cwd: '.', cmd: ['cargo', 'test', '-p', 'sorrel-runners'] },
  'sorrel-slices': { cwd: 'sorrel-slices', cmd: ['npm', 'run', 'check'] },
  'sorrel-hub': { cwd: 'sorrel-hub', cmd: ['npm', 'run', 'check'] },
  'sorrel-hub-desktop': { cwd: 'sorrel-hub-desktop', cmd: ['npm', 'run', 'check'] },
  'sorrel-hub-ui': { cwd: 'sorrel-hub-ui', cmd: ['npm', 'run', 'check'] },
  'sorrel-hub-web': { cwd: 'sorrel-hub-web', cmd: ['npm', 'run', 'check'] },
  'sorrel-sdk-js': { cwd: 'sorrel-sdk-js', cmd: ['npm', 'run', 'check'] },
  'sorrel-sdk-rust': { cwd: '.', cmd: ['cargo', 'test', '-p', 'sorrel-sdk'] },
  'sorrel-agents': { cwd: 'sorrel-agents', cmd: ['npm', 'run', 'check'] },
  'sorrel-web': { cwd: 'sorrel-web', cmd: ['npm', 'run', 'check'] },
};

const manifestModules = Object.keys(releaseManifest.modules);
const undefinedModules = manifestModules.filter((name) => !JOB_DEFINITIONS[name]);
const unregisteredJobs = Object.keys(JOB_DEFINITIONS).filter(
  (name) => !(name in releaseManifest.modules),
);
if (undefinedModules.length > 0 || unregisteredJobs.length > 0) {
  if (undefinedModules.length > 0) {
    console.error(`Modules without test jobs: ${undefinedModules.join(', ')}`);
  }
  if (unregisteredJobs.length > 0) {
    console.error(`Test jobs missing from release manifest: ${unregisteredJobs.join(', ')}`);
  }
  process.exit(2);
}

const JOBS = manifestModules.map((name) => ({ name, ...JOB_DEFINITIONS[name] }));

const requested = process.argv.slice(2);
if (requested.includes('--list')) {
  for (const job of JOBS) console.log(job.name);
  process.exit(0);
}

const unknown = requested.filter(
  (name) => !JOBS.some((job) => job.name === name),
);
if (unknown.length > 0) {
  console.error(`Unknown module(s): ${unknown.join(', ')}`);
  console.error('Use --list to show valid module names.');
  process.exit(2);
}

const selectedJobs =
  requested.length === 0
    ? JOBS
    : JOBS.filter((job) => requested.includes(job.name));

function ensureNodeModules(cwd) {
  const abs = join(ROOT, cwd);
  if (!existsSync(join(abs, 'package.json'))) return true;
  const pkg = JSON.parse(readFileSync(join(abs, 'package.json'), 'utf8'));
  const dependencyCount = [
    pkg.dependencies,
    pkg.devDependencies,
    pkg.optionalDependencies,
  ].reduce((count, dependencies) => count + Object.keys(dependencies ?? {}).length, 0);
  if (dependencyCount === 0 || existsSync(join(abs, 'node_modules'))) return true;
  console.log(`Installing dependencies in ${cwd}…`);
  const install = spawnSync(
    'npm',
    [existsSync(join(abs, 'package-lock.json')) ? 'ci' : 'install'],
    { cwd: abs, stdio: 'inherit', env: process.env },
  );
  return install.status === 0;
}

let failed = 0;
for (const job of selectedJobs) {
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
console.log(`\nAll ${selectedJobs.length} selected module suite(s) passed.`);
