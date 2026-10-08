import assert from 'node:assert/strict';
import { createSign, generateKeyPairSync } from 'node:crypto';
import http from 'node:http';
import { setTimeout } from 'node:timers/promises';
import test from 'node:test';

import { clearJwksCache, verifyOidcAccessToken } from '../src/auth/oidc-jwt.js';

function signingKey(kid) {
  const { privateKey, publicKey } = generateKeyPairSync('rsa', { modulusLength: 2048 });
  return { privateKey, jwk: { ...publicKey.export({ format: 'jwk' }), kid, alg: 'RS256', use: 'sig' } };
}
const oldKey = signingKey('old-key');
const newKey = signingKey('new-key');

function token(issuer, key, kid = key.jwk.kid) {
  const input = [
    { alg: 'RS256', kid },
    { iss: issuer, sub: 'alice', aud: 'hub', exp: Math.floor(Date.now() / 1000) + 3600 },
  ].map((value) => Buffer.from(JSON.stringify(value)).toString('base64url')).join('.');
  const signer = createSign('RSA-SHA256');
  signer.update(input);
  return `${input}.${signer.sign(key.privateKey).toString('base64url')}`;
}

async function withJwksServer(t, callback) {
  clearJwksCache();
  t.mock.timers.enable({ apis: ['Date'], now: Date.now() });
  const state = { keys: [oldKey.jwk], requests: 0, status: 200, delay: 0, hang: false };
  const server = http.createServer(async (request, response) => {
    assert.equal(request.url, '/.well-known/jwks.json');
    state.requests += 1;
    if (state.hang) return;
    if (state.delay) await setTimeout(state.delay);
    response.writeHead(state.status, { 'content-type': 'application/json' });
    response.end(JSON.stringify({ keys: state.keys }));
  });
  await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
  const issuer = `http://127.0.0.1:${server.address().port}`;
  const options = { issuer, audience: 'hub' };
  try {
    await callback(state, options);
  } finally {
    server.closeAllConnections();
    await new Promise((resolve, reject) => server.close((error) => error ? reject(error) : resolve()));
    clearJwksCache();
  }
}

test('rotation refreshes an unknown kid before TTL, coalesces misses, and bounds attacker refreshes', async (t) => {
  await withJwksServer(t, async (state, options) => {
    const oldToken = token(options.issuer, oldKey);
    assert.equal((await verifyOidcAccessToken(oldToken, options)).sub, 'alice');
    await verifyOidcAccessToken(oldToken, options);
    assert.equal(state.requests, 1, 'known keys reuse the cache');
    state.keys = [oldKey.jwk, newKey.jwk];
    state.delay = 25;
    const newToken = token(options.issuer, newKey);
    const payloads = await Promise.all(Array.from({ length: 32 }, () => verifyOidcAccessToken(newToken, options)));
    assert.ok(payloads.every((payload) => payload.sub === 'alice'));
    assert.equal(state.requests, 2, 'concurrent unknown kids share one refresh');
    await verifyOidcAccessToken(oldToken, options);
    for (let n = 0; n < 20; n += 1) {
      await assert.rejects(verifyOidcAccessToken(token(options.issuer, newKey, `unknown-${n}`), options), /signature verification failed/);
    }
    assert.equal(state.requests, 2, 'different unknown kids cannot bypass issuer cooldown');
    t.mock.timers.tick(30001);
    await assert.rejects(verifyOidcAccessToken(token(options.issuer, newKey, oldKey.jwk.kid), options), /signature verification failed/);
    assert.equal(state.requests, 2, 'bad signatures with known kids do not trigger refresh');
    await assert.rejects(verifyOidcAccessToken(token(options.issuer, newKey, 'still-unknown'), options), /signature verification failed/);
    assert.equal(state.requests, 3, 'one retry is allowed after cooldown');
  });
});

test('failed rotation refresh rejects new keys, preserves known keys, and permits bounded recovery', async (t) => {
  await withJwksServer(t, async (state, options) => {
    const oldToken = token(options.issuer, oldKey);
    const newToken = token(options.issuer, newKey);
    await verifyOidcAccessToken(oldToken, options);
    state.status = 503;
    await assert.rejects(verifyOidcAccessToken(newToken, options), /jwks fetch failed: 503/);
    assert.equal(state.requests, 2);
    assert.equal((await verifyOidcAccessToken(oldToken, options)).sub, 'alice');
    await assert.rejects(verifyOidcAccessToken(newToken, options), /signature verification failed/);
    assert.equal(state.requests, 2, 'failed refreshes also observe cooldown');
    state.status = 200;
    state.keys = [oldKey.jwk, newKey.jwk];
    t.mock.timers.tick(30001);
    assert.equal((await verifyOidcAccessToken(newToken, options)).sub, 'alice');
    assert.equal(state.requests, 3);
  });
});

test('cold-cache requests coalesce and unavailable endpoints cannot be hammered', async (t) => {
  await withJwksServer(t, async (state, options) => {
    state.status = 503;
    state.delay = 25;
    const oldToken = token(options.issuer, oldKey);
    const results = await Promise.allSettled(Array.from({ length: 32 }, () => verifyOidcAccessToken(oldToken, options)));
    assert.ok(results.every((result) => result.status === 'rejected'));
    assert.equal(state.requests, 1);
    for (let n = 0; n < 10; n += 1) await assert.rejects(verifyOidcAccessToken(oldToken, options), /jwks/);
    assert.equal(state.requests, 1);
    state.status = 200;
    t.mock.timers.tick(30001);
    await verifyOidcAccessToken(oldToken, options);
    assert.equal(state.requests, 2);
    t.mock.timers.tick(10 * 60 * 1000 + 1);
    await verifyOidcAccessToken(oldToken, options);
    assert.equal(state.requests, 3, 'normal cache expiry still refreshes');
    state.status = 503;
    t.mock.timers.tick(10 * 60 * 1000 + 1);
    await assert.rejects(verifyOidcAccessToken(oldToken, options), /jwks fetch failed: 503/);
    await assert.rejects(verifyOidcAccessToken(oldToken, options), /jwks refresh temporarily unavailable/);
    assert.equal(state.requests, 4, 'expired keys fail closed during refresh failure and cooldown');
  });
});

test('fetchJwks overrides retain one-call behavior without automatic HTTP refresh', async (t) => {
  await withJwksServer(t, async (state, options) => {
    let calls = 0;
    const override = { ...options, fetchJwks: async () => { calls += 1; return [oldKey.jwk]; } };
    await assert.rejects(verifyOidcAccessToken(token(options.issuer, newKey), override), /signature verification failed/);
    assert.equal(calls, 1);
    assert.equal(state.requests, 0);
    await verifyOidcAccessToken(token(options.issuer, oldKey), override);
    assert.equal(calls, 2);
  });
});

test('a hanging JWKS endpoint times out and shares the failed refresh cooldown', async (t) => {
  await withJwksServer(t, async (state, options) => {
    state.hang = true;
    const oldToken = token(options.issuer, oldKey);
    await assert.rejects(verifyOidcAccessToken(oldToken, options), /timeout|aborted/i);
    assert.equal(state.requests, 1);
    await assert.rejects(verifyOidcAccessToken(oldToken, options), /jwks refresh temporarily unavailable/);
    assert.equal(state.requests, 1);
  });
});
