# Sorrel Progress Dashboard

Last updated: 2026-07-17 UTC (repo cleanup pass: legacy root `crates/` workspace removed, notes consolidated, `ROADMAP.md` added, protocol pointer advanced)

This is the single live status document for Sorrel orchestration. The forward
plan lives in [`ROADMAP.md`](ROADMAP.md); the full architecture lives in
[`AGENT_NATIVE_VERSION_CONTROL_REPORT.md`](AGENT_NATIVE_VERSION_CONTROL_REPORT.md).
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

## Where the system stands (2026-07-17)

- **Engine (`sorrel-core`)**: real and tested — content-addressed
  `FileObjectStore`, snapshots, changes, path-level diff, lanes/stacks, policy
  and authority spine, sync transport closure helpers, stat-cache. 58 tests,
  fmt/clippy clean. Perf benches (`cargo bench`) with coarse budgets.
- **CLI (`sorrel-cli`)**: a persistent, single-user local VCS over the real
  engine. `init / status / change create / diff (line-level) / log / lane
  create / grant / slice create / workflow validate|run / remote add / push /
  pull` are all real and persist under `.sorrel/`; no mocked commands remain.
  Pins `sorrel-core` by git rev. See `sorrel-cli/DEMO.md` and
  `sorrel-cli/SYNC.md`.
- **Sync**: protocol spec + Core transport + Hub endpoints
  (`/{repoId}/refs`, `/objects`, `/objects/missing`) + CLI push/pull all
  landed and conformance-covered. Hub sync objects/refs now persist to disk
  by default (FS-backed store, 2026-07-17); product metadata (projects,
  proposals, ...) is still in-memory.
- **Policy spine**: unified across core, cli, hub, runners, vault via vendored
  protocol fixtures with automated drift guards. One real drift (CLI authority
  rotation) was surfaced and fixed by this machinery.
- **Hub split**: `sorrel-hub` is the JSON API server (no UI), `sorrel-hub-web`
  the browser frontend (read-only Projects + Administration views),
  `sorrel-web` the unrelated public landing page.
- **Protocol**: newest addition is the `Workspace` object with
  `componentLinks` (branch-tracked members vs revision-pinned dependencies)
  for multi-repo/monorepo layouts — `sorrel-protocol` PR #6 (`8a37620`), plus
  an ajv strict-mode schema fix on top (`7240252`; PR #6 had landed with a
  failing `npm test`).

## Module status

| Module | Status | Root pointer | Notes |
| --- | --- | --- | --- |
| `sorrel-protocol` | Active | `7240252` (main) | Schemas, sync-transport spec, policy conformance manifest + sidecar, Workspace componentLinks. `npm test` 8/8, `npm run validate` ok. |
| `sorrel-core` | Active | `7a5d7f6` (main) | Full engine incl. transport + stat-cache. **An agent is currently working in this repo — coordinate before advancing the pointer.** |
| `sorrel-cli` | Active | `14ccdee` (main) | All commands real + persistent; stat-cache wired; sync client against real engine; 59 workspace tests. |
| `sorrel-vault` | Active | `b710fff` (main) | Secrets spec, local backend, dev CLI (import/list/grant/redact); Core-grant gated; 13 tests. |
| `sorrel-runners` | Active | `e8effa2` (main) | Local/container runner model + `sorrel.workflow.yml` parser; Core policy gate + redaction. |
| `sorrel-slices` | Active | `bd820c9` (main) | TS/JS slice manifest generator prototype. |
| `sorrel-hub` | Active | `e926caf` (main) | Collaboration API server; sync transport endpoints; FS-backed sync store (objects/refs persist across restarts; `SORREL_HUB_DATA_DIR`). 32 tests. |
| `sorrel-hub-web` | Scaffolded | `c1a9a88` (main) | Framework-free browser frontend proxying `/api/*` to Hub; read-only views; 4 tests. |
| `sorrel-web` | Done for now | `6786303` (main) | Public marketing/landing site (static, Nord theme). Not the Hub UI. |
| `sorrel-agents` | Not started | scaffold | Agent control plane; starts once lanes/claims semantics settle. |
| `sorrel-sdk-js` | Not started | scaffold | Starts after the embedding surface (roadmap) stabilizes. |
| `sorrel-sdk-rust` | Not started | scaffold | Starts after core APIs settle. |

## Active agents

| Agent | Target | Goal | Notes |
| --- | --- | --- | --- |
| (user-reported, 2026-07-17) | `sorrel-core` | Assess/clean/re-plan `sorrel-core` | Do not advance the root `sorrel-core` pointer or start conflicting core work until it reports. |
| This pass | root, `sorrel-protocol`, `sorrel-hub` | Root cleanup + roadmap; protocol schema fix; Hub FS-backed sync store | Roadmap item 1 shipped (`sorrel-hub` `e926caf`). |

## Known debt / follow-ups

- Hub product metadata (projects, proposals, admin collections) is still in-memory; only sync objects/refs persist.
- `apply_change` validates but does not patch/merge; conflicts are a placeholder type (roadmap #2).
- No Git bridge anywhere yet (roadmap #3).
- CLI `log` shows snapshot-DAG entries; change ids/authors need a richer change-graph index.
- Vendored conformance copies could later be replaced by a published shared package (npm/crate).
- `sorrel-hub-web` is read-only; no sync-status or write flows yet.

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
