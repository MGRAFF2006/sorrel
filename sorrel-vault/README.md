# sorrel-vault

Sorrel Vault defines the draft `sorrel.secrets.yml` contract and a local Node
development backend for resolving Sorrel secret handles.

**Primary UX:** humans and agents should use `sorrel secret *` in `sorrel-cli`,
which resolves through upstream [SecretSpec](https://github.com/cachix/secretspec)
providers (`keyring`, `dotenv`, `env`, …) after Core `secret.read` /
`secret.inject` grants. This package remains for schema, examples, and
conformance tests — not the front door.

Providers beyond the local Node backend are implemented by SecretSpec; do not
add custom AWS/1Password adapters here.

## What is included

- `schemas/sorrel-secrets.schema.json` - JSON Schema draft for
  `sorrel.secrets.yml`.
- `examples/sorrel.secrets.dev.yml` - local development SecretRef examples and
  fake `.env` import bindings.
- `examples/sorrel.secrets.staging.yml` - shared pre-production SecretRef and
  grant examples.
- `examples/sorrel.secrets.prod.yml` - protected production SecretRef examples
  with no materialization grant.
- `scripts/lib/local-backend.mjs` - in-memory local dev backend prototype.
- `scripts/lib/grants.mjs` - grant evaluator for agent, workflow, and runner
  access, mapped to Core-style `PolicyRequest`/`PolicyDecision` objects.
- `scripts/lib/redaction.mjs` - redaction helpers for resolved values and
  SecretRef/env-style output.
- `scripts/vault-cli.mjs` - dependency-free CLI (`list`, `grant`, `import`,
  `redact`) that composes the library modules; logic lives in
  `scripts/lib/cli.mjs`.

## CLI

A small CLI exposes the local vault operations by composing the library modules.
It never prints or persists raw secret values.

```sh
node scripts/vault-cli.mjs --help          # usage
node scripts/vault-cli.mjs list            # declared SecretRef handles
node scripts/vault-cli.mjs import --env dev # import declared .env files (keys only)
node scripts/vault-cli.mjs grant \
  --secret secret_npm_token_dev --env dev --action read \
  --principal "AgentPolicy:agent_policy_local_dev,Workflow:workflow_validate_vault,Runner:runner_local_process"
cat some.log | node scripts/vault-cli.mjs redact \
  --principal "AgentPolicy:agent_policy_local_dev,Workflow:workflow_validate_vault,Runner:runner_local_process"
```

The `vault` npm script (`npm run vault -- list`) wraps the same entrypoint. See
`docs/local-dev.md` for full flag documentation.

## Relationship to sorrel-protocol

The vault spec embeds the protocol `SecretRef` shape and uses protocol object
references for:

- `Policy` - environment and grant policy ownership.
- `Runner` - runner allowlists and grant constraints.
- `Workflow` - workflow-level grant constraints.
- `AgentPolicy` - agent access constraints.

Raw secret values never belong in `SecretRef`, `Policy`, `Runner`, `Workflow`,
or `AgentPolicy` objects. `sorrel.secrets.yml` stores handles and local import
bindings only.

## Local development flow

1. Declare `SecretRef` handles in `sorrel.secrets.yml`.
2. Bind handles to `.env` keys under `localDev.bindings`.
3. Import fake or developer-local `.env` files with the `local-dev` backend.
4. Resolve handles only when a trusted Core policy `evaluate()` returns `allow` for
   the requesting `AgentPolicy`/`Workflow`/`Runner` tuple.
5. Emit/model an audit event and redaction metadata for the access attempt.
6. Redact SecretRef identifiers/URIs/store keys and values before logging.

## Core policy consultation

Every resolution builds a Core-shaped `PolicyRequest` and requires a trusted Core
policy decision before returning raw values:

- `read` and `redact` actions require `secret.read`.
- `materialize` and `import` actions require `secret.inject`.
- `subject` is the caller-provided `AgentPolicy`/`Workflow`/`Runner` context.
- `resource` is the protocol `{ kind: "SecretRef", id }`.

Vault never treats local `grants[]` YAML as an authorization source on its own.
Callers must inject a `corePolicy` that exposes `evaluate(request)` (or a
function/`decide(request)` adapter). Any `deny` or `needs_grant` decision blocks
resolution and is attached to `AccessDeniedError`.

For local development, `createLocalDevCorePolicy(spec)` in `scripts/lib/grants.mjs`
provides an explicit adapter that consults vault grants through Core's
`evaluate()` shape. Wire it deliberately; it is not the default backend behavior.

### Policy conformance

`tests/policy-conformance.test.mjs` runs the canonical `sorrel-protocol` policy
conformance manifest (vendored at `tests/conformance/policy-conformance.json`)
against the `corePolicy.evaluate()` adapter for `secret.read` and `secret.inject`,
and asserts that local grant YAML alone cannot bypass Core (no injected
`corePolicy` returns `needs_grant`, never `allow`). It runs as part of
`npm test`.

The vendored manifest is paired with a sidecar `policy-conformance.meta.json`
(version + SHA-256) from `sorrel-protocol`. `tests/conformance-sync.test.mjs`
recomputes the manifest hash and fails if it drifts from the sidecar, so a stale
vendored copy is caught by `npm test`. To refresh, re-export from a
`sorrel-protocol` checkout:

```sh
# from sorrel-protocol/
npm run export:conformance -- <path-to>/sorrel-vault/tests/conformance
```

(or run the root `scripts/sync-conformance.sh`), then re-run the tests. See
`tests/conformance/README.md`.

Run the demo:

```sh
npm run demo:local
```

Expected output is redacted:

```text
Local dev backend resolved handles:
- NPM_TOKEN (pr***EN) = de***se
- DATABASE_URL (pr***RL) = po***ev
No values are persisted by this prototype.
```

## Validation

Install dependencies once:

```sh
npm ci
```

Validate the schema and bundled YAML examples:

```sh
npm run validate
```

Run the local backend tests:

```sh
npm test
```

See `docs/secrets-spec.md` and `docs/local-dev.md` for the schema draft,
`.env` import design, grant semantics, and validation examples.
