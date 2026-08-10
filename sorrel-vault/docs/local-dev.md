# Local development backend prototype

The local dev backend is an in-memory prototype for exercising
`sorrel.secrets.yml` without a cloud provider.

It is implemented by `scripts/lib/local-backend.mjs`.

## Import

```js
import { createLocalDevCorePolicy } from "./scripts/lib/grants.mjs";
import { LocalDevSecretBackend } from "./scripts/lib/local-backend.mjs";
import { loadSecretSpec } from "./scripts/lib/spec-loader.mjs";

const spec = await loadSecretSpec("examples/sorrel.secrets.dev.yml");
const backend = new LocalDevSecretBackend(spec, {
  corePolicy: createLocalDevCorePolicy(spec)
});

await backend.importEnvFiles();
```

The backend reads files declared in `localDev.import.envFiles`, parses
`KEY=VALUE` pairs, then stores values by `localDev.bindings[].storeKey`.

## Resolve

```js
const resolved = backend.resolve({
  secret: { kind: "SecretRef", id: "secret_npm_token_dev" },
  environment: "dev",
  action: "read",
  actor: {
    agent: { kind: "AgentPolicy", id: "agent_policy_local_dev" },
    workflow: { kind: "Workflow", id: "workflow_validate_vault" },
    runner: { kind: "Runner", id: "runner_local_process" }
  }
});

console.log(resolved.redacted);
```

Resolution returns the protocol `SecretRef`, matching grant, Core-shaped
`policyDecision`, local store key, raw value, redacted value, redaction metadata,
and a modeled `AuditEvent`. Callers should avoid logging `resolved.value`.

## Materialize environment

```js
const env = backend.materializeEnv([
  {
    secret: { kind: "SecretRef", id: "secret_database_url_dev" },
    environment: "dev",
    actor
  }
]);
```

`materializeEnv` returns an object suitable for process environment injection.
It still requires Core policy to allow `secret.inject`.

## Core policy

The local backend builds a Core-shaped `PolicyRequest` for every resolution:

- `read`/`redact` map to `secret.read`
- `materialize`/`import` map to `secret.inject`
- `subject` is the supplied `AgentPolicy`/`Workflow`/`Runner` actor tuple
- `resource` is the requested `{ kind: "SecretRef", id }`

Vault requires a trusted Core policy decision via `corePolicy.evaluate(request)`
(or a function/`decide(request)` adapter). Without an injected `corePolicy`, every
resolution returns `needs_grant` even when matching `grants[]` entries exist in
the spec. Local grant YAML documents intent but does not bypass Core.

For local development, wire `createLocalDevCorePolicy(spec)` to consult vault
grants through Core's `evaluate()` interface. Production callers should inject
sorrel-core's permission evaluator instead.

## Denied access

If Core policy returns `deny` or `needs_grant`, `resolve` throws
`AccessDeniedError` from `scripts/lib/grants.mjs`. The error includes the
`decision` and a redacted audit event for the denied attempt.

Common causes:

- wrong `AgentPolicy` id
- workflow not listed in the grant
- runner not listed in the grant
- action missing from `grant.actions`
- environment mismatch

## CLI

`scripts/vault-cli.mjs` is a small, dependency-free CLI that composes the library
modules above. It never prints or persists raw secret values. Run it via the
`vault` npm script or directly with `node`:

```sh
node scripts/vault-cli.mjs --help
```

Global options:

- `--spec <path>` — secret spec (default `examples/sorrel.secrets.dev.yml`).
- `--env <name>` — environment filter/target.

Commands:

```sh
# List declared SecretRef handles and the environments that define them.
node scripts/vault-cli.mjs list

# Import a .env file into the local backend (defaults to spec-declared
# envFiles; use --file to override). Reports bound keys only.
node scripts/vault-cli.mjs import --env dev
node scripts/vault-cli.mjs import --env dev --file examples/.env.dev.example

# Evaluate access for a principal/secret/environment/action. Prints the
# Core-grant decision (allow / deny / needs_grant). Exits non-zero when not
# allowed.
node scripts/vault-cli.mjs grant \
  --secret secret_npm_token_dev --env dev --action read \
  --principal "AgentPolicy:agent_policy_local_dev,Workflow:workflow_validate_vault,Runner:runner_local_process"

# Redact text from a file or stdin using the spec redaction policy.
cat some.log | node scripts/vault-cli.mjs redact --principal "AgentPolicy:agent_policy_local_dev,Workflow:workflow_validate_vault,Runner:runner_local_process"
```

`--principal` (alias `--actor`) accepts comma-separated `Kind:id` components
(`AgentPolicy`, `Workflow`, `Runner`) or a bare id applied to all three slots.
Vault grants often constrain agent, workflow, and runner together, so supply all
three components to satisfy a fully-specified grant.

The command logic lives in `scripts/lib/cli.mjs` as pure functions
(`listSecretRefs`, `evaluateGrant`, `importEnv`, `redactInput`, `buildActor`,
`parseArgs`) so it is unit-testable without spawning a process; see
`tests/vault-cli.test.mjs`.

## Validation commands

Compile the JSON Schema:

```sh
npm run validate:schemas
```

Validate all YAML examples and semantic references:

```sh
npm run validate:examples
```

Run the prototype tests:

```sh
npm test
```

Run the redacted local demo:

```sh
npm run demo:local
```
