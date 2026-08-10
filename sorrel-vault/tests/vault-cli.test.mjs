import assert from "node:assert/strict";
import path from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";

import {
  buildActor,
  evaluateGrant,
  importEnv,
  listSecretRefs,
  loadSpecForCli,
  parseArgs,
  redactInput
} from "../scripts/lib/cli.mjs";

const here = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(here, "..");
const SPEC_PATH = path.join("examples", "sorrel.secrets.dev.yml");

async function loadSpec() {
  const { spec, baseDir } = await loadSpecForCli({ specPath: SPEC_PATH, baseDir: repoRoot });
  return { spec, baseDir };
}

// Actor tuple that satisfies the dev grants in examples/sorrel.secrets.dev.yml,
// which constrain agent, workflow, and runner dimensions simultaneously.
const GRANTED_PRINCIPAL =
  "AgentPolicy:agent_policy_local_dev,Workflow:workflow_validate_vault,Runner:runner_local_process";

// Raw fixture values from examples/.env.dev.example; tests assert these never
// appear in CLI output.
const RAW_NPM_TOKEN = "dev-token-example-do-not-use";
const RAW_DATABASE_URL = "postgres://sorrel:sorrel@localhost:5432/sorrel_dev";

test("parseArgs collects flags, positionals, and boolean help", () => {
  const parsed = parseArgs(["grant", "--secret", "s1", "--action=read", "--help"]);
  assert.equal(parsed.command, "grant");
  assert.equal(parsed.flags.secret, "s1");
  assert.equal(parsed.flags.action, "read");
  assert.equal(parsed.flags.help, true);
});

test("buildActor applies a bare id across all actor slots", () => {
  const actor = buildActor("workflow_validate_vault");
  assert.equal(actor.agent.id, "workflow_validate_vault");
  assert.equal(actor.workflow.id, "workflow_validate_vault");
  assert.equal(actor.runner.id, "workflow_validate_vault");
});

test("buildActor honors an explicit Kind:id principal", () => {
  const actor = buildActor("Workflow:workflow_validate_vault");
  assert.equal(actor.workflow.kind, "Workflow");
  assert.equal(actor.workflow.id, "workflow_validate_vault");
  assert.equal(actor.agent, undefined);
});

test("list returns declared SecretRef handles without any values", async () => {
  const { spec } = await loadSpec();
  const result = listSecretRefs(spec);

  assert.equal(result.count, 2);
  const ids = result.secretRefs.map((ref) => ref.id).sort();
  assert.deepEqual(ids, ["secret_database_url_dev", "secret_npm_token_dev"]);

  const serialized = JSON.stringify(result);
  assert.ok(!serialized.includes(RAW_NPM_TOKEN), "list must not contain a raw value");
  assert.ok(!serialized.includes(RAW_DATABASE_URL), "list must not contain a raw value");
  // No SecretRef-derived field carries a plaintext value.
  for (const ref of result.secretRefs) {
    assert.equal(ref.value, undefined);
    assert.equal(ref.secretValue, undefined);
  }

  for (const ref of result.secretRefs) {
    assert.ok(ref.name, "ref exposes a name handle");
    assert.equal(ref.environment, "dev");
    assert.ok(ref.boundEnvironments.includes("dev"));
  }
});

test("grant returns allow for a granted actor/action", async () => {
  const { spec } = await loadSpec();
  const decision = evaluateGrant(spec, {
    secret: "secret_npm_token_dev",
    environment: "dev",
    action: "read",
    principal: GRANTED_PRINCIPAL
  });

  assert.equal(decision.status, "allow");
  assert.equal(decision.allowed, true);
  assert.equal(decision.grant, "grant_dev_validation_runner");
  assert.ok(!JSON.stringify(decision).includes(RAW_NPM_TOKEN));
});

test("grant returns needs_grant for an unknown actor", async () => {
  const { spec } = await loadSpec();
  const decision = evaluateGrant(spec, {
    secret: "secret_npm_token_dev",
    environment: "dev",
    action: "read",
    principal: "Workflow:not_a_real_workflow"
  });

  assert.equal(decision.status, "needs_grant");
  assert.equal(decision.allowed, false);
  assert.equal(decision.grant, undefined);
});

test("grant returns needs_grant for an action not in the grant", async () => {
  const { spec } = await loadSpec();
  // The dev grants allow read/materialize/redact but not inject.
  const decision = evaluateGrant(spec, {
    secret: "secret_npm_token_dev",
    environment: "dev",
    action: "inject",
    principal: GRANTED_PRINCIPAL
  });

  assert.equal(decision.allowed, false);
  assert.notEqual(decision.status, "allow");
});

test("import reports imported keys without exposing values", async () => {
  const { spec, baseDir } = await loadSpec();
  const result = await importEnv(spec, { baseDir, environment: "dev" });

  assert.ok(result.importedCount >= 2);
  const envKeys = result.importedKeys.map((key) => key.envKey).sort();
  assert.deepEqual(envKeys, ["DATABASE_URL", "NPM_TOKEN"]);

  const serialized = JSON.stringify(result);
  assert.ok(!serialized.includes(RAW_NPM_TOKEN), "import must not expose raw NPM token");
  assert.ok(!serialized.includes(RAW_DATABASE_URL), "import must not expose raw database URL");
});

test("redact masks known secret values in sample text", async () => {
  const { spec, baseDir } = await loadSpec();
  const sample = [
    `NPM token is ${RAW_NPM_TOKEN} right here.`,
    `DB is ${RAW_DATABASE_URL}.`
  ].join("\n");

  const result = await redactInput(spec, sample, {
    baseDir,
    principal: GRANTED_PRINCIPAL
  });

  assert.ok(!result.redacted.includes(RAW_NPM_TOKEN), "raw NPM token must be redacted");
  assert.ok(!result.redacted.includes(RAW_DATABASE_URL), "raw database URL must be redacted");
  assert.ok(result.redacted.includes("***"), "redacted output contains the mask");
  assert.equal(result.redactedSecretCount, 2);
});

test("redact does not surface values for an ungranted principal", async () => {
  const { spec, baseDir } = await loadSpec();
  const sample = `NPM token is ${RAW_NPM_TOKEN}.`;

  const result = await redactInput(spec, sample, {
    baseDir,
    principal: "Workflow:not_a_real_workflow"
  });

  // No grants resolved; nothing contributed to the secret-value set, but the
  // env-key detector still redacts and no raw value is ever emitted by the lib.
  assert.equal(result.redactedSecretCount, 0);
});
