# Shared Convex metadata package (SaaS Cloud + self-host).

Same `convex/` schema and functions for both deploys. Only wiring differs:

| Deploy | Convex | App code |
| --- | --- | --- |
| SaaS | Convex Cloud | this directory |
| Self-host | Compose `convex-backend` | this directory; `CONVEX_URL` → local |

**Keep out of Convex:** VCS objects/refs (sync object store).

## Local self-host

```sh
# from monorepo root
docker compose --profile convex \
  -f docker-compose.yml -f docker-compose.convex.yml up

# generate admin key
docker compose -f docker-compose.yml -f docker-compose.convex.yml \
  exec convex-backend ./generate_admin_key.sh

# in sorrel-hub/
CONVEX_SELF_HOSTED_URL=http://127.0.0.1:3210 \
CONVEX_SELF_HOSTED_ADMIN_KEY=<key> \
npx convex dev --url "$CONVEX_SELF_HOSTED_URL" --admin-key "$CONVEX_SELF_HOSTED_ADMIN_KEY"
```

Spike query: `proposals.countOpen` — live open-proposals badge in hub-ui.

Compose uses `http://convex-backend:3210` for Hub-to-Convex traffic and exposes
`CONVEX_PUBLIC_URL` (default `http://127.0.0.1:3210`) to browser clients. Set
`CONVEX_PUBLIC_URL` explicitly when the browser reaches Convex at another URL.
