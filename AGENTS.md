# Sorrel

Sorrel is an agent-native version-control system in a **single monorepo**.

Implementation packages live as normal directories:

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

Rust crates are a Cargo workspace. Prefer workspace commands from the repo root:

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

## Common checks

Root E2E (no mocks, all active modules): `npm test`

All package suites: `npm run test:modules`

Node packages: `npm test` (+ `npm run validate` where defined) inside each package dir.

## Workflow

Edit files under `sorrel-core/`, `sorrel-cli/`, etc. directly. Commit once in this
root repository — there are no submodule pointer advances and no
`SUBMODULES_TOKEN` requirement for CI.
