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
| [`docs/RELEASE.md`](docs/RELEASE.md) | Version, validation, and tagging process |
| [`SECURITY.md`](SECURITY.md) | Security scope and vulnerability reporting |
| [`ROADMAP.md`](ROADMAP.md) | Sequenced forward plan |
| [`AGENT_NATIVE_VERSION_CONTROL_REPORT.md`](AGENT_NATIVE_VERSION_CONTROL_REPORT.md) | Full architecture |

The same status and getting-started docs are published on the landing site
(`sorrel-web` → Cloudflare) under `/docs/`.

## Status at a glance (2026-08-10)

**Working today**

- Real engine + persistent CLI: init, status, change, diff, log, lanes, merge, push/pull
- Git import, export, and colocated bidirectional sync
- Policy/conformance spine across protocol, core, CLI, hub, runners, vault
- Development Hub with FS-backed sync + metadata; writable Hub UI companion
- Vault, runners, slices prototypes; public landing site live on Cloudflare
- **Single monorepo** — clone once, no submodule tokens

**Still missing**

- Production Hub authentication and signed client identity
- Stable embedding surface (C ABI / N-API / WASM / daemon)
- SecretSpec-backed resolve/inject under Core grants (in progress)
- devenv-first runners + Blacksmith-grade execution logs (planned)
- Production Hub authentication and desktop/mobile applications

Details: [`docs/STATUS.md`](docs/STATUS.md).

## Quick start

```sh
git clone https://github.com/MGRAFF2006/sorrel.git
cd sorrel

# Local VCS demo
cargo build -p sorrel-cli
SORREL="$(pwd)/target/debug/sorrel"
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

Everything lives in this repository as normal packages:

| Package | Role | Maturity |
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

Rust crates form one Cargo workspace (`Cargo.toml` at the root).

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

# Rust workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```
