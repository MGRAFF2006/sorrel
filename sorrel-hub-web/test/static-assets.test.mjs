import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFile, stat } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';

test('thin host entrypoints exist', async () => {
  for (const name of ['index.html', 'src/main.tsx', 'vite.config.ts', 'server/static-server.mjs']) {
    const info = await stat(new URL(`../${name}`, import.meta.url));
    assert.ok(info.isFile(), `${name} should be a file`);
  }
});

test('index.html mounts shared hub-ui', async () => {
  const html = await readFile(new URL('../index.html', import.meta.url), 'utf8');
  assert.match(html, /Sorrel Hub/);
  assert.match(html, /id="root"/);
  assert.match(html, /src\/main\.tsx/);
});

test('main.tsx mounts sorrel-hub-ui with web platform', async () => {
  const main = await readFile(new URL('../src/main.tsx', import.meta.url), 'utf8');
  assert.match(main, /mountHubApp/);
  assert.match(main, /sorrel-hub-ui/);
  assert.match(main, /platformKind:\s*'web'/);
});

test('package depends on shared hub-ui', async () => {
  const pkg = JSON.parse(await readFile(new URL('../package.json', import.meta.url), 'utf8'));
  assert.equal(pkg.name, 'sorrel-hub-web');
  assert.ok(pkg.dependencies['sorrel-hub-ui']);
  assert.ok(pkg.dependencies['solid-js']);
  assert.match(pkg.scripts.dev, /vite/);
  assert.match(pkg.scripts.build, /vite build/);
});

test('static server module path resolves', async () => {
  const url = fileURLToPath(new URL('../server/static-server.mjs', import.meta.url));
  assert.ok(url.endsWith('static-server.mjs'));
});
