<!-- Generated from docs/ARCHITECTURE.md by npm run sync:docs. Do not edit. -->

# Sorrel architecture

Last updated: 2026-10-02

This document describes the current source tree. The latest published release
is `v0.1.0-alpha.2`; unreleased source changes are described here too.
Long-term product research lives in
[`AGENT_NATIVE_VERSION_CONTROL_REPORT.md`](https://github.com/MGRAFF2006/sorrel/blob/main/AGENT_NATIVE_VERSION_CONTROL_REPORT.md);
it is design background, not a statement that every described feature ships.

## System boundary

Sorrel is a monorepo containing an independent version-control engine, a local
CLI, shared protocol contracts, optional collaboration services, workflow and
secret prototypes, and small SDK/control-plane packages.

```text
human / agent / SDK
        |
        v
  sorrel CLI or Hub client (web / desktop / mobile)
        |
        +----------------------+
        |                      |
        v                      v
  sorrel-core              sorrel-hub API
  objects + policy         metadata + sync transport
        |                      |
        +----------+-----------+
                   |
                   v
        protocol-shaped objects and refs

Git <---- incremental import / export / colocated sync ----> Sorrel workspace
```

Core owns version-control and authorization semantics. Hub administers and
transports them but does not replace them. UI packages display state but hold
no authoritative policy or VCS data.

## Package map

| Package | Responsibility | Runtime |
| --- | --- | --- |
| `sorrel-protocol` | JSON schemas, examples, compatibility docs, canonical policy conformance | Node validation tooling |
| `sorrel-core` | Object store, snapshots, changes, history, merge, lanes/stacks, policy/authority, sync closure, Git bridge | Rust library |
| `sorrel-cli` | Persistent `.sorrel/` workspace and user/agent command surface | Rust binary/library |
| `sorrel-hub` | JSON API, product metadata, sync store, auth adapters, capabilities | Node HTTP server |
| `sorrel-hub-ui` | Shared SolidJS product UI and platform seams | Browser/Tauri-ready library |
| `sorrel-hub-desktop` | Native host, scoped Hub transport, notifications, external links | Tauri on Windows/macOS/Linux |
| `sorrel-hub-web` | Browser mount, Vite build, static server, `/api` proxy | Browser + Node host |
| `sorrel-hub-mobile` | Native projects/reviews/refs companion, secure connection profile, adaptive navigation | React Native / iOS / Android |
| `sorrel-runners` | Workflow parsing and local/container execution | Rust library |
| `sorrel-vault` | Secret declaration schema and local reference/redaction tooling | Node tooling |
| `sorrel-slices` | Deterministic TS/JS dependency-closure manifests | Node CLI/library |
| `sorrel-agents` | Persistent advisory agent registrations and path claims | Node library |
| `sorrel-sdk-js` | Small Hub HTTP client | Node/browser library |
| `sorrel-sdk-rust` | Small workspace wrapper over Core | Rust library |
| `sorrel-web` | Static public website and published docs | Static files |

## Object and workspace model

Core stores immutable bytes behind `ObjectStore`. An object id is the BLAKE3
digest of its exact stored bytes; reads verify the digest. Files become `Blob`
objects, directories become sorted `Tree` objects, and a `Snapshot` points to a
root tree and parent snapshots. A `Change` describes movement between snapshots
and the paths touched.

A CLI workspace adds mutable references around those immutable objects:

```text
.sorrel/
  objects/          content-addressed object store
  lanes/            lane metadata
  heads/            per-lane snapshot heads
  stacks/           stack metadata
  grants/           local grant records
  secrets/          secret-reference handles, never values
  runs/             structured workflow results and redacted logs
  slices/           slice manifests
  manifest.json     repository identity and format version
  HEAD              active lane and snapshot
  changes.index     snapshot-to-change lookup
  remotes.json      Hub remotes
  git-map.json      Git SHA ↔ Sorrel snapshot mapping, when used
  agents/           shared advisory agent records and claims
  workspaces/       owner links to external agent checkouts
  tracked.json      explicit ignore exceptions
  LOCK              OS advisory lock
  HEAD_TRANSACTION / WORKSPACE_CREATE / CHECKOUT_STATE  operation journals
  MERGE_STATE       only while resolving a conflicted merge
```

Metadata writes use unique temporary files, fsync, and atomic replacement. CLI
repository operations and SDK access share an OS advisory lock. HEAD/lane writes
replay `HEAD_TRANSACTION` after interruption; Change metadata is published before
HEAD. Checkout saves `CHECKOUT_STATE` before modifying files and requires
`recover --abort` after failure. Workspace creation similarly replays
`WORKSPACE_CREATE`. These journals recover process interruption; a complete
checkout is not power-loss atomic or isolated from arbitrary external edits.
Unknown object/workspace versions fail closed; there is no general workspace or
Hub migration framework. Back up alpha data before upgrading.

## Change, lane, and merge flow

`status`, `diff`, and `change create` share tracked-file selection, including
nested Git/Sorrel ignore rules and explicit `track add` exceptions. Metadata,
local dotenv secrets, and common build/dependency output are excluded by default;
existing tracked files remain tracked. The cache combines size/mtime with Unix
device/inode/change time; unsupported platforms and legacy entries rehash.

One directory still has one active HEAD. `workspace create` gives each agent an
independent directory/store and assigned lane, carrying validated snapshot and
Change history plus agent/task/base metadata. The owner serializes integration
with the worker lock held, checks identity, ancestry, history and cleanliness,
then merges the imported tip into its active lane. Worker files remain untouched.
Path claims are advisory owner-side records, separate from filesystem isolation.

Merging first finds the best common ancestor. A fast-forward only moves the
active lane. A divergent merge compares base/ours/theirs and either writes a
two-parent snapshot or persists `Conflict` objects plus a `MergeResult`.
The engine also exposes the complete provisional merge tree while conflicts
remain. The CLI restores its clean changes before overlaying markers, and leaves
`MERGE_STATE` for
`merge --continue` or restores the pre-merge tree with `merge --abort`.
File/directory collisions fail explicitly rather than dropping a side. Core restoration and CLI checkout validate typed graphs, tree paths, blob
metadata, and metadata/symlink boundaries before modifying files. Atomic file
replacement avoids overwriting external hardlinks. Host serialization remains
necessary for simultaneous filesystem changes.

## Git compatibility

The engine imports Git commits as Sorrel snapshots and exports Sorrel history as
Git commits. `.sorrel/git-map.json` makes subsequent operations incremental.
Colocated `sorrel git sync` fast-forwards whichever side moved. If both moved,
the Git tip is imported onto a normal `git/<branch>` lane; users resolve it
through the regular Sorrel merge flow before the next export.

## Hub and sync

Hub separates product metadata from VCS transport:

- Product metadata—projects, repositories, proposals, comments, workflow runs,
  and policy references—is stored as atomic JSON records.
- Sync objects and refs use a filesystem store with digest verification,
  missing-object negotiation, closure checks, and fast-forward/expected-head
  enforcement.
- CLI pull downloads and validates a complete snapshot closure without changing
  HEAD. The command rejects dirty worktrees, in-progress merges, and replacement
  of recorded history; fresh empty workspaces may adopt unrelated history.
  It rechecks local state after download, restores files, then advances HEAD.
- `/capabilities` describes installed modules, auth mode, deployment shape, and
  optional Convex availability. `/session` exposes the resolved Hub session.
- The shared SolidJS UI calls Hub through a host-injected transport: the
  browser host's `/api` proxy or the desktop shell's scoped Tauri HTTP client.
- The desktop shell currently permits only loopback Hub URLs and does not claim
  local Core, keychain, or deep-link capabilities. Those require stable
  embedding and production-auth contracts.
- The native mobile companion calls Hub directly through `sorrel-sdk-js`;
  bearer credentials stay in the platform keychain/keystore.
- Optional Convex state mirrors proposal metadata only. VCS objects and refs do
  not move into Convex.

The development stack is intentionally modular: Hub API, shared web/desktop UI,
desktop host, browser host, native mobile companion, and public website are
distinct packages.

The mobile app uses platform-native stack and tab controllers, including an
adaptive iPad sidebar, while rendering product content with React Native. It
supports projects, reviews/comments, proposal lifecycle updates, and connected
repository refs. It does not embed `sorrel-core`, clone workspaces, or make
authorization decisions; those require the future stable embedding surface.

## Identity, policy, and secrets

Protocol/Core define principals, capabilities, resources, grants, policies,
decisions, authority roots, and signed policy changes. A policy change is
evaluated against the previous effective authority, so it cannot authorize
itself. The canonical conformance manifest is vendored into Core, CLI, Hub,
Runners, and Vault; checksum and behavior tests keep implementations aligned.

Hub development auth accepts an acting-principal header only with an explicit
development adapter on loopback or under an insecure-demo override. OIDC/WorkOS
guarded operations require a verified session and never fall back to that header.
Verified sessions gate guarded metadata mutations and private sync reads;
discovery is filtered by trusted grants. Hub enforces matching deny precedence,
lifecycle and expiry checks, and fails closed on effects/conditions it cannot
execute. Ref publication validates a typed, bounded snapshot closure; persistence
failures roll memory state back before readers can observe it. This is still a
single-writer Hub with a limited policy evaluator: native Core rule/signature
hydration, sealed WorkOS sessions, and a browser IdP flow remain ahead. Internal
Convex mirror functions require service credentials; the UI polls Hub unless a
public counter capability is explicitly advertised.

Secret values are not Sorrel objects. Protocol, CLI, Vault, Runners, Hub, and UI
carry `SecretRef` handles and policy/audit metadata. The CLI integrates upstream
SecretSpec providers (`keyring`, `dotenv`, and environment), checks Core
`secret.read`/`secret.inject` grants before resolution or injection, and redacts
persisted output. The standalone Vault remains local-only and the runner library
does not resolve values by itself.

## Workflows, slices, agents, and SDKs

Runners expose serializable `JobBundle` objects, a versioned workflow parser,
Core-shaped permission gates, local process execution, and an experimental
Docker/Podman adapter. CLI workflow execution prefers devenv when detected,
falls back to the local process runner, and records structured results under
`.sorrel/runs/`. Execution logs redact resolved secret values and secret-like
environment data; follow/Hub streaming is not implemented.

Slices compute deterministic TS/JS dependency closures for focused context.
The agent package persists advisory agent/lane registrations and path claims;
it is coordination, not an enforcement boundary. The Rust SDK shares the CLI workspace/lock/manifest contract and creates detached
filtered snapshots without moving refs. JavaScript remains a thin Hub client;
neither SDK claims a stable complete embedding surface.

## Compatibility and trust rules

- Object ids are content integrity, not identity or authorization.
- Core/protocol policy is authoritative; Hub and UI must not invent permission
  rules.
- Secret values must not enter objects, logs, diffs, proposals, or committed
  configuration.
- CLI `--json`, protocol schemas, persisted layouts, and Rust public APIs are
  prerelease contracts; coordinate changes and document them in changelogs.
- VCS objects stay in the object store; product metadata may use filesystem or
  optional Convex storage.
- Git remains the import/export escape hatch and interoperability boundary.

## Where to read next

- [`DEVELOPMENT.md`](DEVELOPMENT.md) — build, test, debug, and change routing
- [`GETTING_STARTED.md`](GETTING_STARTED.md) — first local run
- [`STATUS.md`](STATUS.md) — exact shipped/missing snapshot
- [`RELEASE.md`](RELEASE.md) — version and release process
- [`sorrel-protocol/docs/validation.md`](https://github.com/MGRAFF2006/sorrel/blob/main/sorrel-protocol/docs/validation.md) — wire/object contract validation
- [`../sorrel-core/README.md`](https://github.com/MGRAFF2006/sorrel/blob/main/sorrel-core/README.md) — engine API concepts
- [`../sorrel-cli/README.md`](https://github.com/MGRAFF2006/sorrel/blob/main/sorrel-cli/README.md) — CLI and disk layout
- [`../sorrel-hub/README.md`](https://github.com/MGRAFF2006/sorrel/blob/main/sorrel-hub/README.md) — Hub API/configuration
