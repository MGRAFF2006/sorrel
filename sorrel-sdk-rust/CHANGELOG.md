# Changelog

## 0.1.0-alpha.1 - 2026-07-30

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
