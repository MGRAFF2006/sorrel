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
  assert.match(html, /<main id="top" tabindex="-1">/);
  assert.match(html, /aria-label="Primary navigation"/);
  assert.match(html, /class="nav-toggle"/);
  assert.match(html, /class="theme-toggle"/);
  assert.match(html, /src="\.\/site\.js"/);
  assert.match(html, /href="\.\/docs\/index\.html"/);
});

test('landing page presents the agent-native model and honest alpha status', async () => {
  const html = await readFile(resolve(ROOT, 'index.html'), 'utf8');
  assert.match(html, /id="model"/);
  assert.match(html, /id="architecture"/);
  assert.match(html, /class="lane-map"/);
  assert.match(html, /Working now/);
  assert.match(html, /Still ahead/);
  assert.match(html, /Production Hub authentication/);
});

test('every page uses the current Sorrel logo for its brand and favicon', async () => {
  const htmlFiles = (await filesUnder(ROOT)).filter((path) => path.endsWith('.html'));

  for (const htmlFile of htmlFiles) {
    const html = await readFile(htmlFile, 'utf8');
    assert.match(html, /<link rel="icon" href="(?:\.\.\/|\.\/)assets\/logo\.svg" type="image\/svg\+xml">/);
    assert.match(html, /<img class="brand-logo" src="(?:\.\.\/|\.\/)assets\/logo\.svg" alt="" aria-hidden="true">/);
  }
});

test('every page exposes keyboard and compact-navigation controls', async () => {
  const htmlFiles = (await filesUnder(ROOT)).filter((path) => path.endsWith('.html'));

  for (const htmlFile of htmlFiles) {
    const html = await readFile(htmlFile, 'utf8');
    const skipTarget = /<a class="skip-link" href="#([^"]+)">Skip to main content<\/a>/.exec(html)?.[1];
    assert.ok(skipTarget, `${htmlFile}: missing skip link`);
    assert.match(html, new RegExp(`<main[^>]*id="${skipTarget}"[^>]*tabindex="-1"`));
    assert.match(html, /<button class="nav-toggle"[^>]*aria-expanded="false"[^>]*aria-controls="primary-nav"/);
    assert.match(html, /<nav class="nav" id="primary-nav" aria-label="Primary navigation">/);
  }
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
