# Sorrel CLI — Sync with sorrel-hub

This document covers pushing and pulling Sorrel snapshots over the HTTP sync
transport against [`sorrel-hub`](../sorrel-hub/).
For the local-only persistent workflow, see [`DEMO.md`](DEMO.md).

## Prerequisites

1. **sorrel-hub** running locally (default `http://127.0.0.1:3000`).
2. A Sorrel workspace initialized in your project (`sorrel init`).
3. The monorepo dependencies installed with `npm run setup`; the CLI consumes
   `sorrel-core` through the workspace path dependency.

## Build

```bash
cargo build
SORREL=target/debug/sorrel
```

## 1. Start sorrel-hub

From the monorepo root:

```bash
cd sorrel-hub && npm start
# listening on http://127.0.0.1:3000
```

Create or note the hub repository id (must match your local `manifest.json`
`repoId`, or pass `--repo-id` explicitly when adding the remote).

## 2. Add a remote

```bash
$SORREL remote add origin http://127.0.0.1:3000
# Added remote origin -> http://127.0.0.1:3000 (repo_<hex>)
```

When the local and hub repo ids differ, pass `--repo-id`:

```bash
$SORREL remote add origin http://127.0.0.1:3000 --repo-id repo_hub_abc123
```

List configured remotes:

```bash
$SORREL remote list
# origin  http://127.0.0.1:3000  repo_<hex>

$SORREL remote list --json
```

Remotes persist in `.sorrel/remotes.json`:

```json
{
  "remotes": {
    "origin": {
      "url": "http://127.0.0.1:3000",
      "repoId": "repo_<hex>"
    }
  }
}
```

## 3. Record local changes

```bash
echo "hello" > hello.txt
$SORREL change create -m "add hello.txt"
```

## 4. Push to the hub

```bash
$SORREL push
# Pushed <snapshot> to origin/HEAD (N object(s))

$SORREL push --json
```

Push flow (per sync-transport spec):

1. `GET /{repoId}/refs` — read remote `HEAD` (and `grantRefs` when present)
2. `POST /{repoId}/objects/missing` with `want` = `[localSnapshot]`, `have` = remote object ids
3. `POST /{repoId}/objects` — upload missing objects from `.sorrel/objects/`
4. `POST /{repoId}/refs/HEAD` — advance remote ref

Mutating calls include `x-sorrel-acting-principal: {"type":"user","id":"local"}`
and `grantRefs` for Hub's local bootstrap grants:

```json
"grantRefs": [{ "id": "grant_local_object_write", "source": "core" }]
```

(object upload) / `grant_local_ref_write` (ref advance). Hub enables these
bootstrap grants by default (`SORREL_HUB_BOOTSTRAP_GRANTS`, see sorrel-hub
README). Disabling bootstrap grants without supplying a
`SORREL_HUB_TRUSTED_GRANTS_FILE` will cause push to return 403.

Before pushing specialized workflow/secret grants, ensure local grants are
recorded and their objects are in the object closure:

```bash
$SORREL grant create \
  --action secret.read \
  --agent docs \
  --workflow validate-protocol \
  --runner local-container \
  --secret secret_npm_token_dev \
  --environment dev \
  --reason "CI read access"
```

The CLI uploads grant objects as part of the missing-object closure when the
hub requests them. If push is denied for missing grant authority, create the
required grants locally and push again so `grantRefs` on the hub match your
workspace policy state.

## 5. Pull from the hub

On another machine (or a fresh directory with `sorrel init` + the same remote):

```bash
$SORREL remote add origin http://127.0.0.1:3000 --repo-id repo_<hex>
$SORREL pull
# Pulled origin/HEAD to <snapshot> (N object(s))
```

Pull downloads any objects missing locally, then updates `.sorrel/HEAD` to the
remote snapshot without deleting unrelated local-only objects.

## Example session

```bash
mkdir /tmp/sorrel-sync-demo && cd /tmp/sorrel-sync-demo
SORREL=/path/to/sorrel-cli/target/debug/sorrel

# Terminal 1: hub
# (in sorrel-hub repo) npm start

$SORREL init
$SORREL remote add origin http://127.0.0.1:3000
echo "synced" > note.txt
$SORREL change create -m "add note.txt"
$SORREL push --json
$SORREL pull --json
```

Expected push JSON shape (stable fields):

```json
{
  "command": "push",
  "mocked": false,
  "status": "pushed",
  "remote": "origin",
  "ref": "HEAD",
  "snapshot": { "kind": "Snapshot", "id": "<64-hex>" },
  "uploaded": 3
}
```

## Manual verification against hub (R3)

1. Start sorrel-hub on `http://127.0.0.1:3000` (`npm start`).
2. In a workspace: `sorrel init`, `sorrel remote add origin http://127.0.0.1:3000`.
3. Create a file, `sorrel change create -m "test"`, `sorrel push`.
4. Confirm hub reports the new snapshot id on `GET /{repoId}/refs`.
5. In a second workspace with the same remote: `sorrel pull` — HEAD matches and
   files materialize from the downloaded object closure.
6. If hub policy is enabled: create grants with `sorrel grant create`, push
   again, and confirm `grantRefs` on the hub reflect the local grant registry.

## Out of scope (this milestone)

- `sorrel clone` (pull into an empty/uninitialized tree is follow-up).
- Force push (`force: true` is not exposed; pushes always send `force: false`).
