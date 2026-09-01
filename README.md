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

Sorrel is an **agent-native version-control system** — a new VCS core built for
modern software work: parallel AI agents, cloud and in-memory workspaces,
first-class permissions and secrets, portable workflows, and shareable slices of
unfinished work. It is not "Git but nicer"; it is a layered system with a
content-addressed object store, changes/lanes/slices, a Core-native
identity/permission/policy spine, a bidirectional Git bridge, and a collaboration
product on top.

> **Latest release:** [`v0.1.0-alpha.2`](https://github.com/MGRAFF2006/sorrel/releases/tag/v0.1.0-alpha.2)
> is an installable developer preview with downloadable CLI artifacts and
> hostable server images. The Hub still requires production authentication and
> network controls before untrusted network exposure.

| Doc | Purpose |
| --- | --- |
| [GitHub Releases](https://github.com/MGRAFF2006/sorrel/releases) / [`CHANGELOG.md`](CHANGELOG.md) | **Shipped progress and release history** |
| [`docs/STATUS.md`](docs/STATUS.md) | **What works / what is missing** |
| [`docs/GETTING_STARTED.md`](docs/GETTING_STARTED.md) | **How to clone and run everything** |
| [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) | Current system boundaries and data flow |
| [`docs/DEVELOPMENT.md`](docs/DEVELOPMENT.md) | Human and AI contributor workflow |
| [`docs/RELEASE.md`](docs/RELEASE.md) | Version, validation, and tagging process |
| [`SECURITY.md`](SECURITY.md) | Security scope and vulnerability reporting |
| [`ROADMAP.md`](ROADMAP.md) | Sequenced forward plan |
| [`AGENT_NATIVE_VERSION_CONTROL_REPORT.md`](AGENT_NATIVE_VERSION_CONTROL_REPORT.md) | Full architecture |

The public guides and changelog are mirrored onto the landing site
(`sorrel-web` → Cloudflare) under `/docs/`.

## Status at a glance (2026-09-01)

**Working today**

- Real engine + persistent CLI: init, status, change, diff, log, lanes, merge, push/pull
- Git import, export, and colocated bidirectional sync
- Policy/conformance spine across protocol, core, CLI, hub, runners, vault
- SecretSpec-backed secret resolution/injection under Core grants, devenv-aware
  local workflows, and structured redacted run logs
- Development Hub with FS-backed sync + metadata; writable Hub UI companion
- Native Sorrel Hub desktop companion for Windows, macOS, and Linux (x64/ARM64)
- Native Hub companion for iPhone, iPad, Android phones, and Android tablets
- Vault, runners, slices prototypes; public landing site live on Cloudflare
- **Single monorepo** — clone once, no submodule tokens

**Still missing**

- Production Hub authentication and signed client identity
- Stable embedding surface (C ABI / N-API / WASM / daemon)
- Complete devenv task mapping, run-log streaming, and a hosted/BYO secret backend
- Signed desktop distribution and on-device Core embedding for native apps

Details: [`docs/STATUS.md`](docs/STATUS.md).

## Quick start

Install the prebuilt CLI from a release that includes installer assets (replace
`<TAG>` with its tag from the
[Releases page](https://github.com/MGRAFF2006/sorrel/releases)):

```sh
# Linux and macOS
curl --proto '=https' --tlsv1.2 -LsSf \
  https://github.com/MGRAFF2006/sorrel/releases/download/<TAG>/sorrel-cli-installer.sh | sh

# Windows PowerShell
powershell -ExecutionPolicy Bypass -c "irm https://github.com/MGRAFF2006/sorrel/releases/download/<TAG>/sorrel-cli-installer.ps1 | iex"
```

Then start a repository—no source checkout, Rust, or Node.js installation is
required:

```sh
mkdir /tmp/sorrel-demo && cd /tmp/sorrel-demo
sorrel init
echo hello > a.txt
sorrel status
sorrel change create -m "add a.txt"
sorrel log
```

Those releases also provide platform archives, desktop GUI installers, and
SHA-256 checksum files for a manual, auditable installation. Windows ARM64 is
a required target for both CLI and desktop artifacts. Older alpha releases
remain source-only. See
[`docs/GETTING_STARTED.md`](docs/GETTING_STARTED.md) for platform details and
source builds.

Release tags produced by the current pipeline also publish versioned Linux
amd64/arm64 images for the Hub API and browser host. Download the attached
`sorrel-server.compose.yml`, set `SORREL_VERSION` to the tag without its `v`,
and start the stack without a source checkout or Node.js installation:

```sh
SORREL_VERSION=<VERSION> \
SORREL_HUB_AUTH=dev \
SORREL_HUB_ALLOW_INSECURE_DEV_AUTH=1 \
docker compose -f sorrel-server.compose.yml up -d
```

The release Compose file binds to `127.0.0.1` by default and does not enable
broad bootstrap grants. Development auth remains suitable only for a trusted,
isolated environment; see the [deployment guide](docs/GETTING_STARTED.md#host-a-release-server)
before changing the bind address or auth mode.

To run the development Hub stack from a source checkout instead:

```sh
git clone https://github.com/MGRAFF2006/sorrel.git
cd sorrel
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
| `sorrel-hub-ui` | Shared SolidJS Hub product UI | Active (dev-only) |
| `sorrel-hub-desktop` | Native Tauri host for Hub UI | Active (local-Hub companion) |
| `sorrel-hub-web` | Thin browser host for Hub UI | Active (dev-only) |
| `sorrel-hub-mobile` | Native phone/tablet Hub companion | Active (dev-only) |
| `sorrel-web` | Public landing (Cloudflare) | Live |
| `sorrel-agents` | Agent control plane (minimal) | Active |
| `sorrel-sdk-js` | Hub HTTP client SDK | Active |
| `sorrel-sdk-rust` | Rust SDK over `sorrel-core` | Active |

Hub is split across `sorrel-hub` (API), `sorrel-hub-ui` (shared product UI),
`sorrel-hub-desktop` (native host), `sorrel-hub-web` (browser host), and
`sorrel-hub-mobile` (native phone/tablet companion); `sorrel-web` is the
marketing landing — **not** the Hub UI.

Rust crates form one Cargo workspace (`Cargo.toml` at the root). Brand assets
used by this README live under [`assets/`](assets/).

## Toolchains

- Rust **1.85+** with clippy + rustfmt
- Node **22+** for Hub / hub-web; **20+** for protocol / vault / slices

```sh
rustup toolchain install stable --profile minimal -c clippy -c rustfmt
rustup default stable
```

## Common checks

```sh
# Fresh checkout
npm run setup

# Fast consistency checks / full repository gate
npm run check:quick
npm run check

# Full-stack E2E (no mocks) — from repo root
npm test
npm run test:modules

# Focused package suite
npm run test:module -- sorrel-hub

# Rust workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```
