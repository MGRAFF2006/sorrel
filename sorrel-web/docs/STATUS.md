<!-- Generated from docs/STATUS.md by npm run sync:docs. Do not edit. -->

# Sorrel status

Last updated: 2026-10-15

What works today, what does not, and where to look next. For how to run the
stack, see [GETTING_STARTED.md](GETTING_STARTED.md). The forward plan is
[`ROADMAP.md`](https://github.com/MGRAFF2006/sorrel/blob/main/ROADMAP.md).

## Snapshot

### Current source versus the published alpha

The current source adds WorkOS browser sign-in, SDK-sealed cookies, refresh,
logout, authenticated UI gating, and an invite-only HTTPS deployment/provisioning
path. It also protects first Git import and Hub pull from overwriting unfinished
work, rejects non-fast-forward pulls, and checks restore paths and objects before
deletions. **These changes are not in the published alpha artifacts.** The
invite-only staging deployment is running; end-to-end live sessions remain
unverified. This is not a general production-readiness claim.

The source-built CLI now includes a durable operation log, named checkpoints of
recorded snapshots, and guarded undo of local active-HEAD transitions. Checkpoint
creation, restore and undo refuse dirty working trees and pending merges. Restore
uses the current lane; undo appends history and preserves unrelated lane heads.
These commands do not save unfinished work, ignored/untracked files or secrets,
and are not in the published alpha. See
[the CLI recovery guide](https://github.com/MGRAFF2006/sorrel/blob/main/sorrel-cli/README.md#local-operations-checkpoints-and-undo).

The source-built CLI also adds repeatable literal `change create --path` selectors
for exact files and directory subtrees. Selected additions, modifications,
deletions and modes overlay HEAD without changing working files; unrelated edits
remain dirty and can be recorded later. Existing ignore/provider protections and
operation history still apply. This is absent from published alpha artifacts;
hunk staging remains future work. See
[selective recording](https://github.com/MGRAFF2006/sorrel/blob/main/sorrel-cli/README.md#workspace-file-selection).

The source-built CLI adds opt-in fixed-lane isolated directories through
`workspace enable`, explicit `workspace enable --apply`, `workspace add` and
`workspace list`. Preflight and a durable original-metadata backup precede the
local format gate; older CLIs refuse enabled repositories. Native checkouts share
objects, authoritative lane heads, policy registries and one command lock while
keeping HEAD, merges, caches, operations and checkpoints local. Linked Git
commands, moves/removal, ownership transfer and SDK linkage are unsupported.
This is absent from published alpha binaries and is not a security sandbox or
working-file rollback. See [the opt-in and backup guide](https://github.com/MGRAFF2006/sorrel/blob/main/sorrel-cli/README.md#isolated-lane-directories-source-builds-only).

The source agent library adds owner-specific release, heartbeat of active claims,
and 15-minute default leases. Expired claims leave the active view and overlap
warnings; registrations and stored records remain. Legacy claims without expiry
stay active indefinitely until explicitly released, reclaimed or renewed. This
is local advisory coordination with caller-supplied owner IDs and lexical,
case-sensitive paths, without authentication, enforcement, cleanup or Hub APIs.
See [claim lifecycle and limitations](https://github.com/MGRAFF2006/sorrel/blob/main/sorrel-agents/README.md#claim-lifecycle).

The operator selected `sorrel.humunkulud.com` on `homelab-services` through the
existing Cloudflare Tunnel. Tested source-built containers are healthy, DNS and
ingress are active, and no additional public ports are open. The WorkOS account,
Sorrel Hub staging application, exact callback/sign-out/login URLs, passwordless
email login and disabled public signup are configured. The operator's pilot user
has exact Core workspace grants; signing into the application still needs live
email verification. Private credentials are outside Git.

The origin displays the Hub sign-in gate. Public access is client-dependent:
laptop curl reaches health/session endpoints, receives 401 for anonymous projects,
and receives the WorkOS login redirect, while normal Python requests and the
development browser receive Cloudflare 403 responses. A scoped Cloudflare API
configuration rule still needs owner access before external invitations; the
existing tunnel credential cannot change those settings.

A daily encrypted Restic backup runs at 00:30 Europe/Berlin using the existing
homelab repository and shared backup lock. The first snapshot (`73ee29b9`) was
restored separately with all 627 files verified; metadata, grants and private
configuration matched, and an isolated restored Hub passed health and anonymous
access checks. Live session/refresh/revocation, two-team isolation, public CLI
upload and independent off-host recovery checks remain pending.
The updated pilot's scheduled 2026-10-15 backup completed successfully at 00:30
Berlin time using discovery of the running Hub's actual data mount; both
containers resumed healthy. This is backup evidence, not an off-host restore.

The 2026-10-14 audit reproduced first-import data loss, dirty/diverged pulls,
ignored-file and symlink restore collisions, and broken browser cookie/redirect
transport. Regression coverage is in
[`git_import.rs`](https://github.com/MGRAFF2006/sorrel/blob/main/sorrel-cli/tests/git_import.rs),
[`restore_safety.rs`](https://github.com/MGRAFF2006/sorrel/blob/main/sorrel-cli/tests/restore_safety.rs),
[`sync.rs`](https://github.com/MGRAFF2006/sorrel/blob/main/sorrel-cli/tests/sync.rs), and
[`workos-browser.test.js`](https://github.com/MGRAFF2006/sorrel/blob/main/sorrel-hub/test/workos-browser.test.js).

Important remaining product gaps:

- Lanes keep independent heads but share a checkout; separate directories,
  hunk staging and authenticated remote agent coordination remain future work. Local leases do not isolate execution or authorize writes.
- Hub reviews compare recorded snapshots. “Mark as merged” changes metadata;
  real Core integration and checks/approvals tied to exact snapshots are pending.
- Hosted access is operator-provisioned; self-service team access, managed CLI
  login, quotas, pagination, safe format migrations, and general disaster-recovery
  validation are pending. Core grants remain authoritative after login.
- Restore preflight handles known invalid paths, missing blobs, inconsistent
  trees, ignored collisions, symlinks and Unix hardlinks. It does not provide
  full filesystem rollback on arbitrary I/O failure, process death, or concurrent
  external file edits. Back up important work and use separate checkouts.
- Git import protects conventional provider paths and current provider
  configuration, not arbitrary leaked credentials or every historical provider
  configuration. Audit existing history and rotate any exposed credentials.
- The verified alpha release has five CLI platform archives, no Windows ARM64
  archive and no desktop bundles, despite the intended next-release matrix.
  Its Linux x64 archive checksum and binary startup were verified. Other native
  platform behavior was not tested in this audit.
- The repository is already public. GitHub's homepage field is empty; the live
  landing URL and its current deployment were not verified in this audit.

The forward plan in [`ROADMAP.md`](https://github.com/MGRAFF2006/sorrel/blob/main/ROADMAP.md) incorporates the hosted launch
choice and the recovery/isolation/review feedback. Architecture scope alone does
not establish market demand; validate a complete workflow with design partners.

### Published alpha baseline

Sorrel already has a **working local VCS loop** (init → change → lanes → merge →
push/pull) on a real content-addressed engine, plus **Git import/export and
colocated bidirectional sync**, a deployable Hub API, and a writable Hub UI
companion. The public landing site is live. A root **no-mock E2E** (`npm test`)
wires every active module together. A native mobile Hub companion now covers
projects, reviews, and repository refs, and native desktop hosts cover Windows,
macOS, and Linux. Still ahead: production auth, richer agents/SDKs, signed
desktop distribution, and on-device Core embedding.

The latest coordinated release is
**[`v0.1.0-alpha.2`](https://github.com/MGRAFF2006/sorrel/releases/tag/v0.1.0-alpha.2)**,
an installable developer preview with downloadable CLI artifacts and hostable
server images. The Hub is not safe for untrusted network exposure without a
production AuthAdapter and network controls. See the root
[`CHANGELOG.md`](https://github.com/MGRAFF2006/sorrel/blob/main/CHANGELOG.md) for the complete shipped record.

## Working

| Area | What you can do |
| --- | --- |
| **Protocol** | Canonical object schemas, examples, sync-transport spec, policy conformance manifest + checksum drift guards. |
| **Engine (`sorrel-core`)** | Content-addressed object store, snapshots, changes, path/line-level diff helpers, lanes/stacks, policy/authority spine, sync closure helpers, stat-cache, three-way merge + protocol-aligned conflict/merge-result objects, **incremental `git_import` / `git_export`**. |
| **CLI (`sorrel-cli`)** | Persistent `.sorrel/` workspace: `init`, `status`, `change create`/`list`, `diff`, `log`, `lane create`/`list`/`switch`/`submit`, `stack create`/`list`/`show`, `merge` / `merge --abort` / **`merge --continue`**, **`git import` / `git export` / `git sync`**, `grant`, `slice create`, `workflow validate`/`run`, **`secret list\|sync\|check\|get\|set\|run`**, **`env init\|ensure\|info\|shell`**, **`run list\|show\|logs`**, `remote add`/`list`, `push`, `pull`. |
| **Git bridge** | `sorrel git import`, `git export`, and colocated `git sync`; incremental fast-forwards in either direction, divergence parked on a normal Sorrel lane, `.sorrel/git-map.json` links SHAs ↔ snapshots. See `sorrel-cli/GIT.md`. |
| **Sync** | CLI ↔ Hub over HTTP sync transport; Hub FS-backed object/ref store; isolated demos can opt into `user:local` bootstrap grants with `SORREL_HUB_LOCAL_DEMO=1` and `SORREL_HUB_BOOTSTRAP_GRANTS=1`. |
| **Vault** | Secrets schema + local Node backend for tests. **Primary UX:** `sorrel secret *` resolves via upstream SecretSpec (`keyring` / `dotenv` / `env`) under Core grants; workflow jobs can inject authorized `secretRefs` with log redaction. |
| **Runners** | Shared CLI/library workflow parser and local executor, named/legacy YAML, job dependencies and literal env, native Core workflow policy, host-resolved secret injection/redaction. Simple CLI jobs prefer devenv; dependency/env jobs use `local-fallback`. ContainerRunner remains experimental. Structured logs under `.sorrel/runs/`; workflow failures return nonzero status. |
| **Slices** | TS/JS slice manifest generator (prototype). |
| **Hub API** | JSON HTTP server: health, projects/repository linking, admin collections with GET/PATCH, server-owned proposal transitions, lane-submit collaboration, sync transport, bounded tree/text browsing and recorded snapshot comparison, FS persistence; private reads/writes require verified sessions and Core grants; explicit local demo remains available. |
| **Hub UI** | Shared Solid `sorrel-hub-ui` — guided organization/repository connection, file previews and recorded before/after review comparisons, proposal-backed Work board, global Inbox, organization/profile surfaces; keyboard-accessible dialogs with pending/error/retry states. Hosted by thin `sorrel-hub-web`. |
| **Desktop app** | Tauri host for the shared Hub UI; native installers are built for Windows, macOS, and Linux on x64 and ARM64. The app connects to a local loopback Hub and wires scoped native notification/external-link adapters. |
| **Mobile app** | Native Expo/React Native Hub companion for iPhone, iPad, Android phones, and Android tablets; native stacks/tabs, secure bearer storage, project/review/comment lifecycle, and repository refs. |
| **Hub install seams** | `GET /capabilities` + `GET /session`; AuthAdapter (`dev` / `workos` / OIDC JWKS); server-only internal `sorrel-hub/convex/` metadata mirror for SaaS + self-host; scoped proposal counters poll Hub. |
| **Server distribution** | Release automation for unprivileged, health-checked Linux amd64/arm64 Hub API and browser-host images, with GHCR publication, SBOM/provenance, immutable digests, checksums, and a release Compose file. |
| **Landing (`sorrel-web`)** | Static marketing site (Nord theme). Production deploy is Cloudflare Pages; local Docker is optional preview only. |
| **Root E2E / CI** | `npm test` E2E and `npm run test:modules` from one checkout. Root Actions checks out the monorepo directly — no submodule PAT. |

Privileged Hub sync and repository/policy administration now use the packaged
Rust Core policy adapter rather than a reduced JavaScript evaluator. Operator
trusted grant/policy records supply the effective policy; grant lifecycle and
Core deny/redact/review precedence are enforced. Unsupported conditions and
unresolved protocol object references fail closed. Private metadata, collaboration
and sync reads/writes require exact Core scope decisions; list results and summary
counts exclude inaccessible records. Local demos require explicit opt-in and
actual native grants. Authority-chain signatures remain unverified.

## Missing / not ready

| Area | Gap |
| --- | --- |
| **Production auth** | AuthAdapter (`dev` / WorkOS / OIDC JWKS), `GET /session`, bind-safety; WorkOS sealed sessions + UI IdP login still ahead. |
| **Format migrations** | Protocol and object stores are `v0`; unknown versions fail closed, but no general workspace/Hub migration framework is shipped. |
| **Agents control plane** | Minimal register/claim/active-work surface shipped; no instruction overlays yet. |
| **SDKs** | Minimal Hub JS client + Rust `Workspace` wrapper shipped; embedding surface (C ABI / N-API / WASM / daemon) not shipped. |
| **App embedding** | Desktop and mobile ship as thin Hub companions only; neither embeds Core or operates a local workspace until the stable embedding surface exists. Desktop remote-Hub selection, keychain use, and deep links are not yet shipped. |
| **Hub secret backend** | Optional hosted / BYO provider binding (Phase 4) not shipped; local keyring/dotenv remain default. |
| **devenv task mapping** | Prefer devenv when present; full `sorrel.workflow.yml` → devenv tasks shim and remote runners are still thin. |
| **Run log follow / Hub stream** | Local `.sorrel/runs/` + `run show\|logs` shipped; unsupported `--follow` requests now fail explicitly, and Hub streaming is not implemented. |

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

1. Validate real WorkOS identity and a credentialed hosted pilot, including
   managed CLI onboarding, two-team access boundaries and backup restoration.
2. Extend recovery to interrupted working-tree writes, directory lifecycle,
   hunk staging and authenticated remote agent coordination.
3. Implement real proposal integration with snapshot-bound checks and approvals.
4. Add a narrow MCP adapter, handoff bundles, useful stacks and materialized slices.
5. Add migrations, quotas, self-service access, workflow streaming/cancellation
   and native remote-Hub integration as pilot use establishes priorities.
