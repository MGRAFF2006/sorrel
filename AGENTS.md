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

## Agent workspace (important)

Submodules are a **publish** layout, not a **work** layout. Agents should use the
**root checkout as one workspace**: edit files under `sorrel-core/`, `sorrel-cli/`,
etc. directly from the parent tree. You do not need separate sessions per submodule.

See **[`docs/AGENT_WORKSPACE.md`](docs/AGENT_WORKSPACE.md)** — first-party modules
are **branch-tracked** (`branch = main` in `.gitmodules`, `submodule update
--remote`); **commit pins** are for foreign consumers and release snapshots only.

## Submodules

First-party `sorrel-*` repos are monorepo members on **`main`**, not frozen foreign
deps. After merging submodule work, optionally snapshot root gitlinks with
`./scripts/sync-submodule-pointers.sh` (see agent workspace doc).
