# Agent instructions for sorrel-core

## What this module is

The Rust engine at the heart of Sorrel: content-addressed object store
(`FileObjectStore`/`InMemoryObjectStore`, BLAKE3 ids), snapshots, changes,
path-level diff, history/merge-base, three-way snapshot merge with first-class
conflicts, lanes/stacks, sync transport, and the headless
policy/permission/authority spine. This crate is the source of truth for VCS
objects and authorization semantics; CLI, runners, vault, and Hub consume it.

The engine exposes only its **native** policy/authority API
(`policy::evaluate_policy`, `authority::evaluate_policy_change`). The former
`cli_policy` compat module has been removed; the CLI owns its own CLI-facing
policy surface now.

## Stack and conventions

- Rust, edition 2021, **rust-version 1.85+** (edition2024-era toolchain).
- `unsafe_code = "forbid"`.
- Keep the public API additive; several workspace crates depend on it through
  path dependencies.

## Core boundary

- Policy mutation is governed by the *previous* effective policy: never let a
  change authorize itself. Reject self-grants/self-escalation/unsigned authority
  changes unless an already-authorized authority delegated the power.
- Keep raw secret values out of the object graph; only `SecretRef` + metadata.

## Common checks

```sh
# From the monorepo root
cargo build -p sorrel-core
cargo test -p sorrel-core
cargo clippy -p sorrel-core --all-targets -- -D warnings
cargo fmt --all -- --check
cargo bench -p sorrel-core --bench engine
```

`benches/engine.rs` is a `harness = false` (no criterion) micro-benchmark for
snapshot/diff/log-walk. It enforces loose mean-time budgets of 1.5 s for
snapshotting 2,000 files, 500 ms for diffing 2,000 files with 20
modifications, and 300 ms for walking 500 changes. These budgets catch
order-of-magnitude regressions rather than assert throughput; tune them in
`benches/engine.rs` as the engine matures.

If the toolchain is older than 1.85:
`rustup toolchain install stable --profile minimal -c clippy -c rustfmt && rustup default stable`.

Do not modify `tests/conformance/`, `tests/policy_conformance.rs`, or
`tests/conformance_sync.rs` by hand — the conformance manifest is owned by
`sorrel-protocol`.

## Workflow

- Keep changes scoped to this package and required workspace consumers.
- Prefer small, reviewable commits.
- Do not commit secrets.
- Coordinate shared contracts through `sorrel-protocol`.
