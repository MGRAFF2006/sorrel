import assert from 'node:assert/strict';
import http from 'node:http';
import { Readable } from 'node:stream';
import test from 'node:test';

import { createApp } from '../src/app.js';
import { createCliDeviceFlow, DEVICE_GRANT_TYPE, MAX_PENDING_DEVICE_CODES, normalizeUserCode } from '../src/auth/cli-device.js';
import { createCliTokens } from '../src/auth/cli-tokens.js';
import { createWorkOsBrowserAuth } from '../src/auth/workos-browser.js';
import { createInMemoryStore } from '../src/store.js';

const origin = 'https://hub.example.com';
const env = { SORREL_HUB_AUTH: 'workos', SORREL_HUB_PUBLIC_URL: origin,
  WORKOS_API_KEY: 'sk_test_fixture', WORKOS_CLIENT_ID: 'client_fixture', WORKOS_COOKIE_PASSWORD: 'fixture-cookie-password-of-at-least-32-chars' };
const approver = { type: 'user', id: 'workos:user_approver' };
const intruder = { type: 'user', id: 'workos:user_intruder' };
const idpBearer = 'Bearer identity-provider-token';

/** Fake WorkOS client: only what login/callback touch in these tests. */
function fakeWorkos() {
  return {
    pkce: { async generate() { return { codeVerifier: 'fixture-verifier', codeChallenge: 'fixture-challenge' }; } },
    userManagement: {
      getAuthorizationUrl({ state }) { return `https://idp.example/authorize?state=${state}`; },
      async authenticateWithCode() { return { sealedSession: 'sealed-fixture-session' }; },
    },
  };
}

/**
 * Real browser-auth origin rules with a stubbed cookie session: the cookie
 * value names the fixture user, standing in for a sealed WorkOS session.
 */
function stubbedBrowserAuth() {
  const real = createWorkOsBrowserAuth(env, { workos: fakeWorkos() });
  return { ...real, async resolveSession(request) {
    const match = /__Host-sorrel-session=(\w+)/.exec(String(request.headers.cookie ?? ''));
    if (!match) return null;
    if (!['GET', 'HEAD', 'OPTIONS'].includes(request.method)) real.requireSameOrigin(request);
    const principal = match[1] === 'approver' ? approver : match[1] === 'intruder' ? intruder : null;
    return principal && { principal, sessionId: `sid_${match[1]}`, authMode: 'workos', idpSubject: principal.id.slice(7), transport: 'cookie' };
  } };
}

async function withHub(t, { browserAuth = stubbedBrowserAuth() } = {}) {
  const store = createInMemoryStore();
  const audits = [];
  let now = Date.parse('2026-11-24T10:00:00Z');
  const clock = () => now;
  const audit = (event, fields) => audits.push({ event, ...fields });
  const cliTokens = createCliTokens(store, { env: {}, now: clock, audit });
  const deviceFlow = browserAuth ? createCliDeviceFlow({ cliTokens, browserAuth, now: clock, audit }) : null;
  const authAdapter = { mode: 'workos', async resolveSession(request) {
    return request.headers.authorization === idpBearer ? { principal: approver, authMode: 'workos', sessionId: 'sid_bearer' } : null;
  } };
  const app = createApp({ store, cliTokens, deviceFlow, browserAuth, authAdapter, env: {} });
  const server = http.createServer(app.handleRequest);
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  t.after(async () => { await new Promise(resolve => server.close(resolve)); await app.close(); });
  const url = `http://127.0.0.1:${server.address().port}`;
  async function call(pathname, { method = 'POST', headers = {}, body } = {}) {
    const response = await fetch(url + pathname, { method, headers: { 'content-type': 'application/json', ...headers },
      ...(body === undefined ? {} : { body: typeof body === 'string' ? body : JSON.stringify(body) }) });
    const text = await response.text();
    return { status: response.status, headers: response.headers, body: text ? JSON.parse(text) : null };
  }
  const browser = who => ({ cookie: `__Host-sorrel-session=${who}`, origin });
  const poll = deviceCode => call('/auth/cli/token', { body: { grant_type: DEVICE_GRANT_TYPE, device_code: deviceCode } });
  return { audits, call, browser, poll, advance(ms) { now += ms; } };
}

test('device login: request, pending polls, cookie approval, single-use token exchange', async t => {
  const hub = await withHub(t);
  const capabilities = await hub.call('/capabilities', { method: 'GET' });
  assert.equal(capabilities.body.data.auth.cliLogin, 'device');
  assert.equal((await hub.call('/session', { method: 'GET' })).body.data.auth.cliLogin, 'device');

  const started = await hub.call('/auth/cli/device', { body: { label: 'work laptop' } });
  assert.equal(started.status, 200);
  assert.equal(started.headers.get('cache-control'), 'no-store');
  const grant = started.body;
  assert.match(grant.user_code, /^[BCDFGHJKLMNPQRSTVWXZ]{4}-[BCDFGHJKLMNPQRSTVWXZ]{4}$/);
  assert.match(grant.device_code, /^[A-Za-z0-9_-]{43}$/);
  assert.equal(grant.verification_uri, `${origin}/cli/activate`);
  assert.equal(grant.verification_uri_complete, `${origin}/cli/activate?code=${grant.user_code}`);
  assert.equal(grant.expires_in, 600);
  assert.equal(grant.interval, 5);

  let polled = await hub.poll(grant.device_code);
  assert.equal(polled.status, 400);
  assert.equal(polled.body.error, 'authorization_pending');
  assert.equal(polled.headers.get('cache-control'), 'no-store');
  polled = await hub.poll(grant.device_code);
  assert.equal(polled.body.error, 'slow_down', 'polling faster than the interval is throttled');
  hub.advance(10_000);
  assert.equal((await hub.poll(grant.device_code)).body.error, 'authorization_pending');

  // Typed codes tolerate case, spaces and a missing dash.
  const typed = grant.user_code.replace('-', ' ').toLowerCase();
  const approved = await hub.call('/auth/cli/approve', { headers: hub.browser('approver'), body: { user_code: typed, decision: 'approve' } });
  assert.equal(approved.status, 200);
  assert.deepEqual(approved.body.data, { status: 'approved', label: 'work laptop' });
  assert.equal(approved.headers.get('cache-control'), 'no-store');

  hub.advance(10_000);
  const exchanged = await hub.poll(grant.device_code);
  assert.equal(exchanged.status, 200);
  assert.equal(exchanged.headers.get('cache-control'), 'no-store');
  assert.match(exchanged.body.access_token, /^srl_cli_[0-9a-f]{24}_/);
  assert.equal(exchanged.body.token_type, 'Bearer');
  assert.equal(exchanged.body.expires_in, 30 * 24 * 60 * 60);
  assert.deepEqual(exchanged.body.principal, approver);
  assert.equal(exchanged.body.label, 'work laptop');

  const session = await hub.call('/session', { method: 'GET', headers: { authorization: `Bearer ${exchanged.body.access_token}` } });
  assert.deepEqual(session.body.data.session.principal, approver);
  assert.equal(session.body.data.session.credential.id, exchanged.body.token_id);

  hub.advance(10_000);
  assert.equal((await hub.poll(grant.device_code)).body.error, 'invalid_grant', 'device codes work once');
  const reused = await hub.call('/auth/cli/approve', { headers: hub.browser('approver'), body: { user_code: grant.user_code, decision: 'approve' } });
  assert.equal(reused.status, 404);
  assert.equal(reused.body.error.code, 'invalid_user_code');

  const approval = hub.audits.find(entry => entry.event === 'cli_device.approved');
  const issued = hub.audits.find(entry => entry.event === 'cli_token.issued');
  assert.equal(approval.principal, 'user:workos:user_approver');
  assert.equal(issued.requestId, approval.requestId);
  const serialized = JSON.stringify(hub.audits);
  for (const secret of [grant.device_code, grant.user_code, exchanged.body.access_token]) assert.ok(!serialized.includes(secret));
});

test('approval needs a same-origin cookie session and takes the principal only from it', async t => {
  const hub = await withHub(t);
  const grant = (await hub.call('/auth/cli/device', { body: {} })).body;
  const body = { user_code: grant.user_code, decision: 'approve' };

  for (const [headers, status, code] of [
    [{}, 401, 'authentication_required'],
    [{ authorization: idpBearer }, 401, 'authentication_required'],
    [{ cookie: '__Host-sorrel-session=approver' }, 403, 'invalid_origin'],
    [{ cookie: '__Host-sorrel-session=approver', origin: 'https://evil.example' }, 403, 'invalid_origin'],
  ]) {
    const result = await hub.call('/auth/cli/approve', { headers, body });
    assert.equal(result.status, status, JSON.stringify(headers));
    assert.equal(result.body.error.code, code);
  }

  // A CLI token is never an approval session.
  const other = (await hub.call('/auth/cli/device', { body: {} })).body;
  await hub.call('/auth/cli/approve', { headers: hub.browser('intruder'), body: { user_code: other.user_code, decision: 'approve' } });
  hub.advance(5_000);
  const intruderToken = (await hub.poll(other.device_code)).body.access_token;
  const viaCli = await hub.call('/auth/cli/approve', { headers: { authorization: `Bearer ${intruderToken}` }, body });
  assert.equal(viaCli.status, 401);

  const smuggled = await hub.call('/auth/cli/approve', { headers: hub.browser('intruder'),
    body: { ...body, principal: approver } });
  assert.equal(smuggled.status, 400, 'identity fields in the body are rejected outright');
  assert.equal((await hub.poll(grant.device_code)).body.error, 'authorization_pending', 'nothing was approved');

  assert.equal((await hub.call('/auth/cli/approve', { headers: hub.browser('intruder'), body })).status, 200);
  hub.advance(5_000);
  const token = (await hub.poll(grant.device_code)).body;
  assert.deepEqual(token.principal, intruder, 'token belongs to the approving session');
});

test('denied and expired codes end the flow', async t => {
  const hub = await withHub(t);
  const denied = (await hub.call('/auth/cli/device', { body: {} })).body;
  const result = await hub.call('/auth/cli/approve', { headers: hub.browser('approver'), body: { user_code: denied.user_code, decision: 'deny' } });
  assert.deepEqual(result.body.data, { status: 'denied', label: 'Sorrel CLI' });
  assert.equal((await hub.poll(denied.device_code)).body.error, 'access_denied');
  assert.equal((await hub.poll(denied.device_code)).body.error, 'invalid_grant');
  assert.equal(hub.audits.filter(entry => entry.event === 'cli_device.denied').length, 1);
  assert.equal(hub.audits.filter(entry => entry.event === 'cli_token.issued').length, 0);

  const expiring = (await hub.call('/auth/cli/device', { body: {} })).body;
  hub.advance(10 * 60 * 1000);
  const late = await hub.call('/auth/cli/approve', { headers: hub.browser('approver'), body: { user_code: expiring.user_code, decision: 'approve' } });
  assert.equal(late.status, 404);
  assert.equal((await hub.poll(expiring.device_code)).body.error, 'expired_token');

  assert.equal((await hub.call('/auth/cli/token', { body: { grant_type: 'password', device_code: 'x' } })).body.error, 'unsupported_grant_type');
  assert.equal((await hub.call('/auth/cli/token', { body: '{' })).body.error, 'invalid_request');
  assert.equal((await hub.call('/auth/cli/device', { body: { label: 'a\u0007b' } })).body.error, 'invalid_request');
  assert.equal((await hub.call('/auth/cli/device', { method: 'GET' })).status, 405);
});

test('device requests are rate-limited per caller, pending codes are bounded, wrong codes throttle the approver', async t => {
  const hub = await withHub(t);
  for (let i = 0; i < 10; i += 1) assert.equal((await hub.call('/auth/cli/device', { body: {} })).status, 200);
  const limited = await hub.call('/auth/cli/device', { body: {} });
  assert.equal(limited.status, 429);
  assert.equal(limited.body.error, 'slow_down');
  assert.ok(Number(limited.headers.get('retry-after')) > 0);
  hub.advance(60_000);
  assert.equal((await hub.call('/auth/cli/device', { body: {} })).status, 200);

  for (let i = 0; i < 10; i += 1) {
    const wrong = await hub.call('/auth/cli/approve', { headers: hub.browser('approver'), body: { user_code: 'BBBB-BBBB', decision: 'approve' } });
    assert.equal(wrong.status, 404);
  }
  const throttled = await hub.call('/auth/cli/approve', { headers: hub.browser('approver'), body: { user_code: 'BBBB-BBBB', decision: 'approve' } });
  assert.equal(throttled.status, 429);

  // Capacity: drive the flow directly with distinct callers.
  const flow = createCliDeviceFlow({ cliTokens: createCliTokens(createInMemoryStore(), { env: {}, audit() {} }), browserAuth: { origin, requireSameOrigin() {} } });
  const start = async i => {
    const request = Object.assign(Readable.from([Buffer.from('{}')]), { method: 'POST', headers: {}, socket: { remoteAddress: `10.0.${i >> 8}.${i & 255}` } });
    let status;
    const response = { setHeader() {}, writeHead(code) { status = code; return this; }, end() {} };
    await flow.handle(request, response, 'device', null);
    return status;
  };
  for (let i = 0; i < MAX_PENDING_DEVICE_CODES; i += 1) assert.equal(await start(i), 200);
  assert.equal(await start(MAX_PENDING_DEVICE_CODES), 429);
  assert.equal(flow.pendingCount, MAX_PENDING_DEVICE_CODES);
});

test('hubs without browser sign-in do not offer device login', async t => {
  const hub = await withHub(t, { browserAuth: null });
  assert.equal((await hub.call('/capabilities', { method: 'GET' })).body.data.auth.cliLogin, 'none');
  for (const action of ['device', 'token', 'approve']) {
    const result = await hub.call(`/auth/cli/${action}`, { body: {} });
    assert.equal(result.status, 404);
    assert.equal(result.body.error.code, 'cli_login_unavailable');
  }
});

test('user codes normalize and browser sign-in returns only to allow-listed activation paths', async () => {
  assert.equal(normalizeUserCode('bcdf ghjk'), 'BCDF-GHJK');
  assert.equal(normalizeUserCode('BCDF-GHJA'), null);
  assert.equal(normalizeUserCode('x'.repeat(40)), null);

  const browserAuth = createWorkOsBrowserAuth(env, { workos: fakeWorkos() });
  async function signIn(returnTo) {
    const headers = new Map();
    const loginResponse = { getHeader: name => headers.get(name), setHeader: (name, value) => headers.set(name, value),
      writeHead(_status, values) { this.location = values.location; return this; }, end() {} };
    const query = returnTo === undefined ? '' : `?returnTo=${encodeURIComponent(returnTo)}`;
    await browserAuth.handleRequest({ method: 'GET', headers: {} }, loginResponse, new URL(`${origin}/auth/login${query}`));
    const state = new URL(loginResponse.location).searchParams.get('state');
    const loginCookie = headers.get('set-cookie')[0].split(';')[0];
    const callbackHeaders = new Map();
    const callbackResponse = { getHeader: name => callbackHeaders.get(name), setHeader: (name, value) => callbackHeaders.set(name, value),
      writeHead(_status, values) { this.location = values.location; return this; }, end() {} };
    await browserAuth.handleRequest({ method: 'GET', headers: { cookie: loginCookie } }, callbackResponse,
      new URL(`${origin}/auth/callback?state=${state}&code=fixture`));
    return callbackResponse.location;
  }
  assert.equal(await signIn('/cli/activate?code=BCDF-GHJK'), `${origin}/cli/activate?code=BCDF-GHJK`);
  assert.equal(await signIn('/cli/activate'), `${origin}/cli/activate`);
  for (const hostile of [undefined, 'https://evil.example/', '//evil.example', '/cli/activate?code=BCDF-GHJK&next=//evil', '/projects', '/cli/activate/../x']) {
    assert.equal(await signIn(hostile), `${origin}/`, String(hostile));
  }
});
