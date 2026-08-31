#!/usr/bin/env node

import { readFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const manifest = JSON.parse(
  readFileSync(join(ROOT, 'release', 'manifest.json'), 'utf8'),
);
const tag = process.argv[2] ?? manifest.release;
const version = tag.replace(/^v/, '');
const changelog = readFileSync(join(ROOT, 'CHANGELOG.md'), 'utf8');
const heading = new RegExp(`^## \\[${escapeRegExp(version)}\\](?: - .+)?$`, 'm');
const match = heading.exec(changelog);

if (!match) {
  console.error(`CHANGELOG.md has no release section for ${version}`);
  process.exit(1);
}

const bodyStart = match.index + match[0].length;
const remainder = changelog.slice(bodyStart);
const nextHeading = remainder.search(/^## /m);
const linkDefinitions = remainder.search(/^\[Unreleased\]:/m);
const candidates = [nextHeading, linkDefinitions].filter((index) => index >= 0);
const bodyEnd = candidates.length > 0 ? Math.min(...candidates) : remainder.length;
const body = remainder.slice(0, bodyEnd).trim();

if (!body) {
  console.error(`CHANGELOG.md release section for ${version} is empty`);
  process.exit(1);
}

process.stdout.write(`# Sorrel ${tag}\n\n${body}\n`);

function escapeRegExp(value) {
  return value.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}
