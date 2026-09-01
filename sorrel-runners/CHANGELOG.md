# Changelog

All notable changes to `sorrel-runners` are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

No changes yet.

## [0.1.0-alpha.2] - 2026-09-01

### Changed

- Replace deprecated Rust YAML parser ([#65](https://github.com/MGRAFF2006/sorrel/pull/65)).

## [0.1.0-alpha.1] - 2026-08-31

Initial alpha release.

### Added

- Portable `JobBundle`, job, runner-capability, policy, and result models.
- Experimental local-process and local Docker/Podman execution for development
  and testing.
- Version 1 `sorrel.workflow.yml` parsing with deterministic dependency order.
- Core-shaped permission evaluation before process or container execution.
- JSON Lines execution logs and output redaction metadata.

### Limitations

- Local and container execution are experimental, development-only interfaces.
- Secret injection is unsupported; secret references remain unresolved.
- Hosted compute, Kubernetes, SSH execution, production authentication, and
  external secret providers are not included.

[0.1.0-alpha.1]: https://github.com/MGRAFF2006/sorrel/releases/tag/v0.1.0-alpha.1

[Unreleased]: https://github.com/MGRAFF2006/sorrel/compare/v0.1.0-alpha.2...HEAD
[0.1.0-alpha.2]: https://github.com/MGRAFF2006/sorrel/releases/tag/v0.1.0-alpha.2
