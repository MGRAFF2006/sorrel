<!-- Generated from docs/GETTING_STARTED.md by npm run sync:docs. Do not edit. -->

# Getting started

Last updated: 2026-08-31

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

Install the locked Node dependencies for every package and prefetch the Rust
workspace dependencies:

```sh
npm run setup
```

## Local VCS happy path (CLI)

```sh
cargo build -p sorrel-cli
SORREL="$(pwd)/target/debug/sorrel"

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

Longer walkthrough: [`sorrel-cli/DEMO.md`](https://github.com/MGRAFF2006/sorrel/blob/main/sorrel-cli/DEMO.md).  
Sync (push/pull) against Hub: [`sorrel-cli/SYNC.md`](https://github.com/MGRAFF2006/sorrel/blob/main/sorrel-cli/SYNC.md).  
Git import/export/sync: [`sorrel-cli/GIT.md`](https://github.com/MGRAFF2006/sorrel/blob/main/sorrel-cli/GIT.md).

## Import an existing Git repo

```sh
cd /path/to/git-checkout
SORREL=/path/to/sorrel/target/debug/sorrel
$SORREL git import
# optional: --ref main --limit 20 --json
$SORREL log
$SORREL status
```

Creates `.sorrel/` if needed, imports commits reachable from `HEAD` as Sorrel
snapshots/changes, writes `.sorrel/git-map.json`, and restores the working tree
to the tip. Use `sorrel git export` for one-way export or `sorrel git sync` to
incrementally keep colocated Git and Sorrel histories aligned. Divergence is
parked on a normal `git/<branch>` lane for explicit merge resolution.

## Secrets, environments, and run logs

Secret values never enter Sorrel objects. The CLI discovers `SecretRef`
declarations, uses upstream SecretSpec providers after Core grant checks, and
redacts persisted workflow output:

```sh
$SORREL secret list               # handles only, never values
$SORREL secret sync               # generate/refresh secretspec.toml
$SORREL secret check --provider dotenv:.env
$SORREL env info                  # devenv when available, local fallback otherwise
$SORREL workflow run test
$SORREL run list
$SORREL run show <run-id>
$SORREL run logs <run-id>
```

Resolution, `secret set`, and injection require scoped `secret.read` and/or
`secret.inject` grants. Prefer stdin over `--value` in real scripts, keep
provider files ignored, and set `SECRETSPEC_REASON` when you need an audit
reason more specific than Sorrel's operation default. See
[`sorrel-cli/README.md`](https://github.com/MGRAFF2006/sorrel/blob/main/sorrel-cli/README.md) for the complete grant and
workflow examples. Full devenv task mapping, log streaming to Hub, and a hosted
secret backend are not part of this alpha.

## Hub API + Hub UI

### One-command local dashboard (recommended for testing)

From the repo root:

```sh
npm run dev
# or: node scripts/dev-dashboard.mjs
```

This builds the CLI and Hub UI, seeds `.dev/workspace`, starts Hub (:3000) + Hub UI
(:5180) with bootstrap grants and FS persistence under `.dev/hub-data`, then
opens a status dashboard at http://127.0.0.1:5200 with health checks and
copy-paste commands. Stop with Ctrl+C.

Flags: `--skip-build` (reuse existing builds), `--no-open`, `--no-seed`,
`--port 5200`.

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
npm run dev        # http://127.0.0.1:5180
# or: npm run build && npm start
```

Useful Hub env vars:

- `SORREL_HUB_DATA_DIR` — sync store (default `./data/sync`)
- `SORREL_HUB_METADATA_DIR` — product metadata (default `./data/metadata`)
- `HOST` — listen address (default `127.0.0.1`; use `0.0.0.0` only in an
  isolated container/network)
- `SORREL_HUB_BOOTSTRAP_GRANTS=1` — explicitly enable broad local demo grants
  (disabled by default)
- `SORREL_HUB_AUTH` — `dev` (default), `workos`, or `oidc`; production login
  and sealed WorkOS sessions are not complete in this alpha
- `SORREL_HUB_ALLOW_INSECURE_DEV_AUTH=1` — permit dev auth/bootstrap grants on
  a non-loopback bind for an isolated demo only
- `HUB_API_URL` — hub-web upstream (default `http://localhost:3000`)

### Optional Convex metadata profile

The filesystem store remains authoritative for VCS objects. To exercise the
optional Convex metadata mirror, configure the documented Convex environment
variables and layer the profile over the normal stack:

```sh
docker compose -f docker-compose.yml -f docker-compose.convex.yml up --build
```

`CONVEX_URL` is the Hub's internal endpoint; `CONVEX_PUBLIC_URL` is the
browser-visible endpoint injected into the web build. This profile is an
integration spike, not a production deployment recipe.

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
untrusted network. See [`SECURITY.md`](https://github.com/MGRAFF2006/sorrel/blob/main/SECURITY.md).

## Validate modules

From the repo root, run the full-stack E2E (real Hub + CLI + vault + slices +
hub-web + SDKs + agents — **no mocks**):

```sh
npm run check:quick   # release metadata, conformance, docs and links
npm run test:module -- sorrel-hub  # focused package suite
npm test              # tests/e2e/happy-path.mjs
npm run test:modules  # each package's own suite
npm run check         # complete repository gate
```

Rust (`sorrel-core`, `sorrel-cli`, `sorrel-runners`, `sorrel-sdk-rust`):

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

Node (`sorrel-protocol`, `sorrel-hub`, `sorrel-hub-ui`, `sorrel-hub-web`,
`sorrel-vault`, `sorrel-slices`, `sorrel-sdk-js`, `sorrel-agents`). Run each
package's complete local gate:

```sh
npm test
npm run check
```

## Landing site

- **Production:** Cloudflare Pages → publish `sorrel-web` root (no build).
- **Local preview:** `cd sorrel-web && python3 -m http.server 4173`
- **Docs on the site:** [/docs/](/docs/) (markdown under `sorrel-web/docs/`)

## More reading

- [STATUS.md](STATUS.md) — working vs missing
- [ARCHITECTURE.md](ARCHITECTURE.md) — package boundaries and data flow
- [DEVELOPMENT.md](DEVELOPMENT.md) — contributor and AI-agent workflow
- [RELEASE.md](RELEASE.md) — changelogs, tags, and publication
- [ROADMAP.md](https://github.com/MGRAFF2006/sorrel/blob/main/ROADMAP.md) — sequenced plan
- [CHANGELOG.md](https://github.com/MGRAFF2006/sorrel/blob/main/CHANGELOG.md) — shipped progress
- [AGENT_NATIVE_VERSION_CONTROL_REPORT.md](https://github.com/MGRAFF2006/sorrel/blob/main/AGENT_NATIVE_VERSION_CONTROL_REPORT.md) — design background
