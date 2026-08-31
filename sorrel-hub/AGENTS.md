# Agent instructions for sorrel-hub

## What this module is

Sorrel Hub is the collaboration **API server** — a Node HTTP service exposing
JSON endpoints (`/healthz`, `/projects`, `/admin/*`). It is the backend, **not** a
web interface; `sorrel-hub-ui` owns the shared product UI,
`sorrel-hub-web` hosts it in browsers, and `sorrel-web` is the unrelated public
marketing site.

## Stack and conventions

- Node >= 22, ES modules, **zero runtime dependencies** for the HTTP server
  (uses `node:http`, `node:crypto`, etc.). Do not add web frameworks unless asked.
- Product metadata persists via the filesystem metadata store
  (`src/fs-metadata-store.js`, default `./data/metadata`, override with
  `SORREL_HUB_METADATA_DIR` or relative to `SORREL_HUB_DATA_DIR`); the
  in-memory store (`src/store.js`) remains the API surface and the
  `createApp()` / test default. AuthAdapter supports `dev` (acting-principal
  header), `workos`, and `oidc` (Bearer JWT + JWKS verification). `GET /session`
  exposes the resolved Hub session.
- Sync objects/refs persist via the filesystem sync store
  (`src/fs-sync-store.js`, default `./data/sync`, override with
  `SORREL_HUB_DATA_DIR`, opt out with `SORREL_HUB_SYNC_STORE=memory` —
  which also keeps metadata in memory). Tests default to in-memory stores.
- Modular install: `GET /capabilities` (env-driven modules + auth + Convex).
- Shared Convex metadata schema lives in `convex/` (SaaS Cloud and self-host).
  VCS objects must not move into Convex.
- Keep internal `CONVEX_URL` separate from browser-facing
  `CONVEX_PUBLIC_URL`; service DNS names are not reachable from the browser.

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
npm ci
npm run check     # routes, policy conformance, sidecar drift guard
npm start         # starts the API server on PORT (default 3000)
```

The vendored policy-conformance manifest under `test/conformance/` must stay in
sync with `sorrel-protocol`; `test/conformance-sync.test.js` guards drift. Do not
hand-edit the manifest — run `./scripts/sync-conformance.sh` from the monorepo
root after changing the canonical manifest.

## Workflow

- Keep changes scoped to this package and required workspace consumers.
- Prefer small, reviewable commits.
- Do not commit secrets.
- Coordinate shared contracts through `sorrel-protocol`.
