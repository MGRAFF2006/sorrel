# Sorrel CLI — Sync with sorrel-hub

This document covers pushing and pulling Sorrel snapshots over the HTTP sync
transport against [`sorrel-hub`](../sorrel-hub/).
For the local-only persistent workflow, see [`DEMO.md`](DEMO.md).

`sorrel lane submit` records both the active source snapshot and the local target
lane tip for a Hub review. With its default push, it uploads both snapshot
closures, including diverged target history, while advancing only the source
`HEAD` ref. Hub compares those recorded snapshots without merging them. With
`--no-push`, both closures must already be available remotely for comparison.
The Hub's “Mark as merged” action updates review metadata; use the local merge
command to integrate file contents.

## Prerequisites

1. **sorrel-hub** running locally (default `http://127.0.0.1:3000`).
2. A Sorrel workspace initialized in your project (`sorrel init`).
3. The monorepo dependencies installed with `npm run setup`; the CLI consumes
   `sorrel-core` through the workspace path dependency.

For a Hub using OIDC or WorkOS bearer authentication, provide the access token
through the process environment. It is never written to `.sorrel/remotes.json`:

```bash
export SORREL_HUB_TOKEN="$(your-idp-token-command)"
$SORREL push
```

Unset `SORREL_HUB_TOKEN` when returning to development acting-principal auth.

## Build

```bash
cargo build
SORREL=target/debug/sorrel
```

## 1. Start sorrel-hub

From the monorepo root:

```bash
cd sorrel-hub && SORREL_HUB_LOCAL_DEMO=1 SORREL_HUB_BOOTSTRAP_GRANTS=1 npm start
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

Current CLI mutating calls send an empty `grantRefs` list and rely on the Hub's
configured effective Core grants. They do not require local-demo grant IDs.
Development acting-principal headers are accepted only in explicit demo mode;
hosted calls require `SORREL_HUB_TOKEN`. Bootstrap grants are disabled by default.

Local workflow and secret permissions can be recorded separately:

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

Creating or uploading a local grant does not authorize a hosted Hub action.
If the Hub denies an upload, its operator must inspect the authenticated
principal and trusted Core grants for the intended repository. Hosted access
comes from operator-provisioned authority, not client-supplied grant objects.

## 5. Pull from the hub

On another machine (or a fresh directory with `sorrel init` + the same remote):

```bash
$SORREL remote add origin http://127.0.0.1:3000 --repo-id repo_<hex>
$SORREL pull
# Pulled origin/HEAD to <snapshot> (N object(s))
```

Current source refuses pull while a merge is pending or the worktree is dirty.
It downloads and verifies the snapshot closure, requires a fast-forward (or an
empty initial workspace), and restores files before publishing the new HEAD.
Diverged history is preserved and rejected; pull does not merge it automatically.
Ignored/untracked collisions and unsupported restore paths are rejected before
deletions. Downloaded immutable objects can remain after a rejected pull.

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
