# Sorrel architecture

Last updated: 2026-10-15

This document describes the alpha architecture and explicitly labeled current
source additions. Those additions require a source build; the published
`v0.1.0-alpha.2` artifacts do not include them.
Long-term product research lives in
[`AGENT_NATIVE_VERSION_CONTROL_REPORT.md`](../AGENT_NATIVE_VERSION_CONTROL_REPORT.md);
it is design background, not a statement that every described feature ships.

## System boundary

Current source adds a WorkOS browser session boundary around Hub: hosted AuthKit
login → browser-bound callback with PKCE → SDK-sealed HttpOnly cookie → verified
Core principal → existing Core policy checks. The browser host forwards cookies,
origins and redirects without establishing authority itself. Cookie writes
require the configured HTTPS origin; bearer auth remains available to CLI/mobile.
Login and Core authorization are separate: the invite-only pilot provisions
exact native grants as an operator action. No browser metadata creates grants.

Pending login transactions use bounded memory in one API instance. A restart
requires retrying an unfinished login; multiple instances need shared transaction
storage and coordinated metadata. Native WorkOS sessions survive a Hub restart
when the cookie password and IdP configuration remain the same.

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
  operations/       immutable hash-linked local ref-transition history (optional)
  checkpoints/      named recorded-snapshot references (optional)
  manifest.json     repository identity and format version
  HEAD              active lane and snapshot
  changes.index     snapshot-to-change lookup
  remotes.json      Hub remotes
  git-map.json      Git SHA ↔ Sorrel snapshot mapping, when used
  MERGE_STATE       only while resolving a conflicted merge
  write.lock        advisory command lock (released by the OS on process exit)
  metadata-transaction.json  only while publishing refs, changes or recovery metadata
```

Mutable files are written through unique temporary files and atomic replacement.
CLI workspace commands hold an advisory OS lock while reading and changing
metadata; a competing command fails with a retryable busy error. Workflow
processes run outside that lock so they can invoke Sorrel. HEAD, its lane head,
and a new change-index entry are staged with an immutable operation record in a
recovery journal; the next locked command validates and completes interrupted
metadata publication before reading state. Old repositories can omit operation
and checkpoint registries without a format migration. Checkpoints name recorded
snapshots; restore and undo refuse dirty work or pending merges, preflight the
restore and publication, and compare saved ref state before publishing. Undo
records a new transition and retains later history. This metadata recovery does
not roll back arbitrary filesystem I/O or capture unfinished/ignored files.
Unknown format versions fail closed.
There is no general migration framework yet, so alpha workspaces and Hub data
must be backed up before upgrading.

### Opt-in isolated lane directories (source builds)

`workspace enable` previews compatibility; `workspace enable --apply` performs
full metadata/reference preflight and a durable, digested backup of original
mutable metadata before atomically opting into `sorrel.workspace.shared.v1`.
Standalone repositories retain v0 without startup conversion. Older CLIs reject
the shared version and `sorrel.workspace.link.v1` descriptors before recovery or
mutation. Protocol schemas and immutable object formats remain v0.

`workspace add <inactive-lane-id> <new-path>` materializes independent native files
before registering the fixed-lane binding in the common manifest. Roots must not
overlap. The default directory owns its active lane too; every directory remains
pinned to its lane. Shared objects, lane/stack/slice registries, lane heads, change
index, grants, secret references and remotes have one common authority. `HEAD`,
merge state, stat cache, operations, checkpoints, run logs and previews stay local.
Core's additive registry-aware selection APIs protect shared provider paths while
retaining local configuration and root metadata exclusions. Incoming restores and
shared ref publication validate provider paths in the receiving directory.

All locked commands use the common `write.lock`. Shared publications use typed
common/workspace targets staged beside each destination. Replay validates the
registered canonical roots, exact link digest, pinned HEAD, lane ownership,
operation chain, exact ref payloads and current before/after states, change-index
pairs and full object closure before renaming anything. A missing or stale
recovery destination blocks publication; it is never silently recreated. Lane
publication guards cover normal recording, merge, recovery, Hub pull and Git
divergence parking. Default-directory Git behavior remains; linked-directory Git
commands are refused before side effects. There is no security sandbox, SDK
linkage, move/remove/ownership transfer or universal working-file rollback. See
[the opt-in and backup guide](../sorrel-cli/README.md#isolated-lane-directories-source-builds-only).

## Change, lane, and merge flow

`status`, `diff`, and `change create` use Core's `materialize_workspace_snapshot`,
shared with the Rust SDK. It excludes root `.sorrel/` and `.git/`, applies nested
`.gitignore` and `.sorrelignore` rules (Sorrel rules take precedence in the same
directory), and skips ignored untracked files before reading or storing their
contents. Existing ordinary tracked files remain tracked when a new ignore rule
matches them; removing a tracked file from disk still records a deletion.

Current source adds repeatable `change create --path <relative-path>` selectors
through Core's additive `materialize_workspace_snapshot_paths`. Literal files
and directory subtrees overlay HEAD while every unselected recorded entry stays
unchanged. Working files are untouched; unselected edits stay dirty for a later
full recording, and unvisited stat-cache entries are dropped. Explicit ignored,
protected, metadata, unknown, absolute/traversal and symlink selectors fail before
publication. Containing directories retain normal ignore/secret filtering;
file/directory replacements require selecting the replaced path. The normal
change objects, durable operation journal and active-lane publication still apply.
Omitting selectors keeps full-workspace behavior. Selective recording rejects a
pending merge and does not bypass recovery's dirty-work guard. This source-only
feature provides no hunk staging or separate lane checkout.

Files named `.env` or `.env.*` are always protected, except `.env.example`, which
follows ordinary ignore rules unless explicitly configured as a secret source.
Configured `dotenv:` / `dotenv://` file paths in `sorrel.secrets.yml`,
`.sorrel/secrets/*.json`, and `secretspec.toml`, plus Vault's declared local import
files, are also protected. Protection includes case variants and canonical
symlink targets, even when ignore rules try to reinclude them. Malformed provider
configuration fails closed. Explicit provider overrides outside these project
configs must be added to `.sorrelignore` before use. This selection runs before
stat-cache lookup, and excluded paths are removed from a successfully saved
cache. A size/mtime cache with Unix device/inode/ctime fingerprints avoids
rehashing unchanged selected files. Entries verified within their ctime second,
old entries without fingerprints, whole-second ctimes, and unsupported platforms
reread file contents. Cache reuse assumes the filesystem updates ctime on writes;
concurrent workspace writes are not an atomic snapshot.

A HEAD that already tracks a protected secret path causes an error before any
new objects are written. This does not purge previously stored secret blobs or
rewrite history. Back up the workspace, rotate exposed credentials, and rebuild
a clean workspace from source files without the protected paths before syncing
it. Core's low-level directory materialization APIs remain unfiltered for callers
that deliberately import arbitrary trees.
Each lane has an independent head, so parallel work can advance without sharing
one mutable branch pointer.

Merging first finds the best common ancestor. A fast-forward only moves the
active lane. A divergent merge compares base/ours/theirs and either writes a
two-parent snapshot or persists `Conflict` objects plus a `MergeResult`.
For a conflicted merge, Core can also produce a tentative working snapshot that
contains every clean change and retains unresolved file contents. The CLI
restores that snapshot, overlays conflict markers, and keeps the lane head
unchanged. It leaves marker-annotated working files and `MERGE_STATE` for
`merge --continue` or restores the pre-merge tree with `merge --abort`.
`MERGE_STATE` records the tentative snapshot as the tracked-file baseline, so
incoming clean files remain tracked even when the receiving lane ignores them.
Older merge records without that baseline fall back to HEAD.

## Git compatibility

The engine imports Git commits as Sorrel snapshots and exports Sorrel history as
Git commits. `.sorrel/git-map.json` makes subsequent operations incremental.
Colocated `sorrel git sync` fast-forwards whichever side moved. If both moved,
the Git tip is imported onto a normal `git/<branch>` lane; users resolve it
through the regular Sorrel merge flow before the next export.

## Hub and sync

Hub separates product metadata from VCS transport:

- Product metadata—projects, repositories, proposals, comments, workflow runs,
  and policy references—is stored as atomic JSON records. A record becomes
  visible in the running server only after its filesystem write succeeds.
- Privileged sync and repository/policy administration call the packaged Rust
  Core policy adapter over bounded asynchronous JSON subprocess requests. All
  operator-configured trusted grants and native policies participate; client
  references cannot omit denies. Unsupported constraints fail closed. These
  configured records are the trust boundary; authority-chain verification
  remains separate work.
- Sync objects and refs use a filesystem store with digest verification,
  missing-object negotiation, closure checks, and fast-forward/expected-head
  enforcement.
- `/capabilities` describes installed modules, auth mode, deployment shape, and
  optional Convex availability, including the server-owned proposal transitions.
  `/session` exposes the resolved Hub session. Non-development mutations require
  that verified session; creator, author, and requester attribution uses its
  principal. All private reads and writes require native Core authorization on
  actual resource scopes. Lists filter inaccessible records. Development acting
  headers and anonymous `user:local` sessions require explicit local demo mode.
- The shared SolidJS UI calls Hub through a host-injected transport: the
  browser host's `/api` proxy or the desktop shell's scoped Tauri HTTP client.
- The desktop shell currently permits only loopback Hub URLs and does not claim
  local Core, keychain, or deep-link capabilities. Those require stable
  embedding and production-auth contracts.
- The native mobile companion calls Hub directly through `sorrel-sdk-js`;
  bearer credentials stay in the platform keychain/keystore.
- Optional Convex state mirrors proposal metadata only through internal functions
  and a server-held admin key. Product counters use the authenticated Hub API;
  browsers have no direct Convex subscription. VCS objects and refs do not move
  into Convex.

Projects can explicitly link an already synchronized repository. Reviews record
source and target snapshot IDs; comparison reads those immutable trees without
advancing refs or performing a merge. CLI submission uploads both closures when
pushing, including diverged target history. Preview limits bound tree traversal,
changed files, and text bytes; binary and large files show unavailable previews.
The shared UI distinguishes closed reviews from integrated reviews and labels
the metadata transition “Mark as merged.” Native dialogs and pending/error states
keep mutations keyboard-accessible and retryable.

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

Hub development auth accepts an acting-principal header only on loopback or
under an explicit insecure-demo override. OIDC bearer verification and WorkOS
adapter seams exist in the released alpha. Current source adds the WorkOS
browser flow described above; live provider and deployment validation remain
launch gates.

Secret values are not Sorrel objects. Protocol, CLI, Vault, Runners, Hub, and UI
carry `SecretRef` handles and policy/audit metadata. The CLI integrates upstream
SecretSpec providers (`keyring`, `dotenv`, and environment), checks Core
`secret.read`/`secret.inject` grants before resolution or injection, and redacts
persisted output. The standalone Vault remains local-only and the runner library
does not resolve values by itself.

## Workflows, slices, agents, and SDKs

Runners expose serializable `JobBundle` objects, a versioned workflow parser,
Core-shaped permission gates, local process execution, and an experimental
Docker/Podman adapter. The CLI delegates parsing/execution to this library and
uses a native Core policy adapter; its CLI-shaped JSON and policy-command
compatibility surface remain intact. Both named `workflows` and legacy `jobs`
YAML are accepted. Selected jobs include their dependency closure and stop on
failure; literal environment values and authorized secret aliases are applied.

The CLI resolves SecretSpec values out of band and gives them to the authorized
local runner without adding values to `JobBundle`. Results and all log records
redact secrets. Simple jobs can use detected devenv; jobs requiring environment
injection or dependency execution use the shared local runner. Results persist
under `.sorrel/runs/`; failures to persist are reported. Workflow commands print
structured JSON before returning nonzero status for failure/denial/invalid or
missing input. Human output includes redacted streams and a saved-log hint;
follow/Hub streaming is not implemented.

Slices compute deterministic TS/JS dependency closures for focused context.
The agent package persists registrations and leased advisory path claims. New
claims default to 15 minutes; heartbeat renews active claims and release is
owner-specific and idempotent. Expired claims leave the active view and
cross-agent component-boundary overlap warnings, while registrations and stored
records remain. Legacy records without expiry remain indefinite until explicit
action. Writers lock and reload before atomic replacement; readers refresh from
disk. Owner IDs are caller-supplied and matching is lexical and case-sensitive;
there is no authentication, cleanup scheduler, symlink resolution or enforcement.
Core remains authoritative, including for claims marked `blocking`.
The JavaScript and Rust SDKs
are intentionally thin until a stable embedding surface is designed.

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
- [`sorrel-protocol/docs/validation.md`](../sorrel-protocol/docs/validation.md) — wire/object contract validation
- [`../sorrel-core/README.md`](../sorrel-core/README.md) — engine API concepts
- [`../sorrel-cli/README.md`](../sorrel-cli/README.md) — CLI and disk layout
- [`../sorrel-hub/README.md`](../sorrel-hub/README.md) — Hub API/configuration
