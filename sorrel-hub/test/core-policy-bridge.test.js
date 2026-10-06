import assert from 'node:assert/strict';
import fs from 'node:fs';
import http from 'node:http';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';

import { createApp } from '../src/app.js';
import { objectId } from '../src/blake3.js';
import { evaluate, evaluateWithTrustedGrants, PolicyDeniedError, PolicyEvaluationError } from '../src/core-policy.js';
import { resolveTrustedPolicies } from '../src/bootstrap-grants.js';

const principal = { type: 'user', id: 'local' };
const resource = { kind: 'repo', id: 'repo_bridge' };
function grant(id, effect = 'allow') {
  return { schemaVersion: 'sorrel.protocol.v0', kind: 'Grant', id,
    principal: { kind: 'user', id: 'local' }, capabilities: ['repo.object.write'], resource, effect };
}
const request = { principal, action: 'repo.object.write', resource };

async function withServer(options, callback) {
  const app = createApp({ authAdapter: { mode: 'oidc', resolveSession: async () => ({ principal }) }, ...options });
  const server = http.createServer(app.handleRequest);
  await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
  try { await callback(`http://127.0.0.1:${server.address().port}`, app); }
  finally { await new Promise((resolve, reject) => server.close((error) => error ? reject(error) : resolve())); }
}

async function upload(url, grantRefs, extras = {}) {
  const bytes = Buffer.from('guarded content');
  return await fetch(`${url}/repo_bridge/objects`, {
    method: 'POST', headers: { 'content-type': 'application/json', 'x-sorrel-acting-principal': JSON.stringify(principal) },
    body: JSON.stringify({ grantRefs, objects: [{ id: objectId(bytes), data: bytes.toString('base64') }], ...extras }),
  });
}

test('real Core handles all effects and returns its native decision', async () => {
  for (const [effects, expected] of [
    [[], 'needs_grant'], [['allow'], 'allow'], [['allow', 'deny'], 'deny'],
    [['allow', 'redact'], 'redact'], [['allow', 'review'], 'needs_review'],
  ]) {
    const result = await evaluate({ ...request, grants: effects.map((effect) => grant(`grant_${effect}`, effect)) });
    assert.equal(result.allowed, expected === 'allow');
    assert.equal(result.decision.schemaVersion, 'sorrel.protocol.v0');
    assert.equal(result.decision.kind, 'PolicyDecision');
    assert.equal(result.decision.decision, expected);
    assert.notEqual(result.decision.evaluatedAt, '1970-01-01T00:00:00Z');
  }
});

test('HTTP callers cannot omit an effective deny from request grantRefs', async () => {
  await withServer({ trustedGrantsById: { allow: grant('allow'), deny: grant('deny', 'deny') } }, async (url, app) => {
    const response = await upload(url, [{ id: 'allow', source: 'core' }]);
    assert.equal(response.status, 403);
    const body = await response.json();
    assert.equal(body.error.code, 'policy_denied');
    assert.equal(body.error.decision.decision, 'deny');
    assert.deepEqual(body.error.decision.matchedGrants.map((ref) => ref.id), ['allow', 'deny']);
    assert.equal(app.store.sync.has(resource.id, objectId(Buffer.from('guarded content'))), false);
  });
});

test('request policy and grant payloads cannot authorize themselves', async () => {
  await withServer({}, async (url, app) => {
    const response = await upload(url, [], {
      grants: [grant('self_allow')],
      policies: [{ schemaVersion: 'sorrel.protocol.v0', kind: 'Policy', id: 'self_policy', resource, rules: [], defaultDecision: 'allow' }],
    });
    assert.equal(response.status, 403);
    assert.equal((await response.json()).error.decision.decision, 'needs_grant');
    assert.equal(app.store.sync.has(resource.id, objectId(Buffer.from('guarded content'))), false);
  });
});

test('lifecycle, unsupported conditions, versions and malformed trusted IDs fail closed', async () => {
  for (const fields of [
    { status: 'revoked' }, { status: 'expired' }, { expiresAt: '2000-01-01T00:00:00Z' },
    { issuedAt: '2099-01-01T00:00:00+00:00' }, { revokedAt: '2000-01-01T00:00:00Z' },
  ]) {
    await assert.rejects(evaluateWithTrustedGrants(principal, request.action, resource, [], { allow: { ...grant('allow'), ...fields } }), PolicyDeniedError);
  }
  for (const fields of [
    { schemaVersion: 'future' }, { conditions: { runner: 'only-trusted' } },
    { expiresAt: null }, { capabilities: [{ kind: 'Capability', id: 'unresolved' }] },
    { resource: { ...resource, path: 'only.txt' } }, { principal: { kind: 'Principal', id: 'unresolved' } },
  ]) {
    await assert.rejects(evaluateWithTrustedGrants(principal, request.action, resource, [], { allow: { ...grant('allow'), ...fields } }), PolicyEvaluationError);
  }
  await assert.rejects(evaluateWithTrustedGrants(principal, request.action, resource, [], { wrong_id: grant('allow') }), /ID must match/);
});

test('configured policies apply even when callers omit their refs, and unknown refs reject', async () => {
  const policy = { schemaVersion: 'sorrel.protocol.v0', kind: 'Policy', id: 'deny_policy', resource,
    rules: [{ id: 'deny_rule', effect: 'deny', capabilities: ['repo.object.write'], resources: [] }] };
  await withServer({ trustedGrantsById: { allow: grant('allow') }, trustedPoliciesById: { deny_policy: policy } }, async (url) => {
    const denied = await upload(url, [{ id: 'allow' }], { policyRefs: [] });
    assert.equal(denied.status, 403);
    assert.equal((await denied.json()).error.decision.matchedPolicy.id, 'deny_policy');
  });
  await withServer({ trustedGrantsById: { allow: grant('allow') } }, async (url) => {
    const unknown = await upload(url, [{ id: 'allow' }], { policyRefs: [{ kind: 'Policy', id: 'not_hydrated' }] });
    assert.equal(unknown.status, 403);
    assert.equal((await unknown.json()).error.code, 'policy_evaluation_failed');
  });
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'sorrel-policy-config-'));
  try {
    const file = path.join(dir, 'policies.json');
    fs.writeFileSync(file, JSON.stringify({ deny_policy: policy }));
    assert.deepEqual(resolveTrustedPolicies({ SORREL_HUB_TRUSTED_POLICIES_FILE: file }), { deny_policy: policy });
    assert.deepEqual(resolveTrustedPolicies({}), {});
  } finally { fs.rmSync(dir, { recursive: true, force: true }); }
});

test('explicit legacy records are supported without implicit effects or universal resources', async () => {
  const legacy = { id: 'legacy', source: 'core', principal, action: request.action, resource, effect: 'allow' };
  assert.equal((await evaluate({ ...request, grants: [legacy] })).allowed, true);
  for (const field of ['effect', 'resource']) {
    const incomplete = { ...legacy }; delete incomplete[field];
    await assert.rejects(evaluate({ ...request, grants: [incomplete] }), PolicyEvaluationError);
  }
  for (const [acting, granted, allowed] of [['workflow', 'workflow', true], ['service', 'service', true], ['service', 'workflow', false]]) {
    const result = await evaluate({ ...request, principal: { type: acting, id: 'same' }, grants: [{ ...grant('allow'), principal: { kind: granted, id: 'same' } }] });
    assert.equal(result.allowed, allowed);
  }
});

test('process errors and byte limits fail closed without exposing stderr', async (t) => {
  await assert.rejects(evaluate({ ...request, grants: [grant('x'.repeat(1024 * 1024))] }), /request exceeds byte limit/);
  const original = process.env.SORREL_HUB_CORE_POLICY_BIN;
  t.after(() => { if (original === undefined) delete process.env.SORREL_HUB_CORE_POLICY_BIN; else process.env.SORREL_HUB_CORE_POLICY_BIN = original; });
  process.env.SORREL_HUB_CORE_POLICY_BIN = path.join(os.tmpdir(), 'nonexistent-sorrel-policy-binary');
  await assert.rejects(evaluate(request), (error) => error.statusCode === 503 && /executable is unavailable/.test(error.message));
  if (process.platform === 'win32') return;
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'sorrel-policy-process-'));
  t.after(() => fs.rmSync(dir, { recursive: true, force: true }));
  const binary = path.join(dir, 'policy');
  process.env.SORREL_HUB_CORE_POLICY_BIN = binary;
  fs.writeFileSync(binary, '#!/usr/bin/env node\nprocess.stdin.resume();process.stdin.on("end",()=>{process.stderr.write("private-diagnostic");process.stdout.write("not json")});\n', { mode: 0o755 });
  await assert.rejects(evaluate(request), (error) => error.statusCode === 503 && !error.message.includes('private-diagnostic'));
  fs.writeFileSync(binary, '#!/usr/bin/env node\nprocess.stdin.resume();process.stdin.on("end",()=>process.stdout.write("x".repeat(1024*1024+1)));\n');
  await assert.rejects(evaluate(request), /response exceeds byte limit/);
  fs.writeFileSync(binary, '#!/usr/bin/env node\nprocess.stdin.resume();setInterval(()=>{},1000);\n');
  const pending = Array.from({ length: 16 }, () => evaluate(request));
  const settled = Promise.allSettled(pending);
  await assert.rejects(evaluate(request), (error) => error.statusCode === 503 && /busy/.test(error.message));
  const results = await settled;
  assert.ok(results.every((result) => result.status === 'rejected' && /timed out/.test(result.reason.message)));
});
