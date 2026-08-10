import assert from "node:assert/strict";
import path from "node:path";
import process from "node:process";
import {
  AccessDeniedError,
  createLocalDevCorePolicy,
  evaluateCorePolicyFromGrants
} from "./lib/grants.mjs";
import { LocalDevSecretBackend } from "./lib/local-backend.mjs";
import { collectResolvedSecretRefs, collectResolvedSecretValues, redactText } from "./lib/redaction.mjs";
import { loadSecretSpec } from "./lib/spec-loader.mjs";

const root = process.cwd();
const spec = await loadSecretSpec(path.join(root, "examples", "sorrel.secrets.dev.yml"));
const corePolicy = createLocalDevCorePolicy(spec);
const backend = new LocalDevSecretBackend(spec, { baseDir: root, corePolicy });

await backend.importEnvFiles();

const allowedActor = {
  agent: { kind: "AgentPolicy", id: "agent_policy_local_dev" },
  workflow: { kind: "Workflow", id: "workflow_validate_vault" },
  runner: { kind: "Runner", id: "runner_local_process" }
};

const deniedActor = {
  agent: { kind: "AgentPolicy", id: "agent_policy_docs" },
  workflow: { kind: "Workflow", id: "workflow_validate_vault" },
  runner: { kind: "Runner", id: "runner_local_process" }
};

const token = backend.resolve({
  secret: { kind: "SecretRef", id: "secret_npm_token_dev" },
  environment: "dev",
  action: "read",
  actor: allowedActor
});

assert.equal(token.value, "dev-token-example-do-not-use");
assert.equal(token.redacted, "de***se");
assert.equal(token.policyDecision.status, "allow");
assert.equal(token.policyDecision.capability, "secret.read");
assert.equal(token.auditEvent.outcome, "allow");
assert.equal(token.auditEvent.redaction.kind, "RedactionMetadata");

try {
  backend.resolve({
    secret: { kind: "SecretRef", id: "secret_npm_token_dev" },
    environment: "dev",
    action: "read",
    actor: deniedActor
  });
  assert.fail("denied actor should require a grant");
} catch (error) {
  assert.ok(error instanceof AccessDeniedError);
  assert.equal(error.decision.status, "needs_grant");
  assert.equal(error.decision.capability, "secret.read");
  assert.equal(error.auditEvent.outcome, "needs_grant");
}

const grantOnlyBackend = new LocalDevSecretBackend(spec, { baseDir: root });
grantOnlyBackend.importValues({ NPM_TOKEN: "dev-token-example-do-not-use" }, "dev");

try {
  grantOnlyBackend.resolve({
    secret: { kind: "SecretRef", id: "secret_npm_token_dev" },
    environment: "dev",
    action: "read",
    actor: allowedActor
  });
  assert.fail("local grant YAML alone must not bypass Core");
} catch (error) {
  assert.ok(error instanceof AccessDeniedError);
  assert.equal(error.decision.status, "needs_grant");
  assert.match(error.decision.reason, /Trusted Core policy decision required/);
  assert.match(error.decision.reason, /local grant YAML does not bypass Core/);
  assert.equal(error.auditEvent.outcome, "needs_grant");
}

const evaluateCalls = [];
const evaluateBackend = new LocalDevSecretBackend(spec, {
  baseDir: root,
  corePolicy: {
    evaluate(request) {
      evaluateCalls.push(request);
      return evaluateCorePolicyFromGrants(spec, {
        secret: request.resource,
        environment: request.environment,
        action: request.action,
        actor: request.subject ?? {}
      });
    }
  }
});
evaluateBackend.importValues({ NPM_TOKEN: "dev-token-example-do-not-use" }, "dev");

const evaluateResolved = evaluateBackend.resolve({
  secret: { kind: "SecretRef", id: "secret_npm_token_dev" },
  environment: "dev",
  action: "read",
  actor: allowedActor
});

assert.equal(evaluateResolved.value, "dev-token-example-do-not-use");
assert.equal(evaluateCalls.length, 1);
assert.equal(evaluateCalls[0].kind, "PolicyRequest");
assert.equal(evaluateCalls[0].capability, "secret.read");

const denyingBackend = new LocalDevSecretBackend(spec, {
  baseDir: root,
  corePolicy: {
    evaluate() {
      return {
        status: "deny",
        reason: "test policy denial"
      };
    }
  }
});
denyingBackend.importValues({ NPM_TOKEN: "dev-token-example-do-not-use" }, "dev");

assert.throws(
  () =>
    denyingBackend.resolve({
      secret: { kind: "SecretRef", id: "secret_npm_token_dev" },
      environment: "dev",
      action: "read",
      actor: allowedActor
    }),
  AccessDeniedError
);

const env = backend.materializeEnv([
  {
    secret: { kind: "SecretRef", id: "secret_database_url_dev" },
    environment: "dev",
    actor: allowedActor
  }
]);

assert.deepEqual(env, {
  DATABASE_URL: "postgres://sorrel:sorrel@localhost:5432/sorrel_dev"
});
assert.equal(
  backend.getAuditEvents().some((event) => event.capability === "secret.inject" && event.outcome === "allow"),
  true
);

const log = `token=${token.value}\nNPM_TOKEN=${token.value}`;
const redacted = redactText(
  `${log}\nsecret=${token.secretRef.id}\nuri=${token.secretRef.uri}\nstore=${token.storeKey}`,
  collectResolvedSecretValues([token]),
  spec.redaction,
  { secretRefs: collectResolvedSecretRefs([token]) }
);

assert.equal(redacted.includes(token.value), false);
assert.equal(redacted.includes(token.secretRef.id), false);
assert.equal(redacted.includes(token.secretRef.uri), false);
assert.equal(redacted.includes(token.storeKey), false);
assert.match(redacted, /NPM_TOKEN=de\*\*\*se/);

console.log("ok local dev backend core policy, grants, and redaction");
