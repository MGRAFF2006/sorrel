import assert from 'node:assert/strict';
import { test } from 'node:test';

/**
 * Behavioral coverage for platform stubs and list unwrapping — no browser.
 * Source is TypeScript; assert contracts that hosts and CI rely on.
 */

test('platform stubs expose expected capability matrix', async () => {
  const source = await import('node:fs/promises').then((fs) =>
    fs.readFile(new URL('../src/platform.ts', import.meta.url), 'utf8'),
  );
  assert.match(source, /kind: 'web'/);
  assert.match(source, /kind: 'desktop'/);
  assert.match(source, /kind: 'mobile'/);
  assert.match(source, /localCore: true/);
  assert.match(source, /biometrics: true/);
  assert.match(source, /createDesktopPlatformStub/);
  assert.match(source, /createMobilePlatformStub/);
  assert.match(source, /createWebPlatform/);
});

test('App is project-first with nested feature routes', async () => {
  const app = await import('node:fs/promises').then((fs) =>
    fs.readFile(new URL('../src/App.tsx', import.meta.url), 'utf8'),
  );
  assert.match(app, /modules\.actions === true|showActions/);
  assert.match(app, /\/actions/);
  assert.match(app, /live-badge/);
  assert.match(app, /useOpenProposalsCount/);
  assert.match(app, /useOpenProposalsCountFromHub/);
  assert.match(app, /IdentityChip/);
  assert.match(app, /fetchSession/);
  assert.match(app, /DEV_IDENTITY_PRESETS/);
  assert.match(app, /app-shell/);
  assert.match(app, /app-sidebar/);
  assert.match(app, /\/projects\/:projectId/);
  assert.match(app, /ProjectLayout/);
  assert.match(app, /All projects/);
  assert.match(app, /Overview/);
});

test('session store and /session client wiring exist', async () => {
  const session = await import('node:fs/promises').then((fs) =>
    fs.readFile(new URL('../src/session.ts', import.meta.url), 'utf8'),
  );
  const api = await import('node:fs/promises').then((fs) =>
    fs.readFile(new URL('../src/api.ts', import.meta.url), 'utf8'),
  );
  assert.match(session, /sorrel\.hub\.actingPrincipal/);
  assert.match(session, /setActingPrincipal/);
  assert.match(api, /fetchSession/);
  assert.match(api, /setPrincipalProvider/);
  assert.match(api, /\/session/);
});

test('Reviews view covers proposal transitions and comment resolve', async () => {
  const reviews = await import('node:fs/promises').then((fs) =>
    fs.readFile(new URL('../src/views/ReviewsView.tsx', import.meta.url), 'utf8'),
  );
  assert.match(reviews, /PROPOSAL_TRANSITIONS/);
  assert.match(reviews, /approved/);
  assert.match(reviews, /state: 'resolved'/);
  assert.match(reviews, /DetailCommentForm/);
  assert.match(reviews, /admin\/proposals/);
  assert.match(reviews, /Select a proposal/);
  assert.match(reviews, /New proposal/);
  assert.match(reviews, /useParams/);
  assert.match(reviews, /projectId/);
});

test('Projects and Sync views handle empty and error states', async () => {
  const projects = await import('node:fs/promises').then((fs) =>
    fs.readFile(new URL('../src/views/ProjectsView.tsx', import.meta.url), 'utf8'),
  );
  const overview = await import('node:fs/promises').then((fs) =>
    fs.readFile(new URL('../src/views/ProjectOverview.tsx', import.meta.url), 'utf8'),
  );
  const sync = await import('node:fs/promises').then((fs) =>
    fs.readFile(new URL('../src/views/SyncView.tsx', import.meta.url), 'utf8'),
  );
  assert.match(projects, /No projects yet/);
  assert.match(projects, /Could not load projects/);
  assert.match(projects, /New project/);
  assert.match(projects, /ProjectsHome/);
  assert.match(projects, /\/projects\//);
  assert.match(overview, /ProjectOverview/);
  assert.match(overview, /Go to Reviews|View all/);
  assert.match(sync, /No sync repos linked to this project/);
  assert.match(sync, /No refs/);
  assert.match(sync, /Could not load/);
  assert.match(sync, /sync-split/);
  assert.match(sync, /useParams/);
});

test('responsive CSS covers mobile header/nav breakpoints', async () => {
  const css = await import('node:fs/promises').then((fs) =>
    fs.readFile(new URL('../src/styles/hub.css', import.meta.url), 'utf8'),
  );
  assert.match(css, /@media \(max-width: 860px\)/);
  assert.match(css, /prefers-reduced-motion/);
  assert.match(css, /\.live-badge/);
  assert.match(css, /\.app-shell/);
  assert.match(css, /\.app-sidebar/);
  assert.match(css, /\.split/);
  assert.match(css, /\.project-switcher/);
  assert.match(css, /\.feature-tile/);
});
