// Test-only real Hub process for `sorrel login` (device flow) tests.
//
// Everything is the real Hub (token store, device flow, Core authorization)
// except browser sign-in: the cookie `__Host-sorrel-session=<fixture>` stands in
// for a sealed WorkOS session, so tests can approve codes without an identity
// provider. Origin checks use the real browser-auth rule (exact origin).
//
// Usage: node tests/e2e/cli-login-hub.mjs --repo <repo-id>
// Stdout (one line): {"url":"http://127.0.0.1:<port>","cookie":"__Host-sorrel-session=member","principal":{...}}
import http from 'node:http';
import { createApp } from '../../sorrel-hub/src/app.js';
import { HttpError } from '../../sorrel-hub/src/http.js';
import { createInMemoryStore } from '../../sorrel-hub/src/store.js';

const repoFlag = process.argv.indexOf('--repo');
const repoId = repoFlag > 0 ? process.argv[repoFlag + 1] : 'repo_cli_login';
const member = { type: 'user', id: 'workos:user_cli_login' };
const fixtures = { member };

const server = http.createServer();
await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
const origin = `http://127.0.0.1:${server.address().port}`;

function requireSameOrigin(request) {
  if (request.headers.origin !== origin || request.headers['sec-fetch-site'] === 'cross-site') {
    throw new HttpError(403, 'Use Sorrel Hub from its configured origin', 'invalid_origin');
  }
}

const browserAuth = {
  origin,
  requireSameOrigin,
  async resolveSession(request) {
    const match = /(?:^|;\s*)__Host-sorrel-session=(\w+)/.exec(String(request.headers.cookie ?? ''));
    const principal = match && fixtures[match[1]];
    if (!principal) return null;
    if (!['GET', 'HEAD', 'OPTIONS'].includes(request.method)) requireSameOrigin(request);
    return { principal, sessionId: `fixture:${match[1]}`, authMode: 'workos', idpSubject: principal.id.slice('workos:'.length), transport: 'cookie' };
  },
  async handleRequest() { return false; },
};

// Native Core grants for the signed-in member on exactly one repository.
const grant = { schemaVersion: 'sorrel.protocol.v0', kind: 'Grant', id: 'grant_cli_login_repo',
  principal: { kind: member.type, id: member.id }, resource: { kind: 'repo', id: repoId },
  capabilities: ['repo.read', 'repo.object.write', 'repo.ref.write'], effect: 'allow' };

const app = createApp({
  store: createInMemoryStore(),
  browserAuth,
  authAdapter: { mode: 'workos', async resolveSession() { return null; } },
  trustedGrantsById: { [grant.id]: grant },
  env: {},
});
await app.ready;
server.on('request', app.handleRequest);
console.log(JSON.stringify({ url: origin, cookie: '__Host-sorrel-session=member', principal: member }));
process.on('SIGTERM', () => server.close(async () => { await app.close(); process.exit(0); }));
