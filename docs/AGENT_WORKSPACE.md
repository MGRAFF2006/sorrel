# Agent workspace model

Sorrel is a single monorepo. Edit any package under the root checkout and commit
once in this repository.

## Layout

| Path | Responsibility |
| --- | --- |
| Root docs / `tests/` / `scripts/` | Architecture, status, full-stack tests |
| `sorrel-*` packages | Implementation and package-local tests |
| Root `Cargo.toml` | Rust workspace for core / cli / runners / sdk-rust |

```sh
git status
cargo test --workspace
npm run test:modules
npm test
```

## Change workflow

1. Branch in the root repository.
2. Change the affected package(s).
3. Run the package suite and, for cross-package work, root E2E.
4. Commit and open one PR against `main`.

Cross-package Rust changes use path dependencies. There are no git revision pins
and no submodule pointer advances.

## Conformance

Protocol conformance fixtures are vendored into consumers and checked by
`npm run validate:conformance`. Update them with
`./scripts/sync-conformance.sh` when the protocol manifest changes.
