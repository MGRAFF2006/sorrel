# Sorrel Prototype Plan

Last updated: 2026-06-26 UTC

This plan defines (1) a **Quick Advance Goal** — the smallest persistent, local, single-user
prototype we can demo soon — and (2) the **broadened long-term scope** including desktop and
mobile apps and a hard performance bar. It is grounded in an assessment of the current code, not
aspiration.

## TL;DR

- The hard engine already exists and is tested: content-addressed `FileObjectStore`, `materialize_snapshot`, `create_change`, `snapshot_diff`, lanes/stacks (in `sorrel-core/src/`).
- The CLI does **not** use it. `sorrel-cli` links a vendored **policy-only** core (`sorrel-cli/crates/sorrel-core` has only `policy/`), so `init/status/change/lane/grant/secret` are mostly **mocked** and nothing persists to disk.
- Therefore the prototype is mostly a **wiring + plumbing** job, plus two small net-new pieces (a persistent HEAD pointer and a history/DAG walk).
- Desktop apps come **after** a solid CLI + core engine + a stable library/IPC surface. Mobile comes **last**. Performance is a first-class requirement throughout.

## Current reality (assessment summary)

What is REAL today:
- `sorrel-core/src/store.rs`: `FileObjectStore` (content-addressed, fanout, atomic writes, digest-verified reads) and `InMemoryObjectStore`. Tested.
- `sorrel-core/src/snapshot.rs`: `materialize_snapshot` (dir -> tree -> blobs), `read/write_snapshot`, `restore_snapshot_to_directory`. Deterministic IDs. Tested (in-memory).
- `sorrel-core/src/change.rs`: `create_change`, `read_change`, `snapshot_diff` (path-level add/mod/delete). `apply_change` validates but does not patch/merge. Conflict type is a placeholder.
- `sorrel-core/src/lane_stack.rs`: Lane/Stack objects with owner/visibility/policy/grant refs, touched resources, audit hooks. Tested (in-memory).
- `sorrel-runners`: real local process execution + Core policy gate + redaction + `sorrel.workflow.yml` parsing. Strongest E2E coverage in the repo.
- `sorrel-cli`: REAL commands are `workflow validate`, `workflow run <job>`, `policy evaluate`, `policy change apply`, and JS/TS `slice create` (persists to `.sorrel/slices/`).

What is MOCKED / MISSING:
- `sorrel-cli` `init` writes a **static mock** `.sorrel/manifest.json`; `status`, `change create/list`, `lane create`, `grant`, `secret` print hardcoded JSON and persist nothing.
- The CLI links the **policy-only** vendored core, so it cannot reach store/snapshot/change/lane.
- No persistent HEAD/lane pointer on disk.
- No `diff`, `log`, `history`, `commit`, `checkout` subcommands.
- No history/DAG traversal anywhere (parents are stored but never walked).
- No Git import/export/colocated bridge anywhere.
- No content-level diff/merge; conflicts are a placeholder.

Root cause of fragmentation: the multi-repo submodule layout means `sorrel-cli` vendors copies of `sorrel-core`/`sorrel-runners` (path deps in its own Cargo workspace) instead of sharing one workspace. The vendored core was only ever populated with the policy module.

---

## PART 1 — QUICK ADVANCE GOAL: "Persistent Local VCS Demo" (Milestone P0)

Goal: a single-user, on-disk, no-network demo where the SAME repo state persists across CLI
invocations. This is the thing we look at first.

### Demo script (acceptance)

```bash
sorrel init                          # real repo: .sorrel/ store + manifest + HEAD
echo "hello" > a.txt
sorrel status                        # shows a.txt as added (real diff vs HEAD)
sorrel change create -m "add a"      # snapshots cwd, diffs vs HEAD, writes objects, advances HEAD
sorrel status                        # clean
echo "world" >> a.txt
sorrel diff                          # shows a.txt modified (path-level, then line-level)
sorrel change create -m "edit a"
sorrel log                           # shows the two changes in order (DAG walk)
# restart shell / new process:
sorrel log                           # SAME history persists from disk
sorrel status                        # SAME clean/dirty state from disk
```

Success criteria: every command reads/writes real objects under `.sorrel/`; nothing mocked in
this path; state survives process restarts; deterministic object IDs.

### Work items (in dependency order)

P0-1. **Unfragment the engine — DECISION: git dependencies.**
  Make `sorrel-cli` depend on `sorrel-core` and `sorrel-runners` as **git-ref dependencies**
  (pinned to a commit/tag of each standalone repo's `main`), and delete the vendored
  policy-only `crates/sorrel-core` and the `crates/sorrel-runners` mirror. Steps:
  - Ensure the standalone `sorrel-core` exposes everything the CLI needs (store/snapshot/change/
    lane_stack + the policy API the CLI already uses) from its public crate API.
  - Switch `sorrel-cli/Cargo.toml` deps to `sorrel-core = { git = ".../sorrel-core", rev = "<sha>" }`
    and likewise for `sorrel-runners`; drop the `[workspace] members` vendored crates.
  - Reconcile any API drift between the vendored policy core and the standalone policy module
    (the CLI's conformance tests must still pass against the standalone core).
  Until this lands, no persistence is possible from the CLI.

P0-2. **Real `init`.** Open/create `FileObjectStore` at `.sorrel/`, materialize an initial empty
  snapshot, write a real manifest (`repo_id`, default lane, HEAD snapshot id) and a HEAD pointer
  file. Replace the static mock manifest.

P0-3. **Persistent HEAD + lane pointer.** A small, atomically-written `.sorrel/HEAD` (or in the
  manifest) recording the current snapshot id and active lane, with a documented update path.

P0-4. **Real `change create`.** `materialize_snapshot(cwd)` -> `snapshot_diff(HEAD,new)` ->
  `create_change` -> persist objects -> advance HEAD/lane. Reject empty changes. `-m/--message`.

P0-5. **Real `status`.** Materialize working tree, diff vs persisted HEAD, report actual
  added/modified/deleted/clean. Honor an ignore list (at minimum ignore `.sorrel/`).

P0-6. **`diff` subcommand — line-level (DECISION).** Path-level set comes from `snapshot_diff`;
  for modified text blobs, compute **line-level hunks** (Myers-style; use a small, well-tested
  diff crate such as `similar`, or implement LCS). Binary/non-UTF8 blobs report as
  "binary changed". Human + `--json` output.

P0-7. **`log`/`history` subcommand.** Net-new: DAG walk over change/snapshot parents from HEAD.
  Deterministic ordering; JSON + human output.

P0-8. **On-disk integration test.** A `FileObjectStore`-backed snapshot+change round-trip across
  two separate store handles (proves cross-process persistence). Plus CLI E2E asserting the demo
  script persists (replace/retire mock-locking assertions in `tests/json_output.rs`).

P0-9. **Demo doc + sample repo.** A short `DEMO.md` walking the script above, plus `--json` on all
  new commands so agents/tools can consume structured output.

Explicitly OUT of P0: Git bridge, content merge/conflict resolution, lanes parallelism beyond a
single default lane, remotes/sync, Hub, secrets injection, GUI.

### Performance note for P0
Even the prototype should set the tone: stream/iterate the working tree (don't slurp), reuse one
`FileObjectStore` handle per invocation, skip re-hashing unchanged files where a cheap stat-based
heuristic is safe (size+mtime cache), and keep object writes atomic. Add a couple of
`cargo bench`/criterion microbenchmarks for snapshot + diff on a synthetic tree so we catch
regressions early.

---

## PART 2 — BROADENED SCOPE (sequenced; apps are LAST)

Ordering principle: prove the engine and a stable, fast library/IPC surface BEFORE building any
GUI. Each app layer consumes the same core; no app re-implements VCS or policy semantics.

### Phase A — Core engine hardening (right after P0)
- Content-level diff/merge (3-way), first-class conflict objects, reusable resolutions.
- History/DAG operations: ancestry, common base, walk, undo/operation-log.
- Lanes/stacks as real workflows (create/switch/list/submit), not just objects.
- Performance pass (see Performance section): chunked/large-file storage, packfiles, indexes,
  lazy loading, mmap reads, parallel hashing.

### Phase B — Git bridge (adoption)
- `init --git-colocated`, `git import`, `git export`, one-way mirror, round-trip exit.
- SHA mapping tables; no lock-in.

### Phase C — Stable embedding surface (the app enabler)
- A single Rust core library crate with a clean, versioned API + C ABI (`cbindgen`).
- Bindings: Node/N-API (for Hub + Electron/Tauri sidecar), WASM (browser/in-memory), and a
  stable JSON-over-IPC/daemon protocol for GUIs.
- This is the contract every desktop/mobile app builds on.

### Phase D — Collaboration (Hub) on Core
- Proposals/reviews/merge-queue consuming Core policy; secrets/runners integration.

### Phase E — DESKTOP APPS (Linux, macOS, Windows) — added LATE
- Single codebase via **Tauri** (Rust core embedded directly; small binaries; native webview) —
  recommended over Electron for performance/footprint and because our core is already Rust.
- Features: repo browser, lanes/stacks, diff/review UI, workflow runs, secrets admin (handles
  only), policy decisions.
- Packaging/signing per OS: `.deb`/`.rpm`/AppImage (Linux), notarized `.dmg`/`.app` (macOS),
  signed MSI/MSIX (Windows). Auto-update.

### Phase F — MOBILE APPS (Android, iOS, iPadOS) — added LAST
- Reuse the Rust core via UniFFI (Kotlin/Swift bindings) or the C ABI.
- Likely thinner clients (browse, review, approve, trigger runs, manage handles) rather than full
  local builds; heavy compute routed to user-owned runners.
- iPadOS gets a more workspace-capable layout than phone.
- This is the final milestone; do not start before desktop is stable.

---

## Performance requirements (first-class, applies from P0 onward)

The spec must feel fast or people won't use it. Concrete targets/approach:

- Object IDs via BLAKE3 (fast, parallelizable) — confirm current hash choice and benchmark.
- Parallel file hashing and tree building (rayon) for snapshot of large trees.
- Stat-cache (size+mtime) to skip re-hashing unchanged files in `status`/`change`.
- mmap or buffered streaming reads; never load whole large files into memory.
- Chunked, deduplicated blob storage for large/binary files; lazy fetch.
- Packfiles + on-disk indexes for many small objects; avoid one-file-per-object cost at scale.
- Bound memory; stream working-tree traversal.
- Criterion benchmarks + a perf budget in CI (fail on regression) for: snapshot N files, diff,
  status on a warm cache, log walk of K changes.
- Rough initial budgets to validate (tune later): `status` on a warm 10k-file repo < ~100ms;
  `change create` dominated by hashing changed files only; `log` of 1k changes < ~50ms.

---

## Decisions

1. **Engine unfragmentation:** git dependencies (see P0-1). `sorrel-cli` depends on the standalone
   `sorrel-core`/`sorrel-runners` by git ref; vendored copies removed.
2. **Where P0 work lands:** `sorrel-core` (add `log`/history DAG walk + HEAD/manifest helpers if
   they belong in the engine) and `sorrel-cli` (real `init/status/change/diff/log`), with root
   pointer advances after each submodule merge.
3. **First `diff`:** line-level text hunks (see P0-6).

## Execution sprint (P0)

Order: P0-1 (git-dep unfragmentation) → P0-2/3 (real init + HEAD) → P0-4/5 (change + status) →
P0-6 (line diff) → P0-7 (log) → P0-8 (on-disk + CLI E2E tests) → P0-9 (DEMO.md + --json).
Each `sorrel-core`/`sorrel-cli` change merges to its submodule `main`, then the root pointer is
advanced. Desktop/mobile remain parked until Phases A–D are solid.
