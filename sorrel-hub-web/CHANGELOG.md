# Changelog

Notable changes to the Sorrel Hub web companion are documented here.

## [Unreleased]

No changes yet.

## [0.1.0-alpha.2] - 2026-09-01

### Added

- Publish hostable Sorrel server releases ([#75](https://github.com/MGRAFF2006/sorrel/pull/75)).

### Changed

- Share Hub web server implementation ([#63](https://github.com/MGRAFF2006/sorrel/pull/63)).
- Build Hub web from its declared UI package ([#69](https://github.com/MGRAFF2006/sorrel/pull/69)).
- Establish review-first Sorrel Hub product interface ([#74](https://github.com/MGRAFF2006/sorrel/pull/74)).

## [0.1.0-alpha.1] - 2026-08-31

### Added

- A thin Vite/Solid browser host for the shared `sorrel-hub-ui` package.
- Development and production static servers with `/api/*` proxying to Hub.
- Forwarding for development acting principals and bearer authorization.
- Locked multi-stage container builds from the monorepo root context.

### Alpha limitations

- This is a development-only companion to a compatible `sorrel-hub`; it has no
  complete login/SSO flow or production authentication.
- Product behavior belongs to `sorrel-hub-ui`; this package intentionally owns
  only browser mounting, build configuration, static serving, and API proxying.

[0.1.0-alpha.1]: https://github.com/MGRAFF2006/sorrel/releases/tag/v0.1.0-alpha.1

[Unreleased]: https://github.com/MGRAFF2006/sorrel/compare/v0.1.0-alpha.2...HEAD
[0.1.0-alpha.2]: https://github.com/MGRAFF2006/sorrel/releases/tag/v0.1.0-alpha.2
