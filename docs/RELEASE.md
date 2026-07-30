# Coordinated release process

Sorrel releases are a set of independently versioned repositories plus one
root manifest that records their exact commits.

## Alpha scope

`v0.1.0-alpha.1` is a local-first developer preview. Core, CLI, protocol, and
the Git bridge are the primary supported surface. Hub is localhost/dev-only;
vault, runners, slices, agents, and SDKs are experimental.

## Dependency order

1. `sorrel-protocol`
2. `sorrel-core`
3. `sorrel-cli`, `sorrel-sdk-rust`, and other Core consumers
4. Hub, Hub UI, vault, runners, slices, agents, JS SDK, and website
5. Root gitlinks, release manifest, documentation, and tags

When Core changes, its final release commit must be pinned by both
`sorrel-cli/Cargo.toml` and `sorrel-sdk-rust/Cargo.toml`. The root release gate
enforces this.

## Release candidate checks

From the root checkout at the intended gitlink set:

```sh
npm run validate:release
npm run validate:conformance
npm run test:modules
npm test
```

Run Rust lint/format checks in all Rust modules and `npm audit` in publishable
Node modules. Run `cargo bench --bench engine` in Core and compare it with
`benchmarks/BASELINE.json`.

## Tagging

After all checks pass and every module branch is merged:

1. Confirm `release/manifest.json` matches the root gitlinks.
2. Create annotated `v0.1.0-alpha.1` tags in each module at the recorded SHA.
3. Tag the root coordination commit last.
4. Publish release notes from `CHANGELOG.md`.

Tags are release anchors; do not move or recreate them. A correction gets a new
prerelease version.

## Rollback

The root tag and `release/manifest.json` identify the complete prior commit
set. Roll back by checking out that root tag and initializing its exact
submodule pointers. Back up `.sorrel/` and Hub data directories before moving
between alpha versions; no general storage migration framework exists yet.

## CI credentials

Root Actions requires `SUBMODULES_TOKEN`, a read-only fine-grained PAT with
Contents access to every private `sorrel-*` repository. Workflow-file updates
also require a credential with GitHub's `workflow` scope.
