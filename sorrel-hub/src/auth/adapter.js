/**
 * AuthAdapter — identity in, Hub session + Core Principal out.
 *
 * Authorization stays in Core grants/policy. Adapters must never invent a
 * second RBAC language. Auth stays off the blob/diff hot path: verify once,
 * attach a Hub session, then evaluate Core policy on mutate paths.
 */

import { createHash, randomUUID } from 'node:crypto';

import { verifyOidcAccessToken } from './oidc-jwt.js';

/**
 * @typedef {{ type: string, id: string }} Principal
 * @typedef {{
 *   principal: Principal,
 *   sessionId: string,
 *   authMode: 'dev' | 'workos' | 'oidc',
 *   idpSubject?: string,
 *   expiresAt?: number,
 * }} HubSession
 * @typedef {{
 *   mode: 'dev' | 'workos' | 'oidc',
 *   resolveSession: (request: import('node:http').IncomingMessage) => Promise<HubSession | null>,
 *   mapPrincipal: (identity: { subject: string, email?: string, orgId?: string }) => Principal,
 * }} AuthAdapter
 */

/**
 * Dev-only adapter: trusts `x-sorrel-acting-principal` (current alpha behavior).
 * Never enable in production network exposure.
 *
 * @returns {AuthAdapter}
 */
export function createDevActingPrincipalAdapter() {
  return {
    mode: 'dev',
    async resolveSession(request) {
      const rawHeader = request.headers['x-sorrel-acting-principal'];
      const rawValue = Array.isArray(rawHeader) ? rawHeader[0] : rawHeader;
      if (!rawValue) {
        return null;
      }
      try {
        const principal = JSON.parse(rawValue);
        if (
          !principal ||
          typeof principal !== 'object' ||
          typeof principal.type !== 'string' ||
          typeof principal.id !== 'string'
        ) {
          return null;
        }
        return {
          principal,
          sessionId: `dev:${principal.type}:${principal.id}`,
          authMode: 'dev',
          idpSubject: `${principal.type}:${principal.id}`,
        };
      } catch {
        return null;
      }
    },
    mapPrincipal(identity) {
      return { type: 'user', id: identity.subject };
    },
  };
}

/**
 * WorkOS SaaS adapter skeleton — org SSO/SAML + directory sync.
 * Session verification happens once; do not round-trip WorkOS per diff fetch.
 *
 * Accepts Bearer access tokens that look like JWTs and maps `sub` when present.
 * Full WorkOS session/cookie verification lands with sealed-session support.
 *
 * @param {{ apiKey?: string, clientId?: string, issuer?: string, audience?: string }} [options]
 * @returns {AuthAdapter}
 */
export function createWorkOsAdapter(options = {}) {
  const configured = Boolean(options.apiKey && options.clientId);
  const issuer = options.issuer ?? 'https://api.workos.com';
  return {
    mode: 'workos',
    async resolveSession(request) {
      if (!configured) {
        return null;
      }
      const bearer = readBearer(request);
      if (!bearer) {
        return null;
      }
      try {
        const payload = await verifyOidcAccessToken(bearer, {
          issuer,
          audience: options.audience ?? options.clientId,
        });
        const subject = String(payload.sub ?? '');
        if (!subject) {
          return null;
        }
        const principal = {
          type: 'user',
          id: `workos:${subject}`,
        };
        return {
          principal,
          sessionId: `workos:${subject}`,
          authMode: 'workos',
          idpSubject: subject,
          expiresAt: typeof payload.exp === 'number' ? payload.exp * 1000 : undefined,
        };
      } catch {
        return null;
      }
    },
    mapPrincipal(identity) {
      return { type: 'user', id: `workos:${identity.subject}` };
    },
  };
}

/**
 * Self-host OIDC adapter (Authentik / Keycloak / Dex / GitHub / invite).
 * Verifies Bearer JWTs against the issuer JWKS (RS256 / ES256).
 *
 * @param {{
 *   issuer?: string,
 *   audience?: string,
 *   fetchJwks?: (uri: string) => Promise<import('./oidc-jwt.js').Jwk[]>,
 * }} [options]
 * @returns {AuthAdapter}
 */
export function createOidcAdapter(options = {}) {
  const configured = Boolean(options.issuer);
  return {
    mode: 'oidc',
    async resolveSession(request) {
      if (!configured || !options.issuer) {
        return null;
      }
      const bearer = readBearer(request);
      if (!bearer) {
        return null;
      }
      try {
        const payload = await verifyOidcAccessToken(bearer, {
          issuer: options.issuer,
          audience: options.audience,
          fetchJwks: options.fetchJwks,
        });
        const subject = String(payload.sub ?? '');
        if (!subject) {
          return null;
        }
        const principal = {
          type: 'user',
          id: `oidc:${subject}`,
        };
        return {
          principal,
          sessionId: sessionIdFor('oidc', subject),
          authMode: 'oidc',
          idpSubject: subject,
          expiresAt: typeof payload.exp === 'number' ? payload.exp * 1000 : undefined,
        };
      } catch {
        return null;
      }
    },
    mapPrincipal(identity) {
      return { type: 'user', id: `oidc:${identity.subject}` };
    },
  };
}

/**
 * Select AuthAdapter from environment.
 *
 *   SORREL_HUB_AUTH=dev|workos|oidc  (default: dev)
 *
 * @param {NodeJS.ProcessEnv} [env]
 * @returns {AuthAdapter}
 */
export function createAuthAdapterFromEnv(env = process.env) {
  const mode = (env.SORREL_HUB_AUTH ?? 'dev').toLowerCase();
  if (mode === 'workos') {
    return createWorkOsAdapter({
      apiKey: env.WORKOS_API_KEY,
      clientId: env.WORKOS_CLIENT_ID,
      issuer: env.WORKOS_ISSUER,
      audience: env.WORKOS_AUDIENCE ?? env.WORKOS_CLIENT_ID,
    });
  }
  if (mode === 'oidc') {
    return createOidcAdapter({
      issuer: env.SORREL_OIDC_ISSUER,
      audience: env.SORREL_OIDC_AUDIENCE,
    });
  }
  return createDevActingPrincipalAdapter();
}

/**
 * @param {import('node:http').IncomingMessage} request
 * @returns {string | null}
 */
function readBearer(request) {
  const auth = request.headers.authorization;
  const raw = Array.isArray(auth) ? auth[0] : auth;
  if (!raw?.startsWith('Bearer ')) {
    return null;
  }
  const token = raw.slice('Bearer '.length).trim();
  return token || null;
}

/**
 * @param {string} mode
 * @param {string} subject
 */
function sessionIdFor(mode, subject) {
  const digest = createHash('sha256').update(`${mode}:${subject}`).digest('hex').slice(0, 16);
  return `${mode}:${digest}:${randomUUID().slice(0, 8)}`;
}
