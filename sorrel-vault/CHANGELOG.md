# Changelog

## 0.1.0-alpha.1 - 2026-07-30

### Capabilities

- Defines the draft `sorrel.secrets.yml` schema and validated examples.
- Provides a dependency-free CLI and in-memory backend for local development,
  including import, listing, grant checks, and redaction.
- Requires a trusted Core-style policy decision before resolving a secret
  handle.

### Limitations

- This alpha is local/dev-only and is not a production secret store.
- No cloud secret managers or hosted provider integrations are implemented.
- Secret values remain in memory, and the package intentionally persists only
  handles, bindings, redaction metadata, and audit metadata.
