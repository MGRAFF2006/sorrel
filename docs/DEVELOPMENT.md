# Developing Sorrel

Last updated: 2026-08-31

This is the operational guide for human and AI contributors. It covers a fresh
checkout, change ownership, focused validation, generated files, local service
debugging, and release hygiene.

## Bootstrap

Requirements:

- Git
- Rust stable 1.85+ with `clippy` and `rustfmt`
- Node.js 22+
- Docker or Podman only for container-runner/Compose work

```sh
git clone https://github.com/MGRAFF2006/sorrel.git
cd sorrel
rustup toolchain install stable --profile minimal -c clippy -c rustfmt
npm run setup
npm run check:quick
```

`npm run setup` installs locked dependencies for packages that need them and
prefetches the Cargo workspace. No submodules or private dependency tokens are
required.

## Find the owner before editing

| Concern | Primary package | Common consumers to check |
| --- | --- | --- |
| Object/schema/policy contract | `sorrel-protocol` | Core, CLI, Hub, Runners, Vault, SDKs |
| Object storage/history/merge/Git | `sorrel-core` | CLI, Rust SDK, E2E |
| User-visible VCS commands/JSON | `sorrel-cli` | E2E, docs, SDK assumptions |
| Hub routes/auth/capabilities/sync | `sorrel-hub` | Hub UI, web proxy, JS SDK, agents, E2E |
| Product UI/interaction | `sorrel-hub-ui` | Browser host, E2E |
| Desktop host/bundles | `sorrel-hub-desktop` | Shared Hub UI, release workflow |
| Browser build/proxy/container | `sorrel-hub-web` | Compose, E2E |
| Workflow model/execution | `sorrel-runners` | CLI workflow/devenv adapter, protocol |
| Secret schema/providers/injection | `sorrel-vault`, `sorrel-cli` | runners, protocol, run logs |
| Slice generation | `sorrel-slices` | CLI slice surface |
| Public messaging/docs | `sorrel-web`, root `docs/` | README, changelog, release |

Every package has a local `AGENTS.md` with invariants and its exact check
command. Read the root instructions and each affected package instruction file
before changing code.

## Validation ladder

Use the smallest level that gives meaningful feedback while iterating:

```sh
# Metadata, conformance drift, mirrored docs, local Markdown links
npm run check:quick

# Discover or run selected package gates
npm run test:module -- --list
npm run test:module -- sorrel-hub sorrel-hub-ui sorrel-hub-web

# Rust workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check

# Complete release gate: quick checks, Rust lint/format, every package, E2E
npm run check
```

Node packages expose `npm run check`; depending on the package it includes
tests, schema validation, syntax lint, type checking, and/or a production build.
The root E2E launches real processes and touches every active module without
mocks.

## Common change recipes

### Protocol or policy contract

1. Change schemas/examples/conformance in `sorrel-protocol`.
2. Run `npm run check` in that package.
3. Refresh consumers with `./scripts/sync-conformance.sh`.
4. Update Rust/JavaScript implementations together.
5. Run `npm run validate:conformance` and all affected module checks.
6. Give the PR a clear user-facing title so release automation can categorize
   it and map the changed paths to affected package changelogs.

Never hand-edit vendored `test(s)/conformance/` copies.

### Core plus CLI

1. Add or adjust the engine primitive in `sorrel-core`.
2. Preserve additive public APIs where practical.
3. Adapt the command in `sorrel-cli` without weakening `--json` contracts.
4. Add engine unit/integration tests and CLI command/JSON tests.
5. Run both package suites and the root E2E.

### Hub API plus UI

1. Keep authoritative policy and VCS semantics in Protocol/Core.
2. Add the Hub route/store/auth change and tests.
3. Update `sorrel-hub-ui` product behavior.
4. Touch `sorrel-hub-web` or `sorrel-hub-desktop` only for their host,
   transport, native-integration, or build concerns.
5. Check the JS SDK if a public route changed.
6. Run the Hub/UI/web module gates and a real browser smoke test.

### Public documentation

1. Update the canonical Markdown under `docs/` and the root/package changelog.
2. Run `npm run sync:docs` for public Markdown mirrors.
3. Update static HTML pages in `sorrel-web/docs/` when navigation or conceptual
   content changes.
4. Run `npm run validate:docs` and preview `sorrel-web` locally.

## Local services

The simplest integrated environment is:

```sh
npm run dev
```

It builds CLI/UI, creates a disposable `.dev/workspace`, starts Hub on `:3000`,
the Hub browser host on `:5180`, and a control dashboard on `:5200`. State lives
under ignored `.dev/`; stop with Ctrl+C.

Container preview:

```sh
docker compose up --build

# Optional self-hosted Convex profile
docker compose --profile convex \
  -f docker-compose.yml -f docker-compose.convex.yml up --build
```

Important Hub variables:

| Variable | Purpose |
| --- | --- |
| `HOST`, `PORT` | Bind address and port; standalone default is loopback/3000 |
| `SORREL_HUB_DATA_DIR` | Sync object/ref storage |
| `SORREL_HUB_METADATA_DIR` | Product metadata storage |
| `SORREL_HUB_SYNC_STORE=memory` | Ephemeral test stores |
| `SORREL_HUB_AUTH=dev|workos|oidc` | AuthAdapter selection |
| `SORREL_HUB_BOOTSTRAP_GRANTS=1` | Broad local demo grants; never production |
| `SORREL_HUB_ALLOW_INSECURE_DEV_AUTH=1` | Allow dev auth on non-loopback for isolated demos |
| `CONVEX_URL` | Hub-to-Convex internal URL |
| `CONVEX_PUBLIC_URL` | Browser-reachable Convex URL advertised in capabilities |

See `sorrel-hub/README.md` for OIDC/WorkOS settings and complete API details.

## Debugging state

- CLI state: inspect `.sorrel/manifest.json`, `HEAD`, `heads/`, and
  `changes.index`; do not edit object bytes by hand.
- Secret/run state: handles live under `.sorrel/secrets/`, never values; inspect
  `.sorrel/runs/<id>/` for structured result and redacted logs.
- Merge state: `MERGE_STATE` exists only during a conflicted merge.
- Git bridge state: `.sorrel/git-map.json` links commit SHAs and snapshots.
- Hub state: use a temporary `SORREL_HUB_DATA_DIR` and
  `SORREL_HUB_METADATA_DIR`; corrupt records are skipped or rejected by tests.
- UI networking: browser calls `/api`; the web host proxies to `HUB_API_URL`.
- Structured automation: prefer CLI `--json` and assert stable shapes rather
  than timestamps or content-derived ids.

## Generated, mirrored, and ignored files

- Canonical conformance: `sorrel-protocol/conformance/`; consumer copies are
  generated by `scripts/sync-conformance.sh`.
- Public Markdown mirrors under `sorrel-web/docs/` are generated by
  `npm run sync:docs` and carry a generated notice.
- `target/`, nested `target/`, `node_modules/`, `dist/`, `.dev/`, Hub `data/`,
  `.env*`, logs, and generated rustdoc output are not source.
- Package locks are committed; use `npm ci` for reproducible installs.

## AI-agent operating rules

Before editing, inspect `git status` and relevant diffs. Existing changes may
belong to another person or agent. Do not reset, discard, or broadly reformat
them. Stay in the owning package, follow its `AGENTS.md`, and expand scope only
to required consumers.

Treat these as hard boundaries:

- no raw secrets in objects, logs, fixtures, docs, or commits;
- no Hub/UI-only authorization model;
- no hand-edited conformance mirrors;
- no change to stable-looking CLI JSON without tests and changelog notes;
- no claim that roadmap/design-report items are shipped without code and tests.

Progress is recorded in releases and changelogs. Do not create ad hoc agent
task packs, progress dashboards, or duplicated feature-audit documents.

## Changelog and release hygiene

- Contributors do not manually maintain routine `Unreleased` entries. The
  **Prepare changelogs** workflow derives them from merged PR metadata and maps
  changed paths to affected package changelogs.
- Use a concise PR title that describes user or operator impact. Conventional
  prefixes such as `feat:` and `fix:` improve categorization but are optional;
  unknown formats safely land under `Changed`.
- Maintainers may apply `skip-changelog` to omit internal-only work. Direct
  commits remain root changes but cannot be mapped to a package, so normal work
  should still go through pull requests.
- The root changelog is the coordinated release summary; package changelogs are
  the detailed component history.
- `STATUS.md` is a current snapshot and `ROADMAP.md` is future-only. Neither is
  a chronological progress log.
- Release tags are immutable. Follow [`RELEASE.md`](RELEASE.md) for validation,
  notes extraction, tagging, and GitHub publication.
