# Changelog

All notable changes to `sorrel-cli` are documented here.

## Unreleased

No changes yet.

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
- Secret references are not resolved or injected by this CLI.
- Git import rejects symlinks and submodule gitlinks.
- Hub clone and force-push workflows are not exposed.
