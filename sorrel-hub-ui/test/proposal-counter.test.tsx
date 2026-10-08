import { render, screen, waitFor } from '@solidjs/testing-library';
import { afterEach, expect, test, vi } from 'vitest';
import { createSignal } from 'solid-js';
import { configureApiClient } from '../src/api.ts';
import { useOpenProposalsCountFromHub } from '../src/convex/openProposals.ts';
import { HubApp } from '../src/App.tsx';
import { createWebPlatform } from '../src/platform.ts';

afterEach(() => { configureApiClient(); vi.unstubAllGlobals(); });

test('counter uses authenticated host transport, hides denied results and clears disabled state', async () => {
  let denied = false;
  const transport = vi.fn(async (_input, init) => {
    expect(init.headers.authorization).toBe('Bearer host-session');
    return new Response(JSON.stringify(denied ? { error: { message: 'Denied' } } : { data: [{ id: 'visible' }] }), { status: denied ? 403 : 200 });
  });
  configureApiClient({ baseUrl: 'https://hub.example', fetch: (input, init) => transport(input, { ...init, headers: { ...init?.headers, authorization: 'Bearer host-session' } }) });
  const [enabled, setEnabled] = createSignal(true);
  render(() => {
    const count = useOpenProposalsCountFromHub(enabled, 20);
    return <output data-testid="count">{count() ?? 'unknown'}</output>;
  });
  await waitFor(() => expect(screen.getByTestId('count')).toHaveTextContent('1'));
  expect(transport.mock.calls[0][0]).toBe('https://hub.example/admin/proposals?status=open');
  denied = true;
  await waitFor(() => expect(screen.getByTestId('count')).toHaveTextContent('unknown'));
  setEnabled(false);
  const calls = transport.mock.calls.length;
  await new Promise(resolve => setTimeout(resolve, 60));
  expect(transport).toHaveBeenCalledTimes(calls);
});

test('configured Convex never changes product counter transport', async () => {
  const calls: string[] = [];
  vi.stubGlobal('fetch', vi.fn(async (input) => {
    const url = String(input); calls.push(url);
    const data = url === '/api/capabilities' ? { modules: { core: true }, auth: { mode: 'oidc' }, convex: { enabled: true, url: 'https://private-backend.convex.cloud' } }
      : url === '/api/admin/proposals?status=open' ? [{ id: 'visible' }]
      : url === '/api/session' ? { auth: { mode: 'oidc' }, session: null } : [];
    return new Response(JSON.stringify({ data }));
  }));
  render(() => <HubApp platform={createWebPlatform()} />);
  await waitFor(() => expect(document.querySelector('.nav-count')).toHaveTextContent('1'));
  expect(calls).toContain('/api/admin/proposals?status=open');
  expect(calls.every(url => url.startsWith('/api/'))).toBe(true);
});
