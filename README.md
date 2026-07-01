# Sorrel

Sorrel is an **agent-native version-control system** — a new VCS core built for
modern software work: parallel AI agents, cloud and in-memory workspaces,
first-class permissions and secrets, portable workflows, and shareable slices of
unfinished work. It is not "Git but nicer"; it is a layered system with a
content-addressed object store, changes/lanes/slices, a Core-native
identity/permission/policy spine, a Git bridge, and a collaboration product on
top.

See [`AGENT_NATIVE_VERSION_CONTROL_REPORT.md`](AGENT_NATIVE_VERSION_CONTROL_REPORT.md)
for the full architecture, and [`SORREL_PROTOTYPE_PLAN.md`](SORREL_PROTOTYPE_PLAN.md)
for the current build plan. Live orchestration status lives in
[`SORREL_PROGRESS.md`](SORREL_PROGRESS.md).

## Repository layout

This root repository coordinates architecture and submodule pointers. Most
implementation lives in submodules:

| Submodule          | Role                                                                 | Status |
| ------------------ | -------------------------------------------------------------------- | ------ |
| `sorrel-protocol`  | Canonical schemas, examples, and the policy-conformance manifest     | Active |
| `sorrel-core`      | Rust engine: object store, snapshots, changes, lanes, policy spine   | Active |
| `sorrel-cli`       | The `sorrel` CLI (persistent local VCS over the engine)              | Active |
| `sorrel-vault`     | Secrets/environment spec, local backend, and dev CLI                 | Active |
| `sorrel-runners`   | Portable workflow runners + `sorrel.workflow.yml` parser             | Active |
| `sorrel-slices`    | TS/JS slice (subproject) manifest generator                          | Active |
| `sorrel-hub`       | Collaboration **API server** (JSON over HTTP)                        | Active |
| `sorrel-hub-web`   | Hub **web interface** (browser frontend for the Hub API)             | Active |
| `sorrel-web`       | Public marketing/landing site (static)                               | Active |
| `sorrel-agents`    | Agent control plane (lanes, claims, policy overlays)                 | Planned |
| `sorrel-sdk-js`    | TypeScript/JavaScript SDK                                            | Planned |
| `sorrel-sdk-rust`  | Rust SDK                                                             | Planned |

### Hub: API vs web interface

Hub is split across three repos so the marketing site, product backend, and
product UI stay independent:

- `sorrel-hub` — the **API server** (no UI).
- `sorrel-hub-web` — the **web interface** that calls the Hub API.
- `sorrel-web` — the unrelated **public landing page**.

## Try the prototype

The CLI demonstrates a persistent, single-user local VCS flow today:

```sh
# from sorrel-cli/
cargo build
target/debug/sorrel init
echo hello > a.txt
target/debug/sorrel status          # dirty: added a.txt
target/debug/sorrel change create -m "add a"
target/debug/sorrel diff
target/debug/sorrel log
```

See `sorrel-cli/DEMO.md` for the full walkthrough.

## Toolchains

Rust modules require **stable Rust 1.85+** (edition2024-era Cargo metadata) with
clippy and rustfmt:

```sh
rustup toolchain install stable --profile minimal -c clippy -c rustfmt
rustup default stable
```

Node modules require **Node 20+** (Hub and hub-web require Node 22+).

## Common checks

Rust repos/workspaces:

```sh
cargo build
cargo test
cargo clippy --all-targets
cargo fmt --all -- --check
```

Node modules:

```sh
npm test
npm run lint        # where present
npm run validate    # protocol/vault
```

## Working with submodules

Some submodules are private. **Agents:** use this root repo as a single workspace
— edit `sorrel-cli/`, `sorrel-core/`, etc. in place. See
[`docs/AGENT_WORKSPACE.md`](docs/AGENT_WORKSPACE.md). After merging submodule
changes to each repo's `main`, update and commit the parent submodule pointer.
See `AGENTS.md` for toolchain and checks.
