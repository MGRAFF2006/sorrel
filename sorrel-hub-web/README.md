# sorrel-hub-web

The **web interface** for Sorrel Hub: a browser frontend for the sibling
[`sorrel-hub`](../sorrel-hub) collaboration API in this monorepo
([`MGRAFF2006/sorrel`](https://github.com/MGRAFF2006/sorrel)).

> **Development-only alpha companion:** this UI relies on the Hub API's
> development acting-principal mechanism. Neither this UI nor the alpha Hub
> provides login, SSO, or production authentication. Do not expose them to an
> untrusted network.

## What this is (and is not)

- **This package is the Hub UI.** It renders projects, writable review and
  selected administration flows, and read-only sync data by calling the
  `sorrel-hub` HTTP API.
- **`sorrel-hub` is the API server.** It owns the data model, store, and Core
  policy administration guard. It has no UI of its own.
- **`sorrel-web` is the public marketing/landing site.** It is unrelated to this
  development product UI.

So the split is:

| Package         | Role                                            |
| --------------- | ----------------------------------------------- |
| `sorrel-web`    | Public marketing landing page (static)          |
| `sorrel-hub`    | Hub **API server** (JSON over HTTP)             |
| `sorrel-hub-web`| Hub **web interface** (this package)            |

This UI holds no authoritative state and defines **no permissions of its own** —
identity, policy, grants, and decisions remain owned by Sorrel Core and surfaced
through the Hub API.

## Stack

Intentionally framework-free and build-step-free, matching the rest of Sorrel's
lightweight modules: plain HTML, CSS, and ES modules, plus a tiny dependency-free
Node server that serves `public/` and proxies `/api/*` to the Hub API.

```text
public/
  index.html   layout
  styles.css   Nord-inspired theme
  app.js       thin API client + rendering
server/
  dev-server.mjs   static server + /api proxy (local + container deploy)
```

## Local development

Run the Hub API in one terminal:

```sh
# from the sorrel-hub repo
npm start            # listens on http://localhost:3000
```

Run this web interface in another:

```sh
npm start            # listens on http://0.0.0.0:5180
# proxies /api/* -> http://localhost:3000 (override with HUB_API_URL)
```

Then open <http://127.0.0.1:5180>. The header shows the live API status; the
Projects, Administration, and Sync views call the live Hub API. Mutations act as
the development principal `{"type":"user","id":"local"}`, matching the JS SDK.

## Local container/static preview

```sh
docker build -t sorrel-hub-web .
docker run --rm -p 5180:5180 -e HUB_API_URL=http://host.docker.internal:3000 sorrel-hub-web
```

Or from the Sorrel root repo: `docker compose up hub hub-web` (opens the UI on
port 5180 and the API on 3000).

For a CDN/static host instead, publish `public/` and put a reverse proxy in
front that maps `/api/*` to the Hub API (the browser client always calls `/api`).

Environment:

- `PORT` — server port (default `5180`)
- `HOST` — bind address (default `0.0.0.0`)
- `HUB_API_URL` — base URL of the `sorrel-hub` API (default `http://localhost:3000`)

## Features

- **Projects** — list and create Hub projects (optional organization filter)
- **Reviews** — create proposals and review comments, inspect proposal threads,
  transition proposal status, and resolve comments
- **Administration** — inspect organizations, repositories, workflow runs, and
  policies; update workflow-run status
- **Sync** — read-only list of synced repositories (`/admin/sync-repos`) and,
  per repo, their refs (`/{repoId}/refs`)

Mutating requests send `x-sorrel-acting-principal` for Core-policy evaluation.
That development header is not proof of identity and is not production auth.

## Tests

```sh
npm test
```

Validates static assets, mutation acting-principal headers, and the live Hub
proxy write path under `/api`.

## Status

`v0.1.0-alpha.1` is a development companion for the Hub API. Project,
proposal/review, and selected administration mutations work; Sync remains
read-only in the browser. Production authentication, identity-provider
integration, richer review UX, and a merge queue are not included.

## License

Licensed under either the Apache License, Version 2.0
([`LICENSE-APACHE`](LICENSE-APACHE)) or the MIT License
([`LICENSE-MIT`](LICENSE-MIT)), at your option.
