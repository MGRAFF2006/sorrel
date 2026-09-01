import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFile, stat } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('../', import.meta.url));

test('package exports mount entry and styles', async () => {
  const pkg = JSON.parse(await readFile(new URL('../package.json', import.meta.url), 'utf8'));
  assert.equal(pkg.name, 'sorrel-hub-ui');
  assert.ok(pkg.exports['.']);
  assert.ok(pkg.exports['./styles.css']);
});

test('platform stubs and mount surface exist', async () => {
  for (const name of [
    'src/platform.ts',
    'src/index.tsx',
    'src/App.tsx',
    'src/views/ProjectsView.tsx',
    'src/views/ProjectOverview.tsx',
    'src/views/InboxView.tsx',
    'src/views/WorkView.tsx',
    'src/views/OrganizationsView.tsx',
    'src/views/ProfileView.tsx',
    'src/views/ReviewsView.tsx',
    'src/views/SyncView.tsx',
    'src/styles/hub.css',
  ]) {
    const info = await stat(new URL(`../${name}`, import.meta.url));
    assert.ok(info.isFile(), `${name} should exist`);
  }
});

test('UI talks to Hub under /api and includes live inbox-count wiring', async () => {
  const api = await readFile(new URL('../src/api.ts', import.meta.url), 'utf8');
  assert.match(api, /\/api/);
  assert.match(api, /\/capabilities/);

  const app = await readFile(new URL('../src/App.tsx', import.meta.url), 'utf8');
  assert.match(app, /nav-count/);
  assert.match(app, /Inbox/);
  assert.match(app, /Work/);
  assert.match(app, /Reviews/);
  assert.match(app, /Sync/);
  assert.match(app, /\/projects\/:projectId/);

  const convex = await readFile(
    new URL('../src/convex/openProposals.ts', import.meta.url),
    'utf8',
  );
  assert.match(convex, /countOpen/);
  assert.match(convex, /ConvexClient/);
});

test('package root resolves', () => {
  assert.ok(root.endsWith('sorrel-hub-ui/') || root.includes('sorrel-hub-ui'));
});
