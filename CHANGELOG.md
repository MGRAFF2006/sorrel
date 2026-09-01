# Changelog

This is the authoritative history of coordinated Sorrel releases. Package-level
details live in each `sorrel-*/CHANGELOG.md`; future work lives in
[`ROADMAP.md`](ROADMAP.md), not in ad hoc progress notes.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and versions follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- Reworked the Hub interface around a familiar developer-platform shell with
  review-first project overviews, clearer onboarding, human-readable
  deployment status, an explicit local-compute product boundary, public-site
  visual language, and focused modal project creation.
- Added the public Sorrel favicon to Hub, labeled the current deployment as
  local development, and refined Reviews and Repositories toward product
  language instead of transport and administration terminology.
- Made the redesigned Hub shell, navigation, status controls, master-detail
  views, tables, forms, and dialogs responsive across tablet and phone widths.
- Overhauled the public documentation with a consistent editorial reading
  system, clearer navigation, improved code and table treatments, responsive
  guide layouts, browser-history-aware guide switching, syntax-highlighted
  code with block and per-line copy actions, and generated page sidebars.
- Overhauled the public landing page with a new editorial layout, interactive
  lane-graph visual, clearer architecture and workflow storytelling, and a
  more candid developer-preview status presentation.
- Added cross-platform CLI release binaries, SHA-256 checksums, and shell and
  PowerShell installers for Linux, macOS, and Windows; `sorrel init` now ends
  with a short guided first-change workflow.
- Added structured pull-request, bug-report, and feature-request templates with
  explicit testing, compatibility, documentation, and changelog prompts.
- Added zero-input contributor changelog automation: maintainers can generate
  coordinated root and package release sections from merged PR metadata and
  open a reviewable preparation PR from GitHub Actions.
- Removed the agent-control-plane workaround that represented agent
  registrations as unrelated Hub projects; agent coordination remains local
  until Hub exposes a real agent contract.
- Replaced the deprecated, unmaintained Rust `serde_yaml` parser with the
  maintained `serde_yaml_ng` continuation for CLI and runner YAML inputs.
- Updated the public website and documentation pages to use the current Sorrel
  leaf logo for their header branding and favicon.
- Added `SORREL_HUB_TOKEN` bearer authentication for CLI Hub requests and made
  authenticated lane submissions derive their author from the verified Hub
  session.
- Made Hub capabilities report only implemented modules and the active
  filesystem or in-memory store; removed the unimplemented Actions route from
  Hub UI navigation.
- Changed unsupported `run logs --follow` requests to fail explicitly instead
  of returning a one-shot log response labeled as following.
- Improved public-site navigation on small screens and for keyboard users with
  an accessible compact menu, skip links, focus indicators, and clearer theme
  toggle labels.

## [0.1.0-alpha.1] - 2026-08-31

First coordinated, local-first developer preview of the Sorrel monorepo.

### Added

- A Rust content-addressed engine with BLAKE3 object ids, filesystem and
  in-memory stores, trees, snapshots, changes, stat-cache-assisted
  materialization, history traversal, lanes, stacks, and sync closure helpers.
- Three-way snapshot and line merge with merge-base discovery, persisted
  `Conflict`/`MergeResult` objects, and CLI `merge --continue`/`--abort` flows.
- A persistent `sorrel` CLI covering repository initialization, status, diff,
  history, changes, lanes, stacks, grants, SecretSpec-backed secret management,
  devenv-aware environments/workflows, structured local run logs, slices,
  remotes, push/pull, and stable `--json` output.
- Incremental Git import/export plus colocated bidirectional `sorrel git sync`;
  true divergence is parked on a normal Sorrel lane for explicit resolution.
- Canonical `sorrel.protocol.v0` schemas, examples, compatibility documents,
  and a checksum-protected policy-conformance manifest shared across Rust and
  JavaScript consumers.
- A filesystem-backed Hub API for projects, administration metadata,
  proposals, review comments, workflow runs, lane submission, and negotiated
  content-addressed sync.
- Hub installation seams: `/capabilities`, `/session`, development, WorkOS,
  and OIDC/JWKS AuthAdapters, non-loopback bind safety, and an optional Convex
  metadata schema/mirror for open-proposal counts.
- A shared SolidJS Hub product UI (`sorrel-hub-ui`) with project-first
  navigation, Reviews and Sync views, a Convex/Hub live-count fallback, and a
  thin Vite browser host (`sorrel-hub-web`).
- Experimental local-process and Docker/Podman workflow runners, a versioned
  workflow parser, Core-policy gates, JSONL execution logs, and redaction. The
  CLI can resolve `keyring`, `dotenv`, and environment SecretSpec providers,
  inject authorized workflow secrets, prefer devenv, and persist redacted run
  records under `.sorrel/runs/`.
- Experimental Vault schemas/local tooling, TS/JS slice manifests, a persistent
  advisory agent-control plane, a Hub JavaScript client, and a Rust workspace
  wrapper over Core.
- A public static website and developer documentation, a one-command local
  dashboard, Docker Compose previews, deterministic workspace setup, focused
  module checks, documentation drift guards, and a no-mock full-stack E2E.
- A Node 24-based GitHub Actions toolchain using the current checkout and
  setup-node action majors.
- Nord-themed brand marks, a wordmark, and a social banner under `assets/`,
  with the root README refreshed around the release and documentation paths.

### Security

- The Hub binds to loopback by default and refuses development auth or broad
  bootstrap grants on non-loopback addresses unless an explicit insecure-demo
  override is set.
- Object reads and uploads verify content ids; ref updates require complete
  closures and fast-forward/expected-head checks.
- Policy changes are evaluated against previous authority, and conformance
  vectors prevent self-grants, unsigned escalation, and scope broadening.
- Secret values remain outside Sorrel objects. CLI resolution and injection
  happens only after Core grant checks, includes an explicit SecretSpec audit
  reason, and redacts persisted output.

### Known limitations

- This is a prerelease. `sorrel.protocol.v0`, CLI JSON, Rust APIs, and persisted
  formats may change before 1.0; automatic workspace/Hub migrations do not yet
  exist.
- Hub production sessions and login UI are incomplete. WorkOS sealed sessions,
  IdP login, and a production authorization-provisioning path are not shipped.
- The Vault has no production or hosted backend. The standalone runner library
  does not inject values; the CLI integration does so through SecretSpec under
  Core grants. Full devenv task mapping and `run logs --follow` are not shipped.
- There is no stable C ABI, N-API, WASM, or daemon embedding surface; SDKs are
  intentionally small and desktop/mobile applications are not shipped.
- Hub lists are unpaginated, typed uploaded objects are not schema-validated,
  and merge queue, hosted compute, virtualized review diffs, and sophisticated
  conflict resolution remain future work.

[Unreleased]: https://github.com/MGRAFF2006/sorrel/compare/v0.1.0-alpha.1...HEAD
[0.1.0-alpha.1]: https://github.com/MGRAFF2006/sorrel/releases/tag/v0.1.0-alpha.1
