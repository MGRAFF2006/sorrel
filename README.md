# Sorrel

Sorrel is an **agent-native version-control system** — a new VCS core built for
modern software work: parallel AI agents, cloud and in-memory workspaces,
first-class permissions and secrets, portable workflows, and shareable slices of
unfinished work. It is not "Git but nicer"; it is a layered system with a
content-addressed object store, changes/lanes/slices, a Core-native
identity/permission/policy spine, a bidirectional Git bridge, and a collaboration
product on top.

> **Release status:** `v0.1.0-alpha.1` is a local-first developer preview.
> The Hub is for localhost development only; it does not provide production
> authentication.

| Doc | Purpose |
| --- | --- |
| [`docs/STATUS.md`](docs/STATUS.md) | **What works / what is missing** |
| [`docs/GETTING_STARTED.md`](docs/GETTING_STARTED.md) | **How to clone and run everything** |
| [`docs/AGENT_WORKSPACE.md`](docs/AGENT_WORKSPACE.md) | Coordinated multi-repository agent workflow |
| [`docs/RELEASE.md`](docs/RELEASE.md) | Coordinated version, validation, and tagging process |
| [`SECURITY.md`](SECURITY.md) | Security scope and vulnerability reporting |
| [`ROADMAP.md`](ROADMAP.md) | Sequenced forward plan |
| [`AGENT_NATIVE_VERSION_CONTROL_REPORT.md`](AGENT_NATIVE_VERSION_CONTROL_REPORT.md) | Full architecture |

The same status and getting-started docs are published on the landing site
(`sorrel-web` → Cloudflare) under `/docs/`.

## Status at a glance (2026-07-30)

**Working today**

- Real engine + persistent CLI: init, status, change, diff, log, lanes, merge, push/pull
- Git import, export, and colocated bidirectional sync
- Policy/conformance spine across protocol, core, CLI, hub, runners, vault
- Development Hub with FS-backed sync + metadata; writable Hub UI companion
- Vault, runners, slices prototypes; public landing site live on Cloudflare

**Still missing**

- Production Hub authentication and signed client identity
- Stable embedding surface (C ABI / N-API / WASM / daemon)
- Secret injection and production vault backends
- Desktop/mobile applications
- Complete standalone CI coverage for every module

Details: [`docs/STATUS.md`](docs/STATUS.md).

## Quick start

```sh
git clone --recurse-submodules https://github.com/MGRAFF2006/sorrel.git
cd sorrel

# Local VCS demo
cd sorrel-cli && cargo build
SORREL=target/debug/sorrel
mkdir /tmp/sorrel-demo && cd /tmp/sorrel-demo
$SORREL init
echo hello > a.txt
$SORREL change create -m "add a.txt"
$SORREL log

# Hub API + UI (from repo root)
docker compose up --build
# API http://localhost:3000  ·  UI http://localhost:5180
```

Full instructions: [`docs/GETTING_STARTED.md`](docs/GETTING_STARTED.md).

## Repository layout

This root repository coordinates architecture and submodule pointers. Most
implementation lives in submodules:

| Submodule | Role | Maturity |
| --- | --- | --- |
| `sorrel-protocol` | Schemas, examples, policy conformance | Active |
| `sorrel-core` | Rust engine | Active |
| `sorrel-cli` | Persistent local VCS CLI | Active |
| `sorrel-vault` | Secrets / env | Active |
| `sorrel-runners` | Workflow runners + YAML parser | Active |
| `sorrel-slices` | TS/JS slice manifests | Active (prototype) |
| `sorrel-hub` | Collaboration **API server** | Active (dev-only) |
| `sorrel-hub-web` | Hub **web UI** (read/write companion) | Active (dev-only) |
| `sorrel-web` | Public landing (Cloudflare) | Live |
| `sorrel-agents` | Agent control plane (minimal) | Active |
| `sorrel-sdk-js` | Hub HTTP client SDK | Active |
| `sorrel-sdk-rust` | Rust SDK over `sorrel-core` | Active |

Hub is split three ways: `sorrel-hub` (API), `sorrel-hub-web` (product UI),
`sorrel-web` (marketing landing — **not** the Hub UI).

## Toolchains

- Rust **1.85+** with clippy + rustfmt
- Node **22+** for Hub / hub-web; **20+** for protocol / vault / slices

```sh
rustup toolchain install stable --profile minimal -c clippy -c rustfmt
rustup default stable
```

## Common checks

```sh
# Full-stack E2E (no mocks) — from repo root
npm test
npm run test:modules

# Rust modules
cargo test && cargo clippy --all-targets && cargo fmt --all -- --check

# Node modules
npm test
npm run validate   # protocol, vault
```

## Working with submodules

Use the root checkout as one filesystem workspace, but commit changes in the
submodule repository that owns them. After merging each change to that
submodule’s `main`, advance and commit the pointer in this root repository.
See [`AGENTS.md`](AGENTS.md) and
[`docs/AGENT_WORKSPACE.md`](docs/AGENT_WORKSPACE.md).
