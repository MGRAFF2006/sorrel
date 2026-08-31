# Agent instructions for sorrel-vault

## What this module is

Sorrel's secrets/environment layer: the secrets spec (`sorrel.secrets.yml`
schema), a local development backend, and a dependency-free CLI
(`vault-cli.mjs`) for import / list / grant / redact. Values are resolved only
after a Core-grant policy decision allows the requesting principal.

## Stack and conventions

- Node >= 20, ES modules. `ajv` and `js-yaml` are devDependencies; **no runtime
  dependencies**. The CLI composes existing `scripts/lib/*` modules — do not
  reimplement parsing, policy, backend, or redaction logic.

## Core boundary (critical)

- Raw secret values must **never** become Sorrel objects, be printed, persisted,
  or written to committed files. Only `.example` placeholders and `SecretRef`
  handles, schemas, grants, redaction, and audit metadata are allowed.
- Authorization maps onto Core grants/policy decisions; vault does not invent its
  own permission language.

## Common checks

```sh
npm ci
npm run check
node scripts/vault-cli.mjs --help
```

Do not modify `tests/conformance/`, `tests/policy-conformance.test.mjs`, or
`tests/conformance-sync.test.mjs` by hand.

## Workflow

- Keep changes scoped to this package and required workspace consumers.
- Prefer small, reviewable commits.
- Never commit real secrets.
- Coordinate shared contracts through `sorrel-protocol`.
