# Agent instructions for sorrel-hub-web

## What this module is

Thin **browser host** for the shared `sorrel-hub-ui` Solid product UI. It is
**not** the API server and **not** the public marketing site.

- `sorrel-hub-ui`: shared Solid UI (web + future Tauri shells)
- `sorrel-hub-web` (this package): Vite browser host + static/proxy server
- `sorrel-hub`: Hub API server
- `sorrel-web`: public marketing landing page

## Stack and conventions

- Vite + Solid; mounts `mountHubApp(..., { platformKind: 'web' })`
- Dev: `npm run dev` (Vite proxies `/api` → `HUB_API_URL`)
- Prod: `npm run build` then `npm start` serves `dist/` + API proxy
- Node >= 22

## Core boundary (do not violate)

- This host holds **no authoritative state** and defines **no permissions**.
  Product UI lives in `sorrel-hub-ui`; identity/policy stay Core via Hub API.
- Treat secrets as opaque references; never display or persist secret values.

## Common checks

```sh
npm ci
npm run check     # host wiring + live Hub proxy write path + production build
npm run dev       # Vite on :5180
```

## Workflow

- Prefer UI changes in `sorrel-hub-ui`, not here.
- Do not commit secrets.
