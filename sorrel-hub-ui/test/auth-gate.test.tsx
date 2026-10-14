import { render, screen, waitFor } from '@solidjs/testing-library';
import { afterEach, expect, test, vi } from 'vitest';
import { HubApp } from '../src/App.tsx';
import { apiGet, configureApiClient } from '../src/api.ts';
import { createWebPlatform } from '../src/platform.ts';

afterEach(() => { configureApiClient(); vi.unstubAllGlobals(); });

for (const unavailable of [false, true]) {
  test(unavailable ? 'unavailable discovery never falls back to development identity' : 'signed-out hosted users see login before any private data request', async () => {
    const calls: string[] = [];
    vi.stubGlobal('fetch', vi.fn(async (input) => {
      const url = String(input); calls.push(url);
      if (unavailable) throw new Error('offline');
      const data = url === '/api/capabilities'
        ? { auth: { mode: 'workos', browserLogin: true }, modules: { core: true }, convex: { enabled: false } }
        : { auth: { mode: 'workos', browserLogin: true }, session: null };
      return new Response(JSON.stringify({ data }));
    }));
    render(() => <HubApp platform={createWebPlatform()} />);
    expect(await screen.findByRole('heading', { name: unavailable ? 'Hub could not be reached' : 'Sign in to Sorrel Hub' })).toBeInTheDocument();
    if (!unavailable) expect(screen.getByRole('link', { name: 'Sign in' })).toHaveAttribute('href', 'http://localhost:3000/api/auth/login');
    expect(screen.queryByRole('combobox', { name: 'Acting principal' })).not.toBeInTheDocument();
    await waitFor(() => expect(calls.some(url => url.includes('/projects') || url.includes('/admin/'))).toBe(false));
  });
}

test('authenticated SaaS users have no project creator and return to login after expiry', async () => {
  let expired = false;
  vi.stubGlobal('fetch', vi.fn(async input => {
    const url = String(input);
    if (expired && url === '/api/projects') return new Response(JSON.stringify({ error: { message: 'Sign in again' } }), { status: 401 });
    const data = url === '/api/capabilities'
      ? { deploy: 'saas', auth: { mode: 'workos', browserLogin: true }, modules: { core: true }, convex: { enabled: false } }
      : url === '/api/session' ? { auth: { mode: 'workos' }, session: { principal: { type: 'user', id: 'workos:user_fixture' } } }
      : [];
    return new Response(JSON.stringify({ data }));
  }));
  render(() => <HubApp platform={createWebPlatform()} />);
  expect(await screen.findByText(/Ask your team administrator to share a project with workos:user_fixture/)).toBeInTheDocument();
  expect(screen.queryByRole('button', { name: /Create.*project/i })).not.toBeInTheDocument();
  expect(screen.queryByRole('combobox', { name: 'Acting principal' })).not.toBeInTheDocument();
  expired = true;
  await expect(apiGet('/projects')).rejects.toThrow('Sign in again');
  expect(await screen.findByRole('heading', { name: 'Sign in to Sorrel Hub' })).toBeInTheDocument();
});
