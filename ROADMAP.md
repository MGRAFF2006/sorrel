# Sorrel Roadmap

Last updated: 2026-07-30

Forward plan for the multi-repo Sorrel project. Live status:
[`docs/STATUS.md`](docs/STATUS.md). Architecture:
[`AGENT_NATIVE_VERSION_CONTROL_REPORT.md`](AGENT_NATIVE_VERSION_CONTROL_REPORT.md).

## Sequenced plan

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

### 6. Stable embedding surface (`sorrel-core`)

Versioned library API + C ABI / N-API / WASM / IPC daemon — the contract for
SDKs and apps.

### 7. Agent control plane + SDKs

`sorrel-agents`, `sorrel-sdk-js`, `sorrel-sdk-rust` after lanes and embedding
settle.

### 8. Apps — desktop then mobile (last)

Tauri desktop, then thinner mobile clients. Do not start before item 6.

## Not yet

Marketplace, full merge queue, hosted compute, production auth, sophisticated
conflict-resolution UI.

## Performance bar

- Keep warm `status` on a 10k-file repo under ~100ms and `log` of 1k changes
  under ~50ms (benches in core).
- Next: packfiles + indexes, chunked large blobs, lazy fetch on sync.

## Submodules

Implementation lives in submodule repos. Merge to each submodule’s `main`, then
advance the root pointer. Run that module’s checks before merging
(`cargo test` / `npm test`, plus clippy/fmt for Rust).
