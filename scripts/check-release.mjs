#!/usr/bin/env node

import { existsSync, readFileSync, readdirSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const manifest = JSON.parse(
  readFileSync(join(ROOT, 'release/manifest.json'), 'utf8'),
);
const expectedVersion = manifest.release.replace(/^v/, '');
const requestedTag = process.argv[2];

function read(path) {
  return readFileSync(join(ROOT, path), 'utf8');
}

function nodePackage(module) {
  const path = join(ROOT, module, 'package.json');
  if (!existsSync(path)) return undefined;
  return JSON.parse(readFileSync(path, 'utf8'));
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
if (requestedTag) {
  check(
    requestedTag === manifest.release,
    `requested release tag ${requestedTag} != manifest release ${manifest.release}`,
  );
}
check(
  read('CHANGELOG.md').includes(`## [${expectedVersion}] - ${manifest.releaseDate}`),
  `CHANGELOG.md must contain ${expectedVersion} dated ${manifest.releaseDate}`,
);

check(!existsSync(join(ROOT, '.gitmodules')), '.gitmodules must not exist in the monorepo');

const distConfig = read('dist-workspace.toml');
check(
  distConfig.includes('"aarch64-pc-windows-msvc"'),
  'dist targets must include Windows ARM64 (aarch64-pc-windows-msvc)',
);
check(
  distConfig.includes('local-artifacts-jobs = ["./build-desktop"]'),
  'dist must register the reusable desktop artifact workflow',
);

const releaseWorkflow = read('.github/workflows/release.yml');
check(
  releaseWorkflow.includes('uses: ./.github/workflows/build-desktop.yml'),
  'generated release workflow must call the desktop artifact workflow',
);

const desktopReleaseWorkflow = read('.github/workflows/build-desktop.yml');
for (const target of [
  'aarch64-apple-darwin',
  'aarch64-pc-windows-msvc',
  'aarch64-unknown-linux-gnu',
  'x86_64-apple-darwin',
  'x86_64-pc-windows-msvc',
  'x86_64-unknown-linux-gnu',
]) {
  check(
    desktopReleaseWorkflow.includes(`target: ${target}`),
    `desktop release matrix must include ${target}`,
  );
}

const desktopCargo = read('sorrel-hub-desktop/src-tauri/Cargo.toml');
check(
  desktopCargo.match(/^version\s*=\s*"([^"]+)"/m)?.[1] === expectedVersion,
  `sorrel-hub-desktop/src-tauri Cargo version must be ${expectedVersion}`,
);
check(
  JSON.parse(read('sorrel-hub-desktop/src-tauri/tauri.conf.json')).version === expectedVersion,
  `sorrel-hub-desktop Tauri version must be ${expectedVersion}`,
);

const manifestModules = new Set(Object.keys(manifest.modules));
const workspaceModules = readdirSync(ROOT, { withFileTypes: true })
  .filter((entry) => entry.isDirectory() && entry.name.startsWith('sorrel-'))
  .map((entry) => entry.name);
for (const module of workspaceModules) {
  check(manifestModules.has(module), `${module}/ is missing from release/manifest.json`);
}

for (const module of Object.keys(manifest.modules)) {
  check(existsSync(join(ROOT, module)), `${module}/ is missing`);

  const pkg = nodePackage(module);
  const version = pkg?.version ?? cargoVersion(module);
  if (version !== undefined) {
    check(
      version === expectedVersion,
      `${module} version ${version} != ${expectedVersion}`,
    );
  }

  for (const file of ['AGENTS.md', 'README.md', 'LICENSE-APACHE', 'LICENSE-MIT', 'CHANGELOG.md']) {
    check(existsSync(join(ROOT, module, file)), `${module}/${file} is missing`);
  }

  if (existsSync(join(ROOT, module, 'CHANGELOG.md'))) {
    const changelog = read(`${module}/CHANGELOG.md`);
    check(
      changelog.includes('## [Unreleased]'),
      `${module}/CHANGELOG.md must keep an [Unreleased] section`,
    );
    check(
      changelog.includes(`## [${expectedVersion}] - ${manifest.releaseDate}`),
      `${module}/CHANGELOG.md must contain ${expectedVersion} dated ${manifest.releaseDate}`,
    );
  }

  if (pkg) {
    check(existsSync(join(ROOT, module, 'package-lock.json')), `${module}/package-lock.json is missing`);
    check(typeof pkg.scripts?.check === 'string', `${module} must define an npm check script`);
  }

  if (existsSync(join(ROOT, module, 'Cargo.toml'))) {
    check(
      read('Cargo.toml').includes(`"${module}"`),
      `${module} must be listed in the root Cargo workspace`,
    );
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

console.log(
  `Release ${manifest.release}: ${manifestModules.size} modules, metadata, docs, checks, and workspace links are consistent.`,
);
