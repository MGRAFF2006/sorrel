# Changelog

## 0.1.0-alpha.1 - 2026-07-30

### Capabilities

- Generates deterministic TypeScript/JavaScript slice manifests from one or
  more entrypoints.
- Follows local static imports and records package metadata, included and
  excluded files, and unresolved imports.
- Exposes both a command-line interface and a JavaScript library API.

### Limitations

- This prototype does not extract files, create repositories, or inspect
  package registries.
- It does not yet project permissions or secret-schema references.
- Dynamic, external, unsupported, and outside-root imports are reported but not
  resolved.
