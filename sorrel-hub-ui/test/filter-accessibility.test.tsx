import { fireEvent, render, screen, waitFor } from '@solidjs/testing-library';
import { afterEach, describe, expect, test, vi } from 'vitest';
import { HubApp } from '../src/App.tsx';
import { createWebPlatform } from '../src/platform.ts';

function installHubFetch() {
  const project = { id: 'project_alpha', name: 'Alpha', repositoryIds: ['repo_alpha', 'repo_beta'] };
  const proposals = [
    { id: 'proposal_open', projectId: project.id, title: 'Open fixture', status: 'open' },
    { id: 'proposal_draft', projectId: project.id, title: 'Draft fixture', status: 'draft' },
  ];
  vi.stubGlobal('fetch', vi.fn(async (input: RequestInfo | URL) => {
    const url = String(input);
    let payload: unknown = { data: [] };
    if (url === '/api/projects/project_alpha') payload = { data: project };
    else if (url.startsWith('/api/projects')) payload = { data: [project] };
    else if (url.startsWith('/api/admin/proposals')) payload = { data: proposals };
    else if (url === '/api/admin/sync-repos') payload = { repos: [{ id: 'repo_alpha' }, { id: 'repo_beta' }] };
    else if (url === '/api/capabilities') payload = { data: { auth: { mode: 'dev' }, convex: { enabled: false } } };
    else if (url === '/api/session') payload = { data: { auth: { mode: 'dev' }, session: null } };
    else if (url === '/api/healthz') payload = { status: 'ok' };
    return new Response(JSON.stringify(payload), { headers: { 'content-type': 'application/json' } });
  }));
}

afterEach(() => vi.unstubAllGlobals());

function mount(path: string) {
  history.replaceState(null, '', path);
  installHubFetch();
  render(() => <HubApp platform={createWebPlatform()} />);
}

describe('accessible Hub filters', () => {
  test('Inbox exposes the active filter and updates it with the displayed queue', async () => {
    mount('/inbox');
    const needs = await screen.findByRole('button', { name: /^Needs you/ });
    const all = screen.getByRole('button', { name: /^All current/ });
    expect(needs).toHaveAttribute('aria-pressed', 'true');
    expect(all).toHaveAttribute('aria-pressed', 'false');
    await waitFor(() => expect(screen.getByRole('button', { name: /Open fixture/ })).toBeInTheDocument());
    expect(screen.queryByRole('button', { name: /Draft fixture/ })).not.toBeInTheDocument();
    await fireEvent.click(all);
    expect(needs).toHaveAttribute('aria-pressed', 'false');
    expect(all).toHaveAttribute('aria-pressed', 'true');
    expect(screen.getByRole('button', { name: /Draft fixture/ })).toBeInTheDocument();
    await fireEvent.click(needs);
    expect(needs).toHaveAttribute('aria-pressed', 'true');
    expect(screen.queryByRole('button', { name: /Draft fixture/ })).not.toBeInTheDocument();
  });

  test('the Projects organization filter keeps its name after entering a value', async () => {
    mount('/');
    const filter = await screen.findByRole('searchbox', { name: 'Filter by organization' });
    await fireEvent.input(filter, { target: { value: 'fixture_org' } });
    expect(screen.getByRole('searchbox', { name: 'Filter by organization' })).toHaveValue('fixture_org');
  });

  test('the Reviews search has a stable name and still filters proposals', async () => {
    mount('/projects/project_alpha/reviews');
    const filter = await screen.findByRole('searchbox', { name: 'Find a review' });
    await screen.findByRole('option', { name: /Draft fixture/ });
    await fireEvent.input(filter, { target: { value: 'Open fixture' } });
    expect(screen.getByRole('searchbox', { name: 'Find a review' })).toHaveValue('Open fixture');
    expect(screen.getByRole('option', { name: /Open fixture/ })).toBeInTheDocument();
    expect(screen.queryByRole('option', { name: /Draft fixture/ })).not.toBeInTheDocument();
  });

  test('the Sync search has a stable name and still filters repositories', async () => {
    mount('/projects/project_alpha/sync');
    const filter = await screen.findByRole('searchbox', { name: 'Find a repository' });
    await screen.findByRole('row', { name: 'repo_beta 0' });
    await fireEvent.input(filter, { target: { value: 'repo_alpha' } });
    expect(screen.getByRole('searchbox', { name: 'Find a repository' })).toHaveValue('repo_alpha');
    expect(screen.getByRole('row', { name: 'repo_alpha 0' })).toBeInTheDocument();
    expect(screen.queryByRole('row', { name: 'repo_beta 0' })).not.toBeInTheDocument();
  });
});
