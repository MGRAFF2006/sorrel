# sorrel-web

Public landing site for Sorrel. Static HTML/CSS/JS (Nord theme). **Production
deploy is Cloudflare Pages** — publish this package directory with no build step.
The published status describes the coordinated `v0.1.0-alpha.2` scope; the Hub
and Hub UI it documents remain development-only and lack production auth.

## Files

| Path | Role |
| --- | --- |
| `index.html` | Landing page (current status + roadmap) |
| `styles.css` | Theme and layout |
| `site.js` | Sticky header + theme toggle |
| `docs/` | Docs hub, HTML deep dives, canonical Markdown mirrors + viewer |
| `scripts/build-api-docs.sh` | Optional rustdoc API reference from `sorrel-core` |
| `Dockerfile` | Optional local nginx preview (does **not** replace Cloudflare) |

## Docs

HTML guides under `docs/` cover Core, CLI, Hub, workflows, and API details.
Canonical project guides and release history are generated mirrors:

- [`docs/CHANGELOG.md`](docs/CHANGELOG.md) — coordinated shipped progress
- [`docs/STATUS.md`](docs/STATUS.md) — working vs missing
- [`docs/GETTING_STARTED.md`](docs/GETTING_STARTED.md) — how to run the stack
- [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) — current system map
- [`docs/DEVELOPMENT.md`](docs/DEVELOPMENT.md) — human and AI development guide
- [`docs/RELEASE.md`](docs/RELEASE.md) — release process

Open `/docs/guides.html` for the Markdown viewer, or `/docs/` for the docs hub.

Do not edit those files here. `npm run sync:docs` copies them from the monorepo
root, which is the source of truth.

## Generated API reference

`docs/api.html` can link to `/api/sorrel-core/sorrel_core/index.html`, which is
generated locally (not committed or currently deployed). To build it with the
root monorepo layout (`sorrel-core` as a sibling package directory):

```bash
scripts/build-api-docs.sh            # or: scripts/build-api-docs.sh /path/to/sorrel-core
```

## Local preview

```bash
npm run check
python3 -m http.server 4173
# http://localhost:4173
# http://localhost:4173/docs/
```

## Cloudflare (production)

- Build command: none required for the pages themselves
- Publish directory: `sorrel-web/` when configured from the monorepo root
- Node version: not required

No Cloudflare config change is required for the docs folder — it is static
content next to `index.html`.

## Optional Docker preview

```sh
docker build -t sorrel-web .
docker run --rm -p 4173:80 sorrel-web
```

Or from the Sorrel root: `docker compose up web` (local only).

## License

Licensed under either the Apache License, Version 2.0
([`LICENSE-APACHE`](LICENSE-APACHE)) or the MIT License
([`LICENSE-MIT`](LICENSE-MIT)), at your option.
