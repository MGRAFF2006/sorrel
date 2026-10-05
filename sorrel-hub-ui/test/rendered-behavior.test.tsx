import { fireEvent, render, screen, waitFor } from '@solidjs/testing-library';
import { afterEach, describe, expect, test, vi } from 'vitest';
import { HubApp } from '../src/App.tsx';
import { createWebPlatform } from '../src/platform.ts';
import { LOCAL_PRINCIPAL } from '../src/api.ts';
import { setActingPrincipal } from '../src/session.ts';

type FetchCall = { url: string; init?: RequestInit };

function json(data: unknown, status = 200) {
  return new Response(JSON.stringify(data), {
    status,
    headers: { 'content-type': 'application/json' },
  });
}

function installHubFetch(projects: unknown[] = [], options: { authenticated?: boolean; developmentSession?: boolean; rejectedMutation?: boolean } = {}) {
  const calls: FetchCall[] = [];
  const fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
    const url = String(input);
    calls.push({ url, init });

    if (url === '/api/capabilities') {
      return json({
        data: {
          modules: { core: true, actions: false, agents: true, secrets: true, objectStorage: 'fs' },
          auth: { mode: options.authenticated ? 'oidc' : 'dev', session: options.authenticated ? 'bearer' : 'none' },
          convex: { enabled: false },
          deploy: 'dev',
        },
      });
    }
    if (url === '/api/session') {
      if (options.authenticated) return json({ data: {
        auth: { mode: 'oidc', session: 'bearer' },
        session: { principal: { type: 'user', id: 'signed-in' }, sessionId: 'session', authMode: 'oidc', idpSubject: 'signed-in', expiresAt: null },
      } });
      if (options.developmentSession) return json({ data: {
        auth: { mode: 'dev', session: 'none' },
        session: { principal: { type: 'user', id: 'maintainer' }, sessionId: 'dev:user:maintainer', authMode: 'dev', idpSubject: 'user:maintainer', expiresAt: null },
      } });
      return json({ data: { auth: { mode: 'dev', session: 'none' }, session: null } });
    }
    if (url === '/api/healthz') return json({ status: 'ok' });
    if (url === '/api/admin/proposals?status=open') return json({ data: [] });
    if (url === '/api/projects' && (init?.method ?? 'GET') === 'GET') {
      return json({ data: projects });
    }
    if (url.startsWith('/api/projects/') && (init?.method ?? 'GET') === 'GET') {
      const id = decodeURIComponent(url.slice('/api/projects/'.length));
      const project = projects.find((item) => (item as { id?: string }).id === id);
      if (project) return json({ data: project });
    }
    if (url === '/api/admin/repositories?projectId=project_alpha') {
      return json({ data: [{ id: 'repo_alpha', name: 'alpha', owner: 'acme', provider: 'sorrel', defaultBranch: 'main' }] });
    }
    if (url === '/api/admin/proposals?projectId=project_alpha') return json({ data: options.rejectedMutation
      ? [{ id: 'proposal_alpha', projectId: 'project_alpha', title: 'Review Alpha', status: 'open' }]
      : [] });
    if (url === '/api/admin/proposals/proposal_alpha' && init?.method === 'PATCH') {
      return json({ error: { message: 'Proposal permission denied' } }, 403);
    }
    if (url === '/api/admin/sync-repos') return json({ repos: [{ id: 'repo_alpha', refCount: 1 }] });
    if (url === '/api/repo_alpha/refs') return json({ refs: [{ name: 'main', snapshot: 'a'.repeat(64) }] });
    if (url === '/api/repo_alpha/tree?ref=main&path=') {
      return json({
        repoId: 'repo_alpha',
        ref: 'main',
        path: '',
        snapshot: { id: 'a'.repeat(64), message: 'Ship repository view', createdAt: '2026-09-01T10:00:00Z', author: { type: 'user', id: 'local' }, parents: [] },
        entries: [{ name: 'README.md', path: 'README.md', type: 'file', mode: 'normal', size: 32, objectId: 'b'.repeat(64) }],
      });
    }
    if (url === '/api/repo_alpha/files?ref=main&path=README.md') {
      return json({ repoId: 'repo_alpha', ref: 'main', path: 'README.md', objectId: 'b'.repeat(64), size: 32, encoding: 'utf-8', content: '# Alpha\n\nRepository-shaped work.' });
    }
    if (url === '/api/projects' && init?.method === 'POST') {
      return json({ data: { id: 'project_new' } }, 201);
    }
    if (url === '/api/projects/project_new') {
      return json({ data: { id: 'project_new', name: 'New project', organizationId: 'org_local' } });
    }
    return json({ data: [] });
  });
  vi.stubGlobal('fetch', fetchMock);
  return calls;
}

afterEach(() => {
  vi.unstubAllGlobals();
  setActingPrincipal(LOCAL_PRINCIPAL);
  localStorage.clear();
});

describe('HubApp rendered behavior', () => {
  test('development session hints do not pin the identity selector or write headers', async () => {
    window.history.pushState({}, '', '/profile');
    const calls = installHubFetch([], { developmentSession: true });
    render(() => <HubApp platform={createWebPlatform()} />);
    await screen.findByRole('heading', { level: 1 });
    await waitFor(() => expect(calls.filter((call) => call.url === '/api/session')).toHaveLength(2));
    await new Promise((resolve) => setTimeout(resolve, 0));
    const identity = await screen.findByRole('combobox', { name: 'Acting principal' });
    await fireEvent.change(identity, { target: { value: 'user:reviewer' } });
    expect(identity).toHaveValue('user:reviewer');

    expect(await screen.findByRole('heading', { level: 1, name: 'reviewer' })).toBeInTheDocument();
    await fireEvent.click(screen.getByRole('link', { name: /^Projects$/ }));
    await screen.findByText('No projects yet');
    await fireEvent.click((await screen.findAllByRole('button', { name: 'Create project' }))[0]);
    await fireEvent.input(screen.getByLabelText('Organization'), { target: { value: 'org_local' } });
    await fireEvent.input(screen.getByLabelText('Project name'), { target: { value: 'Platform' } });
    await fireEvent.submit(screen.getByRole('button', { name: 'Create and continue' }).closest('form')!);
    await waitFor(() => expect(calls.some((call) => call.url === '/api/projects' && call.init?.method === 'POST')).toBe(true));
    const create = calls.find((call) => call.url === '/api/projects' && call.init?.method === 'POST');
    expect(create?.init?.headers).toMatchObject({
      'x-sorrel-acting-principal': JSON.stringify({ type: 'user', id: 'reviewer' }),
    });
  });

  test('uses the authenticated Hub principal without persisting it as a development hint', async () => {
    window.history.pushState({}, '', '/profile');
    installHubFetch([], { authenticated: true });
    render(() => <HubApp platform={createWebPlatform()} />);

    expect(await screen.findByRole('heading', { level: 1, name: 'signed-in' })).toBeInTheDocument();
    expect(screen.queryByRole('combobox', { name: 'Acting principal' })).not.toBeInTheDocument();
    expect(localStorage.getItem('sorrel.hub.actingPrincipal')).toBeNull();
  });

  test('shows rejected proposal mutations instead of leaving an unhandled rejection', async () => {
    window.history.pushState({}, '', '/projects/project_alpha/reviews?proposal=proposal_alpha');
    installHubFetch([{ id: 'project_alpha', name: 'Alpha' }], { rejectedMutation: true });
    render(() => <HubApp platform={createWebPlatform()} />);

    await fireEvent.click(await screen.findByRole('button', { name: 'approved' }));
    expect(await screen.findByText('Proposal permission denied')).toBeInTheDocument();
    expect(screen.getByRole('heading', { name: 'Review Alpha' })).toBeInTheDocument();
  });

  test('attributes submitted comments to the effective Hub session principal', async () => {
    window.history.pushState({}, '', '/projects/project_alpha/reviews?proposal=proposal_alpha');
    const calls = installHubFetch([{ id: 'project_alpha', name: 'Alpha' }], { authenticated: true, rejectedMutation: true });
    render(() => <HubApp platform={createWebPlatform()} />);
    await screen.findByRole('button', { name: 'approved' });
    await waitFor(() => expect(document.querySelector('.avatar-link')).toHaveAttribute('title', 'user:signed-in'));
    await fireEvent.input(screen.getByLabelText('Body'), { target: { value: 'Review feedback' } });
    await fireEvent.submit(screen.getByRole('button', { name: 'Post comment' }).closest('form')!);
    await waitFor(() => expect(calls.some((call) => call.url === '/api/admin/review-comments' && call.init?.method === 'POST')).toBe(true));
    const create = calls.find((call) => call.url === '/api/admin/review-comments' && call.init?.method === 'POST');
    expect(JSON.parse(String(create?.init?.body))).toMatchObject({ authorPrincipal: { type: 'user', id: 'signed-in' } });
    expect(create?.init?.headers).toMatchObject({ 'x-sorrel-acting-principal': JSON.stringify({ type: 'user', id: 'signed-in' }) });
  });

  test('clears the selected review when history removes its proposal query', async () => {
    window.history.pushState({}, '', '/projects/project_alpha/reviews?proposal=proposal_alpha');
    installHubFetch([{ id: 'project_alpha', name: 'Alpha' }], { rejectedMutation: true });
    render(() => <HubApp platform={createWebPlatform()} />);
    expect(await screen.findByRole('button', { name: 'approved' })).toBeInTheDocument();

    window.history.replaceState({}, '', '/projects/project_alpha/reviews');
    window.dispatchEvent(new PopStateEvent('popstate'));
    await waitFor(() => expect(screen.queryByRole('button', { name: 'approved' })).not.toBeInTheDocument());
    expect(screen.getByText('Select a review')).toBeInTheDocument();
  });

  test('renders API health and the empty-project action from live responses', async () => {
    installHubFetch();
    render(() => <HubApp platform={createWebPlatform()} />);

    expect(await screen.findByText('No projects yet')).toBeInTheDocument();
    await waitFor(() => expect(document.querySelector('.api-indicator')).toHaveClass('ok'));
    expect(screen.getAllByRole('button', { name: 'Create project' })).toHaveLength(1);
    expect(screen.queryByRole('link', { name: 'Actions' })).not.toBeInTheDocument();
  });

  test('renders projects returned by Hub as project routes', async () => {
    installHubFetch([
      {
        id: 'project_alpha',
        name: 'Alpha',
        organizationId: 'org_local',
        description: 'First project',
        status: 'active',
      },
    ]);
    render(() => <HubApp platform={createWebPlatform()} />);

    const project = await screen.findByRole('option', { name: /Alpha/ });
    expect(project).toHaveAttribute('href', '/projects/project_alpha');
    expect(screen.getByText('First project')).toBeInTheDocument();
  });

  test('submits project creation with the selected development identity', async () => {
    const calls = installHubFetch();
    render(() => <HubApp platform={createWebPlatform()} />);

    await fireEvent.click((await screen.findAllByRole('button', { name: 'Create project' }))[0]);
    await fireEvent.input(screen.getByLabelText('Organization'), {
      target: { value: 'org_local' },
    });
    await fireEvent.input(screen.getByLabelText('Project name'), { target: { value: 'Platform' } });
    await fireEvent.submit(screen.getByRole('button', { name: 'Create and continue' }).closest('form')!);

    await waitFor(() => {
      expect(calls.some((call) => call.url === '/api/projects' && call.init?.method === 'POST')).toBe(true);
    });
    const create = calls.find(
      (call) => call.url === '/api/projects' && call.init?.method === 'POST',
    );
    expect(create?.init?.headers).toMatchObject({
      'x-sorrel-acting-principal': JSON.stringify({ type: 'user', id: 'local' }),
    });
    expect(JSON.parse(String(create?.init?.body))).toMatchObject({
      organizationId: 'org_local',
      name: 'Platform',
    });
  });

  test('renders the real repository tree and README on a project route', async () => {
    window.history.pushState({}, '', '/projects/project_alpha');
    installHubFetch([
      {
        id: 'project_alpha',
        name: 'Alpha',
        organizationId: 'acme',
        description: 'First project',
        status: 'active',
      },
    ]);
    render(() => <HubApp platform={createWebPlatform()} />);

    expect(await screen.findByText('Ship repository view')).toBeInTheDocument();
    expect(await screen.findByText('Repository-shaped work.')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /README.md/ })).toBeDisabled();
    expect(screen.getByRole('link', { name: /Work/ })).toHaveAttribute('href', '/projects/project_alpha/work');
  });
});
