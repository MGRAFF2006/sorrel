# Sorrel Roadmap

Last updated: 2026-08-10

Forward plan for the Sorrel monorepo. Live status:
[`docs/STATUS.md`](docs/STATUS.md). Architecture:
[`AGENT_NATIVE_VERSION_CONTROL_REPORT.md`](AGENT_NATIVE_VERSION_CONTROL_REPORT.md).

## Sequenced plan

### 0. `v0.1.0-alpha.1` stabilization — IN PROGRESS

Freeze one coordinated release, make localhost/dev-only security boundaries
explicit, keep CI a truthful gate, and record correctness/performance baselines.
**Monorepo absorption is done** (PR #49): one clone, path deps, green CI on
`main`.

### 1. Hub persistence — FS-backed object/ref store (`sorrel-hub`) — DONE

FS-backed sync store and product metadata; default-on for the server.

### 2. Merge/conflict model (`sorrel-core`, then `sorrel-cli`) — DONE

`merge_base` / `merge3` / Conflict / MergeResult / `merge_snapshots`, plus CLI
`sorrel merge` (fast-forward + three-way, markers, `--abort`, `--continue`).
Stored Conflict / MergeResult objects now match the protocol schema (repoId,
base/ours/theirs refs, resolution slot, bare-hex merge-result ids).

### 3. Git bridge (`sorrel-core` + `sorrel-cli`) — DONE

**`sorrel git import`, `sorrel git export`, and colocated `sorrel git sync`
shipped** (`.sorrel/git-map.json` SHA mapping). Sync incrementally fast-forwards
whichever side moved; true divergence is imported onto a `git/<branch>` lane
for the normal Sorrel merge flow, then exported on the next sync.

### 4. Lanes as real workflows (`sorrel-cli` + `sorrel-core`) — MOSTLY DONE

Per-lane heads, `lane list` / `switch`, merge integration, **`lane submit`** →
Hub proposal via `/collaboration/lane-submit`. Remaining: stacked changes UX.

### 5. Hub collaboration surface (`sorrel-hub` + `sorrel-hub-web`) — MOSTLY DONE

FS metadata + Sync view + proposal/review write path (GET/PATCH, lane-submit,
hub-web forms). Remaining: production auth and richer review UX.

### 6. Secrets + SecretSpec → devenv-backed runs → log UX — ACTIVE

Consume upstream SecretSpec and devenv (**do not fork**). Sorrel keeps
`SecretRef` + Core grants as source of truth; SecretSpec is the
provider/resolver. Execution prefers devenv when present, with
`LocalProcessRunner` as a compat fallback. Structured run logs land as a
product surface (Blacksmith-grade detail), then an optional Hub secret backend.

Suggested sequencing:

1. SecretSpec resolve + `sorrel secret *` + inject into local workflow
2. devenv backend for `workflow run` / `env ensure` + local fallback
3. Rich run logs under `.sorrel/runs/<id>/`
4. Optional Hub-hosted / BYO secret backend (same SecretRef + grant contracts)

Intentional follow-up (**DEBT-1**): do **not** fully unify `cli_policy` /
`cli_runner` until secret injection ships.

### 7. Stable embedding surface (`sorrel-core`)

Versioned library API + C ABI / N-API / WASM / IPC daemon — the contract for
SDKs and apps.

### 8. Agent control plane + SDKs

`sorrel-agents`, `sorrel-sdk-js`, `sorrel-sdk-rust` after lanes and embedding
settle.

### 9. Apps — desktop then mobile (last)

Tauri desktop, then thinner mobile clients. Do not start before item 7.

## Not yet

Marketplace, full merge queue, hosted compute, production auth, sophisticated
conflict-resolution UI. Nix is never mandatory — devenv is preferred, local
fallback remains.

## Performance bar

- Keep warm `status` on a 10k-file repo under ~100ms and `log` of 1k changes
  under ~50ms (benches in core).
- Next: packfiles + indexes, chunked large blobs, lazy fetch on sync.

## Monorepo

Implementation lives in-tree under `sorrel-*`. Open one PR against root `main`.
Run package checks plus root E2E before merging
(`cargo test --workspace` / `npm test`, plus clippy/fmt for Rust).
