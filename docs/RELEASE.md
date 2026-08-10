# Release process

Sorrel releases are tagged from this monorepo. `release/manifest.json` lists the
in-tree packages that belong to the release.

## Alpha scope

`v0.1.0-alpha.1` is a local-first developer preview. Core, CLI, protocol, and
the Git bridge are the primary supported surface. Hub is localhost/dev-only;
vault, runners, slices, agents, and SDKs are experimental.

## Release candidate checks

From the repo root:

```sh
npm run validate:release
npm run validate:conformance
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
npm run test:modules
npm test
```

Run `cargo bench -p sorrel-core --bench engine` and compare it with
`benchmarks/BASELINE.json` when the release notes claim performance numbers.

## Tagging

After checks pass on `main`:

1. Confirm package versions match `release/manifest.json`.
2. Create an annotated root tag `v0.1.0-alpha.1`.
3. Publish release notes from `CHANGELOG.md`.

Tags are release anchors; do not move or recreate them. A correction gets a new
prerelease version.

## Rollback

Check out the prior root tag. Back up `.sorrel/` and Hub data directories before
moving between alpha versions; no general storage migration framework exists yet.

## CI

Root Actions checks out this repository directly. No `SUBMODULES_TOKEN` is
required.
