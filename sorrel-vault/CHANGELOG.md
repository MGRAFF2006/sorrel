# Changelog

## [Unreleased]

No changes yet.

## [0.1.0-alpha.2] - 2026-09-01

### Removed

- Remove confirmed dead code ([#64](https://github.com/MGRAFF2006/sorrel/pull/64)).

## [0.1.0-alpha.1] - 2026-08-31

### Capabilities

- Defines the draft `sorrel.secrets.yml` schema and validated examples.
- Provides a dependency-free CLI and in-memory backend for local development,
  including import, listing, grant checks, and redaction.
- Requires a trusted Core-style policy decision before resolving a secret
  handle.
- Accepts `keyring`, `dotenv`, and environment SecretSpec provider identifiers
  for CLI interoperability.

### Limitations

- This alpha is local/dev-only and is not a production secret store.
- No cloud secret managers or hosted provider integrations are implemented.
- Secret values remain in memory, and the package intentionally persists only
  handles, bindings, redaction metadata, and audit metadata.

[0.1.0-alpha.1]: https://github.com/MGRAFF2006/sorrel/releases/tag/v0.1.0-alpha.1

[Unreleased]: https://github.com/MGRAFF2006/sorrel/compare/v0.1.0-alpha.2...HEAD
[0.1.0-alpha.2]: https://github.com/MGRAFF2006/sorrel/releases/tag/v0.1.0-alpha.2
