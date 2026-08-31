<p align="center">
  <img src="assets/logo.svg" alt="Sorrel" width="112" height="112">
</p>

<h1 align="center">Sorrel</h1>

<p align="center">
  <strong>Agent-native version control</strong> for humans and parallel AI agents.
</p>

<p align="center">
  <a href="https://github.com/MGRAFF2006/sorrel/releases"><img alt="Release" src="https://img.shields.io/github/v/release/MGRAFF2006/sorrel?include_prereleases&amp;style=flat-square&amp;color=5E81AC"></a>
  <a href="LICENSE-MIT"><img alt="License: MIT" src="https://img.shields.io/badge/license-MIT-A3BE8C?style=flat-square"></a>
  <a href="LICENSE-APACHE"><img alt="License: Apache-2.0" src="https://img.shields.io/badge/license-Apache%202.0-81A1C1?style=flat-square"></a>
  <a href="sorrel-web/"><img alt="Landing" src="https://img.shields.io/badge/landing-sorrel--web-88C0D0?style=flat-square"></a>
</p>

<p align="center">
  <a href="docs/GETTING_STARTED.md">Get started</a>
  ·
  <a href="docs/STATUS.md">Status</a>
  ·
  <a href="https://github.com/MGRAFF2006/sorrel/releases">Releases</a>
  ·
  <a href="ROADMAP.md">Roadmap</a>
</p>

---

Sorrel is a new VCS core — not “Git but nicer.” It is a layered system with a
content-addressed object store, changes / lanes / slices, a Core-native
identity and policy spine, a bidirectional Git bridge, and a collaboration
product on top. Built for parallel agents, portable workflows, first-class
secrets, and shareable unfinished work.

> **Latest preview:** [`v0.1.0-alpha.1`](https://github.com/MGRAFF2006/sorrel/releases)
> is local-first. The Hub is for localhost development only and does **not**
> provide production authentication.

## Why Sorrel

| Problem | Sorrel approach |
| --- | --- |
| Many agents, one working tree | Changes, lanes, and slices as first-class units of work |
| Permissions bolted on later | Policy / grants in the Core spine |
| Secrets leaking into logs | SecretSpec resolution under Core grants + redaction |
| “Just use Git” forever | Incremental Git import / export / colocated sync |
| Collaboration as an afterthought | Hub API + writable UI companion (dev-only today) |

## Status at a glance

**Working today**

- Real engine + persistent CLI: `init`, `status`, `change`, `diff`, `log`, lanes, merge, push / pull
- Git import, export, and colocated bidirectional sync
- Policy / conformance spine across protocol, core, CLI, hub, runners, vault
- Development Hub with FS-backed sync + metadata; writable Hub UI companion
- Vault, runners, slices prototypes; public landing site on Cloudflare
- Single monorepo — clone once, no submodule tokens

**Still missing**

- Production Hub authentication and signed client identity
- Stable embedding surface (C ABI / N-API / WASM / daemon)
- Full devenv task mapping, run-log streaming, hosted / BYO secret backend
- Desktop / mobile applications

Details: [`docs/STATUS.md`](docs/STATUS.md). Progress lives in
[GitHub Releases](https://github.com/MGRAFF2006/sorrel/releases) and
[`CHANGELOG.md`](CHANGELOG.md).

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

Hub is split three ways: `sorrel-hub` (API), `sorrel-hub-web` (product UI), and
`sorrel-web` (marketing landing — **not** the Hub UI).

Rust crates form one Cargo workspace (`Cargo.toml` at the root). Brand assets
used by this README live under [`assets/`](assets/).

## Docs

| Doc | Purpose |
| --- | --- |
| [`docs/STATUS.md`](docs/STATUS.md) | What works / what is missing |
| [`docs/GETTING_STARTED.md`](docs/GETTING_STARTED.md) | Clone and run everything |
| [`docs/RELEASE.md`](docs/RELEASE.md) | Version, validation, tagging |
| [`SECURITY.md`](SECURITY.md) | Security scope and reporting |
| [`ROADMAP.md`](ROADMAP.md) | Sequenced forward plan |
| [`AGENT_NATIVE_VERSION_CONTROL_REPORT.md`](AGENT_NATIVE_VERSION_CONTROL_REPORT.md) | Full architecture write-up |

Canonical status and getting-started guides are mirrored onto the landing site
under `/docs/`.

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
## License

Dual-licensed under [MIT](LICENSE-MIT) and [Apache-2.0](LICENSE-APACHE).
