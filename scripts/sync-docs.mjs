#!/usr/bin/env node

import { existsSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const checkOnly = process.argv.includes('--check');
const docs = [
  { source: 'docs/GETTING_STARTED.md', output: 'GETTING_STARTED.md' },
  { source: 'docs/STATUS.md', output: 'STATUS.md' },
  { source: 'docs/ARCHITECTURE.md', output: 'ARCHITECTURE.md' },
  { source: 'docs/DEVELOPMENT.md', output: 'DEVELOPMENT.md' },
  { source: 'docs/RELEASE.md', output: 'RELEASE.md' },
  { source: 'CHANGELOG.md', output: 'CHANGELOG.md' },
];
const drifted = [];

function publishedContent(source, content) {
  const notice = `<!-- Generated from ${source} by npm run sync:docs. Do not edit. -->\n\n`;
  let linksForPublishedTree = content.replace(
    /\]\(\.\.\/([^)]*)\)/g,
    '](' + 'https://github.com/MGRAFF2006/sorrel/blob/main/$1)',
  );
  if (!source.startsWith('docs/')) {
    linksForPublishedTree = linksForPublishedTree.replace(
      /\]\((?!https?:|#|\/)([^)]*)\)/g,
      '](' + 'https://github.com/MGRAFF2006/sorrel/blob/main/$1)',
    );
  }
  return notice + linksForPublishedTree;
}

for (const doc of docs) {
  const source = join(ROOT, doc.source);
  const mirror = join(ROOT, 'sorrel-web', 'docs', doc.output);
  const content = publishedContent(doc.source, readFileSync(source, 'utf8'));
  const current = existsSync(mirror) ? readFileSync(mirror, 'utf8') : '';

  if (content === current) continue;
  if (checkOnly) {
    drifted.push(doc.output);
  } else {
    writeFileSync(mirror, content);
    console.log(`synced ${doc.source} -> sorrel-web/docs/${doc.output}`);
  }
}

if (drifted.length > 0) {
  console.error('Published Markdown mirrors are stale:');
  for (const name of drifted) console.error(`- sorrel-web/docs/${name}`);
  console.error('Run `npm run sync:docs` after editing the source docs.');
  process.exit(1);
}

if (checkOnly) console.log('Published Markdown mirrors match canonical docs.');
