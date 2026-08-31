# Changelog

All notable changes to `sorrel-cli` are documented here.

## [Unreleased]

### Added

- `SORREL_HUB_TOKEN` forwards an OIDC/WorkOS bearer access token for sync,
  project discovery, and lane submission without persisting it in remote
  configuration.

## [0.1.0-alpha.1] - 2026-08-31

Initial coordinated alpha release.

### Capabilities

- Persistent content-addressed repositories with status, diff, change history,
  lanes, stacks, slices, grants, and secret-reference registries.
- Three-way lane merges with conflict continuation and abort.
- Git history import/export and bidirectional colocated mirror sync.
- Hub push/pull remotes and lane proposal submission.
- Policy evaluation plus policy-gated local workflow validation and execution.
- SecretSpec-backed `secret list|refs|sync|check|get|set|run` under Core
  `secret.read` / `secret.inject` grants (`keyring`, `dotenv`, and `env`).
- Authorized workflow `secretRefs` injection with persisted-output redaction.
- devenv-aware `env init|ensure|info|shell` with local-process fallback.
- Structured run records under `.sorrel/runs/` and `run list|show|logs`.
- Stable structured output through the global `--json` flag.

### Changed

- Default `grant create --agent` is `agent_mock_cli`, matching the CLI workflow
  principal.
- SecretSpec operations provide an operation-specific audit reason and honor a
  non-empty `SECRETSPEC_REASON` override.

### Limitations

- Alpha storage and JSON contracts may change before a stable release.
- Hub operations require a separately deployed compatible `sorrel-hub`.
- The devenv task shim is thin; secret-bearing workflows use the local runner.
- `run logs --follow` and Hub run streaming are not implemented.
- Git import rejects symlinks and submodule gitlinks.
- Hub clone and force-push workflows are not exposed.

[Unreleased]: https://github.com/MGRAFF2006/sorrel/compare/v0.1.0-alpha.1...HEAD
[0.1.0-alpha.1]: https://github.com/MGRAFF2006/sorrel/releases/tag/v0.1.0-alpha.1
