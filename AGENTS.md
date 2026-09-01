# Sorrel

Sorrel is an agent-native version-control system in a **single monorepo**.

Implementation packages live as normal directories:

- sorrel-protocol: schemas, examples, and the policy-conformance manifest
- sorrel-core: Rust engine — object store, snapshots, changes, lanes, policy spine
- sorrel-cli: the `sorrel` CLI (persistent local VCS over the engine)
- sorrel-vault: secrets/environment spec, local backend, and dev CLI
- sorrel-runners: local/container workflow runners + workflow-file parser
- sorrel-slices: TypeScript/JavaScript slice manifest generator
- sorrel-hub: collaboration **API server** (JSON over HTTP; AuthAdapter + capabilities)
- sorrel-hub-ui: shared SolidJS Hub product UI (web + desktop shells)
- sorrel-hub-web: thin browser host for `sorrel-hub-ui`
- sorrel-hub-mobile: native iOS/iPadOS/Android Hub companion (React Native + Expo)
- sorrel-web: public marketing/landing site (static)
- sorrel-agents: minimal agent control plane (register / claim / active work)
- sorrel-sdk-js: Hub HTTP client
- sorrel-sdk-rust: thin Rust SDK over `sorrel-core`

Hub is split: `sorrel-hub` is the API server, `sorrel-hub-ui` is the shared
web/desktop product UI, `sorrel-hub-web` is the thin browser host,
`sorrel-hub-mobile` is the native phone/tablet companion, and `sorrel-web` is
the unrelated public landing page.

Start with [`docs/AGENT_WORKSPACE.md`](docs/AGENT_WORKSPACE.md) for change
routing and [`docs/DEVELOPMENT.md`](docs/DEVELOPMENT.md) for setup, validation,
services, and release hygiene. Current architecture and behavior live in
[`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) and
[`docs/STATUS.md`](docs/STATUS.md).

Project progress is recorded in [GitHub Releases](https://github.com/MGRAFF2006/sorrel/releases)
and [`CHANGELOG.md`](CHANGELOG.md). `ROADMAP.md` is future work only. Do not add
parallel agent task packs, progress dashboards, or feature-audit ledgers.

Do not manually add routine `Unreleased` changelog entries. Release preparation
derives entries from merged pull requests, maps changed paths to affected
packages, and opens a reviewable changelog PR. Write a clear, user-facing PR
title; the automation understands Conventional Commit prefixes but does not
require them. Maintainers may apply `skip-changelog` to omit changes with no
user or operator impact. Hand-edit generated release prose only in the release
PR when it needs clarification.

## Rust toolchain

Rust modules require stable Rust 1.85+ with clippy and rustfmt. If needed:

```sh
rustup toolchain install stable --profile minimal -c clippy -c rustfmt
rustup default stable
cargo fetch
```

Rust crates are a Cargo workspace. Prefer workspace commands from the repo root:

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

## Common checks

Install every package's locked dependencies once: `npm run setup`

Fast repository consistency checks: `npm run check:quick`

Focused module suite: `npm run test:module -- <module>` (discover names with
`npm run test:module -- --list`)

All module suites (tests plus package-defined validation/builds):
`npm run test:modules`

Root E2E (no mocks, all active modules): `npm test`

Complete repository gate: `npm run check`

Node packages expose `npm run check` as their complete local gate.

After changing a canonical public Markdown guide or the root changelog, run
`npm run sync:docs`; `npm run validate:docs` checks every public mirror and
local Markdown link.

Do not hand-edit vendored policy-conformance fixtures. Change
`sorrel-protocol/conformance/`, then run `./scripts/sync-conformance.sh`.

## Workflow

Edit files under `sorrel-core/`, `sorrel-cli/`, etc. directly. Package-level
`AGENTS.md` files add local boundaries and checks. Commit once in this root
repository — there are no submodule pointer advances and no `SUBMODULES_TOKEN`
requirement for CI.

Preserve a dirty worktree: existing changes may belong to another human or
agent. Inspect `git status` and relevant diffs before editing, avoid unrelated
formatting, and never discard changes you did not create.

Use the repository pull-request template and keep its impact checklist intact.
PR descriptions must explain why the change is needed, summarize user-visible
and compatibility effects, and list the checks actually run. Issues should use
the closest matching issue form and include reproducible evidence or concrete
acceptance criteria. Never include secrets in issue or PR text, screenshots,
logs, or fixtures.
