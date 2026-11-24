import { fireEvent, render, screen, waitFor } from '@solidjs/testing-library';
import { afterEach, expect, test, vi } from 'vitest';
import { HubApp } from '../src/App.tsx';
import { configureApiClient } from '../src/api.ts';
import { createWebPlatform } from '../src/platform.ts';
import { formatUserCode } from '../src/views/CliActivateView.tsx';

type Call = { url: string; method: string; body?: unknown };
const principal = { type: 'user', id: 'workos:user_fixture' };

function installHub(options: { signedIn: boolean; tokens?: unknown[]; approve?: (body: Record<string, unknown>) => Response }) {
  const calls: Call[] = [];
  let tokens = options.tokens ?? [];
  vi.stubGlobal('fetch', vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
    const url = String(input);
    const method = init?.method ?? 'GET';
    const body = init?.body ? JSON.parse(String(init.body)) : undefined;
    calls.push({ url, method, body });
    const json = (data: unknown, status = 200) => new Response(JSON.stringify(data), { status });
    if (url === '/api/capabilities') return json({ data: { deploy: 'saas', auth: { mode: 'workos', browserLogin: true, cliLogin: 'device' }, modules: { core: true }, convex: { enabled: false } } });
    if (url === '/api/session') return json({ data: { auth: { mode: 'workos', cliLogin: 'device' }, session: options.signedIn ? { principal } : null } });
    if (url === '/api/auth/cli/approve') return options.approve?.(body) ?? json({ data: { status: body.decision === 'approve' ? 'approved' : 'denied', label: 'work laptop' } });
    if (url === '/api/auth/cli/tokens') return json({ data: tokens });
    if (url.startsWith('/api/auth/cli/tokens/') && method === 'DELETE') {
      const id = decodeURIComponent(url.slice('/api/auth/cli/tokens/'.length));
      tokens = tokens.map(token => (token as { id: string }).id === id ? { ...(token as object), status: 'revoked' } : token);
      return json({ data: {} });
    }
    return json({ data: [] });
  }));
  return calls;
}

afterEach(() => { configureApiClient(); vi.unstubAllGlobals(); window.history.pushState({}, '', '/'); });

test('user codes are formatted as typed', () => {
  expect(formatUserCode('bcdf ghjk')).toBe('BCDF-GHJK');
  expect(formatUserCode('bcd')).toBe('BCD');
  expect(formatUserCode('BCDF-GHJK-EXTRA')).toBe('BCDF-GHJK');
});

test('signed-out activation links sign-in back to the pending code', async () => {
  installHub({ signedIn: false });
  window.history.pushState({}, '', '/cli/activate?code=BCDF-GHJK');
  render(() => <HubApp platform={createWebPlatform()} />);
  expect(await screen.findByRole('link', { name: 'Sign in' })).toHaveAttribute('href',
    `http://localhost:3000/api/auth/login?returnTo=${encodeURIComponent('/cli/activate?code=BCDF-GHJK')}`);
});

test('approving sends only the code and decision and confirms the result', async () => {
  const calls = installHub({ signedIn: true });
  window.history.pushState({}, '', '/cli/activate?code=bcdf-ghjk');
  render(() => <HubApp platform={createWebPlatform()} />);
  expect(await screen.findByRole('heading', { name: 'Approve CLI sign-in' })).toBeInTheDocument();
  expect(screen.getByText(/workos:user_fixture/)).toBeInTheDocument();
  const input = screen.getByLabelText('Code from your terminal') as HTMLInputElement;
  expect(input.value).toBe('BCDF-GHJK');
  fireEvent.click(screen.getByRole('button', { name: 'Approve' }));
  expect(await screen.findByText(/Approved “work laptop”/)).toBeInTheDocument();
  const approve = calls.filter(call => call.url === '/api/auth/cli/approve');
  expect(approve).toEqual([{ url: '/api/auth/cli/approve', method: 'POST', body: { user_code: 'BCDF-GHJK', decision: 'approve' } }]);
});

test('denying and server errors are shown to the user', async () => {
  let fail = true;
  const calls = installHub({ signedIn: true, approve: body => fail
    ? new Response(JSON.stringify({ error: { message: 'This code is invalid, expired or already used; run sorrel login again' } }), { status: 404 })
    : new Response(JSON.stringify({ data: { status: body.decision === 'deny' ? 'denied' : 'approved', label: 'x' } })) });
  window.history.pushState({}, '', '/cli/activate');
  render(() => <HubApp platform={createWebPlatform()} />);
  const input = await screen.findByLabelText('Code from your terminal');
  expect(screen.getByRole('button', { name: 'Approve' })).toBeDisabled();
  fireEvent.input(input, { target: { value: 'bcdfghjk' } });
  fireEvent.click(screen.getByRole('button', { name: 'Deny' }));
  expect(await screen.findByRole('alert')).toHaveTextContent('already used');
  fail = false;
  fireEvent.click(screen.getByRole('button', { name: 'Deny' }));
  expect(await screen.findByText(/No token was issued/)).toBeInTheDocument();
  expect(calls.filter(call => call.url === '/api/auth/cli/approve').map(call => (call.body as { decision: string }).decision)).toEqual(['deny', 'deny']);
});

test('profile lists CLI tokens and revokes an active one', async () => {
  const calls = installHub({ signedIn: true, tokens: [
    { id: 'abc123', label: 'work laptop', status: 'active', createdAt: '2026-11-24T10:00:00.000Z', expiresAt: '2026-12-24T10:00:00.000Z', lastUsedAt: null },
    { id: 'def456', label: 'old box', status: 'revoked', createdAt: '2026-10-01T10:00:00.000Z', expiresAt: '2026-10-31T10:00:00.000Z', lastUsedAt: '2026-10-02T10:00:00.000Z' },
  ] });
  window.history.pushState({}, '', '/profile');
  render(() => <HubApp platform={createWebPlatform()} />);
  expect(await screen.findByText('work laptop')).toBeInTheDocument();
  expect(screen.getByText(/expires 2026-12-24/)).toBeInTheDocument();
  expect(screen.queryByRole('button', { name: 'Revoke old box' })).not.toBeInTheDocument();
  fireEvent.click(screen.getByRole('button', { name: 'Revoke work laptop' }));
  await waitFor(() => expect(screen.queryByRole('button', { name: 'Revoke work laptop' })).not.toBeInTheDocument());
  expect(calls.some(call => call.method === 'DELETE' && call.url === '/api/auth/cli/tokens/abc123')).toBe(true);
});
