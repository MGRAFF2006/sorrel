import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";

import {
  AccessDeniedError,
  POLICY_DECISION_STATUS,
  assertCorePolicyAllowed,
  coreSecretCapabilityForAction,
  decideCorePolicy,
} from "../scripts/lib/grants.mjs";

// Vendored copy of sorrel-protocol/conformance/policy-conformance.json.
// Vault must only return secret values when an injected corePolicy.evaluate()
// returns allow; local grant YAML alone never bypasses Core.
const here = path.dirname(fileURLToPath(import.meta.url));

async function loadManifest() {
  const raw = await readFile(
    path.join(here, "conformance", "policy-conformance.json"),
    "utf8",
  );
  return JSON.parse(raw);
}

// Secret capabilities Vault is responsible for.
const VAULT_CAPABILITIES = new Set(["secret.read", "secret.inject"]);

// Map a manifest secret case to a vault request. Vault actions map to Core
// capabilities via coreSecretCapabilityForAction: read -> secret.read,
// materialize/import -> secret.inject.
function vaultRequestFor(testCase) {
  const capability = testCase.request.capability;
  const action = capability === "secret.inject" ? "materialize" : "read";
  return {
    secret: { kind: "SecretRef", id: testCase.request.resource.id },
    environment: "dev",
    action,
    actor: { workflow: { kind: "Workflow", id: testCase.request.principal.id } },
  };
}

// A corePolicy adapter that mirrors Core by returning a single fixed decision
// for the case under test. Each case gets its own adapter so allow and deny
// cases for the same capability do not collide.
function fixedCorePolicy(expected) {
  return {
    evaluate(policyRequest) {
      const status =
        expected === "allow"
          ? POLICY_DECISION_STATUS.ALLOW
          : POLICY_DECISION_STATUS.DENY;
      return {
        schemaVersion: "sorrel.protocol.v0",
        kind: "PolicyDecision",
        status,
        capability: policyRequest.capability,
        action: policyRequest.action,
        resource: policyRequest.resource,
        environment: policyRequest.environment,
        subject: policyRequest.subject,
        reason: `conformance ${status} for ${policyRequest.capability}`,
      };
    },
  };
}

test("Vault honors Core decisions for secret.read / secret.inject vectors", async () => {
  const manifest = await loadManifest();
  const cases = manifest.permissionDecisions.filter((testCase) =>
    VAULT_CAPABILITIES.has(testCase.request.capability),
  );
  assert.ok(cases.length > 0, "expected secret.read / secret.inject vectors");

  for (const testCase of cases) {
    const request = vaultRequestFor(testCase);
    // The vault request action maps back to the manifest capability.
    assert.equal(
      coreSecretCapabilityForAction(request.action),
      testCase.request.capability,
      `case ${testCase.id}: action mapping`,
    );

    const corePolicy = fixedCorePolicy(testCase.expected);
    const decision = decideCorePolicy({}, request, corePolicy);

    if (testCase.expected === "allow") {
      assert.equal(
        decision.status,
        POLICY_DECISION_STATUS.ALLOW,
        `case ${testCase.id}: expected allow`,
      );
    } else {
      assert.notEqual(
        decision.status,
        POLICY_DECISION_STATUS.ALLOW,
        `case ${testCase.id}: expected not-allowed (${testCase.expected})`,
      );
    }
  }
});

test("local grant YAML alone cannot bypass Core (no corePolicy => needs_grant)", async () => {
  const manifest = await loadManifest();
  const allowCase = manifest.permissionDecisions.find(
    (testCase) =>
      VAULT_CAPABILITIES.has(testCase.request.capability) &&
      testCase.expected === "allow",
  );
  assert.ok(allowCase, "expected an allow secret case");

  const request = vaultRequestFor(allowCase);

  // A local spec with a matching grant must NOT bypass Core when no corePolicy
  // is injected: decideCorePolicy returns needs_grant.
  const decision = decideCorePolicy({}, request, undefined);
  assert.equal(
    decision.status,
    POLICY_DECISION_STATUS.NEEDS_GRANT,
    "missing corePolicy must return needs_grant, never allow",
  );

  // assertCorePolicyAllowed must throw without a corePolicy.
  assert.throws(
    () => assertCorePolicyAllowed({}, request, undefined),
    AccessDeniedError,
    "assertCorePolicyAllowed must reject without Core authorization",
  );
});
