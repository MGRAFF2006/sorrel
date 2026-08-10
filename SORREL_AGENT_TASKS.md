# Sorrel Agent Task Pack

Last updated: 2026-08-10 UTC

Small, self-contained work orders derived from [`ROADMAP.md`](ROADMAP.md).

> **Status 2026-08-10:** Waves 1–2 and the Git mirror are done. The repo is now
> a **single monorepo** — CI no longer needs `SUBMODULES_TOKEN`. Prefer one PR
> against root `main`.

## Completed (high signal)

- Merge/conflict model, `merge --continue`, protocol-aligned Conflict/MergeResult
- Hub FS persistence, collaboration write path, bootstrap grants
- Git import / export / colocated `git sync`
- Monorepo absorption (in-tree packages, Cargo workspace, path deps)

## How to use this pack

- Work in the root checkout; commit once in this repository.
- Run `npm run validate:release`, `npm run test:modules`, and `npm test` for
  cross-package changes.

## Open

### ALPHA-1 — Land monorepo + green CI

```text
Repository: https://github.com/MGRAFF2006/sorrel (this monorepo)

Context: Submodules were absorbed into the root tree. Root CI checks out the
repo directly and runs validate:release, validate:conformance, cargo clippy/fmt
for the workspace, npm run test:modules, and npm test.

Task:
1. Ensure branch CI is green on the monorepo conversion.
2. Merge to main when ready.
3. Tag v0.1.0-alpha.1 only after release checks pass (see docs/RELEASE.md).

Validate:
  npm run validate:release
  npm run validate:conformance
  cargo clippy --workspace --all-targets -- -D warnings
  cargo fmt --all -- --check
  npm run test:modules
  npm test
```

### NEXT-1 — Production Hub auth (after alpha)

Hub still trusts acting-principal headers + bootstrap grants. Design and ship a
minimal signed identity path before exposing Hub beyond localhost.

### NEXT-2 — Embedding surface (after alpha)

Stable library boundary for SDKs/apps (C ABI / N-API / WASM / daemon). Do not
start desktop/mobile apps before this.

### DEBT-1 — Collapse CLI forks (after alpha)

`sorrel-cli` still carries in-tree `cli_policy` and `cli_runner` duplicates of
engine/runners packages. Unify carefully — CLI `--json` shapes may change.
