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

Optional live Convex URL: `VITE_CONVEX_URL=http://127.0.0.1:3210`.

## Tests

```sh
npm run check
```
