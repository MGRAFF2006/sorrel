# Sorrel status

Last updated: 2026-07-22

What works today, what does not, and where to look next. For how to run the
stack, see [GETTING_STARTED.md](GETTING_STARTED.md). The forward plan is
[`ROADMAP.md`](../ROADMAP.md).

## Snapshot

Sorrel already has a **working local VCS loop** (init → change → lanes → merge →
push/pull) on a real content-addressed engine, plus **Git import/export**, a
deployable Hub API, and a writable Hub UI companion. The public landing site is live.
A root **no-mock E2E** (`npm test`) wires every active module together. Still
ahead: colocated Git mirror, production auth, richer agents/SDKs, and apps.

## Working

| Area | What you can do |
| --- | --- |
| **Protocol** | Canonical object schemas, examples, sync-transport spec, policy conformance manifest + checksum drift guards. |
| **Engine (`sorrel-core`)** | Content-addressed object store, snapshots, changes, path/line-level diff helpers, lanes/stacks, policy/authority spine, sync closure helpers, stat-cache, three-way merge + protocol-aligned conflict/merge-result objects, **`git_import` / `git_export`**. |
| **CLI (`sorrel-cli`)** | Persistent `.sorrel/` workspace: `init`, `status`, `change create`/`list`, `diff`, `log`, `lane create`/`list`/`switch`/`submit`, `stack create`/`list`/`show`, `merge` / `merge --abort` / **`merge --continue`**, **`git import` / `git export`**, `grant`, `slice create`, `workflow validate`/`run`, `remote add`/`list`, `push`, `pull`. |
| **Git bridge** | `sorrel git import` and `sorrel git export`; `.sorrel/git-map.json` links SHAs ↔ snapshots. See `sorrel-cli/GIT.md`. |
| **Sync** | CLI ↔ Hub over HTTP sync transport; Hub FS-backed object/ref store; local bootstrap grants so `user:local` can push/pull out of the box. |
| **Vault** | Secrets schema, local backend, dev CLI (`import` / `list` / `grant` / `redact`), Core-grant gated. |
| **Runners** | Local + container runners (ContainerRunner tested), `sorrel.workflow.yml` → `JobBundle` parser, Core policy gate + log redaction. |
| **Slices** | TS/JS slice manifest generator (prototype). |
| **Hub API** | JSON HTTP server: health, projects, admin collections with GET/PATCH, proposals/reviews lifecycle, lane-submit collaboration endpoint, sync endpoints, FS persistence. |
| **Hub UI** | Framework-free Nord companion: Projects, Reviews (proposal detail + comment thread + workflow status), Sync; proxies `/api/*` to Hub. |
| **Landing (`sorrel-web`)** | Static marketing site (Nord theme). Production deploy is Cloudflare Pages; local Docker is optional preview only. |
| **Root E2E / CI** | `npm test` E2E; `.github/workflows/ci.yml` on root, hub, and hub-web. |

## Missing / not ready

| Area | Gap |
| --- | --- |
| **Colocated Git sync** | One-way import/export work; bidirectional/colocated mirror still ahead. |
| **Production auth** | Hub skeleton has no real login, SSO, or signed client identity beyond acting-principal headers + trusted grants. |
| **Agents control plane** | Minimal register/claim/active-work surface shipped; no instruction overlays yet. |
| **SDKs** | Minimal Hub JS client + Rust `Workspace` wrapper shipped; embedding surface (C ABI / N-API / WASM / daemon) not shipped. |
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
| `sorrel-hub` | Hub API | Active (collaboration + sync) |
| `sorrel-hub-web` | Hub UI | Active (read + write for proposals/reviews) |
| `sorrel-web` | Public landing | Live (Cloudflare) |
| `sorrel-agents` | Agent control plane (register/claim/active work) | Active (minimal) |
| `sorrel-sdk-js` | Hub HTTP client SDK | Active (minimal) |
| `sorrel-sdk-rust` | Rust SDK over `sorrel-core` | Active (minimal) |

## Next up (from roadmap)

1. Git export + colocated mirror (import shipped).
2. Hub collaboration write path (proposals/reviews + hub-web mutations).
3. Stable embedding surface, then agents + SDKs.
