#!/usr/bin/env node

import { existsSync, readdirSync, readFileSync, statSync } from 'node:fs';
import { dirname, extname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const SKIP_DIRS = new Set(['.dev', '.git', 'dist', 'node_modules', 'target']);

function markdownFiles(directory) {
  const files = [];
  for (const entry of readdirSync(directory, { withFileTypes: true })) {
    if (entry.isDirectory() && SKIP_DIRS.has(entry.name)) continue;
    const path = join(directory, entry.name);
    if (entry.isDirectory()) files.push(...markdownFiles(path));
    else if (entry.isFile() && extname(entry.name) === '.md') files.push(path);
  }
  return files;
}

function filesWithExtension(directory, extension) {
  const files = [];
  for (const entry of readdirSync(directory, { withFileTypes: true })) {
    if (entry.isDirectory() && SKIP_DIRS.has(entry.name)) continue;
    const path = join(directory, entry.name);
    if (entry.isDirectory()) files.push(...filesWithExtension(path, extension));
    else if (entry.isFile() && extname(entry.name) === extension) files.push(path);
  }
  return files;
}

function withoutFencedCode(markdown) {
  return markdown.replace(/^```[\s\S]*?^```\s*$/gm, '');
}

const failures = [];
for (const file of markdownFiles(ROOT)) {
  const markdown = withoutFencedCode(readFileSync(file, 'utf8'));
  const links = markdown.matchAll(/!?(?:\[[^\]]*\])\(([^)]+)\)/g);

  for (const match of links) {
    let target = match[1].trim();
    if (target.startsWith('<') && target.endsWith('>')) {
      target = target.slice(1, -1);
    } else {
      target = target.split(/\s+["']/)[0];
    }

    if (
      !target ||
      target.startsWith('#') ||
      target.startsWith('/') ||
      /^[a-z][a-z\d+.-]*:/i.test(target)
    ) {
      continue;
    }

    const relativePath = decodeURIComponent(target.split('#')[0].split('?')[0]);
    if (!relativePath) continue;
    const resolved = resolve(dirname(file), relativePath);
    if (!existsSync(resolved)) {
      failures.push(`${file.slice(ROOT.length + 1)} -> ${target}`);
      continue;
    }

    // A directory link is valid only when the directory can act as a docs page.
    if (statSync(resolved).isDirectory() && !existsSync(join(resolved, 'README.md'))) {
      failures.push(`${file.slice(ROOT.length + 1)} -> ${target} (directory has no README.md)`);
    }
  }
}

const siteRoot = join(ROOT, 'sorrel-web');
for (const file of filesWithExtension(siteRoot, '.html')) {
  const html = readFileSync(file, 'utf8');
  const links = html.matchAll(/\b(?:href|src)=["']([^"']+)["']/gi);

  for (const match of links) {
    const target = match[1].trim();
    if (!target || target.startsWith('#') || /^[a-z][a-z\d+.-]*:/i.test(target)) {
      continue;
    }

    const relativePath = decodeURIComponent(target.split('#')[0].split('?')[0]);
    if (!relativePath) continue;
    const resolved = relativePath.startsWith('/')
      ? resolve(siteRoot, relativePath.slice(1))
      : resolve(dirname(file), relativePath);
    const candidate = existsSync(resolved) && statSync(resolved).isDirectory()
      ? join(resolved, 'index.html')
      : resolved;
    if (!existsSync(candidate)) {
      failures.push(`${file.slice(ROOT.length + 1)} -> ${target}`);
    }
  }
}

if (failures.length > 0) {
  console.error('Broken local documentation links:');
  for (const failure of failures) console.error(`- ${failure}`);
  process.exit(1);
}

console.log('Local Markdown and static-site links are valid.');
