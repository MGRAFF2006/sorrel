import assert from 'node:assert/strict';
import { createHash, generateKeyPairSync, sign } from 'node:crypto';
import http from 'node:http';
import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { WorkOS } from '@workos-inc/node';
import { createApp } from '../src/app.js';
import { createWorkOsBrowserAuth } from '../src/auth/workos-browser.js';
import { createHubWebServer } from '../../sorrel-hub-web/server/hub-web-server.mjs';

const env = { SORREL_HUB_AUTH: 'workos', SORREL_HUB_PUBLIC_URL: 'https://hub.example.com',
  WORKOS_API_KEY: 'sk_test_fixture', WORKOS_CLIENT_ID: 'client_fixture', WORKOS_COOKIE_PASSWORD: 'fixture-cookie-password-of-at-least-32-chars' };

async function listen(t, server) {
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  t.after(() => new Promise(resolve => { server.close(resolve); server.closeAllConnections(); }));
  return `http://127.0.0.1:${server.address().port}`;
}

test('hosted login fails closed on partial or insecure configuration', () => {
  assert.equal(createWorkOsBrowserAuth({ SORREL_HUB_AUTH: 'dev' }), null);
  assert.throws(() => createWorkOsBrowserAuth({ ...env, WORKOS_COOKIE_PASSWORD: 'short' }), /32 characters/);
  for (const url of ['http://hub.example.com', 'https://hub.example.com/path', 'https://user:password@hub.example.com']) {
    assert.throws(() => createWorkOsBrowserAuth({ ...env, SORREL_HUB_PUBLIC_URL: url }), /HTTPS origin/);
  }
});

test('real AuthKit SDK seals, verifies and refreshes browser sessions through the actual proxy', async t => {
  const { privateKey, publicKey } = generateKeyPairSync('rsa', { modulusLength: 2048 });
  const jwk = { ...publicKey.export({ format: 'jwk' }), kid: 'fixture', alg: 'RS256', use: 'sig' };
  function token(exp, subject = 'user_fixture') {
    const header = Buffer.from(JSON.stringify({ alg: 'RS256', kid: jwk.kid })).toString('base64url');
    const claims = Buffer.from(JSON.stringify({ iss: 'https://api.workos.com', sub: subject,
      sid: 'session_fixture', client_id: env.WORKOS_CLIENT_ID, exp })).toString('base64url');
    const data = `${header}.${claims}`;
    return `${data}.${sign('RSA-SHA256', Buffer.from(data), privateKey).toString('base64url')}`;
  }
  let challenge;
  let exchanges = 0;
  let refreshes = 0;
  const idp = http.createServer(async (request, response) => {
    if (request.url.startsWith('/sso/jwks/')) {
      response.writeHead(200, { 'content-type': 'application/json' }).end(JSON.stringify({ keys: [jwk] }));
      return;
    }
    const chunks = [];
    for await (const chunk of request) chunks.push(chunk);
    const body = JSON.parse(Buffer.concat(chunks).toString());
    if (body.grant_type === 'authorization_code') {
      exchanges++;
      assert.equal(createHash('sha256').update(body.code_verifier).digest('base64url'), challenge);
    } else {
      assert.equal(body.grant_type, 'refresh_token');
      assert.equal(body.refresh_token, 'fixture-refresh-secret');
      refreshes++;
    }
    const access = token(Math.floor(Date.now() / 1000) + (body.grant_type === 'authorization_code' ? -120 : 300));
    response.writeHead(200, { 'content-type': 'application/json' }).end(JSON.stringify({
      user: { id: 'user_fixture', email: 'fixture@example.com', email_verified: true },
      access_token: access, refresh_token: 'fixture-refresh-secret', authentication_method: 'Password',
    }));
  });
  const idpUrl = new URL(await listen(t, idp));
  const workos = new WorkOS(env.WORKOS_API_KEY, { clientId: env.WORKOS_CLIENT_ID, issuer: 'https://api.workos.com',
    apiHostname: idpUrl.hostname, port: Number(idpUrl.port), https: false, maxRetries: 0 });
  const browserAuth = createWorkOsBrowserAuth(env, { workos });
  const app = createApp({ env, browserAuth });
  const upstreamUrl = await listen(t, http.createServer(app.handleRequest));
  const root = await mkdtemp(join(tmpdir(), 'sorrel-login-proxy-'));
  t.after(() => rm(root, { recursive: true, force: true }));
  await writeFile(join(root, 'index.html'), '<main>Sorrel</main>');
  const url = await listen(t, createHubWebServer({ root, hubApiUrl: upstreamUrl }));
  const anonymous = await fetch(`${url}/api/session`).then(r => r.json());
  assert.equal(anonymous.data.session, null);
  assert.equal(anonymous.data.auth.browserLogin, true);

  const login = await fetch(`${url}/api/auth/login`, { redirect: 'manual' });
  assert.equal(login.status, 302);
  const authorize = new URL(login.headers.get('location'));
  challenge = authorize.searchParams.get('code_challenge');
  assert.equal(authorize.searchParams.get('redirect_uri'), `${env.SORREL_HUB_PUBLIC_URL}/api/auth/callback`);
  assert.equal(authorize.searchParams.get('code_challenge_method'), 'S256');
  const state = authorize.searchParams.get('state');
  const loginCookie = login.headers.getSetCookie()[0].split(';')[0];
  assert.match(login.headers.getSetCookie()[0], /HttpOnly; Secure; SameSite=Lax/);
  const callbackUrl = `${url}/api/auth/callback?state=${state}&code=fixture-code`;
  assert.equal((await fetch(callbackUrl, { redirect: 'manual' })).status, 400);
  const callback = await fetch(callbackUrl, { headers: { cookie: loginCookie }, redirect: 'manual' });
  assert.equal(callback.status, 303, await callback.text());
  assert.equal(exchanges, 1);
  const sealedCookie = callback.headers.getSetCookie().find(value => value.startsWith('__Host-sorrel-session='));
  assert.ok(sealedCookie);
  assert.equal(sealedCookie.includes('fixture-refresh-secret'), false);
  assert.equal((await fetch(callbackUrl, { headers: { cookie: loginCookie }, redirect: 'manual' })).status, 400);
  const session = await fetch(`${url}/api/session`, { headers: { cookie: sealedCookie.split(';')[0] } });
  assert.equal(session.status, 200);
  const sessionBody = await session.json();
  assert.deepEqual(sessionBody.data.session.principal, { type: 'user', id: 'workos:user_fixture' });
  assert.equal(refreshes, 1);
  assert.equal(JSON.stringify(sessionBody).includes('Token'), false);
  const refreshedCookie = session.headers.getSetCookie()[0].split(';')[0];
  const rejected = await fetch(`${url}/api/auth/logout`, { method: 'POST', headers: { cookie: refreshedCookie, origin: 'https://attacker.example' } });
  assert.equal(rejected.status, 403);
  const write = await fetch(`${url}/api/projects`, { method: 'POST', headers: { cookie: refreshedCookie, origin: 'https://attacker.example' } });
  assert.equal(write.status, 403);
  const logout = await fetch(`${url}/api/auth/logout`, { method: 'POST', headers: { cookie: refreshedCookie, origin: env.SORREL_HUB_PUBLIC_URL } });
  assert.equal(logout.status, 200);
  assert.match(logout.headers.getSetCookie().at(-1), /Max-Age=0/);
  const logoutUrl = new URL((await logout.json()).data.logoutUrl);
  assert.equal(logoutUrl.searchParams.get('session_id'), 'session_fixture');
  assert.equal(logoutUrl.searchParams.get('return_to'), `${env.SORREL_HUB_PUBLIC_URL}/`);

  const outage = createWorkOsBrowserAuth(env, { workos: {
    userManagement: { loadSealedSession() {
      return { authenticate: async () => { throw new Error('identity provider unavailable'); } };
    } },
  } });
  const outageApp = createApp({ env, browserAuth: outage });
  const outageUrl = await listen(t, http.createServer(outageApp.handleRequest));
  const localLogout = await fetch(`${outageUrl}/auth/logout`, {
    method: 'POST', headers: { cookie: refreshedCookie, origin: env.SORREL_HUB_PUBLIC_URL },
  });
  assert.equal(localLogout.status, 200);
  assert.match(localLogout.headers.getSetCookie().at(-1), /Max-Age=0/);
  assert.deepEqual((await localLogout.json()).data, {
    logoutUrl: `${env.SORREL_HUB_PUBLIC_URL}/?auth=logout-incomplete`, remoteLogout: false,
  });
});
