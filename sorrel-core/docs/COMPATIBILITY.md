# v0 compatibility policy

This policy applies to `sorrel-core` 0.x releases and the
`sorrel.protocol.v0` objects they read and write. The alpha is suitable for
evaluation and controlled development, not for relying on long-term format or
API stability.

## Rust API and embedding

- Public Rust APIs may change between 0.x prereleases, including patch-level
  alpha releases. Consumers should pin an exact release or revision and test
  upgrades.
- There is no stable embedding ABI. In particular, the crate does not promise
  a stable C ABI, dynamic-library ABI, or compatibility between artifacts built
  with different Rust toolchains or dependency graphs.
- Source compatibility and behavior are best-effort during v0; any compatibility
  guarantee for a release will be stated explicitly in that release's
  changelog.

## Object-store compatibility

- Object IDs are BLAKE3 digests of the exact stored bytes. A future encoding
  change can therefore produce different IDs for logically equivalent objects.
- Typed readers accept only schema versions they understand. Unknown schema
  versions fail closed with an error; they are never silently interpreted as
  the current version.
- A release is expected to read objects it wrote itself. Forward or backward
  compatibility with another 0.x release is not guaranteed unless its
  changelog says otherwise.
- Do not allow different engine versions to write to the same store
  concurrently.

## Upgrades, backups, and migrations

- Back up the complete object store and all external refs or indexes before
  changing engine versions. Verify that the backup can be restored.
- Keep the backup until the upgraded store has been validated. Alpha releases
  can otherwise make data inaccessible to an older or newer engine.
- There is currently no object-store migration command.
- Future migrations must be explicit and documented. They must require a
  deliberate operator action, identify source and destination versions, report
  failures, and preserve or require a restorable backup. The engine will not
  silently rewrite a store as a side effect of opening it.

Stable API and storage guarantees will be defined separately before a 1.0
release.
