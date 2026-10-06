import assert from 'node:assert/strict';
import { createSign, generateKeyPairSync } from 'node:crypto';
import http from 'node:http';
import { test } from 'node:test';

import { createAuthAdapterFromEnv, createOidcAdapter, createWorkOsAdapter } from '../src/auth/adapter.js';
import {
  clearJwksCache,
  decodeJwt,
  verifyOidcAccessToken,
  verifyWithJwk,
} from '../src/auth/oidc-jwt.js';

const ISSUER = 'https://idp.test.sorrel.local';

function base64Url(input) {
  return Buffer.from(input)
    .toString('base64')
    .replace(/=/g, '')
    .replace(/\+/g, '-')
    .replace(/\//g, '_');
}

function signRs256Jwt(privateKey, header, payload) {
  const encodedHeader = base64Url(JSON.stringify(header));
  const encodedPayload = base64Url(JSON.stringify(payload));
  const signingInput = `${encodedHeader}.${encodedPayload}`;
  const signer = createSign('RSA-SHA256');
  signer.update(signingInput);
  signer.end();
  const signature = signer.sign(privateKey);
  return `${signingInput}.${base64Url(signature)}`;
}

test('decodeJwt parses header and payload', () => {
  const token = [
    base64Url(JSON.stringify({ alg: 'RS256', typ: 'JWT' })),
    base64Url(JSON.stringify({ sub: 'alice', iss: ISSUER })),
    base64Url('sig'),
  ].join('.');
  const decoded = decodeJwt(token);
  assert.equal(decoded.header.alg, 'RS256');
  assert.equal(decoded.payload.sub, 'alice');
});

test('verifyOidcAccessToken accepts a valid RS256 JWT against JWKS', async () => {
  clearJwksCache();
  const { privateKey, publicKey } = generateKeyPairSync('rsa', { modulusLength: 2048 });
  const jwk = publicKey.export({ format: 'jwk' });
  jwk.kid = 'test-key';
  jwk.use = 'sig';
  jwk.alg = 'RS256';

  const now = Math.floor(Date.now() / 1000);
  const token = signRs256Jwt(privateKey, { alg: 'RS256', kid: 'test-key', typ: 'JWT' }, {
    sub: 'user-42',
    iss: ISSUER,
    aud: 'sorrel-hub',
    exp: now + 3600,
    iat: now,
  });

  const payload = await verifyOidcAccessToken(token, {
    issuer: ISSUER,
    audience: 'sorrel-hub',
    fetchJwks: async () => [jwk],
  });
  assert.equal(payload.sub, 'user-42');

  const decoded = decodeJwt(token);
  assert.equal(verifyWithJwk(decoded.signingInput, decoded.signature, 'RS256', jwk), true);
});

test('OIDC AuthAdapter maps Bearer JWT to HubSession', async () => {
  clearJwksCache();
  const { privateKey, publicKey } = generateKeyPairSync('rsa', { modulusLength: 2048 });
  const jwk = publicKey.export({ format: 'jwk' });
  jwk.kid = 'adapter-key';
  jwk.alg = 'RS256';

  const now = Math.floor(Date.now() / 1000);
  const token = signRs256Jwt(privateKey, { alg: 'RS256', kid: 'adapter-key' }, {
    sub: 'alice@example',
    iss: ISSUER,
    aud: 'hub',
    exp: now + 600,
  });

  const adapter = createOidcAdapter({
    issuer: ISSUER,
    audience: 'hub',
    fetchJwks: async () => [jwk],
  });
  const session = await adapter.resolveSession({
    headers: { authorization: `Bearer ${token}` },
  });
  assert.ok(session);
  assert.deepEqual(session.principal, { type: 'user', id: 'oidc:alice@example' });
  assert.equal(session.authMode, 'oidc');
  assert.equal(session.idpSubject, 'alice@example');
});

test('OIDC AuthAdapter rejects expired tokens', async () => {
  clearJwksCache();
  const { privateKey, publicKey } = generateKeyPairSync('rsa', { modulusLength: 2048 });
  const jwk = publicKey.export({ format: 'jwk' });
  jwk.kid = 'expired';

  const now = Math.floor(Date.now() / 1000);
  const token = signRs256Jwt(privateKey, { alg: 'RS256', kid: 'expired' }, {
    sub: 'bob',
    iss: ISSUER,
    aud: 'hub',
    exp: now - 120,
  });

  const adapter = createOidcAdapter({
    issuer: ISSUER,
    audience: 'hub',
    fetchJwks: async () => [jwk],
  });
  const session = await adapter.resolveSession({
    headers: { authorization: `Bearer ${token}` },
  });
  assert.equal(session, null);
});

test('signed tokens require expiry before an OIDC session can be created', async () => {
  const { privateKey, publicKey } = generateKeyPairSync('rsa', { modulusLength: 2048 });
  const jwk = publicKey.export({ format: 'jwk' });
  const adapter = createOidcAdapter({ issuer: ISSUER, audience: 'hub', fetchJwks: async () => [jwk] });
  for (const claims of [{}, { exp: null }, { exp: '2099999999' }]) {
    const token = signRs256Jwt(privateKey, { alg: 'RS256' }, {
      sub: 'alice', iss: ISSUER, aud: 'hub', ...claims,
    });
    await assert.rejects(verifyOidcAccessToken(token, {
      issuer: ISSUER, audience: 'hub', fetchJwks: async () => [jwk],
    }), /jwt exp must be a numeric date/);
    assert.equal(await adapter.resolveSession({ headers: { authorization: `Bearer ${token}` } }), null);
  }
});

test('signed token audiences must identify the configured Hub', async () => {
  const { privateKey, publicKey } = generateKeyPairSync('rsa', { modulusLength: 2048 });
  const jwk = publicKey.export({ format: 'jwk' });
  const nowMs = 2000000;
  for (const [aud, accepted] of [
    ['hub', true], [['other-service', 'hub'], true],
    [undefined, false], ['other-service', false], [[], false],
    [['other-service'], false], [42, false], [['hub', 42], false],
  ]) {
    const token = signRs256Jwt(privateKey, { alg: 'RS256' }, {
      sub: 'alice', iss: ISSUER, exp: 2061, aud,
    });
    const verification = verifyOidcAccessToken(token, {
      issuer: ISSUER, audience: 'hub', fetchJwks: async () => [jwk], nowMs,
    });
    if (accepted) assert.equal((await verification).sub, 'alice');
    else await assert.rejects(verification, /jwt aud mismatch/);
  }
});

test('issuer-only OIDC configuration rejects signed tokens before fetching keys', async () => {
  const { privateKey, publicKey } = generateKeyPairSync('rsa', { modulusLength: 2048 });
  const jwk = publicKey.export({ format: 'jwk' });
  const token = signRs256Jwt(privateKey, { alg: 'RS256' }, {
    sub: 'alice', iss: ISSUER, aud: 'other-service', exp: Math.floor(Date.now() / 1000) + 600,
  });
  let keyFetches = 0;
  for (const audience of [undefined, '', ' ']) {
    const options = { issuer: ISSUER, audience, fetchJwks: async () => { keyFetches += 1; return [jwk]; } };
    await assert.rejects(verifyOidcAccessToken(token, options), /audience must be configured/);
    assert.equal(await createOidcAdapter(options).resolveSession({ headers: { authorization: `Bearer ${token}` } }), null);
  }
  assert.equal(keyFetches, 0);
  const adapter = createAuthAdapterFromEnv({ SORREL_HUB_AUTH: 'oidc', SORREL_OIDC_ISSUER: ISSUER });
  assert.equal(await adapter.resolveSession({ headers: { authorization: `Bearer ${token}` } }), null);
});

test('WorkOS verifies AuthKit client_id and retains expiry and optional audience checks', async (t) => {
  clearJwksCache();
  const { privateKey, publicKey } = generateKeyPairSync('rsa', { modulusLength: 2048 });
  const jwk = publicKey.export({ format: 'jwk' });
  const server = http.createServer((request, response) => {
    assert.equal(request.url, '/sso/jwks/fixture-client');
    response.writeHead(200, { 'content-type': 'application/json' });
    response.end(JSON.stringify({ keys: [jwk] }));
  });
  await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
  t.after(() => new Promise((resolve) => server.close(resolve)));
  const issuer = `http://127.0.0.1:${server.address().port}`;
  const options = { issuer, apiKey: 'synthetic-fixture', clientId: 'fixture-client' };
  const payload = { sub: 'alice', iss: issuer, client_id: 'fixture-client', exp: Math.floor(Date.now() / 1000) + 600 };
  const sessionFor = (adapter, claims) => adapter.resolveSession({
    headers: { authorization: `Bearer ${signRs256Jwt(privateKey, { alg: 'RS256' }, claims)}` },
  });
  const adapter = createWorkOsAdapter(options);
  const session = await sessionFor(adapter, payload);
  assert.deepEqual(session?.principal, { type: 'user', id: 'workos:alice' });
  assert.equal(session.expiresAt, payload.exp * 1000);
  for (const claims of [
    { client_id: undefined }, { client_id: null }, { client_id: 42 },
    { client_id: ['fixture-client'] }, { client_id: 'another-client' },
    { client_id: 'another-client', aud: 'fixture-client' },
    { exp: undefined }, { exp: null }, { exp: '2099999999' },
    { exp: Math.floor(Date.now() / 1000) - 120 }, { iss: 'https://wrong-issuer.test' },
    { sub: null }, { sub: 42 }, { sub: ' ' },
  ]) {
    assert.equal(await sessionFor(adapter, { ...payload, ...claims }), null, JSON.stringify(claims));
  }
  const { privateKey: forgedKey } = generateKeyPairSync('rsa', { modulusLength: 2048 });
  assert.equal(await adapter.resolveSession({ headers: {
    authorization: `Bearer ${signRs256Jwt(forgedKey, { alg: 'RS256' }, payload)}`,
  } }), null);
  // aud cannot replace client_id, but remains an optional additional restriction.
  assert.ok(await sessionFor(adapter, { ...payload, aud: 'other-service' }));
  const overridden = createWorkOsAdapter({ ...options, audience: 'explicit-audience' });
  assert.equal(await sessionFor(overridden, payload), null);
  assert.ok(await sessionFor(overridden, { ...payload, aud: 'explicit-audience' }));
});

test('default WorkOS environment verifies documented tokens using the client-specific HTTP JWKS endpoint', async (t) => {
  clearJwksCache();
  t.after(clearJwksCache);
  const { privateKey, publicKey } = generateKeyPairSync('rsa', { modulusLength: 2048 });
  const jwk = { ...publicKey.export({ format: 'jwk' }), kid: 'authkit-key', alg: 'RS256' };
  const requests = [];
  const server = http.createServer((request, response) => {
    requests.push(request.url);
    if (request.url !== '/sso/jwks/client_fixture') { response.writeHead(404); response.end(); return; }
    response.writeHead(200, { 'content-type': 'application/json' });
    response.end(JSON.stringify({ keys: [jwk] }));
  });
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  t.after(() => new Promise(resolve => server.close(resolve)));
  const local = `http://127.0.0.1:${server.address().port}`;
  const originalFetch = globalThis.fetch;
  const fetchUris = [];
  t.mock.method(globalThis, 'fetch', (uri, options) => {
    fetchUris.push(uri);
    assert.equal(new URL(uri).origin, 'https://api.workos.com');
    return originalFetch(`${local}${new URL(uri).pathname}`, options);
  });
  const adapter = createAuthAdapterFromEnv({ SORREL_HUB_AUTH: 'workos',
    WORKOS_API_KEY: 'synthetic-fixture', WORKOS_CLIENT_ID: 'client_fixture' });
  const payload = { iss: 'https://api.workos.com', sub: 'user_fixture', client_id: 'client_fixture',
    sid: 'session_fixture', iat: Math.floor(Date.now() / 1000), exp: Math.floor(Date.now() / 1000) + 600 };
  const sessionFor = (claims) => adapter.resolveSession({ headers: {
    authorization: `Bearer ${signRs256Jwt(privateKey, { alg: 'RS256', kid: 'authkit-key' }, claims)}`,
  } });
  const session = await sessionFor(payload);
  assert.deepEqual(session.principal, { type: 'user', id: 'workos:user_fixture' });
  assert.equal(session.authMode, 'workos');
  assert.equal(session.expiresAt, payload.exp * 1000);
  assert.deepEqual(fetchUris, ['https://api.workos.com/sso/jwks/client_fixture']);
  assert.deepEqual(requests, ['/sso/jwks/client_fixture']);
  assert.equal(await sessionFor({ ...payload, client_id: 'client_other', aud: 'client_fixture' }), null);
  assert.deepEqual(requests, ['/sso/jwks/client_fixture']);
});

test('generic OIDC still requires its configured aud even with an AuthKit client_id claim', async () => {
  const { privateKey, publicKey } = generateKeyPairSync('rsa', { modulusLength: 2048 });
  const jwk = publicKey.export({ format: 'jwk' });
  let keyFetches = 0;
  const adapter = createOidcAdapter({ issuer: ISSUER, audience: 'hub', fetchJwks: async (uri) => {
    assert.equal(uri, `${ISSUER}/.well-known/jwks.json`);
    keyFetches += 1;
    return [jwk];
  } });
  const payload = { iss: ISSUER, sub: 'alice', client_id: 'hub', exp: Math.floor(Date.now() / 1000) + 600 };
  const sessionFor = (claims) => adapter.resolveSession({ headers: {
    authorization: `Bearer ${signRs256Jwt(privateKey, { alg: 'RS256' }, claims)}`,
  } });
  assert.equal(await sessionFor(payload), null);
  assert.equal(keyFetches, 0);
  assert.deepEqual((await sessionFor({ ...payload, aud: 'hub' })).principal, { type: 'user', id: 'oidc:alice' });
  assert.equal(keyFetches, 1);
});

test('expiry keeps the configured clock-skew boundary', async () => {
  const { privateKey, publicKey } = generateKeyPairSync('rsa', { modulusLength: 2048 });
  const jwk = publicKey.export({ format: 'jwk' });
  const token = signRs256Jwt(privateKey, { alg: 'RS256' }, {
    sub: 'alice', iss: ISSUER, aud: 'hub', exp: 2000,
  });
  const options = { issuer: ISSUER, audience: 'hub', fetchJwks: async () => [jwk], clockSkewSec: 60 };
  assert.equal((await verifyOidcAccessToken(token, { ...options, nowMs: 2059999 })).sub, 'alice');
  await assert.rejects(verifyOidcAccessToken(token, { ...options, nowMs: 2060000 }), /jwt expired/);
});


test('JWT date claims cannot disable expiry checks through incorrect types', async () => {
  const { privateKey, publicKey } = generateKeyPairSync('rsa', { modulusLength: 2048 });
  const jwk = publicKey.export({ format: 'jwk' });
  const nowMs = 2000000;
  for (const claims of [{ exp: '0' }, { exp: null }, { nbf: '99999999' }, { iat: {} }, { exp: 2000 }]) {
    const token = signRs256Jwt(privateKey, { alg: 'RS256' }, { sub: 'alice', iss: ISSUER, aud: 'hub', exp: 2061, ...claims });
    await assert.rejects(verifyOidcAccessToken(token, {
      issuer: ISSUER, audience: 'hub', fetchJwks: async () => [jwk], nowMs, clockSkewSec: 0,
    }), /jwt (?:exp|nbf|iat|expired)/);
  }
});

test('JWT verification respects key IDs, algorithms, and allowed operations', async () => {
  const { privateKey, publicKey } = generateKeyPairSync('rsa', { modulusLength: 2048 });
  const jwk = publicKey.export({ format: 'jwk' });
  for (const [header, key] of [
    [{ alg: 'RS256', kid: 'required-key' }, jwk],
    [{ alg: 'RS256' }, { ...jwk, alg: 'RS512' }],
    [{ alg: 'RS256' }, { ...jwk, key_ops: ['sign'] }],
  ]) {
    const token = signRs256Jwt(privateKey, header, { sub: 'alice', iss: ISSUER, aud: 'hub', exp: Math.floor(Date.now() / 1000) + 600 });
    await assert.rejects(verifyOidcAccessToken(token, {
      issuer: ISSUER, audience: 'hub', fetchJwks: async () => [key],
    }), /signature verification failed/);
  }
});

test('ES256 accepts JOSE signatures and rejects DER-encoded JWT signatures', async () => {
  const { privateKey, publicKey } = generateKeyPairSync('ec', { namedCurve: 'P-256' });
  const jwk = publicKey.export({ format: 'jwk' });
  const signingInput = `${base64Url(JSON.stringify({ alg: 'ES256' }))}.${base64Url(JSON.stringify({ iss: ISSUER, sub: 'alice', aud: 'hub', exp: Math.floor(Date.now() / 1000) + 600 }))}`;
  for (const dsaEncoding of ['ieee-p1363', 'der']) {
    const signer = createSign('SHA256');
    signer.update(signingInput);
    signer.end();
    const signature = signer.sign({ key: privateKey, dsaEncoding });
    const promise = verifyOidcAccessToken(`${signingInput}.${base64Url(signature)}`, {
      issuer: ISSUER, audience: 'hub', fetchJwks: async () => [jwk],
    });
    if (dsaEncoding === 'ieee-p1363') {
      assert.equal((await promise).sub, 'alice');
    } else {
      await assert.rejects(promise, /signature verification failed/);
    }
  }
});

test('OIDC principals cannot be created from non-string subject claims', async () => {
  const { privateKey, publicKey } = generateKeyPairSync('rsa', { modulusLength: 2048 });
  const jwk = publicKey.export({ format: 'jwk' });
  const adapter = createOidcAdapter({ issuer: ISSUER, audience: 'hub', fetchJwks: async () => [jwk] });
  for (const sub of [{ id: 'alice' }, 42, ['alice'], ' ']) {
    const token = signRs256Jwt(privateKey, { alg: 'RS256' }, { sub, iss: ISSUER, aud: 'hub', exp: Math.floor(Date.now() / 1000) + 600 });
    assert.equal(await adapter.resolveSession({ headers: { authorization: `Bearer ${token}` } }), null);
  }
});
