# Agent instructions for sorrel-runners

## What this module is

Portable workflow execution for Sorrel: the `JobBundle` model, a
`sorrel.workflow.yml` parser, and runners (`LocalProcessRunner`,
`ContainerRunner`) that execute jobs only after a Core policy gate authorizes
`runner.use` / `workflow.run` / `secret.read` / `secret.inject`.

## Stack and conventions

- Rust, edition 2024, **rust-version 1.85+**.
- `unsafe_code = "forbid"`.
- No `sorrel-core` dependency: the former CLI-facing `cli_runner` compat surface
  now lives in `sorrel-cli`. This crate exposes only its native protocol model.
- Keep the protocol-model API additive.

## Core boundary

- Runners must not decide privilege from local config alone — always consult the
  Core permission evaluator before executing. Bundle-attached `policyDecisions`
  are audit metadata only, never trusted on their own.
- Secret references stay as `SecretRef`; this prototype does not inject secret
  values.

## Common checks

```sh
# From the monorepo root
cargo build -p sorrel-runners
cargo test -p sorrel-runners
cargo clippy -p sorrel-runners --all-targets -- -D warnings
cargo fmt --all -- --check
```

Do not modify `tests/conformance/`, `tests/policy_conformance.rs`, or
`tests/conformance_sync.rs` by hand.

## Workflow

- Keep changes scoped to this package and required workspace consumers.
- Prefer small, reviewable commits.
- Do not commit secrets.
- Coordinate shared contracts through `sorrel-protocol`.
