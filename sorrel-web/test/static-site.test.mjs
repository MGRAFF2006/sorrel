import assert from 'node:assert/strict';
import { readdir, readFile, stat } from 'node:fs/promises';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { test } from 'node:test';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');

async function filesUnder(directory) {
  const entries = await readdir(directory, { withFileTypes: true });
  const nested = await Promise.all(entries.map((entry) => {
    const path = resolve(directory, entry.name);
    return entry.isDirectory() ? filesUnder(path) : [path];
  }));
  return nested.flat();
}

test('landing page exposes its primary navigation and theme controls', async () => {
  const html = await readFile(resolve(ROOT, 'index.html'), 'utf8');
  assert.match(html, /<main id="top">/);
  assert.match(html, /aria-label="Primary navigation"/);
  assert.match(html, /class="theme-toggle"/);
  assert.match(html, /src="\.\/site\.js"/);
  assert.match(html, /href="\.\/docs\/index\.html"/);
});

test('relative HTML assets and page links resolve to files', async () => {
  const htmlFiles = (await filesUnder(ROOT)).filter((path) => path.endsWith('.html'));
  assert.ok(htmlFiles.length > 1, 'expected landing and documentation HTML');

  for (const htmlFile of htmlFiles) {
    const html = await readFile(htmlFile, 'utf8');
    const references = [...html.matchAll(/(?:href|src)="([^"]+)"/g)]
      .map((match) => match[1])
      .filter((reference) =>
        !reference.startsWith('#') &&
        !reference.startsWith('?') &&
        !reference.startsWith('http:') &&
        !reference.startsWith('https:') &&
        !reference.startsWith('data:'),
      );

    for (const reference of references) {
      const clean = reference.split(/[?#]/, 1)[0];
      // Rustdoc is an optional local build and is intentionally not committed.
      if (clean.includes('/api/sorrel-core/')) continue;
      const target = clean.startsWith('/')
        ? resolve(ROOT, clean.slice(1))
        : resolve(dirname(htmlFile), clean);
      const info = await stat(target).catch(() => null);
      assert.ok(info?.isFile(), `${htmlFile}: missing ${reference}`);
    }
  }
});
