# Agent instructions for sorrel-hub-ui

## What this module is

Shared **SolidJS product UI** for Sorrel Hub browser and Tauri desktop hosts.
The native React Native mobile companion shares Hub API/SDK contracts rather
than mounting this DOM UI.

- `sorrel-hub-ui` (this package): shared UI + platform stubs
- `sorrel-hub-web`: thin browser host
- `sorrel-hub`: Hub API server
- `sorrel-web`: unrelated public marketing site

## Stack and conventions

- SolidJS + Vite + Solid Router
- Nord visual language with sidebar product shell
- **Project-first IA** (GitHub-like): `/` lists projects; Reviews and Sync nest
  under `/projects/:id`; unimplemented modules stay out of navigation.
- Motion for route/panel enter only — **never** on diff scroll (diff island later)
- Platform seams via `platform.ts` (`web` | `desktop` | `mobile` stubs)
- Live metadata via Convex when configured; Hub API fallback otherwise
- Dev identity via acting-principal presets (`session.ts`); `GET /session` for Hub auth mode

## Core boundary (do not violate)

- This UI holds **no authoritative state** and defines **no permissions**.
- Identity, policy, grants, and decisions are owned by Sorrel Core and reached
  only through the Hub API / AuthAdapter session. Never reimplement authorization.
- Treat secrets as opaque references; never display or persist secret values.
- Do not put VCS objects in Convex; objects stay on the sync object store.

## Common checks

```sh
npm ci
npm run check  # typecheck, tests, production build
npm run dev   # standalone on :5181 with /api proxy
```

## Workflow

- Keep product UI here; keep host chrome (menus, keychain, deep links) in shells.
- Prefer small, reviewable commits.
- Do not commit secrets.
