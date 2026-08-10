# Changelog

All notable changes to `sorrel-cli` are documented here.

## Unreleased

### Added

- SecretSpec-backed `sorrel secret list|refs|sync|check|get|set|run` under Core
  `secret.read` / `secret.inject` grants (providers: keyring, dotenv, env).
- Workflow `secretRefs` injection into the local process env with log redaction.
- devenv-first `sorrel env init|ensure|info|shell` with `local-fallback` backend.
- Structured run logs under `.sorrel/runs/` and `sorrel run list|show|logs`.

### Changed

- Default `sorrel grant create --agent` is now `agent_mock_cli` (matches CLI
  workflow principal).
- Secret providers in protocol/vault schemas include `keyring`, `dotenv`, `env`.

### Limitations

- devenv task shim from `sorrel.workflow.yml` is still thin; secret injection
  uses the local runner when secrets are present.
- `sorrel run logs --follow` is a stub; Hub run streaming is not shipped.

## 0.1.0-alpha.1 - 2026-07-30

Initial coordinated alpha release.

### Capabilities

- Persistent content-addressed repositories with status, diff, change history,
  lanes, stacks, slices, grants, and secret-reference registries.
- Three-way lane merges with conflict continuation and abort.
- Git history import/export and bidirectional colocated mirror sync.
- Hub push/pull remotes and lane proposal submission.
- Policy evaluation plus policy-gated local workflow validation and execution.
- Stable structured output through the global `--json` flag.

### Limitations

- Alpha storage and JSON contracts may change before a stable release.
- Hub operations require a separately deployed compatible `sorrel-hub`.
- Secret references are resolved via SecretSpec after Core grants; use
  `sorrel secret *` rather than the Node vault-cli for day-to-day work.
- Git import rejects symlinks and submodule gitlinks.
- Hub clone and force-push workflows are not exposed.
