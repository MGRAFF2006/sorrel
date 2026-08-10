import path from "node:path";
import process from "node:process";
import { createLocalDevCorePolicy } from "./lib/grants.mjs";
import { LocalDevSecretBackend } from "./lib/local-backend.mjs";
import { collectResolvedSecretRefs, collectResolvedSecretValues, redactText } from "./lib/redaction.mjs";
import { loadSecretSpec } from "./lib/spec-loader.mjs";

const root = process.cwd();
const specPath = path.join(root, "examples", "sorrel.secrets.dev.yml");
const spec = await loadSecretSpec(specPath);
const backend = new LocalDevSecretBackend(spec, {
  baseDir: root,
  corePolicy: createLocalDevCorePolicy(spec)
});

await backend.importEnvFiles();

const actor = {
  agent: { kind: "AgentPolicy", id: "agent_policy_local_dev" },
  workflow: { kind: "Workflow", id: "workflow_validate_vault" },
  runner: { kind: "Runner", id: "runner_local_process" }
};

const resolved = [
  backend.resolve({
    secret: { kind: "SecretRef", id: "secret_npm_token_dev" },
    environment: "dev",
    action: "read",
    actor
  }),
  backend.resolve({
    secret: { kind: "SecretRef", id: "secret_database_url_dev" },
    environment: "dev",
    action: "read",
    actor
  })
];

const log = [
  "Local dev backend resolved handles:",
  ...resolved.map((secret) => `- ${secret.secretRef.name} (${secret.storeKey}) = ${secret.value}`),
  "No values are persisted by this prototype."
].join("\n");

console.log(
  redactText(log, collectResolvedSecretValues(resolved), spec.redaction, {
    secretRefs: collectResolvedSecretRefs(resolved)
  })
);
