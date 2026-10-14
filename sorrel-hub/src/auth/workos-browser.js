import { randomBytes } from 'node:crypto';
import { WorkOS } from '@workos-inc/node';
import { HttpError, sendJson } from '../http.js';
import { decodeJwt } from './oidc-jwt.js';

const SESSION_COOKIE = '__Host-sorrel-session';
const LOGIN_COOKIE = '__Host-sorrel-login';
const LOGIN_TTL_MS = 10 * 60 * 1000;
const SESSION_MAX_AGE = 7 * 24 * 60 * 60;

function cookie(request, name) {
  const matches = String(request.headers.cookie ?? '').split(';')
    .map(value => value.trim()).filter(value => value.startsWith(`${name}=`));
  return matches.length === 1 ? matches[0].slice(name.length + 1) : null;
}

function setCookie(response, name, value, maxAge) {
  if (value.length > 3800 || /[;\r\n]/.test(value)) throw new HttpError(502, 'Identity provider returned an invalid session', 'auth_unavailable');
  const prior = response.getHeader('set-cookie') ?? [];
  response.setHeader('set-cookie', [...(Array.isArray(prior) ? prior : [prior]),
    `${name}=${value}; Path=/; HttpOnly; Secure; SameSite=Lax; Max-Age=${maxAge}`]);
}

/** WorkOS owns session sealing, JWT verification and refresh-token rotation. */
export function createWorkOsBrowserAuth(env = process.env, options = {}) {
  if (env.SORREL_HUB_AUTH !== 'workos') return null;
  const configured = env.SORREL_HUB_PUBLIC_URL || env.WORKOS_COOKIE_PASSWORD;
  if (!configured && env.SORREL_HUB_DEPLOY !== 'saas') return null;
  if (!env.WORKOS_API_KEY || !env.WORKOS_CLIENT_ID || !env.WORKOS_COOKIE_PASSWORD || env.WORKOS_COOKIE_PASSWORD.length < 32) {
    throw new Error('WorkOS browser login requires WORKOS_API_KEY, WORKOS_CLIENT_ID and WORKOS_COOKIE_PASSWORD (at least 32 characters)');
  }
  let origin;
  try {
    const publicUrl = new URL(env.SORREL_HUB_PUBLIC_URL);
    if (publicUrl.protocol !== 'https:' || publicUrl.username || publicUrl.password || publicUrl.pathname !== '/' || publicUrl.search || publicUrl.hash) throw new Error();
    origin = publicUrl.origin;
  } catch {
    throw new Error('SORREL_HUB_PUBLIC_URL must be an HTTPS origin without a path, query or credentials');
  }
  const workos = options.workos ?? new WorkOS(env.WORKOS_API_KEY, {
    clientId: env.WORKOS_CLIENT_ID,
    timeout: 5_000, maxRetries: 1,
    issuer: [(env.WORKOS_ISSUER || 'https://api.workos.com').replace(/\/$/, ''), `${(env.WORKOS_ISSUER || 'https://api.workos.com').replace(/\/$/, '')}/`],
  });
  const password = env.WORKOS_COOKIE_PASSWORD;
  const callback = `${origin}/api/auth/callback`;
  // ponytail: one API instance owns pending logins; restart means retry login.
  // Bound the map; a shared transaction store is needed before multi-instance hosting.
  const pending = new Map();

  function requireSameOrigin(request) {
    if (request.headers.origin !== origin || request.headers['sec-fetch-site'] === 'cross-site') {
      throw new HttpError(403, 'Use Sorrel Hub from its configured origin', 'invalid_origin');
    }
  }

  function load(request) {
    const data = cookie(request, SESSION_COOKIE);
    if (!data || data.length > 3800) return null;
    return workos.userManagement.loadSealedSession({ sessionData: data, cookiePassword: password });
  }

  async function authenticate(request, response) {
    const session = load(request);
    if (!session) return null;
    if (!['GET', 'HEAD', 'OPTIONS'].includes(request.method)) requireSameOrigin(request);
    let result;
    let refreshedSession;
    try {
      result = await session.authenticate();
      if (!result.authenticated && result.reason === 'invalid_jwt') {
        result = await session.refresh();
        if (result.authenticated) {
          refreshedSession = result.sealedSession;
          // SDK refresh seals the response; authenticate verifies the new JWT.
          result = await session.authenticate();
        }
        else if (result.retryable) throw new Error('retryable refresh');
      }
    } catch {
      throw new HttpError(503, 'Sign-in service is temporarily unavailable; retry shortly', 'auth_unavailable');
    }
    if (!result.authenticated) {
      setCookie(response, SESSION_COOKIE, '', 0);
      return null;
    }
    const accessToken = result.accessToken ?? result.session?.accessToken;
    const claims = decodeJwt(accessToken).payload;
    if (typeof claims.sub !== 'string' || !claims.sub.trim() || claims.sub !== result.user?.id
        || claims.client_id !== env.WORKOS_CLIENT_ID || typeof claims.sid !== 'string' || !claims.sid.trim()
        || claims.sid !== result.sessionId || !Number.isFinite(claims.exp)) {
      setCookie(response, SESSION_COOKIE, '', 0);
      return null;
    }
    if (refreshedSession) setCookie(response, SESSION_COOKIE, refreshedSession, SESSION_MAX_AGE);
    return {
      principal: { type: 'user', id: `workos:${claims.sub}` },
      sessionId: claims.sid,
      authMode: 'workos',
      idpSubject: claims.sub,
      expiresAt: typeof claims.exp === 'number' ? claims.exp * 1000 : undefined,
      transport: 'cookie',
    };
  }

  return {
    resolveSession: authenticate,
    async handleRequest(request, response, url) {
      if (!url.pathname.startsWith('/auth/')) return false;
      response.setHeader('cache-control', 'no-store');
      response.setHeader('referrer-policy', 'no-referrer');
      if (url.pathname === '/auth/login' && request.method === 'GET') {
        for (const [state, transaction] of pending) if (transaction.expires <= Date.now()) pending.delete(state);
        if (pending.size >= 1000) throw new HttpError(429, 'Sign-in is busy; try again shortly', 'auth_busy');
        const state = randomBytes(32).toString('base64url');
        const nonce = randomBytes(32).toString('base64url');
        const transaction = { nonce, verifier: null, expires: Date.now() + LOGIN_TTL_MS };
        // Reserve capacity before asynchronous PKCE generation.
        pending.set(state, transaction);
        try {
          const pkce = await workos.pkce.generate();
          transaction.verifier = pkce.codeVerifier;
          const location = workos.userManagement.getAuthorizationUrl({
            provider: 'authkit', redirectUri: callback, state,
            codeChallenge: pkce.codeChallenge, codeChallengeMethod: 'S256', screenHint: 'sign-in',
          });
          setCookie(response, LOGIN_COOKIE, nonce, LOGIN_TTL_MS / 1000);
          response.writeHead(302, { location }).end();
        } catch (error) {
          pending.delete(state);
          throw error;
        }
        return true;
      }
      if (url.pathname === '/auth/callback' && request.method === 'GET') {
        const state = url.searchParams.get('state');
        const transaction = pending.get(state);
        if (!transaction || transaction.expires <= Date.now() || cookie(request, LOGIN_COOKIE) !== transaction.nonce
            || url.searchParams.getAll('state').length !== 1) {
          throw new HttpError(400, 'Sign-in expired or did not originate in this browser; start again', 'invalid_login_state');
        }
        pending.delete(state);
        setCookie(response, LOGIN_COOKIE, '', 0);
        if (url.searchParams.has('error')) {
          response.writeHead(303, { location: `${origin}/?auth=cancelled` }).end();
          return true;
        }
        const code = url.searchParams.get('code');
        if (!code || code.length > 4096 || url.searchParams.getAll('code').length !== 1) throw new HttpError(400, 'Missing sign-in code; start again', 'invalid_login_code');
        let result;
        try {
          result = await workos.userManagement.authenticateWithCode({
            clientId: env.WORKOS_CLIENT_ID, code, codeVerifier: transaction.verifier,
            session: { sealSession: true, cookiePassword: password },
          });
        } catch {
          throw new HttpError(502, 'Could not finish sign-in; start again', 'auth_unavailable');
        }
        if (!result.sealedSession) throw new HttpError(502, 'Could not establish a session; start again', 'auth_unavailable');
        setCookie(response, SESSION_COOKIE, result.sealedSession, SESSION_MAX_AGE);
        response.writeHead(303, { location: `${origin}/` }).end();
        return true;
      }
      if (url.pathname === '/auth/logout' && request.method === 'POST') {
        requireSameOrigin(request);
        let session;
        try {
          session = await authenticate(request, response);
        } catch {
          setCookie(response, SESSION_COOKIE, '', 0);
          sendJson(response, 200, { data: { logoutUrl: `${origin}/?auth=logout-incomplete`, remoteLogout: false } });
          return true;
        }
        setCookie(response, SESSION_COOKIE, '', 0);
        const logoutUrl = session ? workos.userManagement.getLogoutUrl({ sessionId: session.sessionId, returnTo: `${origin}/` }) : `${origin}/`;
        sendJson(response, 200, { data: { logoutUrl } });
        return true;
      }
      throw new HttpError(405, 'Unsupported sign-in action', 'method_not_allowed');
    },
  };
}
