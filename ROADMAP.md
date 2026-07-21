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

### 2. Merge/conflict model (`sorrel-core`, then `sorrel-cli`) — DONE (2026-07-17)

Shipped via the task pack: `history::merge_base(s)` (BFS over snapshot
parents, deterministic tie-break), `merge3` (dependency-free three-way line
merge with Git-style markers), content-addressed `Conflict`/`MergeResult`
objects, `merge::merge_snapshots` (path-level three-way merge), and CLI
`sorrel merge <lane>` (fast-forward + three-way, conflict markers,
`MERGE_STATE`, `--abort`). Remaining follow-ups: `resolve`/`--continue` flow,
and aligning stored Conflict/MergeResult fields with the protocol schema's
required `repoId`/`ours`/`theirs` (see SORREL_PROGRESS known debt).

### 3. Git bridge (`sorrel-core` + `sorrel-cli`) — PARTIAL (import DONE 2026-07-21)

The adoption path: `sorrel git import` (Git repo → snapshots/changes),
`sorrel git export`, and a colocated mode with SHA mapping tables so teams can
try Sorrel without leaving Git. **One-way import shipped** (`sorrel-core`
`6b5b611`, `sorrel-cli` `226ddce`). Export and colocated mirror next.

### 4. Lanes as real workflows (`sorrel-cli` + `sorrel-core`) — MOSTLY DONE (2026-07-17)

Shipped via the task pack: per-lane heads (`.sorrel/heads/`), `lane
list`/`lane switch` (dirty-tree protection, worktree restore), independent
lane histories, and `merge <lane>` integration. Remaining: `lane submit` and
stacked changes (later, with Hub proposals).

### 5. Hub collaboration surface (`sorrel-hub` + `sorrel-hub-web`) — PARTIAL

Landed 2026-07-17: FS-backed product metadata (records survive restarts),
`GET /admin/sync-repos`, and a read-only hub-web Sync view. Remaining:
proposals/reviews consuming Core policy (a proposal references a lane/stack
pushed via sync), approval state as signed records, and hub-web write flows
(create proposal, review).

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

## Task pack

Items 2, 4, and parts of 5 are broken into small, paste-ready agent work
orders in [`SORREL_AGENT_TASKS.md`](SORREL_AGENT_TASKS.md) (lanes PROTO /
CORE / CLI / HUB / HUB-WEB / CI with an explicit dependency map).

## Working agreement for agents

- One agent lane per submodule at a time; check "Active agents" in
  `SORREL_PROGRESS.md` before starting.
- Work merges into the submodule `main`; the root repo then advances the
  pointer with a small PR that also updates the dashboard.
- Run the module's standard checks before handing off (`cargo test`, `cargo
  clippy --all-targets`, `cargo fmt --all -- --check` / `npm test`, `npm run
  validate`), plus `scripts/sync-conformance.sh --check` when touching policy
  fixtures.
