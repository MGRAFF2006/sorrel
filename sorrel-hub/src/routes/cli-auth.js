/**
 * CLI credential routes under `/auth/cli/`.
 *
 *   GET    /auth/cli/tokens           list the caller's CLI tokens
 *   DELETE /auth/cli/tokens           revoke all of the caller's active tokens
 *   DELETE /auth/cli/tokens/current   revoke the CLI token used for this request
 *   DELETE /auth/cli/tokens/:id       revoke one of the caller's tokens
 *
 * Owners are identified only by the verified session principal. CLI-token
 * sessions may only revoke themselves; managing other tokens needs a browser
 * or identity-provider session.
 */

import { decodePathComponent, HttpError, sendJson, sendMethodNotAllowed } from '../http.js';
import { refuseCliCredential } from '../auth/cli-tokens.js';

export function isCliAuthPath(pathname) {
  return pathname === '/auth/cli' || pathname.startsWith('/auth/cli/');
}

export async function handleCliAuthRoute(request, response, context) {
  const { url, session, cliTokens } = context;
  const segments = url.pathname.split('/').filter(Boolean).slice(2);
  if (segments[0] !== 'tokens' || segments.length > 2) {
    throw new HttpError(404, 'CLI auth route not found', 'not_found');
  }
  if (!session) {
    throw new HttpError(401, 'a verified Hub session is required', 'authentication_required');
  }

  if (segments.length === 1) {
    if (request.method === 'GET') {
      refuseCliCredential(session, 'CLI tokens cannot list tokens; use the Hub web UI');
      return sendJson(response, 200, { data: cliTokens.list(session.principal) });
    }
    if (request.method === 'DELETE') {
      refuseCliCredential(session, 'CLI tokens cannot revoke other tokens; use sorrel logout or the Hub web UI');
      return sendJson(response, 200, { data: { revoked: cliTokens.revokeAll(session.principal) } });
    }
    return sendMethodNotAllowed(response, ['GET', 'DELETE']);
  }

  if (request.method !== 'DELETE') return sendMethodNotAllowed(response, ['DELETE']);
  const id = decodePathComponent(segments[1]);
  if (id === 'current') {
    if (session.credential?.kind !== 'cli') {
      throw new HttpError(400, 'this request was not authenticated with a CLI token', 'not_cli_token');
    }
    return sendJson(response, 200, { data: cliTokens.revoke(session.principal, session.credential.id, 'logout') });
  }
  refuseCliCredential(session, 'CLI tokens cannot revoke other tokens; use sorrel logout or the Hub web UI');
  const revoked = cliTokens.revoke(session.principal, id);
  // Unknown and foreign tokens look the same so ids cannot be probed.
  if (!revoked) throw new HttpError(404, 'CLI token not found', 'not_found');
  return sendJson(response, 200, { data: revoked });
}
