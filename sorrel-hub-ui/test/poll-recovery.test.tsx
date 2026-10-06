import { render } from '@solidjs/testing-library';
import { afterEach, expect, test, vi } from 'vitest';

import { HubApp } from '../src/App.tsx';
import { configureApiClient, useApiConnection } from '../src/api.ts';
import { createWebPlatform } from '../src/platform.ts';

afterEach(() => {
  vi.useRealTimers();
  configureApiClient();
});

for (const failure of ['network', 'server'] as const) {
  test(`idle Hub polling recovers after a ${failure} failure`, async () => {
    vi.useFakeTimers();
    let offline = false;
    let polls = 0;
    configureApiClient({ fetch: async (input) => {
      const path = String(input);
      if (path === '/api/admin/proposals?status=open') {
        polls++;
        if (offline) {
          if (failure === 'network') throw new Error('synthetic network outage');
          return new Response(JSON.stringify({ error: { message: 'synthetic server outage' } }), { status: 503 });
        }
        return new Response(JSON.stringify({ data: [{ id: 'visible' }] }));
      }
      const data = path === '/api/capabilities'
        ? { modules: { core: true }, auth: { mode: 'dev' }, convex: { enabled: false } }
        : path === '/api/session' ? { auth: { mode: 'dev' }, session: null } : [];
      return new Response(JSON.stringify({ data }));
    } });
    render(() => <HubApp platform={createWebPlatform()} />);
    await vi.advanceTimersByTimeAsync(0);
    expect(document.querySelector('.nav-count')).toHaveTextContent('1');

    offline = true;
    await vi.advanceTimersByTimeAsync(5_000);
    expect(useApiConnection()()).toBe(false);
    expect(document.querySelector('.nav-count')).toBeNull();
    const beforeRecovery = polls;

    offline = false;
    await vi.advanceTimersByTimeAsync(5_000);
    expect(polls).toBeGreaterThan(beforeRecovery);
    expect(useApiConnection()()).toBe(true);
    expect(document.querySelector('.nav-count')).toHaveTextContent('1');
  });
}
