import assert from "node:assert/strict";
import { readFile, readdir } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import Ajv2020 from "ajv/dist/2020.js";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const schemaPath = path.join(root, "schemas", "sorrel-object.schema.json");
const examplesDir = path.join(root, "examples");
const invalidExamplesDir = path.join(examplesDir, "invalid");

const schema = JSON.parse(await readFile(schemaPath, "utf8"));
const ajv = new Ajv2020({ allErrors: true, strict: true });
const validate = ajv.compile(schema);

async function readJson(relativePath) {
  return JSON.parse(await readFile(path.join(root, relativePath), "utf8"));
}

async function jsonFiles(dir) {
  return (await readdir(dir))
    .filter((file) => file.endsWith(".json"))
    .sort();
}

function assertEvaluationUsesPreviousPolicy(policyChange, label) {
  assert.equal(
    policyChange.evaluationPolicyRoot.id,
    policyChange.previousPolicyRoot.id,
    `${label}: evaluationPolicyRoot.id must match previousPolicyRoot.id`
  );
  assert.equal(
    policyChange.evaluationPolicyRoot.contentHash.value,
    policyChange.previousPolicyRoot.contentHash.value,
    `${label}: evaluationPolicyRoot.contentHash must match previousPolicyRoot.contentHash`
  );
}

let passed = 0;
let failed = 0;

async function testAsync(name, fn) {
  try {
    await fn();
    passed += 1;
    console.log(`ok ${name}`);
  } catch (error) {
    failed += 1;
    console.error(`not ok ${name}`);
    console.error(error.message);
  }
}

const validFiles = await jsonFiles(examplesDir);
for (const file of validFiles) {
  await testAsync(`valid example ${file}`, async () => {
    const data = JSON.parse(await readFile(path.join(examplesDir, file), "utf8"));
    assert.equal(validate(data), true, JSON.stringify(validate.errors, null, 2));
  });
}

const invalidFiles = await jsonFiles(invalidExamplesDir);
for (const file of invalidFiles) {
  await testAsync(`invalid example ${file} rejected`, async () => {
    const data = JSON.parse(await readFile(path.join(invalidExamplesDir, file), "utf8"));
    assert.equal(validate(data), false, `${file} should fail schema validation`);
  });
}

const policyChangeFiles = validFiles.filter((file) => file.startsWith("policy-change-"));
for (const file of policyChangeFiles) {
  await testAsync(`invariant ${file}`, async () => {
    const policyChange = JSON.parse(await readFile(path.join(examplesDir, file), "utf8"));
    assert.equal(policyChange.kind, "PolicyChange");
    assertEvaluationUsesPreviousPolicy(policyChange, file);
  });
}

await testAsync("authority root includes policy.grant and authority.rotate capabilities", async () => {
  const rootPolicy = await readJson("examples/policy-authority-root.json");
  const capabilityIds = rootPolicy.rules.flatMap((rule) =>
    rule.capabilities.map((capability) => capability.id)
  );
  assert.ok(capabilityIds.includes("capability_policy_grant"));
  assert.ok(capabilityIds.includes("capability_authority_rotate"));
});

await testAsync("denied self-grant example documents deny outcome", async () => {
  const policyChange = await readJson("examples/policy-change-self-grant-denied.json");
  assert.equal(policyChange.trust, "denied");
  assert.equal(policyChange.metadata.outcome, "deny");
  assert.deepEqual(
    policyChange.operation.grant.capabilities.map((capability) => capability.id),
    ["capability_secret_inject", "capability_policy_grant"]
  );
  assert.equal(policyChange.operation.grant.principal.id, "principal_unprivileged_agent");
});

await testAsync("allowed delegated grant stays within docs scope", async () => {
  const policyChange = await readJson("examples/policy-change-delegated-grant-allowed.json");
  assert.equal(policyChange.trust, "trusted");
  assert.equal(policyChange.metadata.outcome, "allow");
  assert.equal(policyChange.operation.grant.scopeConstraint.ref, "docs/");
});

await testAsync("denied scope broadening targets repo-wide policy.grant", async () => {
  const policyChange = await readJson("examples/policy-change-scope-broadening-denied.json");
  assert.equal(policyChange.operation.type, "policy.delegate");
  assert.equal(policyChange.operation.delegation.scope.type, "repo");
  assert.equal(
    policyChange.operation.delegation.capabilities[0].id,
    "capability_policy_grant"
  );
  assert.equal(policyChange.trust, "denied");
});

await testAsync("authority rotation meets weighted threshold signatures", async () => {
  const policyChange = await readJson("examples/policy-change-authority-rotation.json");
  assert.equal(policyChange.operation.type, "authority.rotate");
  const signatureWeight = policyChange.signatures.reduce(
    (total, signature) => total + signature.weight,
    0
  );
  assert.ok(signatureWeight >= policyChange.operation.rotation.threshold.minimum);
  assert.equal(policyChange.signatures.length, 2);
});

await testAsync("unsigned policy change is untrusted", async () => {
  const policyChange = await readJson("examples/policy-change-unsigned-untrusted.json");
  assert.equal(policyChange.signatures.length, 0);
  assert.equal(policyChange.trust, "untrusted");
  assert.equal(policyChange.metadata.outcome, "deny");
});

console.log(`\nTests: ${passed} passed, ${failed} failed`);
if (failed > 0) {
  process.exit(1);
}
