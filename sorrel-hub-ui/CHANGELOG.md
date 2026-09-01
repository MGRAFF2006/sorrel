# Changelog

Notable changes to the shared Sorrel Hub UI (`sorrel-hub-ui`) are documented here.

## [Unreleased]

### Added

- Add global Inbox and organization/profile routes, a proposal-backed Work
  board, safe README rendering, and repository tree browsing.

### Changed

- Replace the dashboard-style Hub shell with repository-shaped project chrome,
  a Code-first project page, and a denser review workbench in Sorrel's Nord
  product theme.

## [0.1.0-alpha.2] - 2026-09-01

### Changed

- Test rendered Hub UI behavior ([#55](https://github.com/MGRAFF2006/sorrel/pull/55)).
- Report only implemented Hub capabilities ([#60](https://github.com/MGRAFF2006/sorrel/pull/60)).
- Establish review-first Sorrel Hub product interface ([#74](https://github.com/MGRAFF2006/sorrel/pull/74)).

### Removed

- Remove confirmed dead code ([#64](https://github.com/MGRAFF2006/sorrel/pull/64)).

## [0.1.0-alpha.1] - 2026-08-31

### Added

- Shared SolidJS + Vite product UI for Projects, Reviews, and Sync.
- Platform stubs (`web` / `desktop` / `mobile`) for future Tauri shells.
- Live open-proposals badge (Convex subscription with Hub poll fallback).
- Thin-host mount API: `mountHubApp(element, options)`.

[0.1.0-alpha.1]: https://github.com/MGRAFF2006/sorrel/releases/tag/v0.1.0-alpha.1

[Unreleased]: https://github.com/MGRAFF2006/sorrel/compare/v0.1.0-alpha.2...HEAD
[0.1.0-alpha.2]: https://github.com/MGRAFF2006/sorrel/releases/tag/v0.1.0-alpha.2
