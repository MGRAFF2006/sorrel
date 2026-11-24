/**
 * Hub-issued CLI credentials (`srl_cli_<id>_<secret>`).
 *
 * A CLI token authenticates the principal copied from the verified session
 * that approved it. It never grants access by itself: every request is still
 * authorized by native Core grants, and the token's scopes only *narrow* which
 * Core actions may be attempted with it. Records store a SHA-256 hash of the
 * secret, never the secret itself.
 */

import { createHash, randomBytes, timingSafeEqual } from 'node:crypto';

import { POLICY_ACTION_GRANT, PolicyDeniedError } from '../core-policy.js';
import { HttpError } from '../http.js';

export const CLI_TOKEN_PREFIX = 'srl_cli_';
const TOKEN_PATTERN = /^srl_cli_([0-9a-f]{24})_([A-Za-z0-9_-]{43})$/;
const DAY_MS = 24 * 60 * 60 * 1000;
export const CLI_TOKEN_DEFAULT_TTL_DAYS = 30;
export const CLI_TOKEN_MAX_TTL_DAYS = 90;
const LAST_USED_RESOLUTION_MS = 60 * 60 * 1000;
export const CLI_TOKEN_MAX_ACTIVE_PER_PRINCIPAL = 50;
const LABEL_MAX_LENGTH = 100;
const DEFAULT_LABEL = 'Sorrel CLI';

/**
 * Core actions a CLI token may attempt (v1). Core grants still decide.
 * Deliberately excluded: `policy.grant` (admin), `org.write`, `project.write`,
 * `proposal.review`, `workflow.run.write`, and proposal integration mutations.
 */
export const CLI_TOKEN_SCOPES = Object.freeze([
  'repo.read', 'repo.object.write', 'repo.ref.write',
  'org.read', 'policy.read',
  'project.read', 'project.create',
  'proposal.read', 'proposal.write',
  'review.comment.read', 'review.comment.write',
  'workflow.run.read',
]);
/** Never allowed for a CLI token, whatever a stored record claims. */
const FORBIDDEN_SCOPES = new Set([POLICY_ACTION_GRANT]);

export class CredentialScopeError extends PolicyDeniedError {
  constructor(message = 'CLI tokens cannot perform this action; use the Hub web UI') {
    super(message, undefined);
    this.name = 'CredentialScopeError';
    this.code = 'credential_scope_denied';
  }
}

/** Throw when a CLI-token session attempts an action outside its scopes. */
export function assertCredentialScope(session, action) {
  const credential = session?.credential;
  if (!credential) return;
  if (FORBIDDEN_SCOPES.has(action) || !Array.isArray(credential.scopes) || !credential.scopes.includes(action)) {
    throw new CredentialScopeError();
  }
}

/** Refuse an operation that CLI tokens may never perform (v1). */
export function refuseCliCredential(session, message) {
  if (session?.credential) throw new CredentialScopeError(message);
}

export function isCliTokenBearer(authorization) {
  const raw = Array.isArray(authorization) ? authorization[0] : authorization;
  return typeof raw === 'string' && raw.startsWith(`Bearer ${CLI_TOKEN_PREFIX}`);
}

/** Absolute token lifetime: 30 days by default, operator cap of 90 days. */
export function resolveCliTokenTtlMs(env = process.env) {
  const raw = env.SORREL_HUB_CLI_TOKEN_TTL_DAYS;
  if (raw === undefined || raw === '') return CLI_TOKEN_DEFAULT_TTL_DAYS * DAY_MS;
  const days = Number(raw);
  if (!Number.isInteger(days) || days < 1 || days > CLI_TOKEN_MAX_TTL_DAYS) {
    throw new Error(`SORREL_HUB_CLI_TOKEN_TTL_DAYS must be an integer from 1 to ${CLI_TOKEN_MAX_TTL_DAYS}`);
  }
  return days * DAY_MS;
}

/** Structured audit line. Callers must never pass a token or secret. */
export function auditLog(event, fields = {}) {
  console.info(`[sorrel-hub] audit ${JSON.stringify({ event, ...fields })}`);
}

export function normalizeCliTokenLabel(value) {
  if (value === undefined || value === null) return DEFAULT_LABEL;
  if (typeof value !== 'string') throw new HttpError(400, 'label must be a string', 'invalid_request_body');
  const label = value.trim();
  // Reject control characters so labels stay safe to display and log.
  if (!label || label.length > LABEL_MAX_LENGTH || /\p{Cc}/u.test(label)) {
    throw new HttpError(400, `label must be 1-${LABEL_MAX_LENGTH} printable characters`, 'invalid_request_body');
  }
  return label;
}

const sha256 = value => createHash('sha256').update(value).digest();
const samePrincipal = (a, b) => Boolean(a && b) && a.type === b.type && a.id === b.id;
const principalRef = principal => `${principal.type}:${principal.id}`;
const validPrincipal = principal => principal && typeof principal === 'object'
  && typeof principal.type === 'string' && principal.type !== ''
  && typeof principal.id === 'string' && principal.id !== '';

function status(record, now) {
  if (record.revokedAt) return 'revoked';
  if (Date.parse(record.expiresAt) <= now) return 'expired';
  return 'active';
}

/** Owner-facing view: everything except the secret hash. */
function publicView(record, now) {
  const { secretHash: _secretHash, ...view } = record;
  return { ...view, status: status(record, now) };
}

/**
 * @param {import('../store.js').InMemoryStore} store
 * @param {{ env?: NodeJS.ProcessEnv, now?: () => number, audit?: typeof auditLog }} [options]
 */
export function createCliTokens(store, options = {}) {
  const ttlMs = resolveCliTokenTtlMs(options.env ?? process.env);
  const now = options.now ?? Date.now;
  const audit = options.audit ?? auditLog;
  const records = () => store.cliCredentials;

  function find(id) {
    return records().get(id) ?? null;
  }

  return {
    ttlMs,

    /**
     * Issue a token for a principal taken from a verified session. Returns the
     * plaintext token exactly once together with its public record.
     */
    issue({ principal, authMode, idpSubject, label, requestId } = {}) {
      if (!validPrincipal(principal)) throw new TypeError('a verified principal is required to issue a CLI token');
      const at = now();
      const active = [...records().values()].filter(record => samePrincipal(record.principal, principal) && status(record, at) === 'active');
      if (active.length >= CLI_TOKEN_MAX_ACTIVE_PER_PRINCIPAL) {
        throw new HttpError(429, `at most ${CLI_TOKEN_MAX_ACTIVE_PER_PRINCIPAL} active CLI tokens are allowed; revoke unused tokens first`, 'cli_token_limit');
      }
      let id;
      do id = randomBytes(12).toString('hex'); while (records().has(id));
      const secret = randomBytes(32).toString('base64url');
      const record = {
        id,
        secretHash: sha256(secret).toString('hex'),
        principal: { type: principal.type, id: principal.id },
        authMode: typeof authMode === 'string' ? authMode : null,
        idpSubject: typeof idpSubject === 'string' ? idpSubject : null,
        label: normalizeCliTokenLabel(label),
        scopes: [...CLI_TOKEN_SCOPES],
        createdAt: new Date(at).toISOString(),
        expiresAt: new Date(at + ttlMs).toISOString(),
        lastUsedAt: null,
        revokedAt: null,
      };
      store.storeRecord('cliCredentials', record);
      audit('cli_token.issued', { tokenId: id, principal: principalRef(record.principal), label: record.label, expiresAt: record.expiresAt,
        ...(typeof requestId === 'string' ? { requestId } : {}) });
      return { token: `${CLI_TOKEN_PREFIX}${id}_${secret}`, record: publicView(record, at) };
    },

    /**
     * Resolve an `Authorization: Bearer srl_cli_...` header to a Hub session.
     * Synchronous between read and write so a concurrent revoke cannot be lost.
     * Unknown, malformed, expired or revoked tokens resolve to null (401).
     */
    resolve(authorization) {
      const raw = Array.isArray(authorization) ? authorization[0] : authorization;
      const match = typeof raw === 'string' ? TOKEN_PATTERN.exec(raw.slice('Bearer '.length).trim()) : null;
      if (!match) return null;
      const [, id, secret] = match;
      const record = find(id);
      const presented = sha256(secret);
      const stored = record && typeof record.secretHash === 'string' && /^[0-9a-f]{64}$/.test(record.secretHash)
        ? Buffer.from(record.secretHash, 'hex') : Buffer.alloc(32);
      // Compare even for unknown ids so lookups and mismatches cost the same.
      if (!timingSafeEqual(presented, stored) || !record) return null;
      const at = now();
      if (status(record, at) !== 'active' || !validPrincipal(record.principal)) return null;

      const lastUsed = record.lastUsedAt ? Date.parse(record.lastUsedAt) : NaN;
      let current = record;
      if (!(at - lastUsed < LAST_USED_RESOLUTION_MS)) {
        current = { ...record, lastUsedAt: new Date(at).toISOString() };
        try {
          store.storeRecord('cliCredentials', current);
          if (!record.lastUsedAt) audit('cli_token.first_use', { tokenId: id, principal: principalRef(record.principal) });
        } catch (error) {
          // Usage timestamps are best effort; authentication itself succeeded.
          console.warn('[sorrel-hub] could not record CLI token use', { code: typeof error?.code === 'string' ? error.code : 'unknown' });
        }
      }
      const scopes = Array.isArray(current.scopes) ? current.scopes.filter(scope => typeof scope === 'string' && !FORBIDDEN_SCOPES.has(scope)) : [];
      return {
        principal: { type: current.principal.type, id: current.principal.id },
        sessionId: `cli:${id}`,
        authMode: current.authMode ?? 'cli',
        idpSubject: current.idpSubject ?? undefined,
        expiresAt: Date.parse(current.expiresAt),
        transport: 'bearer',
        credential: { kind: 'cli', id, label: current.label, scopes, expiresAt: current.expiresAt },
      };
    },

    /** Tokens owned by a principal, newest first, without secret hashes. */
    list(principal) {
      const at = now();
      return [...records().values()]
        .filter(record => samePrincipal(record.principal, principal))
        .sort((a, b) => (a.createdAt < b.createdAt ? 1 : a.createdAt > b.createdAt ? -1 : 0))
        .map(record => publicView(record, at));
    },

    /** Revoke one token owned by `principal`; null when not found or not owned. */
    revoke(principal, id, reason = 'owner') {
      const record = typeof id === 'string' ? find(id) : null;
      if (!record || !samePrincipal(record.principal, principal)) return null;
      const at = now();
      if (record.revokedAt) return publicView(record, at);
      const revoked = { ...record, revokedAt: new Date(at).toISOString() };
      store.storeRecord('cliCredentials', revoked);
      audit('cli_token.revoked', { tokenId: id, principal: principalRef(record.principal), reason });
      return publicView(revoked, at);
    },

    /** Revoke every active token owned by `principal`; returns the count. */
    revokeAll(principal) {
      const at = now();
      let count = 0;
      for (const record of [...records().values()]) {
        if (samePrincipal(record.principal, principal) && status(record, at) === 'active') {
          this.revoke(principal, record.id, 'owner_all');
          count += 1;
        }
      }
      return count;
    },
  };
}
