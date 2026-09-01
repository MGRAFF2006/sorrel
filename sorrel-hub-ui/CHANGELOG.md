# Changelog

Notable changes to the shared Sorrel Hub UI (`sorrel-hub-ui`) are documented here.

## [Unreleased]

### Changed

- Reworked the product shell, project creation flow, and project overview so
  reviews, repositories, ownership, and local-workspace connection are visible
  without exposing deployment internals as primary navigation.
- Aligned Hub with the public Sorrel site's editorial visual language and
  replaced the expanding project form with a focused, accessible modal.
- Refined Reviews and Repositories with product-facing language, a consistent
  review modal, and explicit local-development deployment labeling.
- Removed the Actions placeholder route and navigation until the Hub exposes a
  real Actions module.

## [0.1.0-alpha.1] - 2026-08-31

### Added

- Shared SolidJS + Vite product UI for Projects, Reviews, and Sync.
- Platform stubs (`web` / `desktop` / `mobile`) for future Tauri shells.
- Live open-proposals badge (Convex subscription with Hub poll fallback).
- Thin-host mount API: `mountHubApp(element, options)`.

[Unreleased]: https://github.com/MGRAFF2006/sorrel/compare/v0.1.0-alpha.1...HEAD
[0.1.0-alpha.1]: https://github.com/MGRAFF2006/sorrel/releases/tag/v0.1.0-alpha.1
