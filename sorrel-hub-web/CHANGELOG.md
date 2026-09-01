# Changelog

Notable changes to the Sorrel Hub web companion are documented here.

## [Unreleased]

### Changed

- The Vite host now resolves the declared `sorrel-hub-ui` package dependency
  without reaching into the sibling package's private source path, so a clean
  `sorrel-hub-web` install can build independently.

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

[Unreleased]: https://github.com/MGRAFF2006/sorrel/compare/v0.1.0-alpha.1...HEAD
[0.1.0-alpha.1]: https://github.com/MGRAFF2006/sorrel/releases/tag/v0.1.0-alpha.1
