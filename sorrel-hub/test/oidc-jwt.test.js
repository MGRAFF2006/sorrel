import assert from 'node:assert/strict';
import { createSign, generateKeyPairSync } from 'node:crypto';
import { test } from 'node:test';

import { createOidcAdapter } from '../src/auth/adapter.js';
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
