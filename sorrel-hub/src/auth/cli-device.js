/**
 * RFC 8628-style device authorization for `sorrel login`.
 *
 *   POST /auth/cli/device   (anonymous, rate-limited)  start a login
 *   POST /auth/cli/token    (anonymous, polled)        exchange an approved code
 *   POST /auth/cli/approve  (cookie session, same origin) approve or deny a code
 *
 * Pending requests live in memory on this Hub instance, bounded in number and
 * lifetime, like pending browser logins. Codes work once. The approving
 * principal is always the verified cookie session's principal; the request body
 * can only name the user code and the decision.
 */

import { createHash, randomBytes } from 'node:crypto';

import { HttpError, readJsonBody, sendJson } from '../http.js';
import { auditLog, normalizeCliTokenLabel } from './cli-tokens.js';

export const DEVICE_GRANT_TYPE = 'urn:ietf:params:oauth:grant-type:device_code';
const USER_CODE_ALPHABET = 'BCDFGHJKLMNPQRSTVWXZ'; // no vowels or look-alikes
const USER_CODE_PATTERN = /^[BCDFGHJKLMNPQRSTVWXZ]{4}-[BCDFGHJKLMNPQRSTVWXZ]{4}$/;
export const DEVICE_CODE_TTL_MS = 10 * 60 * 1000;
export const DEVICE_POLL_INTERVAL_S = 5;
export const MAX_PENDING_DEVICE_CODES = 1000;
const START_LIMIT = { max: 10, windowMs: 60 * 1000 };
const APPROVE_FAILURE_LIMIT = { max: 10, windowMs: 10 * 60 * 1000 };
const BODY_LIMIT_BYTES = 4096;

const sha256 = value => createHash('sha256').update(value).digest('hex');

/** RFC 8628 §6.1 user codes: 8 characters from a 20-letter alphabet, XXXX-XXXX. */
function generateUserCode() {
  const bytes = randomBytes(8);
  let code = '';
  // 256 % 20 = 16, so a byte below 240 maps uniformly onto the alphabet.
  for (let i = 0; code.length < 8; i += 1) {
    const byte = i < bytes.length ? bytes[i] : randomBytes(1)[0];
    if (byte < 240) code += USER_CODE_ALPHABET[byte % 20];
  }
  return `${code.slice(0, 4)}-${code.slice(4)}`;
}

/** Accept lower case, spaces and a missing dash when users type the code. */
export function normalizeUserCode(value) {
  if (typeof value !== 'string' || value.length > 32) return null;
  const compact = value.toUpperCase().replace(/[\s-]/g, '');
  const code = `${compact.slice(0, 4)}-${compact.slice(4)}`;
  return USER_CODE_PATTERN.test(code) ? code : null;
}

/** Small fixed-window counter keyed by caller; the map is bounded. */
function createLimiter({ max, windowMs }, now) {
  const windows = new Map();
  return {
    hit(key) {
      const at = now();
      let entry = windows.get(key);
      if (!entry || entry.resetAt <= at) {
        if (windows.size >= 10_000) for (const [k, v] of windows) if (v.resetAt <= at) windows.delete(k);
        if (windows.size >= 10_000) windows.clear();
        entry = { count: 0, resetAt: at + windowMs };
        windows.set(key, entry);
      }
      entry.count += 1;
      return entry.count <= max ? 0 : Math.ceil((entry.resetAt - at) / 1000);
    },
    blocked(key) {
      const entry = windows.get(key);
      return Boolean(entry && entry.resetAt > now() && entry.count >= max);
    },
  };
}

function sendOAuthError(response, statusCode, error, description, headers = {}) {
  sendJson(response, statusCode, { error, error_description: description }, headers);
}

async function readOAuthBody(request, response) {
  try {
    return await readJsonBody(request, BODY_LIMIT_BYTES);
  } catch (error) {
    if (!(error instanceof HttpError)) throw error;
    sendOAuthError(response, error.statusCode === 413 ? 413 : 400, 'invalid_request', error.message);
    return null;
  }
}

/**
 * @param {{
 *   cliTokens: ReturnType<typeof import('./cli-tokens.js').createCliTokens>,
 *   browserAuth: { origin: string, requireSameOrigin: (request: import('node:http').IncomingMessage) => void },
 *   now?: () => number,
 *   audit?: typeof auditLog,
 * }} options
 */
export function createCliDeviceFlow({ cliTokens, browserAuth, now = Date.now, audit = auditLog }) {
  const byUserCode = new Map();
  const byDeviceHash = new Map();
  const startLimiter = createLimiter(START_LIMIT, now);
  const approveFailures = createLimiter(APPROVE_FAILURE_LIMIT, now);
  const verificationUri = `${browserAuth.origin}/cli/activate`;

  function forget(entry) {
    byUserCode.delete(entry.userCode);
    byDeviceHash.delete(entry.deviceHash);
  }

  function sweep() {
    const at = now();
    for (const entry of byUserCode.values()) if (entry.expiresAt <= at) forget(entry);
  }

  async function start(request, response) {
    const caller = request.socket?.remoteAddress ?? 'unknown';
    const retryAfter = startLimiter.hit(caller);
    if (retryAfter) {
      return sendOAuthError(response, 429, 'slow_down', 'Too many login attempts; try again shortly', { 'retry-after': String(retryAfter) });
    }
    const body = await readOAuthBody(request, response);
    if (!body) return;
    let label;
    try { label = normalizeCliTokenLabel(body.label); }
    catch (error) { return sendOAuthError(response, 400, 'invalid_request', error.message); }

    sweep();
    if (byUserCode.size >= MAX_PENDING_DEVICE_CODES) {
      return sendOAuthError(response, 429, 'slow_down', 'Sign-in is busy; try again shortly', { 'retry-after': '30' });
    }
    let userCode;
    do userCode = generateUserCode(); while (byUserCode.has(userCode));
    const deviceCode = randomBytes(32).toString('base64url');
    const at = now();
    const entry = {
      requestId: randomBytes(8).toString('hex'),
      userCode,
      deviceHash: sha256(deviceCode),
      label,
      expiresAt: at + DEVICE_CODE_TTL_MS,
      interval: DEVICE_POLL_INTERVAL_S,
      lastPollAt: 0,
      state: 'pending',
      approval: null,
    };
    byUserCode.set(userCode, entry);
    byDeviceHash.set(entry.deviceHash, entry);
    return sendJson(response, 200, {
      device_code: deviceCode,
      user_code: userCode,
      verification_uri: verificationUri,
      verification_uri_complete: `${verificationUri}?code=${userCode}`,
      expires_in: DEVICE_CODE_TTL_MS / 1000,
      interval: DEVICE_POLL_INTERVAL_S,
    });
  }

  async function exchange(request, response) {
    const body = await readOAuthBody(request, response);
    if (!body) return;
    if (body.grant_type !== DEVICE_GRANT_TYPE) {
      return sendOAuthError(response, 400, 'unsupported_grant_type', `grant_type must be ${DEVICE_GRANT_TYPE}`);
    }
    if (typeof body.device_code !== 'string' || !body.device_code || body.device_code.length > 128) {
      return sendOAuthError(response, 400, 'invalid_request', 'device_code is required');
    }
    const entry = byDeviceHash.get(sha256(body.device_code));
    if (!entry) return sendOAuthError(response, 400, 'invalid_grant', 'Unknown or already used device code; run sorrel login again');
    const at = now();
    if (entry.expiresAt <= at) {
      forget(entry);
      return sendOAuthError(response, 400, 'expired_token', 'The login code expired; run sorrel login again');
    }
    if (entry.state === 'denied') {
      forget(entry);
      return sendOAuthError(response, 400, 'access_denied', 'The login request was denied');
    }
    if (entry.state === 'pending') {
      const tooFast = at - entry.lastPollAt < entry.interval * 1000;
      entry.lastPollAt = at;
      if (tooFast) {
        entry.interval += 5;
        return sendOAuthError(response, 400, 'slow_down', `Poll at most every ${entry.interval} seconds`, {});
      }
      return sendOAuthError(response, 400, 'authorization_pending', 'Waiting for approval in the browser');
    }
    // Approved: the device code is single-use whatever happens next.
    forget(entry);
    let issued;
    try {
      issued = cliTokens.issue({ ...entry.approval, label: entry.label, requestId: entry.requestId });
    } catch (error) {
      if (error instanceof HttpError) return sendOAuthError(response, 400, 'access_denied', error.message);
      throw error;
    }
    return sendJson(response, 200, {
      access_token: issued.token,
      token_type: 'Bearer',
      expires_in: Math.max(0, Math.floor((Date.parse(issued.record.expiresAt) - now()) / 1000)),
      token_id: issued.record.id,
      label: issued.record.label,
      principal: issued.record.principal,
      expires_at: issued.record.expiresAt,
    });
  }

  async function decide(request, response, session) {
    if (session?.transport !== 'cookie') {
      throw new HttpError(401, 'Sign in to Sorrel Hub in this browser to approve a CLI login', 'authentication_required');
    }
    browserAuth.requireSameOrigin(request);
    const principalKey = `${session.principal.type}:${session.principal.id}`;
    if (approveFailures.blocked(principalKey)) {
      throw new HttpError(429, 'Too many invalid codes; wait a few minutes and try again', 'rate_limited');
    }
    const body = await readJsonBody(request, BODY_LIMIT_BYTES);
    // Only the code and decision come from the client; identity comes from the session.
    if (Object.keys(body).some(key => key !== 'user_code' && key !== 'decision')) {
      throw new HttpError(400, 'only user_code and decision may be supplied', 'invalid_request_body');
    }
    if (body.decision !== 'approve' && body.decision !== 'deny') {
      throw new HttpError(400, 'decision must be approve or deny', 'invalid_request_body');
    }
    const userCode = normalizeUserCode(body.user_code);
    const entry = userCode ? byUserCode.get(userCode) : undefined;
    if (!entry || entry.state !== 'pending' || entry.expiresAt <= now()) {
      approveFailures.hit(principalKey);
      throw new HttpError(404, 'This code is invalid, expired or already used; run sorrel login again', 'invalid_user_code');
    }
    if (body.decision === 'deny') {
      entry.state = 'denied';
      audit('cli_device.denied', { requestId: entry.requestId, principal: principalKey, label: entry.label });
      return sendJson(response, 200, { data: { status: 'denied', label: entry.label } });
    }
    entry.state = 'approved';
    entry.approval = { principal: { ...session.principal }, authMode: session.authMode, idpSubject: session.idpSubject };
    audit('cli_device.approved', { requestId: entry.requestId, principal: principalKey, label: entry.label });
    return sendJson(response, 200, { data: { status: 'approved', label: entry.label } });
  }

  return {
    verificationUri,
    get pendingCount() { sweep(); return byUserCode.size; },
    async handle(request, response, action, session) {
      if (request.method !== 'POST') {
        response.setHeader('allow', 'POST');
        throw new HttpError(405, 'method must be POST', 'method_not_allowed');
      }
      if (action === 'device') return await start(request, response);
      if (action === 'token') return await exchange(request, response);
      return await decide(request, response, session);
    },
  };
}
