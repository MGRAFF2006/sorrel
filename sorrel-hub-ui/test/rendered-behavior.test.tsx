import { fireEvent, render, screen, waitFor } from '@solidjs/testing-library';
import { afterEach, describe, expect, test, vi } from 'vitest';
import { HubApp } from '../src/App.tsx';
import { createWebPlatform } from '../src/platform.ts';

const convexCalls = vi.hoisted(() => ({
  construct: vi.fn(),
  subscribe: vi.fn(),
  unsubscribe: vi.fn(),
  close: vi.fn(),
}));

vi.mock('convex/browser', () => ({
  ConvexClient: class {
    constructor(url: string) { convexCalls.construct(url); }
    onUpdate(query: unknown, args: unknown, callback: (value: unknown) => void) {
      convexCalls.subscribe(query, args);
      callback(7);
      return convexCalls.unsubscribe;
    }
    async close() { convexCalls.close(); }
  },
}));

type FetchCall = { url: string; init?: RequestInit };

function json(data: unknown, status = 200) {
  return new Response(JSON.stringify(data), {
    status,
    headers: { 'content-type': 'application/json' },
  });
}

function installHubFetch(
  projects: unknown[] = [],
  convex: { enabled: boolean; url?: string; publicCounter?: boolean } = { enabled: false },
) {
  const calls: FetchCall[] = [];
  const fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
    const url = String(input);
    calls.push({ url, init });

    if (url === '/api/capabilities') {
      return json({
        data: {
          modules: { core: true, actions: false, agents: true, secrets: true, objectStorage: 'fs' },
          auth: { mode: 'dev', session: 'none' },
          convex,
          deploy: 'dev',
        },
      });
    }
    if (url === '/api/session') {
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
    if (url === '/api/admin/proposals?projectId=project_alpha') return json({ data: [] });
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
  vi.clearAllMocks();
});

describe('HubApp rendered behavior', () => {
  test.each([false, undefined])('uses the Hub fallback when the public counter capability is %s', async (publicCounter) => {
    const calls = installHubFetch([], { enabled: true, url: 'https://internal.convex.cloud', publicCounter });
    render(() => <HubApp platform={createWebPlatform()} convexUrl="https://override.convex.cloud" />);

    expect(await screen.findByText('No projects yet')).toBeInTheDocument();
    await waitFor(() => expect(calls.some((call) => call.url === '/api/admin/proposals?status=open')).toBe(true));
    expect(convexCalls.construct).not.toHaveBeenCalled();
    expect(convexCalls.subscribe).not.toHaveBeenCalled();
  });

  test('subscribes only when Hub explicitly advertises a public counter', async () => {
    installHubFetch([], { enabled: true, url: 'https://public.convex.cloud', publicCounter: true });
    render(() => <HubApp platform={createWebPlatform()} />);

    await waitFor(() => expect(convexCalls.construct).toHaveBeenCalledWith('https://public.convex.cloud'));
    expect(convexCalls.subscribe).toHaveBeenCalledTimes(1);
    expect(document.querySelector('.nav-count')).toHaveTextContent('7');
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
