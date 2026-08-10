import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { existsSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";

import { computeMeta } from "../scripts/conformance-meta.mjs";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

async function readJson(relativePath) {
  return JSON.parse(await readFile(path.join(root, relativePath), "utf8"));
}

const suite = await readJson("conformance/policy-conformance.json");

test("conformance suite declares expected metadata", () => {
  assert.equal(suite.kind, "PolicyConformanceSuite");
  assert.equal(suite.schemaVersion, "sorrel.protocol.v0");
  assert.ok(Array.isArray(suite.permissionDecisions));
  assert.ok(Array.isArray(suite.policyChanges));
  assert.ok(suite.permissionDecisions.length > 0);
  assert.ok(suite.policyChanges.length > 0);
});

test("sidecar metadata is in sync with the manifest", async () => {
  // The sidecar (policy-conformance.meta.json) is what consumers vendor to
  // detect drift. If this fails, run `npm run sync:meta`.
  const expected = await computeMeta();
  const sidecar = await readJson("conformance/policy-conformance.meta.json");
  assert.equal(sidecar.kind, "PolicyConformanceMeta");
  assert.equal(sidecar.manifestVersion, expected.manifestVersion);
  assert.equal(sidecar.schemaVersion, expected.schemaVersion);
  assert.equal(
    sidecar.sha256,
    expected.sha256,
    "sidecar sha256 is stale; run `npm run sync:meta`",
  );
});

test("every referenced source fixture exists", () => {
  const allCases = [...suite.permissionDecisions, ...suite.policyChanges];
  for (const testCase of allCases) {
    for (const fixture of testCase.sourceFixtures ?? []) {
      assert.ok(
        existsSync(path.join(root, fixture)),
        `case ${testCase.id} references missing fixture ${fixture}`,
      );
    }
  }
});

test("permission decisions cover required capabilities", () => {
  const capabilities = new Set(
    suite.permissionDecisions.map((testCase) => testCase.request.capability),
  );
  for (const required of ["path.write", "workflow.run", "secret.read", "secret.inject"]) {
    assert.ok(capabilities.has(required), `missing permission case for ${required}`);
  }
});

test("policy changes cover required authority scenarios", () => {
  const ids = new Set(suite.policyChanges.map((testCase) => testCase.id));
  for (const required of [
    "self_grant_denied",
    "unsigned_change_untrusted",
    "forged_signature_untrusted",
    "delegated_grant_allowed",
    "scope_broadening_denied",
    "authority_rotation_threshold_met",
  ]) {
    assert.ok(ids.has(required), `missing policy change case ${required}`);
  }
});

test("permission expected outcomes match referenced PolicyDecision fixtures", async () => {
  for (const testCase of suite.permissionDecisions) {
    const decisionFixture = (testCase.sourceFixtures ?? []).find((fixture) =>
      fixture.includes("policy-decision-"),
    );
    if (!decisionFixture) {
      continue;
    }
    const decision = await readJson(decisionFixture);
    assert.equal(
      decision.decision,
      testCase.expected,
      `case ${testCase.id}: fixture decision ${decision.decision} != expected ${testCase.expected}`,
    );
  }
});

test("policy change expected outcomes match referenced PolicyChange fixtures", async () => {
  for (const testCase of suite.policyChanges) {
    const changeFixture = (testCase.sourceFixtures ?? []).find((fixture) =>
      fixture.includes("policy-change-"),
    );
    if (!changeFixture) {
      continue;
    }
    const change = await readJson(changeFixture);
    // Some cases are synthetic variations of a base fixture (forged signature,
    // sub-threshold rotation). Their inputs share a fixture but the fixture's
    // own trust/outcome differs; only assert against fixtures whose signature
    // posture matches the case.
    if (
      testCase.id === "forged_signature_untrusted" ||
      testCase.id === "authority_rotation_threshold_not_met"
    ) {
      continue;
    }
    assert.equal(
      change.trust,
      testCase.expected.trust,
      `case ${testCase.id}: fixture trust ${change.trust} != expected ${testCase.expected.trust}`,
    );
    assert.equal(
      change.metadata.outcome,
      testCase.expected.outcome,
      `case ${testCase.id}: fixture outcome ${change.metadata.outcome} != expected ${testCase.expected.outcome}`,
    );
  }
});
