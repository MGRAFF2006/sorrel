# Changelog

## [Unreleased]

### Changed

- Removed the optional Hub project mirroring workaround. Agent registration is
  local until Hub has a real agent-control-plane contract.

## [0.1.0-alpha.1] - 2026-08-31

- Initial alpha of the local agent control plane.
- Register agents, persist advisory path claims under `.sorrel/agents`, and
  inspect active work.
- Optionally mirror coordination state to a Sorrel Hub project.

[Unreleased]: https://github.com/MGRAFF2006/sorrel/compare/v0.1.0-alpha.1...HEAD
[0.1.0-alpha.1]: https://github.com/MGRAFF2006/sorrel/releases/tag/v0.1.0-alpha.1
