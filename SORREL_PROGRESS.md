# Sorrel Progress Dashboard

Last updated: 2026-07-21 UTC (happy-path sync/docs/compose landed on submodule mains; Git import next)

This is the single live status document for Sorrel orchestration. The forward
plan lives in [`ROADMAP.md`](ROADMAP.md); ready-to-dispatch agent work orders
live in [`SORREL_AGENT_TASKS.md`](SORREL_AGENT_TASKS.md); the full architecture
lives in [`AGENT_NATIVE_VERSION_CONTROL_REPORT.md`](AGENT_NATIVE_VERSION_CONTROL_REPORT.md).
Update this file whenever an agent reports completion, a PR is merged, or the
plan changes.

## Operating rules

- The root repository stays on `main`. All implementation work happens inside
  the submodule repositories, on branches merged into each submodule's `main`.
- The root repository only points submodules at commits reachable from the
  corresponding submodule `main`.
- After a submodule merge, advance the root submodule pointer and open/merge a
  small root PR (pointer + dashboard update together).
- Core owns identity, permissions, grants, policy decisions, redaction,
  `SecretRef`, and audit semantics. Hub, runners, vault, and the CLI consume
  them; none of them may define an independent authority model. Policy/grant
  changes are signed `PolicyChange` objects evaluated against the previous
  effective policy — no self-escalation.
- Protocol fixtures are the canonical cross-language conformance source:
  `sorrel-protocol/conformance/policy-conformance.json` plus its checksum
  sidecar are vendored into every policy consumer, each of which runs an
  offline drift-guard test. Refresh copies with the root
  `scripts/sync-conformance.sh` (verify with `--check`) after any manifest
  change.

## Where the system stands (2026-07-21)

- **Engine (`sorrel-core`)**: real and tested — content-addressed
  `FileObjectStore`, snapshots, changes, path-level diff, lanes/stacks, policy
  and authority spine, sync transport closure helpers, stat-cache, and the
  **merge/conflict model** (`merge_base`, `merge3`, Conflict/MergeResult,
  `merge_snapshots`). 94 tests, fmt/clippy clean. **Git import (roadmap #3)
  is the next engine/CLI item.**
- **CLI (`sorrel-cli`)**: persistent local VCS over the real engine. Commands
  include init/status/change/diff/log/lane/merge(--abort)/grant/slice/workflow/
  remote/push/pull. Pull restores the working tree; Hub sync sends bootstrap
  `grantRefs` and parses refs as a protocol array. Pins `sorrel-core` by git
  rev (`eed47d0`); no stub. See `sorrel-cli/DEMO.md` and `sorrel-cli/SYNC.md`.
- **Sync**: CLI ↔ Hub happy path works with Hub bootstrap grants for
  `user:local`, Core/protocol closure shapes, and FS-backed Hub storage.
- **Hub split**: `sorrel-hub` API (Dockerized), `sorrel-hub-web` deployable
  static+proxy UI (read-only), `sorrel-web` public landing on Cloudflare with
  `/docs/` (STATUS + GETTING_STARTED + HTML guides).
- **Root**: `docs/STATUS.md`, `docs/GETTING_STARTED.md`, `docker-compose.yml`
  for hub + hub-web (+ optional landing preview).

## Module status

| Module | Status | Root pointer | Notes |
| --- | --- | --- | --- |
| `sorrel-protocol` | Active | `082e86f` (main) | Schemas incl. Workspace + Conflict/MergeResult, sync-transport spec, policy conformance manifest + sidecar. `npm test` 8/8, `npm run validate` ok. |
| `sorrel-core` | Active | `eed47d0` (main) | Full engine incl. transport, stat-cache, merge/conflict model. 94 tests. Git import next. |
| `sorrel-cli` | Active | `ba83dfa` (main) | Happy-path Hub sync: grantRefs, refs array, worktree restore on pull. Pins core `eed47d0`. 74 workspace tests. |
| `sorrel-vault` | Active | `b710fff` (main) | Secrets spec, local backend, dev CLI (import/list/grant/redact); Core-grant gated; 13 tests. |
| `sorrel-runners` | Active | `e8effa2` (main) | Local/container runner model + `sorrel.workflow.yml` parser; Core policy gate + redaction. |
| `sorrel-slices` | Active | `bd820c9` (main) | TS/JS slice manifest generator prototype. |
| `sorrel-hub` | Active | `f9028fc` (main) | Bootstrap local sync grants; Core protocol closure walk; Dockerize. 51 tests. |
| `sorrel-hub-web` | Active | `bb71023` (main) | Deployable static+proxy (`HOST` 0.0.0.0); Dockerfile. Read-only UI. 6 tests. |
| `sorrel-web` | Live | `422eed3` (main) | Cloudflare Pages; docs hub + STATUS/GETTING_STARTED guides viewer. |
| `sorrel-agents` | Not started | scaffold | Agent control plane; starts once lanes/claims semantics settle. |
| `sorrel-sdk-js` | Not started | scaffold | Starts after the embedding surface (roadmap) stabilizes. |
| `sorrel-sdk-rust` | Not started | scaffold | Starts after core APIs settle. |

## Active agents

| Agent | Target | Goal | Notes |
| --- | --- | --- | --- |
| Auto (this session) | `sorrel-core` + `sorrel-cli` | Roadmap #3 one-way `sorrel git import` | Phase A happy-path landed; implementing import next. |

## Known debt / follow-ups

- No CI on any repo (the CI-1/CI-2 task-pack workflows never landed — re-dispatch).
- Conflict resolution flow: `sorrel merge` leaves markers + `MERGE_STATE` and supports `--abort`, but there is no `resolve`/`--continue` yet.
- Stored engine `Change` objects carry no timestamp; CLI `log` falls back to the snapshot's `createdAt`.
- Git bridge: one-way import in progress; export / colocated still ahead (roadmap #3).
- Vendored conformance copies could later be replaced by a published shared package (npm/crate).
- `sorrel-hub-web` is read-only; no write flows yet.
- **Schema drift:** the engine's stored `Conflict` objects omit `repoId`/`ours`/`theirs`, which the protocol schema marks *required* (`MergeResult` similarly omits `repoId` and the three snapshot ids). Engine output would not validate against the protocol schema — align in a small follow-up (engine side or schema side, decide deliberately).

## Completion checklist (when an agent reports done)

1. Submodule repo branch and commit.
2. Validation commands and result (`cargo test` / `npm test` etc., fmt + clippy for Rust).
3. Whether the submodule commit is merged into that submodule repo's `main`.
4. Root submodule pointer advanced to that submodule `main` commit, with a root PR.
5. This dashboard updated.

## Submodule pointer repair checklist

Use this whenever an agent accidentally leaves useful work only on a submodule feature branch.

```bash
# Inside the submodule repo
git checkout main
git pull origin main
git fetch origin <feature-branch>
git merge --ff-only origin/<feature-branch> || git merge --no-ff origin/<feature-branch>
git push origin main

# Inside the root repo
git checkout main
git pull origin main
git -C <submodule> checkout main
git -C <submodule> pull origin main
git add <submodule>
git commit -m "Point <submodule> at main"
git push origin main
```

## Milestone history (condensed)

The detailed 2026-06 orchestration log was consolidated on 2026-07-17. Full
detail remains in this file's git history (see revisions before this date).

| When | Milestone |
| --- | --- |
| 2026-06-24 | Initial module scaffolds: protocol, core object store/snapshots, mocked CLI, vault spec + local backend, runners prototype, slices prototype, Hub skeleton, landing page. Root/submodule branch policy established. |
| 2026-06-24 | Architecture correction: Core owns identity/permissions/policy/audit; Hub consumes them. Authority hardening (signed `PolicyChange`, no self-escalation) landed across protocol/core/cli/hub/runners/vault. |
| 2026-06-24/25 | Policy conformance pass: canonical `policy-conformance.json` in protocol, vendored + tested in all five consumers; then automated drift guards (checksum sidecar + per-consumer sync test + `scripts/sync-conformance.sh`). Fixed a real CLI rotation-capability drift. |
| 2026-06-25 | `sorrel-runners` workflow-file parser (`sorrel.workflow.yml` → `JobBundle`, Core-gated); `sorrel-vault` dev CLI (import/list/grant/redact); CLI `workflow validate/run`. |
| 2026-06-26 | **P0 "persistent local VCS demo" complete** in `sorrel-cli`: real `init/status/change create/diff(line-level)/log`, persistent `.sorrel/` state across processes, `DEMO.md`. Engine unfragmentation: CLI depends on real `sorrel-core`/`sorrel-runners` by git rev; vendored copies deleted. |
| 2026-06-26 | Cleanups A–D: engine-level `.sorrel` exclusion (no scratch copies), `cli_*` compat modules retired, all mocked CLI commands made real, dependency-free perf benches with budgets. |
| 2026-06-30 | **Phase R sync transport**: protocol spec, core `transport` module (closure/missing/transfer), Hub sync endpoints + vendored BLAKE3, CLI `remote/push/pull` + `SYNC.md`. |
| 2026-07-01 | Post-A1/A2 integration: core stat-cache (A1) wired into CLI status/change (A1b); sync policy conformance vectors (A2) vendored everywhere (A3); `sorrel-core-stub` removed; root pointers advanced (root PR #40). |
| 2026-07-01 | Protocol `Workspace` object with `componentLinks` (member vs dependency) — PR #6 `8a37620`. |
| 2026-07-17 | Root cleanup: removed the legacy pre-submodule `crates/sorrel-core` root workspace, retired the completed `SORREL_PROTOTYPE_PLAN.md`, consolidated this dashboard, added `ROADMAP.md`. Found + fixed a broken `npm test` on `sorrel-protocol/main` (Workspace componentLinks schema failed ajv strict mode; fix `7240252`, tests 8/8, validate ok). Protocol pointer advanced to `7240252`. |
| 2026-07-17 | **Roadmap item 1 shipped — Hub FS-backed sync store.** `sorrel-hub` `e926caf` (merge of `3f1a97e`): `src/fs-sync-store.js` drop-in for the in-memory `RepoSyncStore` — content-addressed fanout mirroring core's `FileObjectStore`, atomic temp+rename writes, digest-verified reads, percent-encoded repo/ref path segments; server persists to `SORREL_HUB_DATA_DIR` (default `./data/sync`), `SORREL_HUB_SYNC_STORE=memory` opts out; tests stay in-memory. New tests incl. an end-to-end push → restart → pull flow; `npm test` 32/32. Root `sorrel-hub` pointer advanced. |
| 2026-07-17 | **Task-pack wave merged** (user dispatched + merged all task PRs). Protocol: Conflict/MergeResult schemas + examples + docs (PR #7 `082e86f`). Core: merge_base/merge_bases over the snapshot DAG (PR #9), dependency-free merge3 (PR #10), Conflict/MergeResult objects (PR #11), snapshot-level merge_snapshots (PR #12) → main `eed47d0`, 94 tests. CLI: per-lane heads + lane list/switch (PR #15), snapshot→change index for log (PR #14), `sorrel merge` ff/3-way/--abort (PR #16). Hub: FS metadata store (PR #6), GET /admin/sync-repos (PR #7) → `d8119b7`, 44 tests. Hub-web: read-only Sync view (PR #1) → `5cc7137`, 6 tests. |
| 2026-07-17 | **Integration fix (orchestrator):** the CLI merge work (CLI-3) had been built against a reintroduced local `sorrel-core-stub` whose API drifted from the real engine. Ported `merge` to the real API (MergeOptions; MergeResult fields; conflict markers now regenerated from stored Conflict objects via merge3; log falls back to snapshot `createdAt` since engine Changes carry no timestamp), re-pinned `sorrel-core` to `eed47d0`, deleted the stub + `[patch]` table. `sorrel-cli` main `5340f75`; `cargo test --workspace` 74 pass, clippy + fmt clean. All root pointers advanced. CI-1/CI-2 workflows were NOT found on any main. |
| 2026-07-21 | **Happy-path sync + docs/compose landed.** CLI PR #17 `ba83dfa` (grantRefs, refs array, worktree restore on pull). Hub PR #8 `f9028fc` (bootstrap grants, Core protocol closures, Docker). Hub-web PR #2 `bb71023` (deployable static+proxy). Web PR #2 `422eed3` (STATUS/GETTING_STARTED guides + landing refresh; Cloudflare Pages). Root: `docs/`, `docker-compose.yml`, README rewrite, pointers advanced. Next: roadmap #3 one-way Git import. |
