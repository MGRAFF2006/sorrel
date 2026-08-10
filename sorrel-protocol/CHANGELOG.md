# Changelog

## 0.1.0-alpha.1 - 2026-07-30

### Capabilities

- Defines the `sorrel.protocol.v0` JSON Schema bundle and validated examples for
  version-control, policy, secret-reference, workflow, runner, and agent objects.
- Provides the canonical policy-conformance manifest and metadata used by
  Sorrel consumers.
- Documents workspace links, merge-conflict objects, and the push/pull sync
  transport.

### Limitations

- This is a pre-stable protocol release; the `v0` schemas may still change.
- Compatibility requires consumers to use a known schema bundle and coordinate
  vendored conformance updates.
- Hosted identity, shadow-mode sync, and automatic object migration are not
  defined or implemented by this package.
