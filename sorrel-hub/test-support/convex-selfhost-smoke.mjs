// Run only against a fresh disposable loopback backend with these internal functions deployed.
import assert from 'node:assert/strict';
import { createConvexMirror } from '../src/convex-mirror.js';
const url = process.env.CONVEX_SELF_HOSTED_URL;
const key = process.env.CONVEX_SELF_HOSTED_ADMIN_KEY;
assert.ok(url && key, 'disposable self-host URL and admin key required');
assert.equal(new URL(url).hostname, '127.0.0.1', 'smoke writes are limited to loopback');
async function call(kind, path, args, auth) {
  const response = await fetch(`${url}/api/${kind}`, { method: 'POST',
    headers: { 'content-type': 'application/json', ...(auth ? { authorization: auth } : {}) },
    body: JSON.stringify({ path, args, format: 'json' }) });
  return { status: response.status, body: await response.json() };
}
assert.equal((await call('query', 'proposals:countOpen', {}, `Convex ${key}`)).body.value, 0, 'use a fresh disposable backend');
const proposal = { id: 'privacy-smoke', status: 'open', projectId: 'private-project', title: 'Private', updatedAt: new Date().toISOString() };
for (const auth of [undefined, 'Convex invalid-test-key', 'Bearer invalid-test-jwt']) {
  for (const [kind, path, args] of [
    ['query', 'proposals:countOpen', {}],
    ['mutation', 'proposals:upsert', { hubId: proposal.id, status: 'open', updatedAt: proposal.updatedAt }],
    ['mutation', 'proposals:remove', { hubId: proposal.id }],
  ]) {
    const result = await call(kind, path, args, auth);
    assert.notEqual(result.body.status, 'success', `public ${path} must reject`);
  }
}
const mirror = createConvexMirror(process.env);
assert.equal(mirror.enabled, true);
await mirror.upsertProposal(proposal);
assert.deepEqual(await call('query', 'proposals:countOpen', {}, `Convex ${key}`), { status: 200, body: { status: 'success', value: 1 } });
await mirror.upsertProposal({ ...proposal, status: 'merged' });
assert.equal((await call('query', 'proposals:countOpen', {}, `Convex ${key}`)).body.value, 0);
await mirror.upsertProposal(proposal);
await mirror.removeProposal(proposal.id);
assert.equal((await call('query', 'proposals:countOpen', {}, `Convex ${key}`)).body.value, 0);
console.log('Private Convex: anonymous/forged read+write rejected; privileged Hub mirror upsert/update/remove passed');
