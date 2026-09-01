# Sorrel status

Last updated: 2026-09-01

What works today, what does not, and where to look next. For how to run the
stack, see [GETTING_STARTED.md](GETTING_STARTED.md). The forward plan is
[`ROADMAP.md`](../ROADMAP.md).

## Snapshot

Sorrel already has a **working local VCS loop** (init → change → lanes → merge →
push/pull) on a real content-addressed engine, plus **Git import/export and
colocated bidirectional sync**, a deployable Hub API, and a writable Hub UI
companion. The public landing site is live. A root **no-mock E2E** (`npm test`)
wires every active module together. A native mobile Hub companion now covers
projects, reviews, and repository refs. Still ahead: production auth, richer
agents/SDKs, desktop distribution, and on-device Core embedding.

The latest coordinated release is
**[`v0.1.0-alpha.2`](https://github.com/MGRAFF2006/sorrel/releases/tag/v0.1.0-alpha.2)**,
an installable developer preview with downloadable CLI artifacts and hostable
server images. The Hub is not safe for untrusted network exposure without a
production AuthAdapter and network controls. See the root
[`CHANGELOG.md`](../CHANGELOG.md) for the complete shipped record.

## Working

| Area | What you can do |
| --- | --- |
| **Protocol** | Canonical object schemas, examples, sync-transport spec, policy conformance manifest + checksum drift guards. |
| **Engine (`sorrel-core`)** | Content-addressed object store, snapshots, changes, path/line-level diff helpers, lanes/stacks, policy/authority spine, sync closure helpers, stat-cache, three-way merge + protocol-aligned conflict/merge-result objects, **incremental `git_import` / `git_export`**. |
| **CLI (`sorrel-cli`)** | Persistent `.sorrel/` workspace: `init`, `status`, `change create`/`list`, `diff`, `log`, `lane create`/`list`/`switch`/`submit`, `stack create`/`list`/`show`, `merge` / `merge --abort` / **`merge --continue`**, **`git import` / `git export` / `git sync`**, `grant`, `slice create`, `workflow validate`/`run`, **`secret list|sync|check|get|set|run`**, **`env init|ensure|info|shell`**, **`run list|show|logs`**, `remote add`/`list`, `push`, `pull`. |
| **Git bridge** | `sorrel git import`, `git export`, and colocated `git sync`; incremental fast-forwards in either direction, divergence parked on a normal Sorrel lane, `.sorrel/git-map.json` links SHAs ↔ snapshots. See `sorrel-cli/GIT.md`. |
| **Sync** | CLI ↔ Hub over HTTP sync transport; Hub FS-backed object/ref store; isolated demos can opt into `user:local` bootstrap grants with `SORREL_HUB_BOOTSTRAP_GRANTS=1`. |
| **Vault** | Secrets schema + local Node backend for tests. **Primary UX:** `sorrel secret *` resolves via upstream SecretSpec (`keyring` / `dotenv` / `env`) under Core grants; workflow jobs can inject authorized `secretRefs` with log redaction. |
| **Runners** | Local + container runners (ContainerRunner tested), `sorrel.workflow.yml` → `JobBundle` parser, Core policy gate + log redaction. CLI prefers devenv when present (`backend: devenv`), else `local-fallback`. Structured logs under `.sorrel/runs/`. |
| **Slices** | TS/JS slice manifest generator (prototype). |
| **Hub API** | JSON HTTP server: health, projects, admin collections with GET/PATCH, proposals/reviews lifecycle, lane-submit collaboration endpoint, sync endpoints, FS persistence. |
| **Hub UI** | Shared Solid `sorrel-hub-ui` — project-first (GitHub-like): home lists projects; implemented Reviews/Sync surfaces nest under `/projects/:id`; hosted by thin `sorrel-hub-web`. |
| **Mobile app** | Native Expo/React Native Hub companion for iPhone, iPad, Android phones, and Android tablets; native stacks/tabs, secure bearer storage, project/review/comment lifecycle, and repository refs. |
| **Hub install seams** | `GET /capabilities` + `GET /session`; AuthAdapter (`dev` / `workos` / OIDC JWKS); shared `sorrel-hub/convex/` schema for SaaS + self-host. |
| **Server distribution** | Release automation for unprivileged, health-checked Linux amd64/arm64 Hub API and browser-host images, with GHCR publication, SBOM/provenance, immutable digests, checksums, and a release Compose file. |
| **Landing (`sorrel-web`)** | Static marketing site (Nord theme). Production deploy is Cloudflare Pages; local Docker is optional preview only. |
| **Root E2E / CI** | `npm test` E2E and `npm run test:modules` from one checkout. Root Actions checks out the monorepo directly — no submodule PAT. |

## Missing / not ready

| Area | Gap |
| --- | --- |
| **Production auth** | AuthAdapter (`dev` / WorkOS / OIDC JWKS), `GET /session`, bind-safety; WorkOS sealed sessions + UI IdP login still ahead. |
| **Format migrations** | Protocol and object stores are `v0`; unknown versions fail closed, but no general workspace/Hub migration framework is shipped. |
| **Agents control plane** | Minimal register/claim/active-work surface shipped; no instruction overlays yet. |
| **SDKs** | Minimal Hub JS client + Rust `Workspace` wrapper shipped; embedding surface (C ABI / N-API / WASM / daemon) not shipped. |
| **App embedding** | Mobile ships as a thin Hub companion only; no app embeds Core or operates a local workspace until the stable embedding surface exists. Desktop distribution is not yet shipped. |
| **Hub secret backend** | Optional hosted / BYO provider binding (Phase 4) not shipped; local keyring/dotenv remain default. |
| **devenv task mapping** | Prefer devenv when present; full `sorrel.workflow.yml` → devenv tasks shim and remote runners are still thin. |
| **Run log follow / Hub stream** | Local `.sorrel/runs/` + `run show|logs` shipped; unsupported `--follow` requests now fail explicitly, and Hub streaming is not implemented. |

## Module map

| Module | Role | Maturity |
| --- | --- | --- |
| `sorrel-protocol` | Schemas + conformance | Active |
| `sorrel-core` | Rust engine | Active |
| `sorrel-cli` | Local VCS CLI | Active |
| `sorrel-vault` | Secrets | Active |
| `sorrel-runners` | Workflows | Active |
| `sorrel-slices` | Slice manifests | Active (prototype) |
| `sorrel-hub` | Hub API | Active (collaboration + sync + capabilities/AuthAdapter; release container) |
| `sorrel-hub-ui` | Shared Solid Hub UI | Active (Phase-1 foundation) |
| `sorrel-hub-mobile` | Native phone/tablet Hub companion | Active (HTTP companion) |
| `sorrel-hub-web` | Thin browser host | Active (Vite host over hub-ui; release container) |
| `sorrel-web` | Public landing | Live (Cloudflare) |
| `sorrel-agents` | Agent control plane (register/claim/active work) | Active (minimal) |
| `sorrel-sdk-js` | Hub HTTP client SDK | Active (minimal) |
| `sorrel-sdk-rust` | Rust SDK over `sorrel-core` | Active (minimal) |

## Layout note

Packages that used to be private submodules now live in-tree. Prefer path
dependencies and workspace Cargo commands from the repo root.

## Next up (from roadmap)

1. Finish production auth (WorkOS sealed sessions + IdP login UI) and richer review UX.
2. Deepen devenv workflow mapping, run-log follow/Hub streaming, and optional
   hosted or bring-your-own secret backends.
3. Define the stable embedding surface, then mature agents, SDKs, and local app workflows around it.
4. Add format migrations before persisted `v0` formats begin evolving rapidly.
5. Collapse intentional duplicates (`cli_policy` / `cli_runner`) now that
   SecretSpec injection has landed.
