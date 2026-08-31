import { fireEvent, render, screen, waitFor } from '@solidjs/testing-library';
import { afterEach, describe, expect, test, vi } from 'vitest';
import { HubApp } from '../src/App.tsx';
import { createWebPlatform } from '../src/platform.ts';

type FetchCall = { url: string; init?: RequestInit };

function json(data: unknown, status = 200) {
  return new Response(JSON.stringify(data), {
    status,
    headers: { 'content-type': 'application/json' },
  });
}

function installHubFetch(projects: unknown[] = []) {
  const calls: FetchCall[] = [];
  const fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
    const url = String(input);
    calls.push({ url, init });

    if (url === '/api/capabilities') {
      return json({
        data: {
          modules: { core: true, actions: false, agents: true, secrets: true, objectStorage: 'fs' },
          auth: { mode: 'dev', session: 'none' },
          convex: { enabled: false },
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

afterEach(() => vi.unstubAllGlobals());

describe('HubApp rendered behavior', () => {
  test('renders API health and the empty-project action from live responses', async () => {
    installHubFetch();
    render(() => <HubApp platform={createWebPlatform()} />);

    expect(await screen.findByText('No projects yet')).toBeInTheDocument();
    expect(await screen.findByText('API: ok')).toBeInTheDocument();
    expect(screen.getAllByRole('button', { name: 'New project' })).toHaveLength(2);
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

    await fireEvent.click((await screen.findAllByRole('button', { name: 'New project' }))[0]);
    await fireEvent.input(screen.getByLabelText('Organization id'), {
      target: { value: 'org_local' },
    });
    await fireEvent.input(screen.getByLabelText('Name'), { target: { value: 'Platform' } });
    await fireEvent.submit(screen.getByRole('button', { name: 'Create project' }).closest('form')!);

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
});
