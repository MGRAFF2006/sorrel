# Agent instructions for sorrel-cli

## What this module is

The `sorrel` command-line interface. It drives the real `sorrel-core` engine
(via git dependency) for persistent local version control. All commands are now
real and persist on-disk state under `.sorrel/`: `init`, `status`, `diff`, `log`,
`change create`/`change list`, `lane create`/`lane list`/`lane switch`/
`lane submit`, `merge`/`merge --abort`, `git import`/`git export`/`git sync`
(colocated mirror), `slice create`,
`grant create`/`grant list`, `secret refs`, plus the policy and workflow
commands. `lane submit` pushes (optional) and opens a Hub proposal via
`POST /collaboration/lane-submit`. No command prints fabricated data. See
`DEMO.md` for the persistent demo; `GIT.md` for Git import; `SYNC.md` for Hub
sync.

On-disk registries live under `.sorrel/`: `lanes/`, `heads/` (per-lane snapshot
pointers), `grants/`, `secrets/`, `slices/` (one JSON object per id), alongside
`objects/`, `manifest.json`, `HEAD`, `changes.index` (JSON lines mapping
resulting snapshot id → change id), and `MERGE_STATE` (in-progress conflicted
merge). Registry helpers are in `src/repo.rs`.

## Stack and conventions

- Rust, edition 2021, **rust-version 1.85+** (the `sorrel-core` git dep requires
  edition2024-era toolchain).
- `unsafe_code = "forbid"`.
- `sorrel-core` is a **git dependency** pinned by `rev` in `Cargo.toml`. When the
  engine changes, bump the `rev`. Prefer the git dep over vendored copies. Do
  not reintroduce a local core stub or `[patch]` table — pin a real
  `sorrel-core` revision that is fetchable.
- The crate is **lib + bin**: shared modules live in `src/lib.rs`
  (`cli_policy`, `cli_runner`, `repo`, `linediff`, `workflow_cmd`,
  `CommandOutput`); `src/main.rs` is the thin binary. Integration tests consume
  the library (e.g. `sorrel_cli::cli_policy`).
- `cli_policy` (CLI policy evaluator) and `cli_runner` (workflow parse + local
  run + policy gate) now live **here**, not in `sorrel-core`/`sorrel-runners`.
  `cli_policy` conforms to the protocol policy manifest
  (`tests/policy_conformance.rs`).
- Every command supports `--json` and returns `CommandOutput { json, human }`.
- On-disk layout helpers live in `src/repo.rs`; line diff in `src/linediff.rs`.

## Known debt

- `cli_policy` is a self-contained policy evaluator distinct from the engine's
  native `sorrel_core::policy` API (different type shapes). Both conform to the
  same protocol manifest. A future unification could converge the CLI onto the
  engine-native decision objects, but that changes the CLI `--json` contract.

## Stat cache

Working-tree snapshots use the engine's
`materialize_snapshot_excluding_with_stat_cache`. The CLI loads
`.sorrel/stat-cache.json` (size+mtime → blob id) before snapshotting and saves
it atomically (temp file + rename) after `status` and `change create` succeed;
unchanged files skip re-hashing. `diff` snapshots read-only and does not persist
the cache. Cache helpers (`load_stat_cache`/`save_stat_cache`/`stat_cache_path`)
live in `src/repo.rs`; a corrupt cache is treated as empty (pure optimization).

## Common checks

```sh
cargo build
cargo test --workspace
cargo clippy --all-targets
cargo fmt --all -- --check
```

Do not modify `tests/conformance/` or the conformance tests by hand. When
changing real-command output, update the corresponding `tests/json_output.rs`
assertions (assert stable shapes for non-deterministic ids/timestamps).

## Workflow

- Keep changes scoped to this repository.
- Prefer small, reviewable commits.
- Do not commit secrets.
- Coordinate shared contracts through `sorrel-protocol`.
