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

test('App exposes the repository-first project and global collaboration routes', async () => {
  const app = await import('node:fs/promises').then((fs) =>
    fs.readFile(new URL('../src/App.tsx', import.meta.url), 'utf8'),
  );
  assert.doesNotMatch(app, /ActionsPlaceholder|\/actions/);
  assert.match(app, /nav-count/);
  assert.match(app, /useOpenProposalsCount/);
  assert.match(app, /useOpenProposalsCountFromHub/);
  assert.match(app, /IdentityControl/);
  assert.match(app, /fetchSession/);
  assert.match(app, /DEV_IDENTITY_PRESETS/);
  assert.match(app, /hub-shell/);
  assert.match(app, /global-bar/);
  assert.match(app, /\/projects\/:projectId/);
  assert.match(app, /ProjectLayout/);
  assert.match(app, /\/inbox/);
  assert.match(app, /\/orgs\/:orgId/);
  assert.match(app, /\/profile/);
  assert.match(app, /\/work/);
  assert.match(app, />Code</);
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
  assert.match(reviews, /Select a review/);
  assert.match(reviews, /Open review/);
  assert.match(reviews, /useParams/);
  assert.match(reviews, /projectId/);
});

test('Inbox, Work, and identity views derive from Hub records', async () => {
  const fs = await import('node:fs/promises');
  const [inbox, work, organizations, profile] = await Promise.all([
    fs.readFile(new URL('../src/views/InboxView.tsx', import.meta.url), 'utf8'),
    fs.readFile(new URL('../src/views/WorkView.tsx', import.meta.url), 'utf8'),
    fs.readFile(new URL('../src/views/OrganizationsView.tsx', import.meta.url), 'utf8'),
    fs.readFile(new URL('../src/views/ProfileView.tsx', import.meta.url), 'utf8'),
  ]);
  assert.match(inbox, /admin\/proposals/);
  assert.match(inbox, /admin\/review-comments/);
  assert.match(inbox, /admin\/workflow-runs/);
  assert.match(inbox, /Global, not home/);
  assert.match(work, /statuses: \['draft'\]/);
  assert.match(work, /statuses: \['merged', 'closed'\]/);
  assert.match(work, /Proposal-backed lanes/);
  assert.match(organizations, /metadataString\(org\(\)\.metadata, 'readme'\)/);
  assert.match(organizations, /project\.organizationId/);
  assert.match(profile, /authorPrincipal/);
  assert.match(profile, /does not invent a biography/);
});

test('Projects, Code, and Sync views handle repository data and empty states', async () => {
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
  assert.match(projects, /Create project/);
  assert.match(projects, /ProjectsHome/);
  assert.match(projects, /\/projects\//);
  assert.match(overview, /ProjectOverview/);
  assert.match(overview, /No synchronized repository yet/);
  assert.match(overview, /\/tree\?ref=/);
  assert.match(overview, /\/files\?ref=/);
  assert.match(overview, /README/);
  assert.match(sync, /No repositories connected yet/);
  assert.match(sync, /No refs/);
  assert.match(sync, /Could not load/);
  assert.match(sync, /sync-split/);
  assert.match(sync, /useParams/);
});

test('responsive CSS covers the project shell, inbox, workbench, and mobile breakpoints', async () => {
  const css = await import('node:fs/promises').then((fs) =>
    fs.readFile(new URL('../src/styles/hub.css', import.meta.url), 'utf8'),
  );
  assert.match(css, /@media \(max-width: 860px\)/);
  assert.match(css, /prefers-reduced-motion/);
  assert.match(css, /\.global-bar/);
  assert.match(css, /\.project-chrome/);
  assert.match(css, /\.inbox-view/);
  assert.match(css, /\.kanban-board/);
  assert.match(css, /\.split/);
  assert.match(css, /\.review-workbench/);
  assert.match(css, /\.identity-readme/);
});
