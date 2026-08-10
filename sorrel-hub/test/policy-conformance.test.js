import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';

import { evaluate } from '../src/core-policy.js';

// The manifest is a vendored copy of sorrel-protocol/conformance/policy-conformance.json.
// Hub is an administration/API layer over Core policy semantics, NOT the source
// of truth. These tests prove Hub's grant-based guard agrees with the canonical
// allow/deny decisions for the actions Hub administers.
const here = path.dirname(fileURLToPath(import.meta.url));

async function loadManifest() {
  const raw = await readFile(path.join(here, 'conformance', 'policy-conformance.json'), 'utf8');
  return JSON.parse(raw);
}

// Map a manifest grant (capability + resource{kind,id}) to a Hub Core grant
// (action + resource{kind,id}).
function hubGrant(manifestGrant) {
  return {
    id: manifestGrant.id,
    source: 'core',
    principal: manifestGrant.principal,
    action: manifestGrant.capability,
    resource: manifestGrant.resource,
  };
}

test('Hub guard agrees with permission decision vectors', async () => {
  const manifest = await loadManifest();
  assert.ok(manifest.permissionDecisions.length > 0);

  for (const testCase of manifest.permissionDecisions) {
    const { request, grants = [], expected, id } = testCase;
    const result = evaluate({
      principal: request.principal,
      action: request.capability,
      resource: request.resource,
      grants: grants.map(hubGrant),
    });

    if (expected === 'allow') {
      assert.equal(result.allowed, true, `case ${id}: expected allow`);
      assert.equal(result.decision.outcome, 'allow', `case ${id}: outcome`);
    } else {
      assert.equal(result.allowed, false, `case ${id}: expected not-allowed (${expected})`);
      assert.equal(result.decision.outcome, 'deny', `case ${id}: outcome`);
    }
  }
});

// Hub administers policy.grant admin actions over Core grant semantics. For
// policy-change cases that are decided by grant authority (not signature trust),
// model the actor requesting the change operation against its previous grants.
// Signature-trust cases (unsigned/forged/rotation thresholds) are Core's
// responsibility and are intentionally NOT re-decided by Hub.
const HUB_GRANT_AUTHORITY_CASES = new Set([
  'self_grant_denied',
  'delegated_grant_allowed',
  'scope_broadening_denied',
]);

test('Hub guard agrees with grant-authority policy-change vectors', async () => {
  const manifest = await loadManifest();
  const cases = manifest.policyChanges.filter((testCase) =>
    HUB_GRANT_AUTHORITY_CASES.has(testCase.id),
  );
  assert.ok(cases.length > 0, 'expected grant-authority policy-change cases');

  for (const testCase of cases) {
    const { actor, operation, proposedGrant, previousGrants = [], expected, id } = testCase;

    // The actor must hold `operation` authority on the proposed resource under
    // the previous effective policy (its previousGrants). Hub never counts the
    // proposed grant itself as authority.
    const result = evaluate({
      principal: actor,
      action: operation,
      resource: proposedGrant ? proposedGrant.resource : undefined,
      grants: previousGrants.map(hubGrant),
    });

    const expectAllowed = expected.outcome === 'allow';
    assert.equal(
      result.allowed,
      expectAllowed,
      `case ${id}: expected ${expected.outcome}, got ${result.decision.outcome} (${result.decision.reason})`,
    );
  }
});
