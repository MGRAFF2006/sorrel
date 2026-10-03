# Sorrel status

Last updated: 2026-10-03

What works today, what does not, and where to look next. For how to run the
stack, see [GETTING_STARTED.md](GETTING_STARTED.md). The forward plan is
[`ROADMAP.md`](../ROADMAP.md).

## Snapshot

Sorrel already has a **working local VCS loop** (init → change → lanes → merge →
push/pull) on a real content-addressed engine, plus **Git import/export and
colocated bidirectional sync**, a deployable Hub API, and a writable Hub UI
companion. The public landing site is live. A root **no-mock E2E** (`npm test`)
wires every active module together. A native mobile Hub companion now covers
projects, reviews, and repository refs, and native desktop hosts cover Windows,
macOS, and Linux. Separate agent workspaces connect local agent identity, lanes,
review, and integration without a Hub. Still ahead: complete production auth,
richer embedding, signed desktop distribution, and on-device Core embedding.

This page includes unreleased source changes. The latest coordinated release is
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
| **CLI (`sorrel-cli`)** | Persistent changes, lanes/stacks/merges, Git and Hub sync, isolated workspaces, agent registration/claims, tracking/recovery, grants/slices, workflows, secrets, environment and run logs. See [CLI usage](../sorrel-cli/README.md). |
| **Local agent workflow** | Independent agent directories/stores and assigned lanes; agent-attributed snapshots/changes; recorded commits/file review with optional snapshot-pinned integration, truthful readiness and pending work, and validated history integration using merge continue/abort. Owner-side advisory claims report overlaps. |
| **Safety and tracking** | Cross-process CLI/SDK locks, atomic object/metadata publication, replayable HEAD/workspace journals, explicit interrupted-checkout rollback, typed Core preflight, nested Git/Sorrel ignores, explicit tracking, and Unix change-time cache verification. |
| **Git bridge** | `sorrel git import`, `git export`, and colocated/external `git sync`; external active checkouts advance their files/index together and refuse local collisions; incremental fast-forwards in either direction, divergence parked on a normal Sorrel lane, `.sorrel/git-map.json` links SHAs ↔ snapshots. See `sorrel-cli/GIT.md`. |
| **Sync** | CLI ↔ Hub over HTTP sync transport; Hub FS-backed object/ref store; isolated demos can opt into `user:local` bootstrap grants with `SORREL_HUB_BOOTSTRAP_GRANTS=1`. |
| **Vault** | Secrets schema + local Node backend for tests. **Primary UX:** `sorrel secret *` resolves via upstream SecretSpec (`keyring` / `dotenv` / `env`) under Core grants; workflow jobs can inject authorized `secretRefs` with log redaction. |
| **Runners** | Local + container runners (ContainerRunner tested), `sorrel.workflow.yml` → `JobBundle` parser, Core policy gate + log redaction. CLI prefers devenv when present (`backend: devenv`), else `local-fallback`. Structured logs under `.sorrel/runs/`. |
| **Slices** | TS/JS slice manifest generator (prototype). |
| **Hub API** | JSON HTTP server: health, projects, admin collections with GET/PATCH, proposals/reviews lifecycle, lane-submit collaboration endpoint, sync transport, read-only synchronized tree/text-file browsing, FS persistence. |
| **Hub UI** | Shared Solid `sorrel-hub-ui` — repository-first project chrome with real tree/README browsing, proposal-backed Work board, review workbench, global Inbox, organization/profile README surfaces, and repository sync; hosted by thin `sorrel-hub-web`. |
| **Desktop app** | Tauri host for the shared Hub UI; native installers are built for Windows, macOS, and Linux on x64 and ARM64. The app connects to a local loopback Hub and wires scoped native notification/external-link adapters. |
| **Mobile app** | Native Expo/React Native Hub companion for iPhone, iPad, Android phones, and Android tablets; native stacks/tabs, secure bearer storage, project/review/comment lifecycle, and repository refs. |
| **Hub install seams** | `GET /capabilities` + `GET /session`; AuthAdapter (`dev` / `workos` / OIDC JWKS); shared `sorrel-hub/convex/` schema for SaaS + self-host. |
| **Server distribution** | Release automation for unprivileged, health-checked Linux amd64/arm64 Hub API and browser-host images, with GHCR publication, SBOM/provenance, immutable digests, checksums, and a release Compose file. |
| **Landing (`sorrel-web`)** | Static marketing site (Nord theme). Production deploy is Cloudflare Pages; local Docker is optional preview only. |
| **Root E2E / CI** | `npm test` E2E and `npm run test:modules` from one checkout. Root Actions checks out the monorepo directly — no submodule PAT. |

## Missing / not ready

| Area | Gap |
| --- | --- |
| **Checkout isolation and durability** | Workspaces copy reachable history; no shared object pool yet. Locks serialize Sorrel/SDK operations, not arbitrary filesystem writers. Process-interruption recovery is supported; power-loss atomicity across a checkout is not guaranteed. |
| **Git fidelity** | Normal/executable modes and full UTF-8 messages are supported; non-UTF-8 messages, symlinks and submodules fail explicitly. Tags, signatures, Git notes, staging, and exact author/committer metadata are not fully represented. |
| **Hub policy completeness** | Verified-session read/write guards, scoped discovery, restrictive grant precedence/lifecycle checks, rollback and typed closure checks are implemented. Native Core rule/signature hydration and multi-writer Hub persistence remain incomplete. |
| **Production auth** | AuthAdapter (`dev` / WorkOS / OIDC JWKS), `GET /session`, bind-safety; WorkOS sealed sessions + UI IdP login still ahead. |
| **Format migrations** | Protocol and object stores are `v0`; unknown versions fail closed, but no general workspace/Hub migration framework is shipped. |
| **Agents control plane** | Registrations/claims are concurrent records shared by Node and CLI. Claims are advisory owner-side metadata, not enforcement or a hosted agent scheduler. |
| **SDKs** | Rust SDK shares persistent workspace/lock contracts and creates detached snapshots; it requires caller-supplied ignore filtering. Stable complete embedding (C ABI / N-API / WASM / daemon) is not shipped. |
| **App embedding** | Desktop and mobile ship as thin Hub companions only; neither embeds Core or operates a local workspace until the stable embedding surface exists. Desktop remote-Hub selection, keychain use, and deep links are not yet shipped. |
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
| `sorrel-hub-ui` | Shared Solid Hub UI | Active (repository, work, review, Inbox, identity surfaces) |
| `sorrel-hub-desktop` | Native Tauri host for Hub UI | Active (local-Hub companion) |
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

1. Use the isolated two-agent workflow on real tasks and measure its friction.
2. Extend Git fidelity only from adoption/exit requirements; maintain existing CI.
3. Complete Core policy hydration and production login before untrusted hosting.
4. Add shared storage, performance work, apps or embedding only from measured need.
