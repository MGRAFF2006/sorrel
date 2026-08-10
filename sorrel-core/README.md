# sorrel-core

The Rust engine at the heart of Sorrel, the agent-native version-control
system. This crate owns the content-addressed object model and the headless
policy/permission/authority semantics; the CLI, Hub, runners, and vault all
consume it (the CLI via a workspace path dependency).

`0.1.0-alpha.1` is a pre-stable release. Read the
[v0 compatibility policy](docs/COMPATIBILITY.md) before embedding the crate or
upgrading an object store, and see the [changelog](CHANGELOG.md) for current
capabilities and limitations.

## Module map

| Module        | Contents |
| ------------- | -------- |
| `store`       | `ObjectStore` trait, `FileObjectStore` (fanout dirs, atomic writes, digest-verified reads), `InMemoryObjectStore` |
| `object`      | `ObjectId` (BLAKE3), hex parsing |
| `snapshot`    | `Blob`/`Tree`/`Snapshot` objects, directory materialization (with root-name exclusion and stat-cache), read/restore |
| `change`      | `Change` objects, path-level `snapshot_diff`, `apply_change` |
| `history`     | snapshot-DAG operations: ancestry sets, merge bases (`git merge-base --all` equivalent) |
| `merge`       | first three-way snapshot merge: entry-level merge against the best common ancestor with first-class conflicts |
| `stat_cache`  | size+mtime cache that skips re-hashing unchanged files |
| `transport`   | sync push/pull helpers: object closure, missing-object negotiation, content-verified batch transfer, ancestry check |
| `lane_stack`  | `Lane`/`Stack` metadata objects for agent-native work coordination |
| `policy`      | principals, capabilities, grants, `Policy`, deterministic `evaluate_policy` |
| `authority`   | signed `PolicyChange` evaluation against the previous effective policy |
| `permissions` | lightweight grant/visibility/audit metadata used by lanes and stacks |

## Object model

Sorrel stores immutable objects behind the `ObjectStore` trait. Object IDs are
BLAKE3 digests of the exact bytes written, so identical bytes are deterministic
and deduplicated, and reads verify content against the requested ID.

Four object types form the VCS core:

- `Blob`: raw file content in a small canonical envelope; `content_hash`
  records the BLAKE3 hash of the raw bytes.
- `Tree`: deterministic JSON with sorted entries for one directory, pointing at
  `Blob` objects for files and nested `Tree` objects for directories.
- `Snapshot`: names a repository, root tree, parent snapshots, message,
  timestamp, and author.
- `Change`: movement from a `base_snapshot` to a `resulting_snapshot`, with
  author, parent changes, touched paths, and a path-level diff.

Stored JSON follows the `sorrel-protocol` schemas where useful but never embeds
its own `id`; identity is always the content-addressed `ObjectId` returned by
the store, which keeps IDs deterministic and avoids self-referential hashes
while the protocol is in `v0`.

## Snapshots

`materialize_snapshot` walks a directory and stores its contents: files become
`Blob`s, directories become nested `Tree`s, entries are sorted for
deterministic IDs, and unsupported filesystem objects (e.g. symlinks) error.

```rust
use sorrel_core::{materialize_snapshot, InMemoryObjectStore, SnapshotOptions};

let store = InMemoryObjectStore::new();
let snapshot = materialize_snapshot(&store, "/path/to/workspace", SnapshotOptions::new("repo_example"))?;
```

Two variants matter in practice:

- `materialize_snapshot_excluding` skips top-level names such as `.sorrel`, so
  a workspace can snapshot itself without recursing into its own object store.
- `materialize_snapshot_excluding_with_stat_cache` additionally takes a
  `StatCache` (size + mtime keyed by path) and skips re-hashing files whose
  stats are unchanged — this is what the CLI uses for `status`/`change create`.

`SnapshotOptions::new` uses a deterministic timestamp and system author so
identical content yields identical snapshot IDs; set `created_at`/`author`
explicitly for wall-clock attribution. Read back with `read_snapshot_files`
(into memory) or `restore_snapshot_to_directory` (overwrites files present in
the snapshot, leaves other files untouched).

## Changes and diff

`create_change` diffs two already-stored snapshots and writes the change
object; `snapshot_diff` is the underlying path-level diff (added / modified /
deleted, compared by tree-entry fingerprints). `apply_change` validates that
the caller sits exactly on the change's `base_snapshot` and returns the stored
`resulting_snapshot`; it does not patch or merge content.

```rust
use sorrel_core::{create_change, ChangeOptions, Principal};

let change = create_change(&store, base.id, result.id,
    ChangeOptions::new(Principal::system(), "update readme"))?;
```

The diff is intentionally path-level: no textual hunks, rename detection, or
merge logic. Line-level diff lives in the CLI.

## History and merge

`history` walks the snapshot DAG: `collect_ancestors` returns everything
reachable through parent links, and `merge_bases`/`merge_base` find the best
common ancestor(s) of two snapshots (criss-cross histories can return several;
`merge_base` picks deterministically).

`merge_snapshots` performs a path-level three-way merge of `ours` and `theirs`
against an explicit `base`, using `snapshot_diff` plus content-level `merge3`
for diverging file edits:

- a path changed on only one side takes that side's version;
- identical changes on both sides collapse;
- both-modified text merges cleanly when regions do not overlap; otherwise a
  `Conflict` (`conflictType` `content` or `binary`) is stored and OURS is kept
  in the candidate tree;
- add/add with different content uses `merge3` against an empty base
  (`conflictType` `add_add` on conflict);
- modify/delete stores `conflictType` `modify_delete` and keeps the modified
  version;
- clean merges write a snapshot with parents `[ours, theirs]` and a `MergeResult`
  with status `clean`; any conflicts omit the merged snapshot and return a
  conflicted `MergeResult` listing stored conflict ids.

Stored `Conflict` / `MergeResult` JSON follows the protocol object schema:
conflicts carry `repoId`, `{ "object": "<64-hex>" }` refs for `base` / `ours` /
`theirs`, and an optional `resolution` blob id; merge results carry `repoId`,
`baseSnapshot` / `oursSnapshot` / `theirsSnapshot`, and bare 64-hex conflict and
merged-snapshot ids.

```rust
use sorrel_core::{merge_snapshots, MergeOptions, MergeResultStatus, Principal};

let result = merge_snapshots(&store, &base.id, &ours.id, &theirs.id,
    &MergeOptions::new(Principal::system(), "repo", "merge lane"))?;
match result.status {
    MergeResultStatus::Clean => { /* result.merged_snapshot */ }
    MergeResultStatus::Conflicted => { /* result.conflicts */ }
}
```

Rename detection, conflict *resolution*, and recursive multi-base merging layer
on top of this model later.

## Sync transport

The `transport` module is the engine half of the push/pull protocol documented
in `sorrel-protocol/docs/sync-transport.md`:

- `collect_closure`: every object reachable from a set of snapshots (trees,
  blobs, and ancestor snapshots, so pulled history is walkable).
- `missing_objects` / `missing_in_target`: server- and client-side negotiation
  of exactly which objects must move.
- `transfer_objects`: content-verified batch copy between stores.
- `is_descendant`: the fast-forward ref rule.

## Policy, authority, and permissions

The crate defines the headless Core permission model used by every Sorrel
module: `PrincipalId`/`PrincipalKind`, `Capability`, `ResourceRef`, `Grant`,
`Policy`, `PolicyDecision`, `SecretRef`, `RedactionMarker`, and `AuditEvent`.

`evaluate_policy` is a deterministic in-memory evaluator: deny wins over
redact, redact over review, review over allow, and unmatched requests return
`needs_grant`. `authority::evaluate_policy_change` governs policy *mutation*:
changes are signed `PolicyChange` objects evaluated against the **previous**
effective policy, so a change can never authorize itself — self-grants,
self-escalation, and unsigned authority changes are rejected unless an
already-authorized authority delegated the power (`policy.grant`,
`policy.delegate`, `authority.rotate`, `authority.admin`).

Raw secret values never enter the object graph; only `SecretRef` handles and
metadata do.

`Lane` and `Stack` (in `lane_stack`) are deterministic metadata objects over
stored snapshots and changes: base/head refs, ordered changes, owner principal,
visibility, policy/grant refs, audit hooks, and touched resources lifted from
change paths. The lane/stack layer uses the lightweight `permissions` metadata
types; the policy evaluator types are re-exported with `Policy*` aliases where
names would collide (e.g. `PolicyGrant`, `PolicyResourceRef`).

## Conformance and benchmarks

- `tests/conformance/` vendors the canonical policy-conformance manifest owned
  by `sorrel-protocol`; `tests/policy_conformance.rs` asserts the evaluator
  matches every expected decision and `tests/conformance_sync.rs` fails if the
  vendored copy drifts from its checksum sidecar. Never edit these by hand —
  re-export from a `sorrel-protocol` checkout.
- `benches/engine.rs` (`cargo bench --bench engine`) is a dependency-free perf
  harness with coarse budgets that fail on order-of-magnitude regressions:
  snapshotting 2,000 files must average at most 1.5 s, diffing 2,000 files with
  20 modifications at most 500 ms, and walking 500 changes at most 300 ms.
  These are portable regression guards, not throughput claims.

## Out of scope (today)

No rebase, rename detection, automatic conflict resolution, recursive
multi-base merge, packfiles or chunked large-file storage, production auth, or
hosted compute. Those build on top of this foundation; shared contracts go
through `sorrel-protocol`.
