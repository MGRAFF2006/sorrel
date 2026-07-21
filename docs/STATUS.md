# Sorrel status

Last updated: 2026-07-21

What works today, what does not, and where to look next. For how to run the
stack, see [GETTING_STARTED.md](GETTING_STARTED.md). Orchestration detail for
agents lives in [`SORREL_PROGRESS.md`](../SORREL_PROGRESS.md); the forward plan
is [`ROADMAP.md`](../ROADMAP.md).

## Snapshot

Sorrel already has a **working local VCS loop** (init → change → lanes → merge →
push/pull) on a real content-addressed engine, plus **one-way Git import**, a
deployable Hub API, and a read-only Hub UI. The public landing site is live.
Still ahead: Git export/colocated mirror, Hub write flows, agents/SDKs, and CI.

## Working

| Area | What you can do |
| --- | --- |
| **Protocol** | Canonical object schemas, examples, sync-transport spec, policy conformance manifest + checksum drift guards. |
| **Engine (`sorrel-core`)** | Content-addressed object store, snapshots, changes, path/line-level diff helpers, lanes/stacks, policy/authority spine, sync closure helpers, stat-cache, three-way merge + conflict objects, **one-way `git_import`**. |
| **CLI (`sorrel-cli`)** | Persistent `.sorrel/` workspace: `init`, `status`, `change create`/`list`, `diff`, `log`, `lane create`/`list`/`switch`, `merge` / `merge --abort`, **`git import`**, `grant`, `slice create`, `workflow validate`/`run`, `remote add`/`list`, `push`, `pull` (restores working tree). No core stub — pins real `sorrel-core` by git rev. |
| **Git import** | `sorrel git import [PATH]` walks a Git ref into Sorrel snapshots/changes; writes `.sorrel/git-map.json`. See `sorrel-cli/GIT.md`. |
| **Sync** | CLI ↔ Hub over HTTP sync transport; Hub FS-backed object/ref store; local bootstrap grants so `user:local` can push/pull out of the box. |
| **Vault** | Secrets schema, local backend, dev CLI (`import` / `list` / `grant` / `redact`), Core-grant gated. |
| **Runners** | Local/container runner model, `sorrel.workflow.yml` → `JobBundle` parser, Core policy gate + log redaction. |
| **Slices** | TS/JS slice manifest generator (prototype). |
| **Hub API** | JSON HTTP server: health, projects, admin collections, sync endpoints, FS persistence for sync + product metadata. |
| **Hub UI** | Framework-free browser UI: Projects, Administration, Sync (read-only), proxies `/api/*` to Hub. |
| **Landing (`sorrel-web`)** | Static marketing site (Nord theme). Production deploy is Cloudflare Pages; local Docker is optional preview only. |

## Missing / not ready

| Area | Gap |
| --- | --- |
| **Git export / colocated** | One-way import works; export and bidirectional/colocated mirror are still ahead (roadmap item 3). |
| **Merge continue** | Conflicts write markers + `MERGE_STATE` and support `--abort`; no `resolve` / `--continue`. |
| **Hub write UI** | Hub web is read-only — no create proposal / review / mutate flows in the browser. |
| **Production auth** | Hub skeleton has no real login, SSO, or signed client identity beyond acting-principal headers + trusted grants. |
| **Agents control plane** | `sorrel-agents` is a scaffold only. |
| **SDKs** | `sorrel-sdk-js` and `sorrel-sdk-rust` are scaffolds; embedding surface (C ABI / N-API / WASM / daemon) not shipped. |
| **CI** | No GitHub Actions on module mains yet. |
| **Schema alignment** | Engine-stored `Conflict` / `MergeResult` fields still drift from some protocol-required properties. |
| **Apps** | No desktop/mobile clients (intentionally after embedding surface). |

## Module map

| Module | Role | Maturity |
| --- | --- | --- |
| `sorrel-protocol` | Schemas + conformance | Active |
| `sorrel-core` | Rust engine | Active |
| `sorrel-cli` | Local VCS CLI | Active |
| `sorrel-vault` | Secrets | Active |
| `sorrel-runners` | Workflows | Active |
| `sorrel-slices` | Slice manifests | Active (prototype) |
| `sorrel-hub` | Hub API | Active |
| `sorrel-hub-web` | Hub UI | Active (read-only) |
| `sorrel-web` | Public landing | Live (Cloudflare) |
| `sorrel-agents` | Agent control plane | Not started |
| `sorrel-sdk-js` | JS SDK | Not started |
| `sorrel-sdk-rust` | Rust SDK | Not started |

## Next up (from roadmap)

1. Git export + colocated mirror (import shipped).
2. Hub collaboration write path (proposals/reviews + hub-web mutations).
3. Stable embedding surface, then agents + SDKs.
