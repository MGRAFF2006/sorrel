# Changelog

All notable changes to `sorrel-core` are documented in this file.

## [Unreleased]

No changes yet.

## [0.1.0-alpha.1] - 2026-08-31

Initial alpha release of the Sorrel version-control engine.

### Capabilities

- BLAKE3 content-addressed in-memory and filesystem object stores with
  digest-verified reads and atomic filesystem writes.
- Deterministic trees and snapshots, workspace materialization and restore,
  stat-cache-assisted hashing, changes, and path-level diffs.
- Snapshot-DAG ancestry and merge-base queries, plus three-way file merging
  with persisted conflict and merge-result objects.
- Git repository import and export.
- Object-closure discovery, missing-object negotiation, verified transfer, and
  fast-forward ancestry checks for synchronization.
- Lane and stack metadata for coordinating changes.
- Deterministic policy evaluation and signed policy-change authority checks
  against the previous effective policy.

### Alpha limitations

- The Rust API and persisted `sorrel.protocol.v0` object representations are
  pre-stable and may change between alpha releases. There is no stable
  embedding ABI.
- There is no automatic object-store migration facility. Unknown schema
  versions are rejected, and stores must be backed up before an upgrade.
- Diffs are path-level and do not provide textual hunks or rename detection.
  Three-way merge does not yet resolve conflicts or recursively merge multiple
  merge bases.
- Snapshot materialization rejects unsupported filesystem entries such as
  symbolic links.
- The object store has no packfile or chunked large-file representation.
- Policy evaluation is an engine primitive, not a production authentication
  service.

See [the v0 compatibility policy](docs/COMPATIBILITY.md) for upgrade and
object-store expectations.

[Unreleased]: https://github.com/MGRAFF2006/sorrel/compare/v0.1.0-alpha.1...HEAD
[0.1.0-alpha.1]: https://github.com/MGRAFF2006/sorrel/releases/tag/v0.1.0-alpha.1
