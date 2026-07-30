#!/usr/bin/env node

import { execFileSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { performance } from 'node:perf_hooks';
import { fileURLToPath } from 'node:url';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const CLI_DIR = join(ROOT, 'sorrel-cli');
const CLI = join(CLI_DIR, 'target/release/sorrel');
const fileCount = Number(process.env.SORREL_BENCH_FILES ?? 10_000);
const historyCount = Number(process.env.SORREL_BENCH_HISTORY ?? 1_000);

function run(command, args, options = {}) {
  return execFileSync(command, args, {
    stdio: 'pipe',
    env: process.env,
    ...options,
  });
}

function timed(command, args, options = {}) {
  const start = performance.now();
  run(command, args, options);
  return performance.now() - start;
}

run('cargo', ['build', '--release'], { cwd: CLI_DIR });

const statusRepo = mkdtempSync(join(tmpdir(), 'sorrel-bench-status-'));
for (let i = 0; i < fileCount; i += 1) {
  const bucket = join(statusRepo, `d${Math.floor(i / 1000)}`);
  mkdirSync(bucket, { recursive: true });
  writeFileSync(join(bucket, `f${i}.txt`), `file ${i}\n`);
}
run(CLI, ['init'], { cwd: statusRepo });
run(CLI, ['change', 'create', '-m', 'benchmark baseline'], {
  cwd: statusRepo,
});
run(CLI, ['status', '--json'], { cwd: statusRepo }); // populate stat cache
const warmStatusMs = timed(CLI, ['status', '--json'], { cwd: statusRepo });

const logRepo = mkdtempSync(join(tmpdir(), 'sorrel-bench-log-'));
run(CLI, ['init'], { cwd: logRepo });
const historyFile = join(logRepo, 'history.txt');
for (let i = 0; i < historyCount; i += 1) {
  writeFileSync(historyFile, `${i}\n`);
  run(CLI, ['change', 'create', '-m', `change ${i}`], { cwd: logRepo });
}
const logMs = timed(CLI, ['log', '--limit', String(historyCount), '--json'], {
  cwd: logRepo,
});

const result = {
  generatedAt: new Date().toISOString(),
  cli: {
    warmStatus: { files: fileCount, milliseconds: Number(warmStatusMs.toFixed(3)) },
    log: { changes: historyCount, milliseconds: Number(logMs.toFixed(3)) },
  },
};

console.log(JSON.stringify(result, null, 2));
