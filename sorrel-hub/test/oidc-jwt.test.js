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

test('WorkOS preserves its client-id audience fallback and rejects missing expiry', async (t) => {
  clearJwksCache();
  const { privateKey, publicKey } = generateKeyPairSync('rsa', { modulusLength: 2048 });
  const jwk = publicKey.export({ format: 'jwk' });
  const server = http.createServer((request, response) => {
    assert.equal(request.url, '/.well-known/jwks.json');
    response.writeHead(200, { 'content-type': 'application/json' });
    response.end(JSON.stringify({ keys: [jwk] }));
  });
  await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
  t.after(() => new Promise((resolve) => server.close(resolve)));
  const issuer = `http://127.0.0.1:${server.address().port}`;
  const options = { issuer, apiKey: 'synthetic-fixture', clientId: 'fixture-client' };
  const payload = { sub: 'alice', iss: issuer, aud: 'fixture-client', exp: Math.floor(Date.now() / 1000) + 600 };
  const sessionFor = (adapter, claims) => adapter.resolveSession({
    headers: { authorization: `Bearer ${signRs256Jwt(privateKey, { alg: 'RS256' }, claims)}` },
  });
  const adapter = createWorkOsAdapter(options);
  const session = await sessionFor(adapter, payload);
  assert.deepEqual(session?.principal, { type: 'user', id: 'workos:alice' });
  assert.equal(session.expiresAt, payload.exp * 1000);
  const withoutExpiry = { ...payload }; delete withoutExpiry.exp;
  assert.equal(await sessionFor(adapter, withoutExpiry), null);
  assert.equal(await sessionFor(adapter, { ...payload, aud: 'other-service' }), null);
  const overridden = createWorkOsAdapter({ ...options, audience: 'explicit-audience' });
  assert.equal(await sessionFor(overridden, payload), null);
  assert.ok(await sessionFor(overridden, { ...payload, aud: 'explicit-audience' }));
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
