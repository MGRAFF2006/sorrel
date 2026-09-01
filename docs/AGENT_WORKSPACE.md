# Agent workspace guide

Sorrel is a single monorepo. Edit any package under the root checkout and commit
once in this repository. This is the compact routing guide; use
[`DEVELOPMENT.md`](DEVELOPMENT.md) for the full setup, service, debugging, and
release workflow.

## First five minutes

```sh
git status --short --branch
npm run setup
npm run check:quick
npm run test:module -- --list
```

Read the root [`AGENTS.md`](../AGENTS.md), then the `AGENTS.md` in every package
you will touch. Existing worktree changes are user-owned unless proven
otherwise; do not reset, overwrite, or reformat them as drive-by cleanup.

## Layout

| Path | Responsibility |
| --- | --- |
| Root docs / `tests/` / `scripts/` | Architecture, status, full-stack tests |
| `sorrel-*` packages | Implementation and package-local tests |
| Root `Cargo.toml` | Rust workspace for core / cli / runners / sdk-rust |

## Dependency direction

| Change starts in | Check likely consumers |
| --- | --- |
| `sorrel-protocol` contracts / conformance | core, CLI, Hub, runners, vault, SDKs |
| `sorrel-core` objects / policy / storage | CLI, runners, Rust SDK, Hub contract assumptions |
| `sorrel-hub` routes / auth / capabilities | Hub UI, web host proxy, JS SDK, agents, root E2E |
| `sorrel-hub-ui` product behavior | web host and root E2E |
| `sorrel-hub-desktop` | Tauri host, native adapters, desktop bundles only |
| `sorrel-hub-web` | browser hosting, proxying, Docker only; product UI belongs in `sorrel-hub-ui` |
| `sorrel-hub-mobile` | native Hub companion behavior, secure device storage, mobile/tablet navigation |
| `sorrel-web` | public marketing/docs only; never Hub product behavior |

Shared policy and authority semantics flow from Protocol/Core outward. UI,
Hub, Vault, Runners, Agents, and SDKs must not invent parallel authorization
rules or persist raw secret values.

## Change workflow

1. Inspect `git status` and relevant diffs before editing.
2. Make the smallest coherent change in the owning package(s).
3. Run `npm run check:quick` and focused suites with
   `npm run test:module -- <module> ...`.
4. Run `npm test` for cross-package behavior; use `npm run check` for the full
   repository gate.
5. Commit and open one PR against `main` when asked.

User-visible behavior is collected automatically from merged pull requests
during release preparation. Use a clear user-facing PR title; do not manually
edit routine `Unreleased` entries. GitHub Releases and the generated changelogs
are the progress record; do not create parallel agent-note or feature-audit
documents.

Cross-package Rust changes use path dependencies. There are no git revision pins
and no submodule pointer advances.

`npm run test:modules` runs each Node package's `check` script (tests plus its
lint, schema validation, typecheck, or production build as applicable) and
installs missing dependencies for convenience. However,
`npm run setup` is the explicit, deterministic bootstrap command used for a
fresh checkout.

## Conformance

Protocol conformance fixtures are vendored into consumers and checked by
`npm run validate:conformance`. Update them with
`./scripts/sync-conformance.sh` when the protocol manifest changes.

## Generated and mirrored files

- `sorrel-*/test(s)/conformance/` is generated from
  `sorrel-protocol/conformance/`; never hand-edit consumer copies.
- Public Markdown in `sorrel-web/docs/` mirrors canonical guides under `docs/`
  plus the root `CHANGELOG.md`; run `npm run sync:docs` after changing a source.
- `target/`, package `node_modules/`, package `dist/`, `.dev/`, Hub data, and
  local `.env*` files are not source and must remain uncommitted.

## Validation ladder

```sh
# Cheap structural checks (release metadata, conformance, docs)
npm run check:quick

# One or several affected packages
npm run test:module -- sorrel-hub sorrel-hub-ui sorrel-hub-web

# Rust-only changes
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check

# Full repository behavior
npm run check
```
