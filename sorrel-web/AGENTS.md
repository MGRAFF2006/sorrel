# Agent instructions for sorrel-web

## What this module is

The **public marketing/landing site** for Sorrel. Static HTML/CSS/JS only — no
backend, no build step, no framework. This is **not** the Hub product UI (that is
`sorrel-hub-web`) and not the Hub API (`sorrel-hub`).

## Stack and conventions

- Plain `index.html`, `styles.css`, `site.js`. Nord theme, dark default with a
  light toggle. Keep it dependency-free and easy to host on any static host.
- No authenticated product features, no API calls to Hub. Marketing content +
  static docs under `docs/` only.

## Common checks

```sh
# Static preview:
python3 -m http.server 4173
```

Validate HTML/CSS/JS by eye and with any static linters available; there is no
test runner.

## Workflow

- Keep changes scoped to this repository.
- Prefer small, reviewable commits.
- Do not commit secrets.
