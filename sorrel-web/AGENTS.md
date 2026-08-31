# Agent instructions for sorrel-web

## What this module is

The **public marketing/landing site** for Sorrel. Static HTML/CSS/JS only — no
backend, no build step, no framework. This is **not** the shared Hub product UI
(`sorrel-hub-ui`), its browser host (`sorrel-hub-web`), or the Hub API
(`sorrel-hub`).

## Stack and conventions

- Plain `index.html`, `styles.css`, `site.js`. Nord theme, dark default with a
  light toggle. Keep it dependency-free and easy to host on any static host.
- No authenticated product features, no API calls to Hub. Marketing content +
  static docs under `docs/` only.

## Common checks

```sh
npm run check

# Static preview:
python3 -m http.server 4173
```

The automated gate checks JavaScript syntax and verifies that local links and
assets referenced by HTML pages resolve. Continue to preview visual changes by
eye.

## Workflow

- Keep changes scoped to this package; edit canonical public Markdown under
  root `docs/` (or root `CHANGELOG.md`) and run `npm run sync:docs` from the
  monorepo root.
- Prefer small, reviewable commits.
- Do not commit secrets.
