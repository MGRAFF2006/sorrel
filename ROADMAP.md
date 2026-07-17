# Sorrel Roadmap

Last updated: 2026-07-17 UTC

This is the forward plan for the multi-repo Sorrel project. Live status lives
in [`SORREL_PROGRESS.md`](SORREL_PROGRESS.md); the architecture rationale in
[`AGENT_NATIVE_VERSION_CONTROL_REPORT.md`](AGENT_NATIVE_VERSION_CONTROL_REPORT.md).
It replaces the completed `SORREL_PROTOTYPE_PLAN.md` (P0 shipped 2026-06-26;
see git history for the retired plan).

## Starting point

The engine, the persistent local CLI, the policy/authority spine (with
cross-repo conformance tests), workflows, vault, and the sync transport
(protocol + core + Hub endpoints + CLI push/pull) all exist and are tested.
The biggest gaps are: Hub storage is in-memory, there is no content-level
merge/conflict handling, and there is no Git bridge — which blocks real-world
adoption paths.

## Sequenced plan

Ordering principle: finish durability and the data model before collaboration
features; prove the engine and a stable embedding surface before any GUI.
Items in different repos can run as parallel agent lanes when their "needs"
are met.

### 1. Hub persistence — FS-backed object/ref store (`sorrel-hub`) — DONE (2026-07-17)

Shipped in `sorrel-hub` `e926caf`: `FsRepoSyncStore`, a drop-in for the
in-memory sync store — content-addressed fanout layout mirroring core's
`FileObjectStore` semantics (atomic write-then-rename, digest-verified reads,
refs as small JSON files with atomic replace), default-on for the server
(`SORREL_HUB_DATA_DIR`, opt out with `SORREL_HUB_SYNC_STORE=memory`),
in-memory for tests. No new authority semantics. Follow-up: persist Hub
product metadata (projects/proposals) the same way when item 5 needs it.

### 2. Merge/conflict model (`sorrel-core`, then `sorrel-cli`)

`apply_change` currently validates but cannot patch; `Conflict` is a
placeholder. Build: three-way content merge for text blobs, first-class
conflict objects (stored, addressable, resolvable), common-ancestor
computation over the snapshot DAG, and reusable resolutions. Then CLI
`merge`/`resolve` surfaces. Needs: coordination with the agent currently
working in `sorrel-core`.

### 3. Git bridge (`sorrel-core` + `sorrel-cli`)

The adoption path: `sorrel git import` (Git repo → snapshots/changes),
`sorrel git export`, and a colocated mode with SHA mapping tables so teams can
try Sorrel without leaving Git. Start with one-way import; export and
colocated mirror after. Needs: merge model helps but import can start first.

### 4. Lanes as real workflows (`sorrel-cli` + `sorrel-core`)

Lanes/stacks exist as objects but the CLI has a single implicit lane. Add
`lane switch/list/submit`, per-lane HEADs, and stacked changes — the
foundation for parallel agent work. Needs: merge model (2) for lane
integration.

### 5. Hub collaboration surface (`sorrel-hub` + `sorrel-hub-web`)

Proposals/reviews consuming Core policy (a proposal references a lane/stack
pushed via sync), approval state as signed records, and hub-web write flows
(create proposal, review, see sync status). Needs: 1 (persistence) and 4
(lanes) for anything beyond a skeleton.

### 6. Stable embedding surface (`sorrel-core`)

A versioned core library API + C ABI (cbindgen), Node/N-API and WASM
bindings, and/or a JSON-over-IPC daemon protocol. This is the contract every
SDK, desktop, and mobile client builds on. Needs: core APIs settled (after 2).

### 7. Agent control plane + SDKs (`sorrel-agents`, `sorrel-sdk-js`, `sorrel-sdk-rust`)

Lane claims, agent identity as Core principals, policy overlays for agent
capabilities; SDKs wrap the embedding surface. Needs: 4 and 6.

### 8. Apps — desktop then mobile (last)

Tauri desktop app (Linux/macOS/Windows) on the embedding surface; mobile
(Android/iOS/iPadOS) last, as thinner review/approve clients. Do not start
before 6 is stable.

## Not yet

Marketplace, full merge queue, hosted compute, production auth, sophisticated
conflict-resolution UI. These wait until 1–5 have a stable integration path.

## Performance bar (applies throughout)

- Stat-cache and engine-level exclusion are in; keep `status` on a warm 10k-file
  repo under ~100ms and `log` of 1k changes under ~50ms (benches exist in core).
- Stream working trees, atomic object writes, parallel hashing when needed.
- Next perf items (with 2/3): packfiles + indexes for many-small-object repos,
  chunked storage for large/binary blobs, lazy fetch on sync.

## Working agreement for agents

- One agent lane per submodule at a time; check "Active agents" in
  `SORREL_PROGRESS.md` before starting.
- Work merges into the submodule `main`; the root repo then advances the
  pointer with a small PR that also updates the dashboard.
- Run the module's standard checks before handing off (`cargo test`, `cargo
  clippy --all-targets`, `cargo fmt --all -- --check` / `npm test`, `npm run
  validate`), plus `scripts/sync-conformance.sh --check` when touching policy
  fixtures.
