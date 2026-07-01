# Sorrel progress

Last updated: 2026-07-01

Lightweight status for humans and agents. **Detail lives in specs**, not here:

- [`SORREL_PROTOTYPE_PLAN.md`](SORREL_PROTOTYPE_PLAN.md) — phases and scope
- [`docs/AGENT_WORKSPACE.md`](docs/AGENT_WORKSPACE.md) — how to work across submodules
- [`AGENT_NATIVE_VERSION_CONTROL_REPORT.md`](AGENT_NATIVE_VERSION_CONTROL_REPORT.md) — architecture

Update this file when a milestone lands or the **next** priority changes. Do not paste PR
play-by-plays or agent handoffs here.

## How we work

- Root `sorrel/` is one **workspace**; each `sorrel-*` folder is its own git repo for publish.
- Merge implementation to each submodule's `main`, then advance the root submodule pointer.
- Conformance: `sorrel-protocol/conformance/policy-conformance.json` is canonical;
  `./scripts/sync-conformance.sh` refreshes consumers.
- Submodule pointers: optional auto-sync via `./scripts/sync-submodule-pointers.sh`
  (floating workspace mode — see `docs/AGENT_WORKSPACE.md`).

## Milestones reached

| Milestone | Summary |
| --- | --- |
| **Foundation** | Protocol schemas, core engine, policy spine, conformance; **`Workspace.componentLinks`** (member/dependency) |
| **P0** | Persistent local CLI VCS (`init` … `log`); see `sorrel-cli/DEMO.md` |
| **Integrations** | Runners workflow YAML, vault dev CLI, hub skeleton, hub-web scaffold |
| **Phase R** | Sync transport spec + core `transport` + hub object/ref API + CLI `remote`/`push`/`pull` |
| **A1 / A1b** | Stat-cache in core (PR #7, `3a8f3be`) **and wired into the CLI** (`status`/`change create` persist `.sorrel/stat-cache.json`) |
| **A2 / A3** | Sync policy conformance (`repo.object.write`/`repo.ref.write`, protocol PR #5 `c2ac9cc`) vendored into every consumer; `sorrel-core-stub` removed and CLI sync client fixed against the real engine |

## Module snapshot

| Module | Status | Pointer |
| --- | --- | --- |
| `sorrel-protocol` | Active | submodule `main` |
| `sorrel-core` | Active | submodule `main` |
| `sorrel-cli` | Active | submodule `main` |
| `sorrel-hub` | Active | submodule `main` |
| `sorrel-runners` | Active | submodule `main` |
| `sorrel-vault` | Active | submodule `main` |
| `sorrel-slices` | Active | submodule `main` |
| `sorrel-hub-web` | Scaffold | submodule `main` |
| `sorrel-web` | Active (marketing) | submodule `main` |
| `sorrel-agents` | Planned | — |
| `sorrel-sdk-js` / `sorrel-sdk-rust` | Planned | — |

## Current focus

1. **Post–A1/A2 integration** — DONE. CLI stat-cache wired, A2/A3 conformance vendored into all
   consumers, `sorrel-core-stub` + `[patch]` removed, CLI sync client fixed against the real engine,
   `sorrel-cli` pins `sorrel-core` at `sorrel-core/main` (`7a5d7f6`).
2. **Next product fork (pick one)** — **Hub filesystem object store** (persist sync objects/refs
   across restarts) **or** the **Git bridge** (`init --git-colocated`, import/export).
3. **Phase A tail** — engine perf tuning; 3-way merge / conflict objects.

## Backlog (ordered)

| # | Work | Where |
| --- | --- | --- |
| 1 | ~~Stat-cache CLI wire + core pin~~ **DONE (A1b)** | `sorrel-cli`, `sorrel-core` |
| 2 | ~~Sync policy conformance vendoring (A2/A3)~~ **DONE** | `sorrel-protocol` + all consumers |
| 3 | Hub filesystem object store (survive restart) | `sorrel-hub` |
| 4 | Git bridge (`init --git-colocated`, import/export) | `sorrel-core`, `sorrel-cli` |
| 5 | 3-way merge + conflict objects | `sorrel-core` |
| 6 | Hub proposals/reviews | `sorrel-hub`, `sorrel-hub-web` |
| 7 | Agent control plane | `sorrel-agents` |
| 8 | Wire local grant registry into `grantRefs` on CLI push (mutating-body contract) | `sorrel-cli` |

## Not yet

Marketplace, full merge queue, hosted compute, production auth, sophisticated conflict resolution,
desktop/mobile apps.

## Submodule pointer repair

```bash
git -C <submodule> checkout main && git -C <submodule> pull
git add <submodule> && git commit -m "Point <submodule> at main" && git push
```

Use `GIT_FS_MONITOR_ENABLED=false` if `git add` on submodules hangs.

## Changelog (milestones only)

| Date | Event |
| --- | --- |
| 2026-07-01 | **Post-A1/A2 integration complete.** A1b: stat-cache wired into CLI `status`/`change create` (`.sorrel/stat-cache.json`, atomic). Removed `sorrel-core-stub` + `[patch]`; CLI sync client rewritten for the real engine (correct `/{repoId}/...` paths, array `want`/`have`, closure-seeded push, verified pull). A2 sync policy conformance (protocol PR #5) vendored via `scripts/sync-conformance.sh` into core/cli/hub/vault/runners; `--check` clean. Merge SHAs: protocol `545835d`, core `7a5d7f6`, cli `14ccdee` (PR #13), hub `b04a7e0`, vault `b710fff`, runners `e8effa2`. Root pointers advanced. **Next: Hub FS store or Git bridge.** |
| 2026-07-01 | Phase R merged; root pointers advanced; agent workspace doc added |
| 2026-06-26 | P0 complete; engine cleanups; Phase R started |
| 2026-06-24–25 | Policy conformance + authority hardening across modules |
| 2026-06-24 | Initial multi-module foundation (protocol, core, CLI, hub, runners, vault) |
