import { fireEvent, render, screen, waitFor } from '@solidjs/testing-library';
import { afterEach, describe, expect, test, vi } from 'vitest';
import { HubApp } from '../src/App.tsx';
import { setActingPrincipal, setAuthenticatedPrincipal } from '../src/session.ts';
import { createWebPlatform } from '../src/platform.ts';

type FetchCall = { url: string; init?: RequestInit };

function json(data: unknown, status = 200) {
  return new Response(JSON.stringify(data), {
    status,
    headers: { 'content-type': 'application/json' },
  });
}

function installHubFetch(projects: unknown[] = [], proposals: unknown[] = [], options: { rejectRefs?: boolean; rejectPatch?: boolean; authenticatedPrincipal?: { type: string; id: string } } = {}) {
  const calls: FetchCall[] = [];
  const fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
    const url = String(input);
    calls.push({ url, init });

    if (url === '/api/capabilities') {
      return json({
        data: {
          modules: { core: true, actions: false, agents: true, secrets: true, objectStorage: 'fs' },
          auth: { mode: options.authenticatedPrincipal ? 'oidc' : 'dev', session: options.authenticatedPrincipal ? 'bearer' : 'none' },
          convex: { enabled: false },
          collaboration: { proposalTransitions: { draft: ['open', 'closed'], open: ['approved', 'merged', 'closed'], approved: ['merged', 'open', 'closed'], closed: ['open'] } },
          deploy: 'dev',
        },
      });
    }
    if (url === '/api/session') {
      return json({ data: { auth: { mode: options.authenticatedPrincipal ? 'oidc' : 'dev', session: options.authenticatedPrincipal ? 'bearer' : 'none' }, session: options.authenticatedPrincipal ? {principal: options.authenticatedPrincipal} : null } });
    }
    if (url === '/api/healthz') return json({ status: 'ok' });
    if (url === '/api/admin/proposals?status=open') return json({ data: [] });
    if (url === '/api/projects' && (init?.method ?? 'GET') === 'GET') {
      return json({ data: projects });
    }
    if (url === '/api/projects/project_alpha/repositories' && init?.method === 'POST') {
      const project = projects.find(item => (item as {id?: string}).id === 'project_alpha') as { repositoryIds?: string[] };
      project.repositoryIds = [JSON.parse(String(init.body)).syncRepoId];
      return json({data: project});
    }
    if (url.startsWith('/api/projects/') && (init?.method ?? 'GET') === 'GET') {
      const id = decodeURIComponent(url.slice('/api/projects/'.length));
      const project = projects.find((item) => (item as { id?: string }).id === id);
      if (project) return json({ data: project });
    }
    if (url === '/api/admin/repositories?projectId=project_alpha') {
      return json({ data: [{ id: 'repo_alpha', name: 'alpha', owner: 'acme', provider: 'sorrel', defaultBranch: 'main' }] });
    }
    if (url === '/api/admin/proposals?projectId=project_alpha') return json({ data: proposals });
    if (url === '/api/admin/sync-repos') return json({ repos: [{ id: 'repo_alpha', refCount: 1 }, { id: 'repo_beta', refCount: 1 }] });
    if (url === '/api//refs') return json({error: {message: 'Repository required'}}, 404);
    if (url === '/api/repo_alpha/refs' && options.rejectRefs) { options.rejectRefs = false; return json({error: {message: 'Refs unavailable'}}, 503); }
    if (url === '/api/repo_alpha/refs') return json({ refs: [{ name: 'main', snapshot: 'a'.repeat(64) }, { name: 'HEAD', snapshot: 'c'.repeat(64) }] });
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
    if (url === '/api/admin/proposals' && init?.method === 'POST') {
      const proposal = {id: 'proposal_alpha', ...JSON.parse(String(init.body))};
      proposals.push(proposal);
      return json({data: proposal}, 201);
    }
    if (url === '/api/admin/proposals/proposal_alpha' && init?.method === 'PATCH') {
      if (options.rejectPatch) { options.rejectPatch = false; return json({error: {message: 'Policy denied'}}, 403); }
      Object.assign(proposals[0] as object, JSON.parse(String(init.body)));
      return json({data: proposals[0]});
    }
    if (url === '/api/admin/proposals/proposal_alpha/changes') return json({data: {
      repoId: 'repo_alpha', sourceSnapshot: 'c'.repeat(64), targetSnapshot: 'a'.repeat(64),
      changes: [{path: 'src/main.rs', status: 'modified', before: {content: 'old source'}, after: {content: 'new source'}}],
    }});
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

afterEach(() => { vi.unstubAllGlobals(); setAuthenticatedPrincipal(null); setActingPrincipal({type: 'user', id: 'local'}); });

describe('HubApp rendered behavior', () => {
  test('renders API health and the empty-project action from live responses', async () => {
    installHubFetch();
    render(() => <HubApp platform={createWebPlatform()} />);

    expect(await screen.findByText('No projects yet')).toBeInTheDocument();
    await waitFor(() => expect(document.querySelector('.api-indicator')).toHaveClass('ok'));
    expect(screen.getAllByRole('button', { name: 'Create project' })).toHaveLength(1);
    expect(screen.queryByRole('link', { name: 'Actions' })).not.toBeInTheDocument();
  });

  test('closes the native project dialog on Escape without submitting', async () => {
    const calls = installHubFetch();
    render(() => <HubApp platform={createWebPlatform()} />);
    await fireEvent.click((await screen.findAllByRole('button', { name: 'Create project' }))[0]);
    const dialog = screen.getByRole('dialog');
    expect(dialog).toHaveAttribute('open');
    await fireEvent(dialog, new Event('cancel', { cancelable: true }));
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
    expect(calls.some(call => call.init?.method === 'POST')).toBe(false);
  });

  test('shows authenticated identity without replacing the saved development preset', async () => {
    setActingPrincipal({type: 'user', id: 'reviewer'});
    window.history.pushState({}, '', '/profile');
    installHubFetch([], [], {authenticatedPrincipal: {type: 'user', id: 'oidc:alice'}});
    render(() => <HubApp platform={createWebPlatform()} />);
    expect(await screen.findByRole('heading', {level: 1, name: 'oidc:alice'})).toBeInTheDocument();
    expect(JSON.parse(localStorage.getItem('sorrel.hub.actingPrincipal')!)).toEqual({type: 'user', id: 'reviewer'});
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
    await fireEvent.change(screen.getByLabelText('Organization'), {
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

  test('shows project-specific connection commands and links an existing pushed repository', async () => {
    window.history.pushState({}, '', '/projects/project_alpha/sync');
    const calls = installHubFetch([{ id: 'project_alpha', name: 'Alpha', organizationId: 'org_local' }]);
    render(() => <HubApp platform={createWebPlatform()} />);
    expect(await screen.findByText(/sorrel remote add origin/)).toHaveTextContent("sorrel lane submit --project-id 'project_alpha'");
    const button = await screen.findByRole('button', {name: 'Connect repository'});
    await fireEvent.click(button);
    await waitFor(() => expect(calls.some(call => call.url === '/api/projects/project_alpha/repositories' && call.init?.method === 'POST')).toBe(true));
    const request = calls.find(call => call.url === '/api/projects/project_alpha/repositories' && call.init?.method === 'POST');
    expect(JSON.parse(String(request?.init?.body))).toEqual({syncRepoId: 'repo_beta'});
    await waitFor(() => expect(document.querySelector('[data-repo-id="repo_beta"]')).toBeInTheDocument());
  });

  test('opens reviews without an empty repository request and preserves drafts after failed refs', async () => {
    window.history.pushState({}, '', '/projects/project_alpha/reviews');
    const calls = installHubFetch([{id: 'project_alpha', name: 'Alpha', repositoryIds: ['repo_alpha']}], [], {rejectRefs: true});
    render(() => <HubApp platform={createWebPlatform()} />);
    await fireEvent.click(await screen.findByRole('button', {name: 'Open review'}));
    expect(screen.getByRole('dialog')).toBeInTheDocument();
    expect(calls.some(call => call.url === '/api//refs')).toBe(false);
    await fireEvent.input(screen.getByLabelText('Title'), {target: {value: 'Keep this draft'}});
    await waitFor(() => expect(screen.getByLabelText(/Repository/).querySelector('option[value="repo_alpha"]')).toBeInTheDocument());
    await fireEvent.change(screen.getByLabelText(/Repository/), {target: {value: 'repo_alpha'}});
    expect(await screen.findByText(/Repository refs could not be loaded/)).toBeInTheDocument();
    expect(screen.getByRole('dialog')).toBeInTheDocument();
    expect(screen.getByLabelText('Title')).toHaveValue('Keep this draft');
    expect(screen.getByLabelText(/Source lane/)).toBeDisabled();
    await fireEvent.click(screen.getByRole('button', {name: 'Retry refs'}));
    await waitFor(() => expect(screen.getByLabelText(/Source lane/).querySelector('option[value="HEAD"]')).toBeInTheDocument());
    expect(screen.getByLabelText(/Source lane/)).toBeEnabled();
    expect(screen.getByLabelText('Title')).toHaveValue('Keep this draft');
    expect(calls.filter(call => call.url === '/api/repo_alpha/refs')).toHaveLength(2);
    expect(calls.some(call => call.url === '/api//refs')).toBe(false);
  });

  test('captures selected source and target snapshots when creating a review', async () => {
    window.history.pushState({}, '', '/projects/project_alpha/reviews');
    const calls = installHubFetch([{id: 'project_alpha', name: 'Alpha', repositoryIds: ['repo_alpha']}]);
    render(() => <HubApp platform={createWebPlatform()} />);
    await fireEvent.click(await screen.findByRole('button', {name: 'Open review'}));
    await fireEvent.input(screen.getByLabelText('Title'), {target: {value: 'Inspect work'}});
    await waitFor(() => expect(screen.getByLabelText(/Repository/).querySelector('option[value="repo_alpha"]')).toBeInTheDocument());
    await fireEvent.change(screen.getByLabelText(/Repository/), {target: {value: 'repo_alpha'}});
    await waitFor(() => expect(screen.getByLabelText(/Source lane/).querySelector('option[value="HEAD"]')).toBeInTheDocument());
    await fireEvent.change(screen.getByLabelText(/Source lane/), {target: {value: 'HEAD'}});
    await fireEvent.change(screen.getByLabelText('Compare against'), {target: {value: 'main'}});
    await fireEvent.submit(screen.getByLabelText('Title').closest('form')!);
    await waitFor(() => expect(calls.some(call => call.url === '/api/admin/proposals' && call.init?.method === 'POST')).toBe(true));
    const request = calls.find(call => call.url === '/api/admin/proposals' && call.init?.method === 'POST');
    expect(JSON.parse(String(request?.init?.body))).toMatchObject({syncRepoId: 'repo_alpha', sourceSnapshot: 'c'.repeat(64), targetSnapshot: 'a'.repeat(64)});
  });

  test('renders recorded snapshot changes and recovers a denied status mutation', async () => {
    window.history.pushState({}, '', '/projects/project_alpha/reviews?proposal=proposal_alpha');
    const proposals = [{id: 'proposal_alpha', title: 'Review source', status: 'open'}];
    const calls = installHubFetch([{id: 'project_alpha', name: 'Alpha'}], proposals, {rejectPatch: true});
    render(() => <HubApp platform={createWebPlatform()} />);
    expect(await screen.findByLabelText('Before src/main.rs')).toHaveTextContent('old source');
    expect(await screen.findByLabelText('After src/main.rs')).toHaveTextContent('new source');
    expect(screen.getByRole('button', {name: 'Mark as merged'})).toBeInTheDocument();
    await fireEvent.click(screen.getByRole('button', {name: 'approved'}));
    expect(await screen.findByText('Policy denied')).toBeInTheDocument();
    await fireEvent.click(screen.getByRole('button', {name: 'Retry change'}));
    await waitFor(() => expect(proposals[0].status).toBe('approved'));
    expect(calls.filter(call => call.init?.method === 'PATCH')).toHaveLength(2);
    expect(await screen.findByRole('button', {name: 'open'})).toBeInTheDocument();
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
    await fireEvent.click(screen.getByRole('button', { name: /README.md/ }));
    expect(await screen.findByLabelText('README.md')).toHaveTextContent('Repository-shaped work.');
    expect(screen.getByRole('link', { name: /Work/ })).toHaveAttribute('href', '/projects/project_alpha/work');
  });
});
