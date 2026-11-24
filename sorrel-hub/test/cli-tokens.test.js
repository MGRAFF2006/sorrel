import assert from 'node:assert/strict';
import fs from 'node:fs';
import http from 'node:http';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';

import { createApp } from '../src/app.js';
import { createCliTokens, CLI_TOKEN_MAX_ACTIVE_PER_PRINCIPAL, resolveCliTokenTtlMs } from '../src/auth/cli-tokens.js';
import { createFsMetadataStore } from '../src/fs-metadata-store.js';
import { createInMemoryStore } from '../src/store.js';

const owner = { type: 'user', id: 'workos:user_owner' };
const other = { type: 'user', id: 'workos:user_other' };
const ownerJwt = 'Bearer owner-identity-provider-token';
const otherJwt = 'Bearer other-identity-provider-token';
const DAY = 24 * 60 * 60 * 1000;

function grant(id, principal, kind, resourceId, capabilities) {
  return { schemaVersion: 'sorrel.protocol.v0', kind: 'Grant', id,
    principal: { kind: principal.type, id: principal.id }, resource: { kind, id: resourceId }, capabilities, effect: 'allow' };
}
/** Owner holds every capability, including admin ones, so only the token ceiling can refuse. */
function grants() {
  return Object.fromEntries([
    grant('org_a', owner, 'org', 'org_a', ['org.read', 'org.write', 'project.create', 'policy.read', 'policy.grant']),
    grant('proj_a', owner, 'project', 'proj_a', ['project.read', 'project.write', 'proposal.read', 'proposal.write', 'proposal.review',
      'review.comment.read', 'review.comment.write', 'workflow.run.read', 'workflow.run.write', 'policy.read', 'policy.grant']),
    grant('repo_a', owner, 'repo', 'repo_a', ['repo.read', 'repo.object.write', 'repo.ref.write']),
  ].map(g => [g.id, g]));
}

/** Stand-in for WorkOS/OIDC bearer verification: proves non-CLI bearers keep their path. */
function identityProviderAdapter(seen = []) {
  return { mode: 'workos', async resolveSession(request) {
    seen.push(request.headers.authorization);
    if (request.headers.authorization === ownerJwt) return { principal: owner, authMode: 'workos', sessionId: 'sid_owner', idpSubject: 'user_owner' };
    if (request.headers.authorization === otherJwt) return { principal: other, authMode: 'workos', sessionId: 'sid_other', idpSubject: 'user_other' };
    return null;
  } };
}

function seed(store) {
  store.createOrganization({ id: 'org_a', name: 'Org A' });
  store.createProject({ id: 'proj_a', organizationId: 'org_a', name: 'Project A' });
  const blob = store.sync.put('repo_a', Buffer.from('sorrel.blob.v0\nhello'));
  return { blob };
}

async function withHub(t, options = {}) {
  const store = options.store ?? createInMemoryStore();
  if (!options.store) seed(store);
  const audits = [];
  const seen = [];
  let now = options.now ?? Date.parse('2026-11-24T10:00:00Z');
  const cliTokens = createCliTokens(store, { env: options.env ?? {}, now: () => now,
    audit: (event, fields) => audits.push({ event, ...fields }) });
  const app = createApp({ store, cliTokens, authAdapter: identityProviderAdapter(seen), browserAuth: null, trustedGrantsById: grants(), env: {} });
  const server = http.createServer(app.handleRequest);
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  t.after(async () => { await new Promise(resolve => server.close(resolve)); await app.close(); });
  const url = `http://127.0.0.1:${server.address().port}`;
  async function call(pathname, { method = 'GET', authorization, body } = {}) {
    const headers = { 'content-type': 'application/json' };
    if (authorization) headers.authorization = authorization;
    const response = await fetch(url + pathname, { method, headers, ...(body === undefined ? {} : { body: JSON.stringify(body) }) });
    const text = await response.text();
    return { status: response.status, headers: response.headers, body: text ? JSON.parse(text) : null };
  }
  return { app, store, cliTokens, audits, seen, call, advance(ms) { now += ms; }, get now() { return now; } };
}

const bearer = token => `Bearer ${token}`;
/** The secret part of `srl_cli_<24 hex id>_<secret>`; base64url secrets may contain `_`. */
const secretOf = token => token.slice('srl_cli_'.length + 25);

test('CLI token authenticates the issuing principal; Core grants still decide access', async t => {
  const hub = await withHub(t);
  const { token, record } = hub.cliTokens.issue({ principal: owner, authMode: 'workos', idpSubject: 'user_owner', label: 'laptop' });
  assert.match(token, /^srl_cli_[0-9a-f]{24}_[A-Za-z0-9_-]{43}$/);
  assert.equal(record.secretHash, undefined, 'issue never returns the hash');

  const session = await hub.call('/session', { authorization: bearer(token) });
  assert.equal(session.status, 200);
  assert.equal(session.headers.get('cache-control'), 'no-store');
  assert.deepEqual(session.body.data.session.principal, owner);
  assert.equal(session.body.data.session.credential.kind, 'cli');
  assert.equal(session.body.data.session.credential.label, 'laptop');
  assert.equal(session.body.data.session.credential.expiresAt, '2026-12-24T10:00:00.000Z');
  assert.ok(!session.body.data.session.credential.scopes.includes('policy.grant'));

  assert.equal((await hub.call('/repo_a/refs', { authorization: bearer(token) })).status, 200);
  const write = await hub.call('/repo_a/refs/main', { method: 'POST', authorization: bearer(token), body: { snapshot: 'a'.repeat(64), grantRefs: [] } });
  assert.notEqual(write.status, 401);
  assert.notEqual(write.status, 403, 'push-scope writes pass the token ceiling and reach Core');

  // A token for a principal without grants authenticates but Core denies.
  const stranger = hub.cliTokens.issue({ principal: other, authMode: 'workos' }).token;
  const denied = await hub.call('/repo_a/refs', { authorization: bearer(stranger) });
  assert.equal(denied.status, 403);
  assert.equal(denied.body.error.code, 'policy_denied');
  assert.deepEqual(hub.seen, [], 'srl_cli_ bearers never reach the identity-provider adapter');
});

test('unknown, malformed, tampered, expired and revoked CLI tokens are 401 and never fall through', async t => {
  const hub = await withHub(t);
  const { token, record } = hub.cliTokens.issue({ principal: owner, authMode: 'workos' });
  const tampered = token.slice(0, -1) + (token.endsWith('A') ? 'B' : 'A');
  const unknownId = token.replace(record.id, '0'.repeat(24));
  for (const value of ['srl_cli_', 'srl_cli_nothex_secret', tampered, unknownId, `${token}x`]) {
    const result = await hub.call('/repo_a/refs', { authorization: bearer(value) });
    assert.equal(result.status, 401, value.slice(0, 12));
    assert.equal(result.body.error.code, 'authentication_required');
    assert.equal((await hub.call('/session', { authorization: bearer(value) })).body.data.session, null);
  }
  assert.deepEqual(hub.seen, []);

  // Identity-provider bearers keep their existing path.
  assert.equal((await hub.call('/repo_a/refs', { authorization: ownerJwt })).status, 200);
  assert.deepEqual(hub.seen, [ownerJwt]);

  hub.advance(30 * DAY - 1);
  assert.equal((await hub.call('/repo_a/refs', { authorization: bearer(token) })).status, 200);
  hub.advance(1);
  assert.equal((await hub.call('/repo_a/refs', { authorization: bearer(token) })).status, 401, 'expires 30 days after issue');

  const fresh = hub.cliTokens.issue({ principal: owner, authMode: 'workos' });
  assert.equal((await hub.call('/repo_a/refs', { authorization: bearer(fresh.token) })).status, 200);
  hub.cliTokens.revoke(owner, fresh.record.id);
  assert.equal((await hub.call('/repo_a/refs', { authorization: bearer(fresh.token) })).status, 401);
});

test('admin and policy.grant actions are refused for CLI tokens even when Core grants them', async t => {
  const hub = await withHub(t);
  const { token } = hub.cliTokens.issue({ principal: owner, authMode: 'workos' });
  const attempts = [
    ['/admin/policies', { organizationId: 'org_a', projectId: 'proj_a', name: 'Escalate', grantRefs: [{ id: 'proj_a' }] }],
    ['/admin/repositories', { projectId: 'proj_a', organizationId: 'org_a', provider: 'sorrel', owner: 'o', name: 'r', grantRefs: [{ id: 'proj_a' }] }],
    ['/admin/organizations', { id: 'org_new', name: 'New org' }],
  ];
  for (const [pathname, body] of attempts) {
    const viaCli = await hub.call(pathname, { method: 'POST', authorization: bearer(token), body });
    assert.equal(viaCli.status, 403, pathname);
    assert.equal(viaCli.body.error.code, 'credential_scope_denied', pathname);
  }
  // The same grants allow the same admin action through an identity-provider session.
  const viaBrowserIdentity = await hub.call('/admin/policies', { method: 'POST', authorization: ownerJwt, body: attempts[0][1] });
  assert.equal(viaBrowserIdentity.status, 201);

  const approve = await hub.call('/admin/proposals/prop_missing/approvals', { method: 'POST', authorization: bearer(token), body: {} });
  assert.equal(approve.status, 403);
  assert.equal(approve.body.error.code, 'credential_scope_denied');

  // Ordinary CLI collaboration (project creation for lane submit) stays available.
  const project = await hub.call('/projects', { method: 'POST', authorization: bearer(token), body: { organizationId: 'org_a', name: 'From CLI' } });
  assert.equal(project.status, 201);
});

test('owners list and revoke only their own tokens; CLI tokens can only revoke themselves', async t => {
  const hub = await withHub(t);
  const mine = hub.cliTokens.issue({ principal: owner, authMode: 'workos', label: 'laptop' });
  const second = hub.cliTokens.issue({ principal: owner, authMode: 'workos', label: 'ci box' });
  const theirs = hub.cliTokens.issue({ principal: other, authMode: 'workos' });

  assert.equal((await hub.call('/auth/cli/tokens')).status, 401);
  const listed = await hub.call('/auth/cli/tokens', { authorization: ownerJwt });
  assert.equal(listed.status, 200);
  assert.equal(listed.headers.get('cache-control'), 'no-store');
  assert.deepEqual(listed.body.data.map(t => t.id).sort(), [mine.record.id, second.record.id].sort());
  for (const entry of listed.body.data) {
    assert.equal(entry.secretHash, undefined);
    assert.equal(entry.status, 'active');
    assert.deepEqual(entry.principal, owner);
  }
  assert.ok(!JSON.stringify(listed.body).includes(secretOf(mine.token)));

  const cliList = await hub.call('/auth/cli/tokens', { authorization: bearer(mine.token) });
  assert.equal(cliList.status, 403);
  assert.equal(cliList.body.error.code, 'credential_scope_denied');
  assert.equal((await hub.call(`/auth/cli/tokens/${second.record.id}`, { method: 'DELETE', authorization: bearer(mine.token) })).status, 403);

  const foreign = await hub.call(`/auth/cli/tokens/${theirs.record.id}`, { method: 'DELETE', authorization: ownerJwt });
  assert.equal(foreign.status, 404, 'foreign tokens are indistinguishable from unknown ones');
  assert.equal((await hub.call('/repo_a/refs', { authorization: bearer(theirs.token) })).status, 403, 'still valid (no grants)');

  const revoked = await hub.call(`/auth/cli/tokens/${second.record.id}`, { method: 'DELETE', authorization: ownerJwt });
  assert.equal(revoked.status, 200);
  assert.equal(revoked.body.data.status, 'revoked');
  assert.equal((await hub.call('/repo_a/refs', { authorization: bearer(second.token) })).status, 401);

  assert.equal((await hub.call('/auth/cli/tokens/current', { method: 'DELETE', authorization: ownerJwt })).body.error.code, 'not_cli_token');
  const logout = await hub.call('/auth/cli/tokens/current', { method: 'DELETE', authorization: bearer(mine.token) });
  assert.equal(logout.status, 200);
  assert.equal(logout.body.data.id, mine.record.id);
  assert.equal((await hub.call('/repo_a/refs', { authorization: bearer(mine.token) })).status, 401);

  const a = hub.cliTokens.issue({ principal: owner, authMode: 'workos' });
  const b = hub.cliTokens.issue({ principal: owner, authMode: 'workos' });
  const all = await hub.call('/auth/cli/tokens', { method: 'DELETE', authorization: ownerJwt });
  assert.deepEqual(all.body.data, { revoked: 2 });
  for (const token of [a.token, b.token]) assert.equal((await hub.call('/repo_a/refs', { authorization: bearer(token) })).status, 401);
  assert.equal((await hub.call('/session', { authorization: bearer(theirs.token) })).body.data.session.principal.id, other.id, 'other owner unaffected');

  const events = hub.audits.map(entry => entry.event);
  assert.ok(events.filter(e => e === 'cli_token.revoked').length >= 4);
  assert.equal(hub.audits.find(entry => entry.event === 'cli_token.revoked' && entry.tokenId === mine.record.id).reason, 'logout');
});

test('records persist hashed across restart; lastUsedAt is written at most hourly; audit lines omit secrets', async t => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'sorrel-cli-tokens-'));
  t.after(() => fs.rmSync(dir, { recursive: true, force: true }));
  const consoleLines = [];
  t.mock.method(console, 'info', (...args) => consoleLines.push(args.join(' ')));

  const first = createFsMetadataStore(dir);
  seed(first);
  let now = Date.parse('2026-11-24T10:00:00Z');
  const tokens = createCliTokens(first, { env: {}, now: () => now });
  const kept = tokens.issue({ principal: owner, authMode: 'workos', label: 'kept' });
  const dropped = tokens.issue({ principal: owner, authMode: 'workos', label: 'dropped' });
  tokens.revoke(owner, dropped.record.id);

  const files = fs.readdirSync(path.join(dir, 'cliCredentials'));
  assert.equal(files.length, 2);
  for (const file of files) {
    const raw = fs.readFileSync(path.join(dir, 'cliCredentials', file), 'utf8');
    for (const secret of [kept.token, dropped.token]) assert.ok(!raw.includes(secretOf(secret)), 'plaintext secret is never stored');
    assert.match(JSON.parse(raw).secretHash, /^[0-9a-f]{64}$/);
  }

  const restarted = createFsMetadataStore(dir);
  const after = createCliTokens(restarted, { env: {}, now: () => now });
  assert.equal(after.resolve(bearer(dropped.token)), null, 'revocation survives restart');
  const session = after.resolve(bearer(kept.token));
  assert.deepEqual(session.principal, owner);
  const recordPath = path.join(dir, 'cliCredentials', `${kept.record.id}.json`);
  const usedAt = () => JSON.parse(fs.readFileSync(recordPath, 'utf8')).lastUsedAt;
  assert.equal(usedAt(), '2026-11-24T10:00:00.000Z');
  now += 59 * 60 * 1000;
  after.resolve(bearer(kept.token));
  assert.equal(usedAt(), '2026-11-24T10:00:00.000Z', 'no write within the hour');
  now += 60 * 1000;
  after.resolve(bearer(kept.token));
  assert.equal(usedAt(), '2026-11-24T11:00:00.000Z');

  const events = consoleLines.filter(line => line.startsWith('[sorrel-hub] audit ')).map(line => JSON.parse(line.slice('[sorrel-hub] audit '.length)));
  assert.deepEqual(events.map(e => e.event), ['cli_token.issued', 'cli_token.issued', 'cli_token.revoked', 'cli_token.first_use']);
  for (const line of consoleLines) {
    for (const secret of [kept.token, dropped.token]) assert.ok(!line.includes(secretOf(secret)));
  }
});

test('token lifetime and issuance bounds', t => {
  assert.equal(resolveCliTokenTtlMs({}), 30 * DAY);
  assert.equal(resolveCliTokenTtlMs({ SORREL_HUB_CLI_TOKEN_TTL_DAYS: '90' }), 90 * DAY);
  for (const value of ['0', '91', '1.5', 'thirty']) {
    assert.throws(() => resolveCliTokenTtlMs({ SORREL_HUB_CLI_TOKEN_TTL_DAYS: value }), /1 to 90/);
  }
  t.mock.method(console, 'info', () => {});
  const store = createInMemoryStore();
  const tokens = createCliTokens(store, { env: { SORREL_HUB_CLI_TOKEN_TTL_DAYS: '7' } });
  const { record } = tokens.issue({ principal: owner });
  assert.equal(Date.parse(record.expiresAt) - Date.parse(record.createdAt), 7 * DAY);
  assert.throws(() => tokens.issue({ principal: owner, label: 'bad\nlabel' }), error => error.code === 'invalid_request_body');
  assert.throws(() => tokens.issue({ principal: { type: 'user' } }), TypeError);
  for (let i = 1; i < CLI_TOKEN_MAX_ACTIVE_PER_PRINCIPAL; i += 1) tokens.issue({ principal: owner });
  assert.throws(() => tokens.issue({ principal: owner }), error => error.code === 'cli_token_limit');
  tokens.revoke(owner, record.id);
  assert.ok(tokens.issue({ principal: owner }).token, 'revoking frees capacity');
});
