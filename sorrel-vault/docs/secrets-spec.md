# `sorrel.secrets.yml` draft

`sorrel.secrets.yml` is the Sorrel Vault project-level manifest for secret
handles, local development imports, redaction defaults, and access grants.

The draft schema lives at `schemas/sorrel-secrets.schema.json` and uses
`schemaVersion: sorrel.vault.v0`.

## Top-level shape

```yaml
schemaVersion: sorrel.vault.v0
kind: SecretSpec
project: {}
environments: {}
secretRefs: []
grants: []
localDev: {}
redaction: {}
```

## Secret references

Each item in `secretRefs` uses the `sorrel-protocol` `SecretRef` shape:

```yaml
- schemaVersion: sorrel.protocol.v0
  kind: SecretRef
  id: secret_npm_token_dev
  name: NPM_TOKEN
  provider: sorrel-vault
  uri: secret://project/sorrel-vault/dev/NPM_TOKEN
  environment: dev
  required: false
  valueType: token
```

Only the `sorrel-vault` provider is accepted in this draft. Future cloud
providers should add provider-specific backends without changing the rule that
protocol objects store handles, not raw values.

## `.env` import design

`.env` import is scoped under `localDev`:

```yaml
localDev:
  backend: local-dev
  import:
    envFiles:
      - path: examples/.env.dev.example
        environment: dev
        required: true
    defaultKeyStrategy: secretRefName
    allowProcessEnvFallback: false
  storage:
    namespace: local://sorrel-vault/dev
    persist: false
    materialize: environment
  bindings:
    - secret:
        kind: SecretRef
        id: secret_npm_token_dev
      environment: dev
      envKey: NPM_TOKEN
      storeKey: project/sorrel-vault/dev/NPM_TOKEN
```

Design constraints:

- `.env` files are read only by the local backend.
- The manifest records `envKey` and `storeKey`, never the imported value.
- `persist: false` means the prototype keeps values in memory.
- `allowProcessEnvFallback` is explicit and defaults to false in examples.
- Required `.env` files fail validation/demo imports when absent.

## Grant model

`grants` authorize a specific secret handle, environment, action set, and access
tuple. A request is allowed when:

1. The requested `{ kind: SecretRef, id }` matches `grant.secret`.
2. The requested environment matches `grant.environment`.
3. The action is listed in `grant.actions`.
4. Every constrained access category matches the actor context.

Example:

```yaml
grants:
  - id: grant_dev_validation_runner
    secret:
      kind: SecretRef
      id: secret_npm_token_dev
    environment: dev
    actions: [read, materialize, redact]
    access:
      agents:
        - kind: AgentPolicy
          id: agent_policy_local_dev
      workflows:
        - kind: Workflow
          id: workflow_validate_vault
      runners:
        - kind: Runner
          id: runner_local_process
    policy:
      kind: Policy
      id: policy_vault_dev
```

The access tuple references `AgentPolicy`, `Workflow`, and `Runner` objects from
`sorrel-protocol`. `Policy` references describe the policy object that owns the
grant.

In the local backend, `grants[]` describe intended access but do not authorize
resolution on their own. A trusted Core policy `evaluate()` must return `allow`:

- `secret.read` for `read` and `redact`
- `secret.inject` for `materialize` and `import`

When Core returns `needs_grant` or `deny`, vault resolution and materialization
both block before exposing raw values. Local grant YAML alone does not bypass Core.

## Redaction

The manifest defines redaction defaults:

```yaml
redaction:
  mask: "***"
  minSecretLength: 6
  visiblePrefix: 2
  visibleSuffix: 2
  detectEnvKeys: [TOKEN, SECRET, PASSWORD]
```

The helper in `scripts/lib/redaction.mjs` can redact known resolved values and
env-style assignments such as `NPM_TOKEN=value`. It can also redact resolved
SecretRef ids, URIs, and local store keys so logs can expose shape without
leaking handles or values.

## Environment examples

- `examples/sorrel.secrets.dev.yml` demonstrates local `.env` import and
  materialization for a developer runner.
- `examples/sorrel.secrets.staging.yml` demonstrates shared staging handles and
  release workflow grants.
- `examples/sorrel.secrets.prod.yml` demonstrates protected production handles
  with read/redact-only grants and no local materialization.
