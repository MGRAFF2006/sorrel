<!-- Generated from docs/GETTING_STARTED.md by npm run sync:docs. Do not edit. -->

# Getting started

Last updated: 2026-09-01

How to install Sorrel, record a first local change, host a release server, build
from source, and start the development Hub stack.

## Install the CLI

Sorrel releases that include installer assets provide prebuilt binaries for
Linux x64/ARM64, macOS Intel/Apple Silicon, and Windows x64/ARM64. Older alpha
releases remain source-only. Choose an installer-bearing tag on the
[Releases page](https://github.com/MGRAFF2006/sorrel/releases), then replace
`<TAG>` below with it.

```sh
# Linux and macOS
curl --proto '=https' --tlsv1.2 -LsSf \
  https://github.com/MGRAFF2006/sorrel/releases/download/<TAG>/sorrel-cli-installer.sh | sh
```

```powershell
# Windows PowerShell
powershell -ExecutionPolicy Bypass -c "irm https://github.com/MGRAFF2006/sorrel/releases/download/<TAG>/sorrel-cli-installer.ps1 | iex"
```

The installers select the correct archive, verify its SHA-256 checksum, install
`sorrel` in the Cargo-style user binary directory, and explain any required
`PATH` update. They do not install Rust. For environments that disallow piping
a downloaded script into a shell, download the platform archive and adjacent
`.sha256` file from the release, verify it with the platform SHA-256 tool, and
extract `sorrel` into a directory on `PATH`.

Confirm the installation:

```sh
sorrel --version
sorrel --help
```

## Install the desktop companion

The same installer-bearing releases include Sorrel Hub desktop bundles for
Windows x64/ARM64, macOS Intel/Apple Silicon, and Linux x64/ARM64. Download the
`.msi`/setup executable, `.dmg`, `.AppImage`, or distribution package matching
your machine from the release assets.

The current desktop app is a native companion for a Hub running on
`http://127.0.0.1:3000`; it does not replace the local `sorrel` CLI. Start the
Hub before launching the app:

```sh
cd /path/to/sorrel/sorrel-hub
SORREL_HUB_BOOTSTRAP_GRANTS=1 npm start
```

Installer signing/notarization is not yet configured for this developer
preview, so operating systems may show their standard unverified-publisher
warning. Verify release checksums before installation.

## Host a release server

Releases produced by the current pipeline publish two OCI images for Linux
amd64 and arm64:

- `ghcr.io/mgraff2006/sorrel-hub:<VERSION>` — persistent Hub API and sync
  server.
- `ghcr.io/mgraff2006/sorrel-hub-web:<VERSION>` — browser UI host and `/api`
  proxy.

Each GitHub Release includes `sorrel-server.compose.yml`, immutable image
digests in `sorrel-server-images.txt`, and
`sorrel-server-assets.sha256`. Images run as an unprivileged user, include
health checks, and carry build provenance and an SBOM. Replace `<TAG>` with the
release tag and `<VERSION>` with the same value without the leading `v`:

```sh
curl --proto '=https' --tlsv1.2 -LO \
  https://github.com/MGRAFF2006/sorrel/releases/download/<TAG>/sorrel-server.compose.yml
curl --proto '=https' --tlsv1.2 -LO \
  https://github.com/MGRAFF2006/sorrel/releases/download/<TAG>/sorrel-server-images.txt
curl --proto '=https' --tlsv1.2 -LO \
  https://github.com/MGRAFF2006/sorrel/releases/download/<TAG>/sorrel-server-assets.sha256
sha256sum --check sorrel-server-assets.sha256

SORREL_VERSION=<VERSION> \
SORREL_HUB_AUTH=dev \
SORREL_HUB_ALLOW_INSECURE_DEV_AUTH=1 \
docker compose -f sorrel-server.compose.yml up -d
```

This development-auth example is intentionally bound to `127.0.0.1` by the
Compose file. It persists Hub state in the `hub-data` volume and leaves broad
bootstrap grants disabled. Set `SORREL_HUB_BOOTSTRAP_GRANTS=1` only for an
isolated CLI sync demo.

For bearer-authenticated API hosting, set `SORREL_HUB_AUTH=oidc` with
`SORREL_OIDC_ISSUER` and `SORREL_OIDC_AUDIENCE`, or configure the documented
WorkOS variables. Put a TLS reverse proxy in front and set
`SORREL_BIND_ADDRESS` only after the network boundary is in place. The alpha
browser UI does not yet implement an IdP login flow, sealed WorkOS sessions are
not shipped, and no alpha Hub should be treated as a production security
boundary.

Pinning the `image@sha256:...` references from `sorrel-server-images.txt`
instead of version tags makes a deployment byte-for-byte immutable. Back up the
`hub-data` volume before changing versions because general Hub data migrations
are not available yet.

## Record your first change

```sh
mkdir /tmp/sorrel-demo && cd /tmp/sorrel-demo
sorrel init
echo hello > a.txt
sorrel status
sorrel change create -m "add a.txt"
sorrel diff          # after more edits
sorrel log

# Lanes + merge
sorrel lane create --name feature
sorrel lane list
# switch using the lane id from `lane list`
sorrel lane switch <feature-lane-id>
echo feature > b.txt
sorrel change create -m "add b"
sorrel lane switch lane_main
sorrel merge <feature-lane-id>
```

Longer walkthrough: [`sorrel-cli/DEMO.md`](https://github.com/MGRAFF2006/sorrel/blob/main/sorrel-cli/DEMO.md).  
Sync (push/pull) against Hub: [`sorrel-cli/SYNC.md`](https://github.com/MGRAFF2006/sorrel/blob/main/sorrel-cli/SYNC.md).  
Git import/export/sync: [`sorrel-cli/GIT.md`](https://github.com/MGRAFF2006/sorrel/blob/main/sorrel-cli/GIT.md).

## Import an existing Git repo

```sh
cd /path/to/git-checkout
sorrel git import
# optional: --ref main --limit 20 --json
sorrel log
sorrel status
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
sorrel secret list               # handles only, never values
sorrel secret sync               # generate/refresh secretspec.toml
sorrel secret check --provider dotenv:.env
sorrel env info                  # devenv when available, local fallback otherwise
sorrel workflow run test
sorrel run list
sorrel run show <run-id>
sorrel run logs <run-id>
```

Resolution, `secret set`, and injection require scoped `secret.read` and/or
`secret.inject` grants. Prefer stdin over `--value` in real scripts, keep
provider files ignored, and set `SECRETSPEC_REASON` when you need an audit
reason more specific than Sorrel's operation default. See
[`sorrel-cli/README.md`](https://github.com/MGRAFF2006/sorrel/blob/main/sorrel-cli/README.md) for the complete grant and
workflow examples. Full devenv task mapping, log streaming to Hub, and a hosted
secret backend are not part of this alpha.

## Build from source or run the development Hub

Source builds and the development Hub require Git, Rust stable 1.85+ with
`clippy` and `rustfmt`, and Node.js 22+. Docker or Podman is optional for the
container preview.

```sh
git clone https://github.com/MGRAFF2006/sorrel.git
cd sorrel
rustup toolchain install stable --profile minimal -c clippy -c rustfmt
npm run setup
cargo build --release -p sorrel-cli
./target/release/sorrel --version
```

Sorrel is a single monorepo. No submodules or private dependency tokens are
required.

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
sorrel remote add origin http://127.0.0.1:3000
sorrel push origin

# elsewhere
mkdir /tmp/sorrel-pull && cd /tmp/sorrel-pull
sorrel init
sorrel remote add origin http://127.0.0.1:3000 --repo-id <repoId-from-source>
sorrel pull origin   # downloads objects and restores the working tree
```

The alpha Hub has no production authentication. Do not expose it to an
untrusted network. See [`SECURITY.md`](https://github.com/MGRAFF2006/sorrel/blob/main/SECURITY.md).

## Run the mobile Hub companion

The source app targets iPhone, iPad, Android phones, and Android tablets:

```sh
cd sorrel-hub-mobile
npm ci
npm start         # scan with Expo Go on a physical device
npm run ios       # macOS + Xcode simulator/device
npm run android   # Android emulator/device
```

Enter a Hub origin the device can reach, without `/api`. Remote deployments
should use HTTPS. Plain HTTP is reserved for a trusted development network and
requires the Hub's explicit non-loopback development-auth opt-in. Optional OIDC
bearer credentials are stored in iOS Keychain or Android Keystore and are never
prefilled. See [`sorrel-hub-mobile/README.md`](https://github.com/MGRAFF2006/sorrel/blob/main/sorrel-hub-mobile/README.md)
for navigation, tablet, and EAS build details.

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
`sorrel-hub-mobile`, `sorrel-vault`, `sorrel-slices`, `sorrel-sdk-js`, `sorrel-agents`). Run each
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
