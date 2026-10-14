import { render, screen, waitFor } from '@solidjs/testing-library';
import { afterEach, expect, test, vi } from 'vitest';
import { HubApp } from '../src/App.tsx';
import { configureApiClient } from '../src/api.ts';
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
