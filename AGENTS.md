# Sorrel

Sorrel is an agent-native version-control system split across submodules.

The root repository coordinates architecture and submodule pointers. Most
implementation work lives in submodules:

- sorrel-protocol: schemas, examples, and the policy-conformance manifest
- sorrel-core: Rust engine — object store, snapshots, changes, lanes, policy spine
- sorrel-cli: the `sorrel` CLI (persistent local VCS over the engine)
- sorrel-vault: secrets/environment spec, local backend, and dev CLI
- sorrel-runners: local/container workflow runners + workflow-file parser
- sorrel-slices: TypeScript/JavaScript slice manifest generator
- sorrel-hub: collaboration **API server** (JSON over HTTP; no UI)
- sorrel-hub-web: Hub **web interface** (browser frontend for the Hub API)
- sorrel-web: public marketing/landing site (static)
- sorrel-agents: minimal agent control plane (register / claim / active work)
- sorrel-sdk-js: Hub HTTP client
- sorrel-sdk-rust: thin Rust SDK over `sorrel-core`

Hub is split: `sorrel-hub` is the API server, `sorrel-hub-web` is its web
interface, and `sorrel-web` is the unrelated public landing page.

Key docs: [`docs/STATUS.md`](docs/STATUS.md), [`ROADMAP.md`](ROADMAP.md),
[`AGENT_NATIVE_VERSION_CONTROL_REPORT.md`](AGENT_NATIVE_VERSION_CONTROL_REPORT.md).

## Rust toolchain

Rust modules require stable Rust 1.85+ with clippy and rustfmt. If needed:

```sh
rustup toolchain install stable --profile minimal -c clippy -c rustfmt
rustup default stable
cargo fetch
```

## Common checks

Root E2E (no mocks, all active modules): `npm test`

All submodule suites: `npm run test:modules`

Rust: `cargo build && cargo test && cargo clippy --all-targets && cargo fmt --all -- --check`

Node: `npm test` (+ `npm run validate` where defined)

## Agent workspace (important)

Use the **root checkout as one filesystem workspace**: edit files under
`sorrel-core/`, `sorrel-cli/`, etc. directly from the parent tree. Each
submodule remains an independent Git repository, so commit and push from the
repository that owns the changed files.

See **[`docs/AGENT_WORKSPACE.md`](docs/AGENT_WORKSPACE.md)** for the coordinated
multi-repository workflow and pointer-sync helper.

## Submodules

Some submodules may be private. After changes inside a submodule, merge to that
repo’s `main`, then advance and commit the pointer in this root repository.
`./scripts/sync-submodule-pointers.sh` can fetch every configured `main` and
stage those gitlink updates without changing active submodule checkouts.
