#!/usr/bin/env node

import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { existsSync, readFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const manifest = JSON.parse(
  readFileSync(join(ROOT, 'release/manifest.json'), 'utf8'),
);
const expectedVersion = manifest.release.replace(/^v/, '');

function read(path) {
  return readFileSync(join(ROOT, path), 'utf8');
}

function moduleHead(module) {
  return execFileSync('git', ['-C', module, 'rev-parse', 'HEAD'], {
    cwd: ROOT,
    encoding: 'utf8',
  }).trim();
}

function packageVersion(module) {
  const path = join(ROOT, module, 'package.json');
  if (!existsSync(path)) return undefined;
  return JSON.parse(readFileSync(path, 'utf8')).version;
}

function cargoVersion(module) {
  const path = join(ROOT, module, 'Cargo.toml');
  if (!existsSync(path)) return undefined;
  return readFileSync(path, 'utf8').match(/^version\s*=\s*"([^"]+)"/m)?.[1];
}

function corePin(module) {
  const cargo = read(`${module}/Cargo.toml`);
  return cargo.match(
    /sorrel-core\s*=\s*\{[^}]*\brev\s*=\s*"([0-9a-f]{40})"/,
  )?.[1];
}

const errors = [];
function check(condition, message) {
  if (!condition) errors.push(message);
}

check(
  JSON.parse(read('package.json')).version === expectedVersion,
  `root package version must be ${expectedVersion}`,
);

for (const [module, expectedSha] of Object.entries(manifest.modules)) {
  const actualSha = moduleHead(module);
  check(
    actualSha === expectedSha,
    `${module} HEAD ${actualSha} != release manifest ${expectedSha}`,
  );

  const version = packageVersion(module) ?? cargoVersion(module);
  if (version !== undefined) {
    check(
      version === expectedVersion,
      `${module} version ${version} != ${expectedVersion}`,
    );
  }

  for (const file of ['LICENSE-APACHE', 'LICENSE-MIT', 'CHANGELOG.md']) {
    check(existsSync(join(ROOT, module, file)), `${module}/${file} is missing`);
  }
}

const coreSha = manifest.modules['sorrel-core'];
for (const module of ['sorrel-cli', 'sorrel-sdk-rust']) {
  const pin = corePin(module);
  check(pin === coreSha, `${module} pins core ${pin ?? '(missing)'} != ${coreSha}`);
}

for (const file of ['LICENSE-APACHE', 'LICENSE-MIT', 'CHANGELOG.md']) {
  check(existsSync(join(ROOT, file)), `root ${file} is missing`);
}

if (errors.length > 0) {
  console.error('Release consistency check failed:');
  for (const error of errors) console.error(`- ${error}`);
  process.exit(1);
}

assert.equal(Object.keys(manifest.modules).length, 12);
console.log(
  `Release ${manifest.release}: 12 module SHAs, versions, licenses, changelogs, and core pins are consistent.`,
);
