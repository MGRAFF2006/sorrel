# Getting started

Last updated: 2026-10-14

How to install Sorrel, record a first local change, host a release server, build
from source, and start the development Hub stack.

The published `v0.1.0-alpha.2` remains a developer preview. The new hosted
login and data-safety fixes described below require a source build. Evaluate
older binaries in disposable checkouts.

## Install the CLI

The verified `v0.1.0-alpha.2` assets provide Linux x64/ARM64, macOS Intel/Apple
Silicon, and Windows x64 binaries. That release has no Windows ARM64 archive
or desktop installers. Check the [actual release assets](https://github.com/MGRAFF2006/sorrel/releases/tag/v0.1.0-alpha.2);
an intended build matrix does not establish download availability.

```sh
# Linux and macOS
curl --proto '=https' --tlsv1.2 -LsSf \
  https://github.com/MGRAFF2006/sorrel/releases/download/v0.1.0-alpha.2/sorrel-cli-installer.sh | sh
```

```powershell
# Windows PowerShell
powershell -ExecutionPolicy Bypass -c "irm https://github.com/MGRAFF2006/sorrel/releases/download/v0.1.0-alpha.2/sorrel-cli-installer.ps1 | iex"
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

Desktop source hosts exist for Windows, macOS, and Linux. The published alpha
does not contain their installers. Check a future release's actual bundles
before following native installation instructions.

The current desktop app is a native companion for a Hub running on
`http://127.0.0.1:3000`; it does not replace the local `sorrel` CLI. Start the
Hub before launching the app:

```sh
cd /path/to/sorrel/sorrel-hub
SORREL_HUB_LOCAL_DEMO=1 SORREL_HUB_BOOTSTRAP_GRANTS=1 npm start
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
SORREL_HUB_LOCAL_DEMO=1 \
SORREL_HUB_BOOTSTRAP_GRANTS=1 \
SORREL_HUB_ALLOW_INSECURE_DEV_AUTH=1 \
docker compose -f sorrel-server.compose.yml up -d
```

This development-auth example is intentionally bound to `127.0.0.1` by the
Compose file. It persists Hub state in the `hub-data` volume and explicitly
provisions broad local demo grants. Use these flags only for an isolated demo.
Without demo mode, private API reads and writes require a verified session and
matching operator-configured native Core grants. See the
[Hub capability matrix](../sorrel-hub/README.md#private-route-capabilities).

For bearer-authenticated API hosting, set `SORREL_HUB_AUTH=oidc` with
`SORREL_OIDC_ISSUER` and `SORREL_OIDC_AUDIENCE`, or configure the documented
WorkOS variables. Configure `SORREL_HUB_TRUSTED_GRANTS_FILE` and optionally
`SORREL_HUB_TRUSTED_POLICIES_FILE` with native Core records; in Compose these
paths refer to files mounted inside the API container. Use a read-only Compose
override mount for operator configuration. Put a TLS reverse proxy in front and set
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

Longer walkthrough: [`sorrel-cli/DEMO.md`](../sorrel-cli/DEMO.md).  
Sync (push/pull) against Hub: [`sorrel-cli/SYNC.md`](../sorrel-cli/SYNC.md).  
Git import/export/sync: [`sorrel-cli/GIT.md`](../sorrel-cli/GIT.md).

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
to the tip. Current source rejects a first colocated import whose files differ
from the selected committed tree unless `--force` explicitly permits discarding
those edits. Selected history containing `.env` provider paths, or provider
files configured in the current source checkout, is rejected before copying
blobs. This does not detect arbitrary credentials committed under other names;
audit history before uploading. Symlinks and submodules are unsupported.
Use `sorrel git export` for one-way export or `sorrel git sync` to
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
[`sorrel-cli/README.md`](../sorrel-cli/README.md) for the complete grant and
workflow examples. Full devenv task mapping, log streaming to Hub, and a hosted
secret backend are not part of this alpha.

## Build from source or run the development Hub

Source builds and the development Hub require Git, Rust 1.92+ with
`clippy` and `rustfmt`, and Node.js 22.11+. Docker or Podman is optional for the
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

This source-built CLI includes `operation log`, named checkpoint
creation/listing/restoration, and `undo`; the published alpha does not.
Checkpoints reference recorded snapshots. Creation and recovery require a clean
working tree with no pending merge; they do not save unfinished or ignored work.
Use the source-built binary in your initialized workspace and follow the
[local recovery guide](../sorrel-cli/README.md#local-operations-checkpoints-and-undo).

The source-built CLI also supports literal file/subtree recording, for example
`sorrel change create -m "Parser fix" --path src/parser.rs --path tests/parser`.
Omitting `--path` records the whole visible workspace. Unselected edits remain
untouched and dirty for a later change; ignore and secret protections still
apply. This is absent from the published alpha and does not provide hunk staging
or isolated lane checkouts. See [selector rules](../sorrel-cli/README.md#workspace-file-selection).

## Hub API + Hub UI

### Hosted pilot from current source

Use one API instance with a persistent volume for the first invite-only pilot.
Docker Compose **2.24.4 or newer** is required for both direct hosted TLS and
Cloudflare Tunnel source deployments; their overlays use `!override`.

The source includes WorkOS browser login and an HTTPS Compose overlay; the
published alpha images do not include these changes.

Register a WorkOS AuthKit application with exact callback
`https://YOUR-HUB-HOST/api/auth/callback` and sign-out URI
`https://YOUR-HUB-HOST/`. Configure invitations/login methods in WorkOS. Point
the chosen hostname at the deployment host; Caddy requires ports 80/443 to serve
HTTPS and obtain certificates. Keep the API/web ports on loopback.

```sh
cp deploy/.env.example deploy/.env
chmod 600 deploy/.env
mkdir -p deploy/config
```

Edit the ignored `.env` with the hostname, matching HTTPS origin, WorkOS API
key/client id, and a random cookie password of at least 32 characters. Keep
credentials in Bitwarden and this private file, never in tracked examples or
issue text. Prepare the source-built images:

```sh
dc() {
  docker compose --env-file deploy/.env \
    -f deploy/sorrel-server.compose.yml -f deploy/sorrel-hosted.compose.yml "$@"
}
dc config --quiet
dc build
```

For an existing Cloudflare Tunnel, add `deploy/sorrel-tunnel.compose.yml` as the
last Compose file in `dc`. The same Compose 2.24.4+ prerequisite applies.
Set `SORREL_TUNNEL_NETWORK` to the tunnel container's existing Docker network
and `SORREL_HUB_DATA_PATH` to a persistent host directory. The overlay disables
Caddy unless the `direct-tls` profile is explicitly selected, attaches only the
web host to the tunnel network and bounds container logs. Add an ingress rule
for the chosen hostname with service `http://sorrel-hub-web:5180`, before the
tunnel's final catch-all. Create its DNS route only after local validation.
Keep Cloudflare caching disabled for `/api/*`, including authentication routes.
See [Cloudflare ingress configuration](https://developers.cloudflare.com/cloudflare-one/networks/connectors/cloudflare-tunnel/do-more-with-tunnels/local-management/configuration-file/)
and [DNS routing](https://developers.cloudflare.com/cloudflare-one/networks/connectors/cloudflare-tunnel/routing-to-tunnel/dns/).

The selected operator pilot is `sorrel.humunkulud.com` on `homelab-services`,
using its existing tunnel. Put data under the services SSD's appdata root,
include it and private configuration in consistent backups, and verify a
restore before invitations. Keep the public HTTPS origin for WorkOS cookies
and callback configuration even though the tunnel-to-web connection uses HTTP.

With Hub stopped, provision an invited WorkOS subject and concrete workspace.
Use the user's WorkOS `user_…` id, not an email address. Omit `--write` to preview
the records first. Repeat for another subject to share the same workspace;
choose different organization/project/repository ids for another team.

```sh
dc run --rm --no-deps --user root \
  -v "$PWD/deploy/config:/operator-config" hub sh -c '
    node scripts/provision-pilot.mjs --subject user_REPLACE \
      --organization org_pilot --project proj_pilot --repository repo_pilot \
      --name "Pilot project" --metadata-dir /app/data/metadata \
      --grants-file /operator-config/grants.json --write &&
    chown -R node:node /app/data /operator-config
  '
dc up -d
```

Provisioning is an operator action. It writes ordinary Core grants for exact
scopes and supplies no policy-administration privilege. WorkOS organization
membership does not grant Sorrel access automatically. Hub mounts operator
configuration read-only; restart it after later provisioning changes.

The browser offers Sign in, then projects shared through Core grants. Cookies
are HttpOnly, Secure, host-only and SameSite=Lax; login uses browser-bound state
and PKCE. AuthKit seals/verifies sessions and refreshes expired access tokens.
Cookie-authenticated writes require the configured origin. Signing out clears
the local cookie even when the identity provider is unavailable; in that case
the UI distinguishes local sign-out from incomplete identity-provider sign-out.

CLI/mobile bearer auth remains available. Automatic CLI login is not yet
implemented: an operator must supply an appropriately scoped AuthKit access
token via `SORREL_HUB_TOKEN`. Never paste credentials into chat or command
arguments. The web host's `/api` URL is also the CLI remote URL.

The overlay builds local `sorrel-hub:pilot` and `sorrel-hub-web:pilot` images from
the checkout. Record its Git revision with deployment/backups. Pending login
transactions belong to one API instance; restarting it requires a new login
attempt. General migrations, quotas and multi-instance coordination are pending.
Hub “Mark as merged” records a human confirmation; it does not merge files.

Complete the live identity, two-team isolation, CLI upload, restart and
backup/restore checks in [RELEASE.md](RELEASE.md) before external invitations.

`deploy/backup-pilot.sh` pauses Hub writes, backs up data and the deployment
checkout (including private configuration) to an existing encrypted Restic
repository, then resumes the containers even after a failed backup. It discovers
the running Hub's `/app/data` mount, covering both the direct-TLS named volume
and the tunnel bind mount. Run it on the Docker host with access to that local
directory. Set `RESTIC_REPOSITORY` and `RESTIC_PASSWORD_FILE`; for the tunnel
overlay also set `SORREL_PILOT_TUNNEL=1`. If `SORREL_HUB_DATA_PATH` is supplied,
it must resolve to the actual mount or backup fails before stopping services.
A shared `SORREL_BACKUP_LOCK`
coordinates with other backups. It refuses to start an intentionally stopped
Hub. Backups retain all pilot snapshots; choose retention after measuring growth.

The optional `deploy/sorrel-backup.service` and `.timer` run at 00:30
Europe/Berlin. Install them through the host's normal systemd configuration;
`/etc/sorrel/pilot-backup.env` supplies the variables above and `SORREL_CHECKOUT`.
Keep it private and store the repository password in the existing password file.
Restore a tagged snapshot into a separate private directory with Restic's
`--verify` option, compare metadata, grants and configuration, and start an
isolated instance against the restored data before trusting recovery. Do not
restore over a live workspace. Off-host backups remain a separate launch check.
Do not run `down --volumes` on data you intend to retain.

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
SORREL_HUB_LOCAL_DEMO=1 SORREL_HUB_BOOTSTRAP_GRANTS=1 npm start  # http://127.0.0.1:3000

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
- `SORREL_HUB_LOCAL_DEMO=1` — explicitly enable anonymous local/acting-header
  identities in development mode; Core grants are still required
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
docker compose --profile convex -f docker-compose.yml -f docker-compose.convex.yml up --build
```

`CONVEX_URL` is the Hub's internal endpoint. Configure a server-only
`CONVEX_SELF_HOSTED_ADMIN_KEY` and deploy the internal functions as described in
[`sorrel-hub/convex/README.md`](../sorrel-hub/convex/README.md). Without the key,
the mirror stays disabled. Product clients and counters use the authenticated
Hub API; no browser Convex URL is required. This profile is an integration
spike, not a production deployment recipe.

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
untrusted network. See [`SECURITY.md`](../SECURITY.md).

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
prefilled. See [`sorrel-hub-mobile/README.md`](../sorrel-hub-mobile/README.md)
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
- [ROADMAP.md](../ROADMAP.md) — sequenced plan
- [CHANGELOG.md](../CHANGELOG.md) — shipped progress
- [AGENT_NATIVE_VERSION_CONTROL_REPORT.md](../AGENT_NATIVE_VERSION_CONTROL_REPORT.md) — design background
