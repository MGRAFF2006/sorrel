# Agent instructions for sorrel-protocol

## What this module is

The canonical, language-neutral contracts for Sorrel: JSON schemas, examples,
and the **policy-conformance manifest** that every consumer vendors and tests
against. This repo is the source of truth for shared object shapes (`Principal`,
`Policy`, `Grant`, `PolicyDecision`, `SecretRef`, `AuthorityRoot`,
`PolicyChange`, ...) and for cross-language policy-decision conformance.

## Stack and conventions

- Node + JSON Schema (ajv). ES modules.
- The conformance manifest (`conformance/policy-conformance.json`) and its
  sidecar (`policy-conformance.meta.json`, version + SHA-256) are authoritative.
  After changing the manifest, regenerate the sidecar (`npm run sync:meta`).

## Impact awareness

- Changes here ripple into `sorrel-core`, `sorrel-cli`, `sorrel-hub`,
  `sorrel-runners`, and `sorrel-vault`, which vendor the manifest + sidecar and
  run drift-guard tests. Coordinate breaking schema changes deliberately.
- `npm run export:conformance -- <dir>` (and the root `scripts/sync-conformance.sh`)
  refresh vendored copies in consumers.

## Common checks

```sh
npm test
npm run validate    # schema + examples + sidecar currency
```

## Workflow

- Keep changes scoped to this repository (it *is* the shared-contract home).
- Prefer small, reviewable commits.
- Do not commit secrets.
