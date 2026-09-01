# Changelog

Notable changes to Sorrel Hub are documented here.

## [Unreleased]

No changes yet.

## [0.1.0-alpha.2] - 2026-09-01

### Added

- Publish hostable Sorrel server releases ([#75](https://github.com/MGRAFF2006/sorrel/pull/75)).

### Changed

- Report only implemented Hub capabilities ([#60](https://github.com/MGRAFF2006/sorrel/pull/60)).
- Forward Hub bearer tokens from the CLI ([#61](https://github.com/MGRAFF2006/sorrel/pull/61)).

### Removed

- Remove confirmed dead code ([#64](https://github.com/MGRAFF2006/sorrel/pull/64)).

## [0.1.0-alpha.1] - 2026-08-31

### Added

- Filesystem-backed sync objects, refs, and product metadata.
- Core-policy-conformant administration and sync authorization surfaces.
- Collaboration APIs for proposals, reviews, workflow runs, and lane submission.
- Capability and session discovery through `GET /capabilities` and
  `GET /session`.
- Development, WorkOS, and OIDC/JWKS AuthAdapter seams, with verified bearer
  token support for OIDC.
- Optional shared Convex schema and best-effort proposal metadata mirror for
  SaaS or self-hosted deployments.
- Development-only local bootstrap grants, available only when
  `SORREL_HUB_BOOTSTRAP_GRANTS=1`.

### Security defaults

- The standalone server binds to `127.0.0.1` by default.
- Non-loopback development auth and bootstrap-grant binds fail closed unless
  `SORREL_HUB_ALLOW_INSECURE_DEV_AUTH=1` is explicitly set.
- Local `user:local` bootstrap grants are disabled by default. When explicitly
  enabled, they grant object and ref writes across every local repository.

### Alpha limitations

- WorkOS sealed sessions and the product login UI are not complete; the Hub is
  not yet a production authentication boundary.
- Core remains the source of truth for grants, policy decisions, signatures,
  rotation thresholds, and audit events.
- List endpoints are not paginated.
- Object uploads verify BLAKE3 content ids but do not JSON-Schema-validate typed
  object bodies.
- There is no merge queue, hosted compute, or secret-value storage.

[0.1.0-alpha.1]: https://github.com/MGRAFF2006/sorrel/releases/tag/v0.1.0-alpha.1

[Unreleased]: https://github.com/MGRAFF2006/sorrel/compare/v0.1.0-alpha.2...HEAD
[0.1.0-alpha.2]: https://github.com/MGRAFF2006/sorrel/releases/tag/v0.1.0-alpha.2
