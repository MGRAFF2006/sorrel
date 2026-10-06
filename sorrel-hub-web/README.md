# sorrel-hub-web

Thin **browser host** for the shared [`sorrel-hub-ui`](../sorrel-hub-ui)
Solid product UI. Talks to [`sorrel-hub`](../sorrel-hub) over `/api`.

> **Development-only alpha:** relies on Hub's acting-principal / AuthAdapter
> `dev` mode. Do not expose to an untrusted network.

## Split

| Package | Role |
| --- | --- |
| `sorrel-hub` | Hub API server |
| `sorrel-hub-ui` | Shared Solid product UI (web + future Tauri) |
| `sorrel-hub-web` | This package — Vite browser host |
| `sorrel-web` | Public marketing site (unrelated) |

This UI holds no authoritative state and defines **no permissions of its own** —
identity, policy, grants, and decisions remain owned by Sorrel Core and surfaced
through the Hub API.

## Stack

Vite builds the shared SolidJS UI for browsers. A small Node server serves the
generated `dist/` directory and proxies `/api/*` to the Hub API.

```text
src/main.tsx       browser mount
vite.config.ts     build and development proxy
server/
  static-server.mjs   production static server + /api proxy
```

## Run

```sh
# Hub API on :3000, then:
npm ci
npm run dev          # Vite on :5180, proxies /api → HUB_API_URL
npm run build && npm start   # serve dist/ + proxy
```

Vite development and preview servers bind to `127.0.0.1` and use Vite's built-in
localhost/IP host restrictions by default. To access them over a trusted private
network, explicitly set both the bind address and named hosts, for example
`HOST=0.0.0.0 SORREL_HUB_ALLOWED_HOSTS=desktop npm run dev`. A comma-separated
host list is supported. `SORREL_HUB_ALLOWED_HOSTS=all` (or `true`) explicitly
disables host-header checking; use that only on a trusted network. These
settings do not change the production static server.

Coordinated releases also publish the unprivileged Linux amd64/arm64 image as
`ghcr.io/mgraff2006/sorrel-hub-web:<VERSION>`. Use the release-attached
`sorrel-server.compose.yml` to run it with the matching Hub API image; the
complete flow and alpha security boundary are documented in
[`docs/GETTING_STARTED.md`](../docs/GETTING_STARTED.md#host-a-release-server).

Proposal counters use the authenticated Hub API; browsers do not connect to Convex.

## Tests

```sh
npm run check
```
