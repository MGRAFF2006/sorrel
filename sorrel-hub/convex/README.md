# Shared Convex metadata mirror (SaaS Cloud + self-host)

The shared schema mirrors proposal metadata only. VCS objects and refs stay in
Hub's sync object store. `proposals.upsert`, `proposals.remove`, and
`proposals.countOpen` are **internal functions**: anonymous clients and ordinary
Bearer sessions cannot invoke them directly. Only the server-held deployment
admin key can call them over HTTP. That credential controls the deployment;
keep it out of browser builds, capabilities responses, logs, and version control.

The UI always polls Hub's `/admin/proposals?status=open` through its configured
authenticated transport. Core authorization filters that list to visible
projects; the internal Convex count is for operator inspection only.
`CONVEX_PUBLIC_URL`, `VITE_CONVEX_URL`, and the old `convexUrl` mount option are
no longer used. Capabilities report mirror configuration without its URL.

## Local self-host

From the monorepo root, start the backend and generate a private admin key:

```sh
docker compose --profile convex \
  -f docker-compose.yml -f docker-compose.convex.yml up -d convex-backend

docker compose -f docker-compose.yml -f docker-compose.convex.yml \
  exec convex-backend ./generate_admin_key.sh
```

Put `CONVEX_SELF_HOSTED_URL=http://127.0.0.1:3210` and
`CONVEX_SELF_HOSTED_ADMIN_KEY=<key>` in an ignored local environment file for
Convex CLI use. Install the package's locked dev dependencies, then deploy from
`sorrel-hub/` using the self-hosted variables:

```sh
npx convex dev --once
```

The CLI regenerates `convex/_generated/` against the deployment. The committed
minimal server exports use Convex's real runtime builders even before codegen.

Set `CONVEX_SELF_HOSTED_ADMIN_KEY` in the environment used by Compose and restart
Hub with the profile:

```sh
docker compose --profile convex \
  -f docker-compose.yml -f docker-compose.convex.yml up -d hub
```

Hub uses `CONVEX_URL=http://convex-backend:3210` internally. Without both URL and
key, mirroring remains disabled. `SORREL_HUB_CONVEX=0` or `false` also disables
it. Failures remain best effort and do not make Hub metadata writes fail.
For Cloud, use the deployment URL in `CONVEX_URL` and its server-only
`CONVEX_DEPLOY_KEY`.

**Existing deployments must redeploy these functions** to remove the old public
query/mutations. Updating Hub or UI alone does not revoke deployed public
functions. This changes function visibility without changing the table schema.

## Disposable-backend privacy smoke

After deploying to a fresh disposable loopback backend, run from the monorepo
root with its self-hosted URL/admin-key environment:

```sh
node sorrel-hub/test-support/convex-selfhost-smoke.mjs
```

This checks anonymous/forged query and mutation rejection, then privileged Hub
mirror insert, status update, and removal. It writes a synthetic proposal and
expects an initially empty deployment; use no persistent or production backend.
The script accepts only a `127.0.0.1` backend. The protocol follows Convex's
[internal functions](https://docs.convex.dev/functions/internal-functions) and
[admin HTTP client](https://github.com/get-convex/convex-js/blob/main/src/browser/http_client.ts)
behavior; self-host setup follows the
[official guide](https://github.com/get-convex/convex-backend/tree/main/self-hosted).
