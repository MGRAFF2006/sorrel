#!/usr/bin/env node

import { createHash } from 'node:crypto';
import {
  copyFile,
  mkdir,
  readdir,
  readFile,
  writeFile,
} from 'node:fs/promises';
import { basename, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const BUNDLE_SUFFIXES = ['.AppImage', '.app.tar.gz', '.deb', '.dmg', '.exe', '.msi', '.rpm'];

export async function collectArtifacts(sourceDirectory, outputDirectory) {
  const entries = await readdir(sourceDirectory, { recursive: true, withFileTypes: true });
  const sources = entries
    .filter((entry) => entry.isFile() && BUNDLE_SUFFIXES.some((suffix) => entry.name.endsWith(suffix)))
    .map((entry) => join(entry.parentPath, entry.name))
    .sort();

  if (sources.length === 0) {
    throw new Error(`no desktop bundles found under ${sourceDirectory}`);
  }

  await mkdir(outputDirectory, { recursive: true });
  const names = new Set();
  for (const source of sources) {
    const name = basename(source);
    if (names.has(name)) throw new Error(`duplicate desktop bundle name: ${name}`);
    names.add(name);

    const destination = join(outputDirectory, name);
    await copyFile(source, destination);
    const digest = createHash('sha256').update(await readFile(destination)).digest('hex');
    await writeFile(`${destination}.sha256`, `${digest}  ${name}\n`);
  }

  return [...names];
}

async function main() {
  const [source, output] = process.argv.slice(2);
  if (!source || !output) {
    throw new Error('usage: collect-artifacts SOURCE_DIRECTORY OUTPUT_DIRECTORY');
  }
  const names = await collectArtifacts(resolve(source), resolve(output));
  console.log(`Collected ${names.length} desktop bundle(s): ${names.join(', ')}`);
}

if (process.argv[1] && fileURLToPath(import.meta.url) === resolve(process.argv[1])) {
  main().catch((error) => {
    console.error(error.message);
    process.exit(1);
  });
}
