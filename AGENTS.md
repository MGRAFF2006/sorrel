# Sorrel

Sorrel is an agent-native version-control system split across submodules.

The root repository coordinates architecture and submodule pointers. Most implementation work lives in submodules:

- sorrel-protocol: schemas, examples, and the policy-conformance manifest
- sorrel-core: Rust engine — object store, snapshots, changes, lanes, policy spine
- sorrel-cli: the `sorrel` CLI (persistent local VCS over the engine)
- sorrel-vault: secrets/environment spec, local backend, and dev CLI
- sorrel-runners: local/container workflow runners + workflow-file parser
- sorrel-slices: TypeScript/JavaScript slice manifest generator
- sorrel-hub: collaboration **API server** (JSON over HTTP; no UI)
- sorrel-hub-web: Hub **web interface** (browser frontend for the Hub API)
- sorrel-web: public marketing/landing site (static)
- sorrel-agents, sorrel-sdk-js, sorrel-sdk-rust: planned (not started)

Hub is split: `sorrel-hub` is the API server, `sorrel-hub-web` is its web
interface, and `sorrel-web` is the unrelated public landing page.

## Rust toolchain

Rust modules require stable Rust 1.85+ with clippy and rustfmt because they use edition2024 / modern Cargo metadata. If the base image has an older Rust version, run:

rustup toolchain install stable --profile minimal -c clippy -c rustfmt
rustup default stable
cargo fetch

## Common checks

From Rust repos/workspaces:

cargo build
cargo test
cargo clippy --all-targets
cargo fmt --all -- --check

Node/npm modules may use:

npm test
npm run lint
npm run validate

## Submodules

Some submodules may be private. If `git submodule update --init --recursive` fails with "Repository not found", ensure the agent has access to the private Sorrel repos.

After making changes inside a submodule, update the parent repository's submodule pointer and commit that pointer change in the root repository.
