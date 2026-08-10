#!/usr/bin/env node

import assert from 'node:assert/strict';
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

function packageVersion(module) {
  const path = join(ROOT, module, 'package.json');
  if (!existsSync(path)) return undefined;
  return JSON.parse(readFileSync(path, 'utf8')).version;
}

function cargoVersion(module) {
  const path = join(ROOT, module, 'Cargo.toml');
  if (!existsSync(path)) return undefined;
  const text = readFileSync(path, 'utf8');
  return (
    text.match(/^version\s*=\s*"([^"]+)"/m)?.[1] ??
    (text.includes('version.workspace = true') ? expectedVersion : undefined)
  );
}

function usesPathCore(module) {
  const cargo = read(`${module}/Cargo.toml`);
  return /sorrel-core\s*=\s*\{\s*path\s*=\s*"\.\.\/sorrel-core"\s*\}/.test(
    cargo,
  );
}

const errors = [];
function check(condition, message) {
  if (!condition) errors.push(message);
}

check(
  JSON.parse(read('package.json')).version === expectedVersion,
  `root package version must be ${expectedVersion}`,
);

check(!existsSync(join(ROOT, '.gitmodules')), '.gitmodules must not exist in the monorepo');

for (const module of Object.keys(manifest.modules)) {
  check(existsSync(join(ROOT, module)), `${module}/ is missing`);

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

for (const module of ['sorrel-cli', 'sorrel-sdk-rust']) {
  check(
    usesPathCore(module),
    `${module} must depend on sorrel-core via path = "../sorrel-core"`,
  );
}

for (const file of ['LICENSE-APACHE', 'LICENSE-MIT', 'CHANGELOG.md', 'Cargo.toml']) {
  check(existsSync(join(ROOT, file)), `root ${file} is missing`);
}

if (errors.length > 0) {
  console.error('Release consistency check failed:');
  for (const error of errors) console.error(`- ${error}`);
  process.exit(1);
}

assert.equal(Object.keys(manifest.modules).length, 12);
console.log(
  `Release ${manifest.release}: monorepo modules, versions, licenses, changelogs, and path deps are consistent.`,
);
