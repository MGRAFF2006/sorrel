import assert from 'node:assert/strict';
import { generateKeyPairSync, sign } from 'node:crypto';
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
import { objectId } from '../src/blake3.js';
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
    assert.equal(body.data.convex.publicCounter, false);
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
  assert.throws(() => createAuthAdapterFromEnv({ SORREL_HUB_AUTH: 'odic' }), /unsupported/);
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

test('acting-principal fallback requires an explicit development adapter', async () => {
  const { resolveActingPrincipal } = await import('../src/policy-guard.js');
  const principal = { type: 'user', id: 'header' };
  const request = { headers: { 'x-sorrel-acting-principal': JSON.stringify(principal) } };
  assert.deepEqual(resolveActingPrincipal(request, { authAdapter: { mode: 'dev' } }), principal);
  for (const mode of ['oidc', 'workos', undefined]) {
    assert.throws(
      () => resolveActingPrincipal(request, { authAdapter: mode ? { mode } : undefined }),
      { statusCode: 403, code: 'policy_denied' },
    );
  }
});

test('OIDC and WorkOS cannot impersonate privileged actors through development headers', async () => {
  const principal = { type: 'user', id: 'victim' };
  const bytes = Buffer.from('an object that must remain unwritten');
  const id = objectId(bytes);
  const trustedGrantsById = {
    object: { id: 'object', principal, action: 'repo.object.write', resource: { kind: 'repo', id: 'repo_private' } },
    ref: { id: 'ref', principal, action: 'repo.ref.write', resource: { kind: 'repo', id: 'repo_private' } },
    admin: { id: 'admin', principal, action: 'policy.grant', resource: { kind: 'org', id: 'org_private' } },
  };
  const routes = [
    ['/repo_private/objects', { grantRefs: [{ id: 'object' }], objects: [{ id, bytes: bytes.toString('base64') }] }],
    ['/repo_private/refs/main', { grantRefs: [{ id: 'ref' }], snapshot: id }],
    ['/admin/repositories', { organizationId: 'org_private', projectId: 'project_private', provider: 'sorrel', owner: 'victim', name: 'private', grantRefs: [{ id: 'admin' }] }],
  ];

  for (const authAdapter of [
    createOidcAdapter({ issuer: 'https://idp.example', fetchJwks: async () => [] }),
    createWorkOsAdapter({ apiKey: 'test', clientId: 'test' }),
  ]) {
    const app = createApp({ authAdapter, trustedGrantsById });
    app.store.sync.put('repo_private', bytes, id);
    const { server, url } = await listen(app);
    try {
      for (const authorization of [undefined, 'Bearer invalid.jwt']) {
        for (const [path, body] of routes) {
          const response = await fetch(`${url}${path}`, {
            method: 'POST',
            headers: {
              'content-type': 'application/json',
              'x-sorrel-acting-principal': JSON.stringify(principal),
              ...(authorization ? { authorization } : {}),
            },
            body: JSON.stringify(body),
          });
          assert.equal(response.status, 403, `${authAdapter.mode} ${path}`);
          assert.equal((await response.json()).error.code, 'policy_denied');
        }
      }
      assert.equal(app.store.sync.getRef('repo_private', 'main'), undefined);
      assert.deepEqual(app.store.listRepositories(), []);
    } finally {
      await new Promise((resolve) => server.close(resolve));
    }
  }
});

test('a verified OIDC session authorizes its own grant despite a forged header', async () => {
  const issuer = 'https://idp.example';
  const { privateKey, publicKey } = generateKeyPairSync('rsa', { modulusLength: 2048 });
  const signingInput = [
    { alg: 'RS256' },
    { iss: issuer, aud: 'hub', sub: 'maintainer', exp: Math.floor(Date.now() / 1000) + 600 },
  ].map((value) => Buffer.from(JSON.stringify(value)).toString('base64url')).join('.');
  const token = `${signingInput}.${sign('RSA-SHA256', Buffer.from(signingInput), privateKey).toString('base64url')}`;
  const principal = { type: 'user', id: 'oidc:maintainer' };
  const app = createApp({
    authAdapter: createOidcAdapter({ issuer, audience: 'hub', fetchJwks: async () => [publicKey.export({ format: 'jwk' })] }),
    trustedGrantsById: {
      object: { id: 'object', principal, action: 'repo.object.write', resource: { kind: 'repo', id: 'repo_private' } },
    },
  });
  const bytes = Buffer.from('authorized object');
  const id = objectId(bytes);
  const { server, url } = await listen(app);
  try {
    const response = await fetch(`${url}/repo_private/objects`, {
      method: 'POST',
      headers: {
        'content-type': 'application/json',
        authorization: `Bearer ${token}`,
        'x-sorrel-acting-principal': JSON.stringify({ type: 'user', id: 'forged' }),
      },
      body: JSON.stringify({ grantRefs: [{ id: 'object' }], objects: [{ id, bytes: bytes.toString('base64') }] }),
    });
    assert.equal(response.status, 200);
    assert.deepEqual((await response.json()).stored, [id]);
    assert.deepEqual(app.store.sync.get('repo_private', id), bytes);
  } finally {
    await new Promise((resolve) => server.close(resolve));
  }
});
