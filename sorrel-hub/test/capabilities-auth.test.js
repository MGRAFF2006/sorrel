import assert from 'node:assert/strict';
import http from 'node:http';
import { test } from 'node:test';

import { createApp } from '../src/app.js';
import {
  createAuthAdapterFromEnv,
  createDevActingPrincipalAdapter,
  createOidcAdapter,
  createWorkOsAdapter,
} from '../src/auth/adapter.js';
import { evaluateBindSafety, isLoopbackHost } from '../src/bind-safety.js';
import { resolveCapabilities } from '../src/capabilities.js';

function listen(app) {
  const server = http.createServer(app.handleRequest);
  return new Promise((resolve) => {
    server.listen(0, '127.0.0.1', () => {
      const address = server.address();
      const port = typeof address === 'object' && address ? address.port : 0;
      resolve({
        server,
        url: `http://127.0.0.1:${port}`,
      });
    });
  });
}

test('GET /capabilities advertises modular install defaults', async () => {
  const app = createApp({
    env: {
      SORREL_HUB_AUTH: 'dev',
      SORREL_HUB_MODULE_ACTIONS: '0',
    },
  });
  const { server, url } = await listen(app);
  try {
    const response = await fetch(`${url}/capabilities`);
    assert.equal(response.status, 200);
    const body = await response.json();
    assert.equal(body.data.modules.core, true);
    assert.equal(body.data.modules.actions, false);
    assert.equal(body.data.auth.mode, 'dev');
    assert.equal(body.data.deploy, 'dev');
    assert.equal(body.data.convex.enabled, false);
  } finally {
    server.close();
  }
});

test('capabilities do not advertise unavailable modules or storage backends', () => {
  const caps = resolveCapabilities({
    env: {
      SORREL_HUB_AUTH: 'oidc',
      SORREL_HUB_MODULE_ACTIONS: '1',
      SORREL_HUB_MODULE_AGENTS: 'true',
      SORREL_HUB_MODULE_SECRETS: '1',
      SORREL_HUB_OBJECT_STORAGE: 's3',
      SORREL_HUB_SYNC_STORE: 'memory',
      CONVEX_URL: 'http://127.0.0.1:3210',
    },
  });
  assert.equal(caps.modules.actions, false);
  assert.equal(caps.modules.agents, false);
  assert.equal(caps.modules.secrets, false);
  assert.equal(caps.modules.objectStorage, 'memory');
  assert.equal(caps.auth.mode, 'oidc');
  assert.equal(caps.deploy, 'selfhost');
  assert.equal(caps.convex.enabled, true);
  assert.equal(caps.convex.url, 'http://127.0.0.1:3210');
});

test('capabilities prefer the browser-reachable Convex URL', () => {
  const caps = resolveCapabilities({
    env: {
      CONVEX_URL: 'http://convex-backend:3210',
      CONVEX_PUBLIC_URL: 'http://127.0.0.1:3210',
    },
  });
  assert.equal(caps.convex.enabled, true);
  assert.equal(caps.convex.url, 'http://127.0.0.1:3210');
});

test('AuthAdapter factory selects WorkOS / OIDC / dev', () => {
  assert.equal(createAuthAdapterFromEnv({ SORREL_HUB_AUTH: 'dev' }).mode, 'dev');
  assert.equal(createAuthAdapterFromEnv({ SORREL_HUB_AUTH: 'workos' }).mode, 'workos');
  assert.equal(createAuthAdapterFromEnv({ SORREL_HUB_AUTH: 'oidc' }).mode, 'oidc');
  assert.equal(createDevActingPrincipalAdapter().mode, 'dev');
  assert.equal(createWorkOsAdapter({}).mode, 'workos');
  assert.equal(createOidcAdapter({ issuer: 'https://idp.example' }).mode, 'oidc');
});

test('dev AuthAdapter maps acting-principal header to session', async () => {
  const adapter = createDevActingPrincipalAdapter();
  const session = await adapter.resolveSession({
    headers: {
      'x-sorrel-acting-principal': JSON.stringify({ type: 'user', id: 'local' }),
    },
  });
  assert.deepEqual(session?.principal, { type: 'user', id: 'local' });
  assert.equal(session?.authMode, 'dev');
});

test('bind safety allows loopback with dev auth', () => {
  assert.equal(isLoopbackHost('127.0.0.1'), true);
  assert.equal(isLoopbackHost('0.0.0.0'), false);
  const ok = evaluateBindSafety({
    host: '127.0.0.1',
    authMode: 'dev',
    bootstrapGrantsEnabled: true,
    env: {},
  });
  assert.equal(ok.ok, true);
});

test('bind safety refuses non-loopback dev auth without override', () => {
  const denied = evaluateBindSafety({
    host: '0.0.0.0',
    authMode: 'dev',
    env: {},
  });
  assert.equal(denied.ok, false);
  assert.match(denied.message, /auth=dev/);

  const allowed = evaluateBindSafety({
    host: '0.0.0.0',
    authMode: 'dev',
    env: { SORREL_HUB_ALLOW_INSECURE_DEV_AUTH: '1' },
  });
  assert.equal(allowed.ok, true);
});

test('bind safety refuses bootstrap grants on non-loopback without override', () => {
  const denied = evaluateBindSafety({
    host: '0.0.0.0',
    authMode: 'oidc',
    bootstrapGrantsEnabled: true,
    env: {},
  });
  assert.equal(denied.ok, false);
  assert.match(denied.message, /BOOTSTRAP_GRANTS/);
});

test('GET /session returns null without credentials and principal with header', async () => {
  const app = createApp({ env: { SORREL_HUB_AUTH: 'dev' } });
  const { server, url } = await listen(app);
  try {
    const anonymous = await fetch(`${url}/session`);
    assert.equal(anonymous.status, 200);
    const anonBody = await anonymous.json();
    assert.equal(anonBody.data.auth.mode, 'dev');
    assert.equal(anonBody.data.session, null);

    const authed = await fetch(`${url}/session`, {
      headers: {
        'x-sorrel-acting-principal': JSON.stringify({ type: 'user', id: 'local' }),
      },
    });
    assert.equal(authed.status, 200);
    const body = await authed.json();
    assert.deepEqual(body.data.session.principal, { type: 'user', id: 'local' });
    assert.equal(body.data.session.authMode, 'dev');
  } finally {
    server.close();
  }
});

test('resolveActingPrincipal prefers AuthAdapter session over header', async () => {
  const { resolveActingPrincipal } = await import('../src/policy-guard.js');
  const principal = resolveActingPrincipal(
    {
      headers: {
        'x-sorrel-acting-principal': JSON.stringify({ type: 'user', id: 'header' }),
      },
    },
    { session: { principal: { type: 'user', id: 'session' } } },
  );
  assert.deepEqual(principal, { type: 'user', id: 'session' });
});
