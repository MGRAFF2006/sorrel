# Changelog

Notable changes to the Sorrel public website are documented here.

## [Unreleased]

No changes yet.

## [0.1.0-alpha.2] - 2026-09-01

### Added

- Add automatic coordinated changelog preparation ([#73](https://github.com/MGRAFF2006/sorrel/pull/73)).
- Publish hostable Sorrel server releases ([#75](https://github.com/MGRAFF2006/sorrel/pull/75)).

### Changed

- Use current logo on public website ([#56](https://github.com/MGRAFF2006/sorrel/pull/56)).
- Improve mobile navigation accessibility ([#58](https://github.com/MGRAFF2006/sorrel/pull/58)).
- Report only implemented Hub capabilities ([#60](https://github.com/MGRAFF2006/sorrel/pull/60)).
- Forward Hub bearer tokens from the CLI ([#61](https://github.com/MGRAFF2006/sorrel/pull/61)).
- Overhaul public website design ([#62](https://github.com/MGRAFF2006/sorrel/pull/62)).
- Replace deprecated Rust YAML parser ([#65](https://github.com/MGRAFF2006/sorrel/pull/65)).
- Polish CLI installation and contribution workflow ([#71](https://github.com/MGRAFF2006/sorrel/pull/71)).
- Overhaul public documentation design ([#72](https://github.com/MGRAFF2006/sorrel/pull/72)).
- Establish review-first Sorrel Hub product interface ([#74](https://github.com/MGRAFF2006/sorrel/pull/74)).

### Removed

- Remove agent project mirroring workaround ([#70](https://github.com/MGRAFF2006/sorrel/pull/70)).

### Fixed

- Reject unsupported run log following ([#59](https://github.com/MGRAFF2006/sorrel/pull/59)).

## [0.1.0-alpha.1] - 2026-08-31

### Added

- Static public landing page and documentation hub.
- Published project status and getting-started guides for the coordinated Sorrel
  alpha.
- Developer, architecture, release, and AI-oriented contribution guides synced
  from the monorepo documentation sources.
- Direct links to the coordinated GitHub release and changelog as the public
  record of shipped progress.
- A dependency-free static-site gate for JavaScript syntax and local HTML asset
  and navigation links.

### Alpha documentation scope

- Documents the working local VCS, Git import/export/colocated sync, Hub
  push/pull, and writable Hub review and selected administration UI.
- Calls out that the Hub and Hub UI are development-only and have no production
  authentication.
- Distinguishes shipped alpha capabilities from later embedding, app, hosted
  compute, and richer collaboration work.

[0.1.0-alpha.1]: https://github.com/MGRAFF2006/sorrel/releases/tag/v0.1.0-alpha.1

[Unreleased]: https://github.com/MGRAFF2006/sorrel/compare/v0.1.0-alpha.2...HEAD
[0.1.0-alpha.2]: https://github.com/MGRAFF2006/sorrel/releases/tag/v0.1.0-alpha.2
