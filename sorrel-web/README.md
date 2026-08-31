# sorrel-web

Public landing site for Sorrel. Static HTML/CSS/JS (Nord theme). **Production
deploy is Cloudflare Pages** — publish this repository root with no build step.
The published status describes the coordinated `v0.1.0-alpha.1` scope; the Hub
and Hub UI it documents remain development-only and lack production auth.

## Files

| Path | Role |
| --- | --- |
| `index.html` | Landing page (current status + roadmap) |
| `styles.css` | Theme and layout |
| `site.js` | Sticky header + theme toggle |
| `docs/` | Docs hub (HTML subpages), status/getting-started Markdown + viewer |
| `scripts/build-api-docs.sh` | Optional rustdoc API reference from `sorrel-core` |
| `.github/workflows/deploy-pages.yml` | Optional GitHub Pages assemble + rustdoc publish |
| `Dockerfile` | Optional local nginx preview (does **not** replace Cloudflare) |

## Docs

HTML guides under `docs/` (Core, CLI, Hub, workflows, API). Live status and
how-to run live as Markdown:

- [`docs/STATUS.md`](docs/STATUS.md) — working vs missing
- [`docs/GETTING_STARTED.md`](docs/GETTING_STARTED.md) — how to run the stack

Open `/docs/guides.html` for the Markdown viewer, or `/docs/` for the docs hub.

The monorepo root mirrors the same guides under [`../docs/`](../docs/) for
people browsing the coordination repo.

## Generated API reference

`docs/api.html` links to `/api/sorrel-core/sorrel_core/index.html`, which is
generated (not committed). To build it locally with the root monorepo layout
(`sorrel-core` as a sibling checkout):

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
- Publish directory: repository root (`.`)
- Node version: not required

No Cloudflare config change is required for the docs folder — it is static
content next to `index.html`. Optional GitHub Pages (`deploy-pages.yml`) can
also publish including generated API docs; Cloudflare remains the production
landing host for this project.

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
