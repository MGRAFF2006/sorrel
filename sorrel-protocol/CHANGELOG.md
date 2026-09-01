# Changelog

## [Unreleased]

No changes yet.

## [0.1.0-alpha.2] - 2026-09-01

### Fixed

- Correct policy conformance architecture notes ([#66](https://github.com/MGRAFF2006/sorrel/pull/66)).

## [0.1.0-alpha.1] - 2026-08-31

### Capabilities

- Defines the `sorrel.protocol.v0` JSON Schema bundle and validated examples for
  version-control, policy, secret-reference, workflow, runner, and agent objects.
- Provides the canonical policy-conformance manifest and metadata used by
  Sorrel consumers.
- Supports `keyring`, `dotenv`, and environment SecretSpec provider identifiers
  in secret declarations.
- Documents workspace links, merge-conflict objects, and the push/pull sync
  transport.

### Limitations

- This is a pre-stable protocol release; the `v0` schemas may still change.
- Compatibility requires consumers to use a known schema bundle and coordinate
  vendored conformance updates.
- Hosted identity, shadow-mode sync, and automatic object migration are not
  defined or implemented by this package.

[0.1.0-alpha.1]: https://github.com/MGRAFF2006/sorrel/releases/tag/v0.1.0-alpha.1

[Unreleased]: https://github.com/MGRAFF2006/sorrel/compare/v0.1.0-alpha.2...HEAD
[0.1.0-alpha.2]: https://github.com/MGRAFF2006/sorrel/releases/tag/v0.1.0-alpha.2
