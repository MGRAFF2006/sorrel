<!-- Generated from CHANGELOG.md by npm run sync:docs. Do not edit. -->

# Changelog

This is the authoritative history of coordinated Sorrel releases. Package-level
details live in each `sorrel-*/CHANGELOG.md`; future work lives in
[`ROADMAP.md`](https://github.com/MGRAFF2006/sorrel/blob/main/ROADMAP.md), not in ad hoc progress notes.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and versions follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Add read-only Hub APIs for browsing synchronized repository trees and UTF-8
  files from named refs.

### Changed

- Rebuild Sorrel Hub around repository-shaped project pages, a global Inbox,
  proposal-backed Work board, review workbench, and README-style organization
  and profile pages.

## [0.1.0-alpha.2] - 2026-09-01

Second developer preview focused on installability, a more complete Hub
interface, and repeatable releases. It adds downloadable CLI artifacts and
hostable server images while preserving Sorrel's prerelease boundaries.

### Added

- Add automatic coordinated changelog preparation ([#73](https://github.com/MGRAFF2006/sorrel/pull/73)).
- Publish hostable Sorrel server releases ([#75](https://github.com/MGRAFF2006/sorrel/pull/75)).

### Changed

- Test rendered Hub UI behavior ([#55](https://github.com/MGRAFF2006/sorrel/pull/55)).
- Use current logo on public website ([#56](https://github.com/MGRAFF2006/sorrel/pull/56)).
- Improve mobile navigation accessibility ([#58](https://github.com/MGRAFF2006/sorrel/pull/58)).
- Report only implemented Hub capabilities ([#60](https://github.com/MGRAFF2006/sorrel/pull/60)).
- Forward Hub bearer tokens from the CLI ([#61](https://github.com/MGRAFF2006/sorrel/pull/61)).
- Overhaul public website design ([#62](https://github.com/MGRAFF2006/sorrel/pull/62)).
- Share Hub web server implementation ([#63](https://github.com/MGRAFF2006/sorrel/pull/63)).
- Replace deprecated Rust YAML parser ([#65](https://github.com/MGRAFF2006/sorrel/pull/65)).
- Build Hub web from its declared UI package ([#69](https://github.com/MGRAFF2006/sorrel/pull/69)).
- Polish CLI installation and contribution workflow ([#71](https://github.com/MGRAFF2006/sorrel/pull/71)).
- Overhaul public documentation design ([#72](https://github.com/MGRAFF2006/sorrel/pull/72)).
- Establish review-first Sorrel Hub product interface ([#74](https://github.com/MGRAFF2006/sorrel/pull/74)).

### Removed

- Remove confirmed dead code ([#64](https://github.com/MGRAFF2006/sorrel/pull/64)).
- Remove unused Rust SDK dependencies ([#67](https://github.com/MGRAFF2006/sorrel/pull/67)).
- Eliminate redundant Rust clones ([#68](https://github.com/MGRAFF2006/sorrel/pull/68)).
- Remove agent project mirroring workaround ([#70](https://github.com/MGRAFF2006/sorrel/pull/70)).

### Fixed

- Guard module test inventory against release manifest ([#57](https://github.com/MGRAFF2006/sorrel/pull/57)).
- Reject unsupported run log following ([#59](https://github.com/MGRAFF2006/sorrel/pull/59)).
- Correct policy conformance architecture notes ([#66](https://github.com/MGRAFF2006/sorrel/pull/66)).

### Security

- Published server images run as non-root users, include provenance and SBOM
  attestations, and ship with a localhost-only, read-only Compose example.
- Hosting the images on an untrusted network still requires a production
  AuthAdapter and carefully scoped bootstrap grants; development auth and the
  insecure-demo override are not production-safe.

### Known limitations

- This remains a prerelease. Protocol, CLI JSON, Rust APIs, and persisted
  formats may change before 1.0, and general workspace/Hub migrations are not
  available yet.
- Hub production sessions and login UI remain incomplete. WorkOS sealed
  sessions, IdP login, and a production authorization-provisioning path are not
  shipped in this release.

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

[0.1.0-alpha.1]: https://github.com/MGRAFF2006/sorrel/releases/tag/v0.1.0-alpha.1

[Unreleased]: https://github.com/MGRAFF2006/sorrel/compare/v0.1.0-alpha.2...HEAD
[0.1.0-alpha.2]: https://github.com/MGRAFF2006/sorrel/releases/tag/v0.1.0-alpha.2
