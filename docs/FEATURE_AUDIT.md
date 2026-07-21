# Sorrel feature audit

Last updated: 2026-07-21 (implementation pass: merge continue, git export, stack CLI, hub-web UX, CI, ContainerRunner tests)

Master checklist of every claimed capability vs verification status.

**Legend**

| Status | Meaning |
| --- | --- |
| WORKING | Exercised this pass (module suite and/or root E2E / live probe) |
| IMPLEMENTED | In code + covered by existing tests; treated as green |
| PARTIAL | Exists but incomplete vs protocol/spec/docs |
| MISSING | Documented or planned; not implemented |
| N/A | Spec-only / deferred product surface |

**This pass evidence**

- `cargo test` core (git_export + prior), cli (merge continue, git export, stack)
- `cargo test` runners (ContainerRunner suite)
- Hub-web UI: proposal detail + workflow mutate + Nord polish
- Root CI workflow added; hub + hub-web CI workflows added
- Root E2E extended for merge `--continue`, stack, git export

---

## Summary dashboard

| Area | Working / shipped | Partial | Missing (high-signal) |
| --- | --- | --- | --- |
| Protocol | Schemas, examples, conformance | Workspace-links CLI emission | Shadow sync, report-only object types |
| Core | Store, snapshot, change, merge, policy, git_import, **git_export**, transport | Conflict/MergeResult schema drift | Packfiles, embedding ABI, other backends |
| CLI | Full local VCS + sync + lane submit + workflow + policy + **merge --continue** + **git export** + **stack** | — | undo / agent start wishlist |
| Git | Import + export + git-map | — | Colocated sync, GH mirror |
| Sync | 5 endpoints + bootstrap grants + CLI push/pull | Skeleton principal header | Production auth, shadow mode |
| Hub API | Projects, admin CRUD/PATCH, lane-submit, sync | — | Issues, merge queue, marketplace, SSO |
| Hub UI | Projects/proposals/comments write + proposal detail + workflow mutate + sync read | No login | Marketplace |
| Vault | Spec, local CLI, conformance | — | Cloud providers |
| Runners | LocalProcess + **ContainerRunner tested** + YAML parser + gates | CLI uses in-tree cli_runner | SSH/K8s/GHA/WASM |
| Slices | TS/JS generator + CLI slice create | Prototype only | Multi-lang, sync, extract repo |
| Agents | register / claim / activeWork | Hub mirror is soft | Overlays, AGENTS.md materialize |
| SDKs | Hub JS client; Rust Workspace | Minimal surfaces | Embedding / full protocol SDKs |
| Landing | Static site + local serve in E2E | — | — |
| Infra | Root E2E + module suites; compose; **CI workflows** | — | Apps |

**Bottom line:** Happy-path stack is **WORKING**, including **merge --continue**, **git export**, **stack CLI**, richer Hub review UX, ContainerRunner tests, and root/module CI scaffolding. Still ahead: colocated Git sync, embedding surface, production auth, apps/marketplace.

---

## A. Protocol (`sorrel-protocol`)

| # | Item | Status | Evidence |
| --- | --- | --- | --- |
| A1–A10 | Schemas / examples / sync / merge docs | WORKING | prior pass |
| A11 | Workspace-links CLI emission | PARTIAL | doc; no CLI `componentLinks` |
| A12 | Sync shadow mode | MISSING | |
| A13 | Report-only types | MISSING | |

---

## B. Core (`sorrel-core`)

| # | Item | Status | Evidence |
| --- | --- | --- | --- |
| B1–B14 | Store through benches | WORKING / IMPLEMENTED | |
| B15 | git_export | WORKING | `src/git_export.rs` + tests |
| B16 | Conflict/MergeResult vs protocol | PARTIAL | |
| B17–B19 | Packfiles / ABI / other backends | MISSING | |

---

## C. CLI (`sorrel-cli`)

| # | Item | Status | Evidence |
| --- | --- | --- | --- |
| C1–C9 | init… merge --abort | WORKING | |
| C10 | merge --continue | WORKING | json_output + E2E |
| C11 | git import | WORKING | |
| C12 | git export | WORKING | git_export tests + E2E |
| C13–C23 | grant… sync | WORKING | |
| C24 | stack create/list/show | WORKING | git_export.rs stack tests + E2E |
| C25 | undo / agent start | MISSING | wishlist |

---

## D. Git bridge

| # | Item | Status | Evidence |
| --- | --- | --- | --- |
| D1–D2 | Import + flags | WORKING | |
| D3 | Export | WORKING | core + CLI |
| D4–D5 | Colocated / GH mirror | MISSING | |

---

## E–F. Sync / Hub API

Unchanged: WORKING sync + collaboration; MISSING production auth / marketplace.

---

## G. Hub UI

| # | Item | Status | Evidence |
| --- | --- | --- | --- |
| G1–G7 | Static, projects, admin, sync, health | WORKING | |
| G8 | Rich review UX | WORKING | proposal detail pane, comment thread, workflow mutate |
| G9 | Login | MISSING | |

---

## I. Runners

| # | Item | Status | Evidence |
| --- | --- | --- | --- |
| I1 | LocalProcessRunner | WORKING | |
| I2 | ContainerRunner | WORKING | `tests/container_runner.rs` (docker when present) |
| I3–I4 | YAML / policy / redaction | WORKING | |
| I5 | CLI in-tree cli_runner | PARTIAL | intentional |
| I6 | SSH / K8s / GHA / WASM | MISSING | |

---

## K. Infra

| # | Item | Status | Evidence |
| --- | --- | --- | --- |
| K6–K8 | E2E / modules / compose | WORKING | |
| K9 | CI on root / hub / hub-web | IMPLEMENTED | `.github/workflows/ci.yml` |
| K10–K11 | Apps / marketplace | MISSING | |

---

## Recommended next

1. Colocated / git sync  
2. Conflict/MergeResult protocol alignment  
3. Embedding surface (then richer SDKs)  
4. Production auth  
5. Apps / marketplace
