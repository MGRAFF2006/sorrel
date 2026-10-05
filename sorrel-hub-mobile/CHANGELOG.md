# Changelog

All notable changes to `sorrel-hub-mobile` are documented here.

## [Unreleased]

### Fixed

- Scope saved bearer credentials to their Hub, reuse them during connection checks, and prevent partial keystore writes from pairing credentials with another endpoint.
- Align locked Expo SDK 57 packages with the supported patch versions so native validation and exports pass.
- Update compatible brace-expansion patches to remove reported pattern-expansion denial-of-service vulnerabilities.

## [0.1.0-alpha.2] - 2026-09-01

- Reserved the package version alongside the coordinated Sorrel alpha.2. Mobile
  binaries were not included in that release.
