# Sorrel Roadmap

Last updated: 2026-08-31

Forward-only plan for the Sorrel monorepo. Shipped progress belongs in
[GitHub Releases](https://github.com/MGRAFF2006/sorrel/releases) and
[`CHANGELOG.md`](CHANGELOG.md); current behavior is in
[`docs/STATUS.md`](docs/STATUS.md); current architecture is in
[`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md).

## Sequenced plan

### 0. `v0.1.0-alpha.1` stabilization — DONE

Released the first coordinated module set with aligned versions, explicit
localhost/dev-only security boundaries, release/legal metadata, and a truthful
repository-wide CI gate. Monorepo absorption, SecretSpec-backed CLI injection,
devenv-aware execution, and structured local run logs are included.

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

### 5. Hub collaboration surface (`sorrel-hub` + `sorrel-hub-ui` + `sorrel-hub-web`) — IN PROGRESS

FS metadata, Sync view, and the proposal/review write path are shipped. Shared
Solid UI (`sorrel-hub-ui`) + thin web host, `GET /capabilities`, AuthAdapter
(WorkOS/OIDC JWKS / dev), `GET /session`, and Convex metadata spike
(`proposals.countOpen`) are the Phase-1 foundation. Remaining: WorkOS sealed
sessions + IdP login UI, full Convex metadata migration, virtualized diffs,
authenticated remote-Hub configuration for the desktop shell.

### 6. Secrets + SecretSpec → devenv-backed runs → log UX — MOSTLY DONE

The alpha ships upstream SecretSpec resolution/injection under Core grants,
devenv detection with local fallback, and structured redacted logs under
`.sorrel/runs/<id>/`. Remaining work: fuller workflow-to-devenv task mapping,
log following and Hub streaming, an optional hosted/BYO provider binding, and
then removal of the intentional `cli_policy` / `cli_runner` duplication.

### 7. Stable embedding surface (`sorrel-core`)

Versioned library API + C ABI / N-API / WASM / IPC daemon — the contract for
SDKs and apps.

### 8. Mature the agent control plane + SDKs

Minimal `sorrel-agents`, `sorrel-sdk-js`, and `sorrel-sdk-rust` surfaces shipped
in the alpha. Stabilize and extend them after lanes and embedding settle.

### 9. Apps — desktop then mobile — IN PROGRESS

The shared Hub UI now has a Tauri desktop host for Windows, macOS, and Linux.
It connects to a local Hub while Core embedding waits for item 7's stable API.
Next: signed installers, authenticated remote-Hub selection, local workspace
integration after the embedding contract, then thinner mobile clients.

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
