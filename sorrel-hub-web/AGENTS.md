# Agent instructions for sorrel-hub-web

## What this module is

The **web interface** (browser frontend) for Sorrel Hub. It is a thin client over
the `sorrel-hub` HTTP API. It is **not** the API server and **not** the public
marketing site.

- `sorrel-hub-web` (this repo): Hub UI.
- `sorrel-hub`: Hub API server.
- `sorrel-web`: public marketing landing page.

## Stack and conventions

- Framework-free, build-step-free: plain HTML, CSS, and ES modules in `public/`.
- A tiny dependency-free Node server in `server/dev-server.mjs` serves the
  static assets and proxies `/api/*` to the Hub API (`HUB_API_URL`). Same
  process is used for local development and container deploys.
- Node >= 22. No runtime dependencies; avoid adding frameworks or bundlers
  unless explicitly requested.

## Core boundary (do not violate)

- This UI holds **no authoritative state** and defines **no permissions**.
  Identity, policy, grants, and decisions are owned by Sorrel Core and reached
  only through the Hub API. Never reimplement authorization logic here.
- Treat secrets as opaque references; never display or persist secret values.

## Common checks

```sh
npm test          # static assets + live Hub proxy write path
npm start         # serves the UI and proxies to the Hub API
```

The UI creates projects, proposals, and review comments against a live Hub.
Sync remains read-only in the browser.

## Workflow

- Keep changes scoped to this repository.
- Prefer small, reviewable commits.
- Do not commit secrets.
- Coordinate shared contracts (object shapes, endpoints) through `sorrel-protocol`
  and the `sorrel-hub` API, not by inventing new client-only models.
