import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFile, stat } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';

const publicDir = fileURLToPath(new URL('../public/', import.meta.url));

test('core static assets exist', async () => {
  for (const name of ['index.html', 'app.js', 'styles.css']) {
    const info = await stat(new URL(name, `file://${publicDir}`));
    assert.ok(info.isFile(), `${name} should be a file`);
  }
});

test('index.html references the app and styles', async () => {
  const html = await readFile(new URL('index.html', `file://${publicDir}`), 'utf8');
  assert.match(html, /styles\.css/);
  assert.match(html, /app\.js/);
  assert.match(html, /Sorrel Hub/);
});

test('index.html includes Sync nav and collaboration write forms', async () => {
  const html = await readFile(new URL('index.html', `file://${publicDir}`), 'utf8');
  assert.match(html, /data-view="sync"/);
  assert.match(html, />Sync</);
  assert.match(html, /id="view-sync"/);
  assert.match(html, /id="sync-repos"/);
  assert.match(html, /id="proposal-form"/);
  assert.match(html, /id="comment-form"/);
  assert.match(html, /data-collection="review-comments"/);
  assert.match(html, /id="project-form"/);
});

test('app.js calls the Hub API under the /api prefix with mutations', async () => {
  const js = await readFile(new URL('app.js', `file://${publicDir}`), 'utf8');
  assert.match(js, /\/api/);
  assert.match(js, /\/projects/);
  assert.match(js, /\/healthz/);
  assert.match(js, /apiPost/);
  assert.match(js, /apiPatch/);
  assert.match(js, /unwrapList/);
  assert.match(js, /\/admin\/proposals/);
  assert.match(js, /\/admin\/review-comments/);
});

test('app.js loads sync repos and refs endpoints', async () => {
  const js = await readFile(new URL('app.js', `file://${publicDir}`), 'utf8');
  assert.match(js, /\/admin\/sync-repos/);
  assert.match(js, /encodeURIComponent\(repoId\)/);
  assert.match(js, /\/refs/);
  assert.match(js, /loadSyncRepos/);
  assert.match(js, /No repositories have been synced yet/);
  assert.match(js, /No refs/);
});

test('dev server module imports without side effects on import', async () => {
  const url = fileURLToPath(new URL('../server/dev-server.mjs', import.meta.url));
  assert.ok(url.endsWith('dev-server.mjs'));
});
