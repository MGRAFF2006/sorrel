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

## Milestones reached

| Milestone | Summary |
| --- | --- |
| **Foundation** | Protocol schemas, core engine, policy/authority spine, conformance manifest + drift guards |
| **P0** | Persistent local CLI VCS (`init` … `log`); see `sorrel-cli/DEMO.md` |
| **Integrations** | Runners workflow YAML, vault dev CLI, hub skeleton, hub-web scaffold |
| **Phase R** | Sync transport spec + core `transport` + hub object/ref API + CLI `remote`/`push`/`pull` |
| **Phase A (partial)** | Stat-cache in core; sync policy in conformance — integration in flight |

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

1. **Post–A1/A2 integration** — CLI stat-cache wire, conformance vendoring, remove any core stub;
   correct `sorrel-core` git pin in `sorrel-cli`.
2. **Phase A** — stat-cache in daily CLI paths; engine perf; merge/conflicts (see prototype plan).
3. **Next product fork** — Hub persistent sync store **or** Git bridge (pick one).

## Backlog (ordered)

| # | Work | Where |
| --- | --- | --- |
| 1 | Stat-cache CLI wire + core pin | `sorrel-cli`, `sorrel-core` |
| 2 | Conformance export after protocol changes | `scripts/sync-conformance.sh` |
| 3 | Hub filesystem object store (survive restart) | `sorrel-hub` |
| 4 | Git bridge (`init --git-colocated`, import/export) | `sorrel-core`, `sorrel-cli` |
| 5 | 3-way merge + conflict objects | `sorrel-core` |
| 6 | Hub proposals/reviews | `sorrel-hub`, `sorrel-hub-web` |
| 7 | Agent control plane | `sorrel-agents` |

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
| 2026-07-01 | Phase R merged; root pointers advanced; agent workspace doc added |
| 2026-06-26 | P0 complete; engine cleanups; Phase R started |
| 2026-06-24–25 | Policy conformance + authority hardening across modules |
| 2026-06-24 | Initial multi-module foundation (protocol, core, CLI, hub, runners, vault) |
