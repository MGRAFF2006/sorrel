# Changelog

## [Unreleased]

### Fixed

- Reject entrypoints and package metadata that resolve through symlinks outside
  the project root, and report such imports as `outside_project_root` without
  reading their contents. Preserve links within the project and symlinked roots.
- Accept valid project paths whose names begin with `..`.
- Ignore import-like examples inside strings, template text, and regex literals
  while retaining dependency calls in template expressions and division operands.

## [0.1.0-alpha.2] - 2026-09-01

No package-specific changes.

## [0.1.0-alpha.1] - 2026-08-31

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

[0.1.0-alpha.1]: https://github.com/MGRAFF2006/sorrel/releases/tag/v0.1.0-alpha.1

[Unreleased]: https://github.com/MGRAFF2006/sorrel/compare/v0.1.0-alpha.2...HEAD
[0.1.0-alpha.2]: https://github.com/MGRAFF2006/sorrel/releases/tag/v0.1.0-alpha.2
