import assert from "node:assert/strict";
import test from "node:test";
import { isGrantAllowed, evaluateCorePolicyFromGrants } from "../scripts/lib/grants.mjs";

const request = { secret: { kind: "SecretRef", id: "fixture" }, environment: "dev", action: "read", actor: {} };
const grant = { id: "fixture-grant", secret: request.secret, environment: "dev", actions: ["read"], access: {} };

test("local grants expire at their boundary and invalid expiry fails closed", () => {
  const expiry = "2030-01-01T00:00:00Z";
  const now = Date.parse(expiry);
  assert.equal(isGrantAllowed({ ...grant, expiresAt: expiry }, request, now - 1), true);
  assert.equal(isGrantAllowed({ ...grant, expiresAt: expiry }, request, now), false);
  assert.equal(isGrantAllowed({ ...grant, expiresAt: expiry }, request, now + 1), false);
  assert.equal(isGrantAllowed(grant, request, now), true);
  for (const expiresAt of [null, 123, "invalid", ""]) {
    assert.equal(isGrantAllowed({ ...grant, expiresAt }, request, now), false);
  }
});

test("local policy does not authorize an expired matching grant", () => {
  const expired = { ...grant, expiresAt: "2000-01-01T00:00:00Z" };
  assert.equal(evaluateCorePolicyFromGrants({ grants: [expired] }, request).status, "needs_grant");
  assert.equal(evaluateCorePolicyFromGrants({ grants: [expired, grant] }, request).status, "allow");
});
