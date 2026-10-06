/**
 * Minimal OIDC access-token verification (JWKS + RS256/ES256).
 * Zero npm deps — uses node:crypto only. Auth stays off the blob hot path:
 * verify once per request, then attach HubSession.
 */

import { createPublicKey, createVerify } from 'node:crypto';

/**
 * @typedef {{
 *   kty: string,
 *   kid?: string,
 *   use?: string,
 *   alg?: string,
 *   key_ops?: string[],
 *   n?: string,
 *   e?: string,
 *   crv?: string,
 *   x?: string,
 *   y?: string,
 * }} Jwk
 */

/** @type {Map<string, { fetchedAt: number, keys: Jwk[] }>} */
const jwksCache = new Map();

const JWKS_TTL_MS = 10 * 60 * 1000;

/**
 * @param {string} token
 * @returns {{ header: Record<string, unknown>, payload: Record<string, unknown>, signingInput: string, signature: Buffer }}
 */
export function decodeJwt(token) {
  const parts = token.split('.');
  if (parts.length !== 3) {
    throw new Error('jwt must have three segments');
  }
  const [headerB64, payloadB64, signatureB64] = parts;
  const header = JSON.parse(base64UrlToUtf8(headerB64));
  const payload = JSON.parse(base64UrlToUtf8(payloadB64));
  if (!header || typeof header !== 'object' || Array.isArray(header) ||
      !payload || typeof payload !== 'object' || Array.isArray(payload)) {
    throw new Error('jwt header and payload must be objects');
  }
  return {
    header,
    payload,
    signingInput: `${headerB64}.${payloadB64}`,
    signature: base64UrlToBuffer(signatureB64),
  };
}

/**
 * @param {string} token
 * @param {{
 *   issuer: string,
 *   audience?: string,
 *   clientId?: string,
 *   jwksUri?: string,
 *   fetchJwks?: (uri: string) => Promise<Jwk[]>,
 *   nowMs?: number,
 *   clockSkewSec?: number,
 * }} options
 */
export async function verifyOidcAccessToken(token, options) {
  const { header, payload, signingInput, signature } = decodeJwt(token);
  const alg = typeof header.alg === 'string' ? header.alg : '';
  if (alg !== 'RS256' && alg !== 'ES256') {
    throw new Error(`unsupported jwt alg: ${alg || 'missing'}`);
  }

  const issuer = options.issuer.replace(/\/$/, '');
  if (payload.iss !== issuer && payload.iss !== `${issuer}/`) {
    throw new Error('jwt iss mismatch');
  }

  if (options.clientId !== undefined) {
    if (typeof options.clientId !== 'string' || !options.clientId.trim()) {
      throw new Error('jwt client_id must be configured');
    }
    if (payload.client_id !== options.clientId) {
      throw new Error('jwt client_id mismatch');
    }
  }
  if (options.clientId === undefined || options.audience !== undefined) {
    if (typeof options.audience !== 'string' || !options.audience.trim()) {
      throw new Error('jwt audience must be configured');
    }
    const aud = payload.aud;
    const audienceMatches =
      aud === options.audience ||
      (Array.isArray(aud) && aud.every((value) => typeof value === 'string') &&
        aud.includes(options.audience));
    if (!audienceMatches) {
      throw new Error('jwt aud mismatch');
    }
  }

  const nowSec = Math.floor((options.nowMs ?? Date.now()) / 1000);
  const skew = options.clockSkewSec ?? 60;
  if (!Number.isFinite(payload.exp)) {
    throw new Error('jwt exp must be a numeric date');
  }
  for (const claim of ['nbf', 'iat']) {
    if (payload[claim] !== undefined && !Number.isFinite(payload[claim])) {
      throw new Error(`jwt ${claim} must be a numeric date`);
    }
  }
  if (nowSec >= payload.exp + skew) {
    throw new Error('jwt expired');
  }
  if (typeof payload.nbf === 'number' && nowSec + skew < payload.nbf) {
    throw new Error('jwt not yet valid');
  }

  const jwksUri = options.jwksUri ?? `${issuer}/.well-known/jwks.json`;
  const keys =
    (await options.fetchJwks?.(jwksUri)) ?? (await fetchJwksCached(jwksUri));
  const kid = typeof header.kid === 'string' ? header.kid : undefined;
  const candidates = keys.filter((key) => {
    if (kid !== undefined && key.kid !== kid) return false;
    if (key.use && key.use !== 'sig') return false;
    if (key.alg && key.alg !== alg) return false;
    if (key.key_ops && !key.key_ops.includes('verify')) return false;
    if (alg.startsWith('RS') && key.kty !== 'RSA') return false;
    if (alg === 'ES256' && (key.kty !== 'EC' || key.crv !== 'P-256')) return false;
    return true;
  });

  for (const jwk of candidates) {
    if (verifyWithJwk(signingInput, signature, alg, jwk)) {
      return payload;
    }
  }
  throw new Error('jwt signature verification failed');
}

/**
 * @param {string} signingInput
 * @param {Buffer} signature
 * @param {string} alg
 * @param {Jwk} jwk
 */
export function verifyWithJwk(signingInput, signature, alg, jwk) {
  try {
    const keyObject = createPublicKey({ key: jwk, format: 'jwk' });
    const verifier = createVerify(alg === 'ES256' ? 'SHA256' : 'RSA-SHA256');
    verifier.update(signingInput);
    verifier.end();
    if (alg === 'ES256') {
      if (signature.length !== 64) return false;
      // node:crypto expects DER for ECDSA; jose-style JWTs use raw r||s.
      const der = joseEs256ToDer(signature);
      return verifier.verify(keyObject, der);
    }
    return verifier.verify(keyObject, signature);
  } catch {
    return false;
  }
}

/**
 * @param {string} uri
 * @returns {Promise<Jwk[]>}
 */
async function fetchJwksCached(uri) {
  const cached = jwksCache.get(uri);
  const now = Date.now();
  if (cached && now - cached.fetchedAt < JWKS_TTL_MS) {
    return cached.keys;
  }
  const response = await fetch(uri, {
    headers: { accept: 'application/json' },
  });
  if (!response.ok) {
    throw new Error(`jwks fetch failed: ${response.status}`);
  }
  const body = /** @type {{ keys?: Jwk[] }} */ (await response.json());
  const keys = Array.isArray(body.keys) ? body.keys : [];
  jwksCache.set(uri, { fetchedAt: now, keys });
  return keys;
}

/** Clear JWKS cache (tests). */
export function clearJwksCache() {
  jwksCache.clear();
}

/**
 * Convert JOSE raw (r||s) ECDSA P-256 signature to DER.
 * @param {Buffer} raw
 */
function joseEs256ToDer(raw) {
  if (raw.length !== 64) {
    return raw;
  }
  const r = encodeDerInt(raw.subarray(0, 32));
  const s = encodeDerInt(raw.subarray(32, 64));
  const len = r.length + s.length;
  return Buffer.concat([Buffer.from([0x30, len]), r, s]);
}

/**
 * @param {Buffer} value
 */
function encodeDerInt(value) {
  let v = Buffer.from(value);
  while (v.length > 1 && v[0] === 0x00 && (v[1] & 0x80) === 0) {
    v = v.subarray(1);
  }
  if (v[0] & 0x80) {
    v = Buffer.concat([Buffer.from([0x00]), v]);
  }
  return Buffer.concat([Buffer.from([0x02, v.length]), v]);
}

/**
 * @param {string} value
 */
function base64UrlToUtf8(value) {
  return base64UrlToBuffer(value).toString('utf8');
}

/**
 * @param {string} value
 */
function base64UrlToBuffer(value) {
  if (!/^[A-Za-z0-9_-]+$/.test(value) || value.length % 4 === 1) {
    throw new Error('jwt segments must use base64url encoding');
  }
  const padded = value.replace(/-/g, '+').replace(/_/g, '/') + '==='.slice((value.length + 3) % 4);
  return Buffer.from(padded, 'base64');
}
