# Changelog

## [Unreleased]

No changes yet.

## [0.1.0-alpha.1] - 2026-08-31

Initial experimental release.

### API

- Re-exports selected `sorrel-core` object-store and snapshot types.
- Provides `Workspace::init` for local object-store initialization.
- Provides `Workspace::snapshot_working_tree` for snapshots that exclude
  `.sorrel`.
- Exposes workspace root, repository ID, and object-store accessors.

### Limitations

- The API is unstable and may change before a stable release.
- Only local filesystem storage and basic snapshot creation are wrapped.
- Broader `sorrel-core` embedding, collaboration, runners, and remote
  operations are not covered.

[Unreleased]: https://github.com/MGRAFF2006/sorrel/compare/v0.1.0-alpha.1...HEAD
[0.1.0-alpha.1]: https://github.com/MGRAFF2006/sorrel/releases/tag/v0.1.0-alpha.1
