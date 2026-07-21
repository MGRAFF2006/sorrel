# Sorrel

Sorrel is an **agent-native version-control system** — a new VCS core built for
modern software work: parallel AI agents, cloud and in-memory workspaces,
first-class permissions and secrets, portable workflows, and shareable slices of
unfinished work. It is not "Git but nicer"; it is a layered system with a
content-addressed object store, changes/lanes/slices, a Core-native
identity/permission/policy spine, a Git bridge (planned), and a collaboration
product on top.

| Doc | Purpose |
| --- | --- |
| [`docs/STATUS.md`](docs/STATUS.md) | **What works / what is missing** |
| [`docs/GETTING_STARTED.md`](docs/GETTING_STARTED.md) | **How to clone and run everything** |
| [`ROADMAP.md`](ROADMAP.md) | Sequenced forward plan |
| [`SORREL_PROGRESS.md`](SORREL_PROGRESS.md) | Live orchestration dashboard |
| [`AGENT_NATIVE_VERSION_CONTROL_REPORT.md`](AGENT_NATIVE_VERSION_CONTROL_REPORT.md) | Full architecture |

The same status and getting-started docs are published on the landing site
(`sorrel-web` → Cloudflare) under `/docs/`.

## Status at a glance (2026-07-21)

**Working today**

- Real engine + persistent CLI: init, status, change, diff, log, lanes, merge, push/pull
- **One-way Git import** (`sorrel git import`) — Git commits → Sorrel snapshots/changes
- Policy/conformance spine across protocol, core, CLI, hub, runners, vault
- Hub API with FS-backed sync + metadata; Hub UI (read-only) + Docker Compose deploy
- Vault, runners, slices prototypes; public landing site live on Cloudflare

**Still missing**

- Git export / colocated mirror
- Merge `resolve` / `--continue`
- Hub write UI and production auth
- Agents control plane, JS/Rust SDKs, embedding surface
- CI on module repos

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
| `sorrel-hub` | Collaboration **API server** | Active |
| `sorrel-hub-web` | Hub **web UI** (read-only) | Active |
| `sorrel-web` | Public landing (Cloudflare) | Live |
| `sorrel-agents` | Agent control plane | Planned |
| `sorrel-sdk-js` | TypeScript/JavaScript SDK | Planned |
| `sorrel-sdk-rust` | Rust SDK | Planned |

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
# Rust modules
cargo test && cargo clippy --all-targets && cargo fmt --all -- --check

# Node modules
npm test
npm run validate   # protocol, vault
```

## Working with submodules

After changing a submodule, merge to that repo’s `main`, then advance and commit
the pointer in this root repository. See [`AGENTS.md`](AGENTS.md).
