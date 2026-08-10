# Getting started

Last updated: 2026-08-10

How to clone Sorrel, run the local VCS demo, and start the Hub stack.

## Prerequisites

- **Git**
- **Rust** stable 1.85+ with clippy + rustfmt
- **Node.js** 22+ (Hub / hub-web); Node 20+ is enough for protocol/vault/slices
- **Docker** (optional) for `docker compose` Hub + Hub UI preview

```sh
rustup toolchain install stable --profile minimal -c clippy -c rustfmt
rustup default stable
```

## Clone

```sh
git clone https://github.com/MGRAFF2006/sorrel.git
cd sorrel
```

Sorrel is a single monorepo. No submodules and no `SUBMODULES_TOKEN` are required.

## Local VCS happy path (CLI)

```sh
cargo build -p sorrel-cli
SORREL=target/debug/sorrel

mkdir /tmp/sorrel-demo && cd /tmp/sorrel-demo
$SORREL init
echo hello > a.txt
$SORREL status
$SORREL change create -m "add a.txt"
$SORREL diff          # after more edits
$SORREL log

# Lanes + merge
$SORREL lane create --name feature
$SORREL lane list
# switch using the lane id from `lane list`
$SORREL lane switch <feature-lane-id>
echo feature > b.txt
$SORREL change create -m "add b"
$SORREL lane switch lane_main
$SORREL merge <feature-lane-id>
```

Longer walkthrough: [`sorrel-cli/DEMO.md`](../sorrel-cli/DEMO.md).  
Sync (push/pull) against Hub: [`sorrel-cli/SYNC.md`](../sorrel-cli/SYNC.md).  
Git import/export/sync: [`sorrel-cli/GIT.md`](../sorrel-cli/GIT.md).

## Import an existing Git repo

```sh
cd /path/to/git-checkout
# from a built CLI:
/path/to/sorrel/target/debug/sorrel git import
# optional: --ref main --limit 20 --json
sorrel log
sorrel status
```

Creates `.sorrel/` if needed, imports commits reachable from `HEAD` as Sorrel
snapshots/changes, writes `.sorrel/git-map.json`, and restores the working tree
to the tip. Use `sorrel git export` for one-way export or `sorrel git sync` to
incrementally keep colocated Git and Sorrel histories aligned. Divergence is
parked on a normal `git/<branch>` lane for explicit merge resolution.

## Hub API + Hub UI

### One-command local dashboard (recommended for testing)

From the repo root:

```sh
npm run dev
# or: node scripts/dev-dashboard.mjs
```

This builds the CLI, seeds `.dev/workspace`, starts Hub (:3000) + Hub UI
(:5180) with bootstrap grants and FS persistence under `.dev/hub-data`, then
opens a status dashboard at http://127.0.0.1:5200 with health checks and
copy-paste commands. Stop with Ctrl+C.

Flags: `--skip-build`, `--no-open`, `--no-seed`, `--port 5200`.

### With Docker Compose (from repo root)

```sh
docker compose up --build
```

If a preview port is occupied, override it with `SORREL_HUB_PORT`,
`SORREL_HUB_WEB_PORT`, or `SORREL_WEB_PORT` before running Compose.

| Service | URL |
| --- | --- |
| Hub API | http://localhost:3000 |
| Hub UI | http://localhost:5180 |
| Landing (local preview only) | http://localhost:4173 |

Production landing is already on **Cloudflare Pages** from `sorrel-web`. The
compose `web` service is an optional local mirror; it does not replace Cloudflare.

### Without Docker

```sh
# terminal 1 — API (explicit insecure local bootstrap for the demo)
cd sorrel-hub
SORREL_HUB_BOOTSTRAP_GRANTS=1 npm start  # http://127.0.0.1:3000

# terminal 2 — UI (proxies /api → Hub)
cd sorrel-hub-web
npm start          # http://0.0.0.0:5180
```

Useful Hub env vars:

- `SORREL_HUB_DATA_DIR` — sync store (default `./data/sync`)
- `SORREL_HUB_METADATA_DIR` — product metadata (default `./data/metadata`)
- `HOST` — listen address (default `127.0.0.1`; use `0.0.0.0` only in an
  isolated container/network)
- `SORREL_HUB_BOOTSTRAP_GRANTS=1` — explicitly enable broad local demo grants
  (disabled by default)
- `HUB_API_URL` — hub-web upstream (default `http://localhost:3000`)

## CLI ↔ Hub sync

With Hub listening on port 3000:

```sh
cd /tmp/sorrel-demo   # an initialized workspace
$SORREL remote add origin http://127.0.0.1:3000
$SORREL push origin

# elsewhere
mkdir /tmp/sorrel-pull && cd /tmp/sorrel-pull
$SORREL init
$SORREL remote add origin http://127.0.0.1:3000 --repo-id <repoId-from-source>
$SORREL pull origin   # downloads objects and restores the working tree
```

The alpha Hub has no production authentication. Do not expose it to an
untrusted network. See [`SECURITY.md`](../SECURITY.md).

## Validate modules

From the repo root, run the full-stack E2E (real Hub + CLI + vault + slices +
hub-web + SDKs + agents — **no mocks**):

```sh
npm test              # tests/e2e/happy-path.mjs
npm run test:modules  # each package's own suite
```

Rust (`sorrel-core`, `sorrel-cli`, `sorrel-runners`, `sorrel-sdk-rust`):

```sh
cargo test
cargo clippy --all-targets
cargo fmt --all -- --check
```

Node (`sorrel-protocol`, `sorrel-hub`, `sorrel-hub-web`, `sorrel-vault`,
`sorrel-slices`, `sorrel-sdk-js`, `sorrel-agents`):

```sh
npm test
npm run validate   # where defined (protocol, vault)
```

## Landing site

- **Production:** Cloudflare Pages → publish `sorrel-web` root (no build).
- **Local preview:** `cd sorrel-web && python3 -m http.server 4173`
- **Docs on the site:** [/docs/](/docs/) (markdown under `sorrel-web/docs/`)

## More reading

- [STATUS.md](STATUS.md) — working vs missing
- [ROADMAP.md](../ROADMAP.md) — sequenced plan
- [AGENT_NATIVE_VERSION_CONTROL_REPORT.md](../AGENT_NATIVE_VERSION_CONTROL_REPORT.md) — architecture
