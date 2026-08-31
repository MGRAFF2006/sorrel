# Agent instructions for sorrel-sdk-rust

## What this module is

The Rust SDK for Sorrel: ergonomic, typed bindings over the core APIs for
embedding Sorrel in Rust applications and tools.

**Status: minimal SDK shipped.** `Workspace` helper over `sorrel-core` for
init/snapshot. Broader embedding surface still ahead (see root `ROADMAP.md`).

## Conventions

- Rust, edition 2021, **rust-version 1.85+**, `unsafe_code = "forbid"`.
- Re-export and wrap `sorrel-core` types rather than redefining them; depend on
  the engine through the workspace path dependency.

## Common checks

```sh
# From the monorepo root
cargo build -p sorrel-sdk
cargo test -p sorrel-sdk
cargo clippy -p sorrel-sdk --all-targets -- -D warnings
cargo fmt --all -- --check
```

## Workflow

- Keep changes scoped to this package and required workspace consumers.
- Prefer small, reviewable commits.
- Do not commit secrets.
- Coordinate shared contracts through `sorrel-protocol`.
