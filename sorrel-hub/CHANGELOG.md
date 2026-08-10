# Changelog

Notable changes to Sorrel Hub are documented here.

## [0.1.0-alpha.1] - 2026-07-30

### Added

- Filesystem-backed sync objects, refs, and product metadata.
- Core-policy-conformant administration and sync authorization surfaces.
- Collaboration APIs for proposals, reviews, workflow runs, and lane submission.
- Development-only local bootstrap grants, available only when
  `SORREL_HUB_BOOTSTRAP_GRANTS=1`.

### Security defaults

- The standalone server binds to `127.0.0.1` by default.
- Local `user:local` bootstrap grants are disabled by default. When explicitly
  enabled, they grant object and ref writes across every local repository.

### Alpha limitations

- There is no production authentication or identity-provider integration.
  Non-loopback binds and bootstrap grants are for controlled development and
  E2E environments only.
- Core remains the source of truth for grants, policy decisions, signatures,
  rotation thresholds, and audit events.
- List endpoints are not paginated.
- Object uploads verify BLAKE3 content ids but do not JSON-Schema-validate typed
  object bodies.
- There is no merge queue, hosted compute, or secret-value storage.
