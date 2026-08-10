# Agent instructions for sorrel-hub

## What this module is

Sorrel Hub is the collaboration **API server** — a Node HTTP service exposing
JSON endpoints (`/healthz`, `/projects`, `/admin/*`). It is the backend, **not** a
web interface; the browser frontend is the separate `sorrel-hub-web` repo, and
`sorrel-web` is the unrelated public marketing site.

## Stack and conventions

- Node >= 22, ES modules, **zero runtime dependencies** (uses `node:http`,
  `node:crypto`, etc.). Do not add web frameworks or a database unless asked.
- Product metadata persists via the filesystem metadata store
  (`src/fs-metadata-store.js`, default `./data/metadata`, override with
  `SORREL_HUB_METADATA_DIR` or relative to `SORREL_HUB_DATA_DIR`); the
  in-memory store (`src/store.js`) remains the API surface and the
  `createApp()` / test default. No production auth in the skeleton.
- Sync objects/refs persist via the filesystem sync store
  (`src/fs-sync-store.js`, default `./data/sync`, override with
  `SORREL_HUB_DATA_DIR`, opt out with `SORREL_HUB_SYNC_STORE=memory` —
  which also keeps metadata in memory). Tests default to in-memory stores.

## Core boundary (do not violate)

- Hub administers and displays policy but is **not** the source of truth.
  Identity, grants, policy decisions, and audit events are owned by Sorrel Core
  and referenced by id. Do not invent a Hub-only permission model.
- Signature-trust decisions (unsigned/forged policy changes, rotation
  thresholds) remain Core's responsibility; Hub only re-checks allow/deny for
  actions it administers.
- Never store or return secret values; carry `SecretRef` only.

## Common checks

```sh
npm test          # node --test (routes, policy conformance, sidecar drift guard)
npm start         # starts the API server on PORT (default 3000)
```

The vendored policy-conformance manifest under `test/conformance/` must stay in
sync with `sorrel-protocol`; `test/conformance-sync.test.js` guards drift. Do not
hand-edit the manifest — re-export it from a `sorrel-protocol` checkout.

## Workflow

- Keep changes scoped to this repository.
- Prefer small, reviewable commits.
- Do not commit secrets.
- Coordinate shared contracts through `sorrel-protocol`.
