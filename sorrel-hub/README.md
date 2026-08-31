# sorrel-hub

Sorrel module: sorrel-hub.

**Sorrel Hub is the collaboration API server** — a Node HTTP service exposing
JSON endpoints. It is the **backend**, not a web interface. The shared product
UI is [`sorrel-hub-ui`](../sorrel-hub-ui), hosted in browsers by
[`sorrel-hub-web`](../sorrel-hub-web).

| Package | Role |
| --- | --- |
| `sorrel-hub` | Hub **API server** (this package; JSON over HTTP) |
| `sorrel-hub-ui` | Shared SolidJS product UI |
| `sorrel-hub-web` | Thin browser host and API proxy |
| `sorrel-web` | Public marketing landing page (static, unrelated) |

Hub stores product metadata and administration surfaces over Core policy
semantics; it does not define a separate authorization language. Principals use
the protocol `Principal` shape, policies reference protocol `Policy` and
`AgentPolicy` objects, and Core grants, policy decisions, and audit events are
kept as external references.

The `v0.1.0-alpha.1` server has development, WorkOS, and OIDC AuthAdapter seams,
but production session/login integration is incomplete. Treat it as a localhost
development server, not an internet-facing service. It also
omits merge queue behavior, hosted compute, and secret values. Two further
declared gaps: list endpoints return full arrays (no pagination), and object
upload verifies BLAKE3 content ids but does not JSON-Schema-validate object
bodies (blobs are raw bytes, so structural validation only applies to typed
objects; deferred).

Hub model objects reference the Core permission spine (`Principal`, `ResourceRef`,
`Policy`, `PolicyDecision`, `Grant`, and `SecretRef`) so Hub can administer and
display policy without becoming the only source of truth.

### Policy conformance

To keep Hub's administration guard aligned with Core, `test/policy-conformance.test.js`
runs the canonical `sorrel-protocol` policy conformance manifest
(vendored at `test/conformance/policy-conformance.json`) against Hub's
`evaluate()` and asserts Hub agrees on allow/deny for the actions it administers.
Signature-trust decisions (unsigned/forged changes, rotation thresholds) remain
Core's responsibility and are intentionally not re-decided by Hub.

The vendored manifest is paired with a sidecar `policy-conformance.meta.json`
(version + SHA-256) from `sorrel-protocol`. `test/conformance-sync.test.js`
recomputes the manifest hash and fails if it drifts from the sidecar, so a stale
vendored copy is caught by `npm test`. To refresh, re-export from a
canonical package:

```sh
# from the monorepo root
./scripts/sync-conformance.sh
```

(or run the root `scripts/sync-conformance.sh`), then re-run `npm test`. See
`test/conformance/README.md`.

## Getting started

```sh
npm ci
npm run check
npm start
```

The server listens on `127.0.0.1` and `PORT=3000` by default. `HOST` overrides
the bind address.

> **Local development only:** dev auth and bootstrap grants are rejected on a
> non-loopback bind unless `SORREL_HUB_ALLOW_INSECURE_DEV_AUTH=1` is explicitly
> set for an isolated demo. Do not expose that override to an untrusted network.

### Persistence

Sync objects/refs and product metadata persist to disk by default so data
survives restarts:

- `SORREL_HUB_DATA_DIR` — sync store directory (default `./data/sync`).
- `SORREL_HUB_METADATA_DIR` — metadata store directory (default
  `<SORREL_HUB_DATA_DIR>/../metadata`, i.e. `./data/metadata` when using the
  sync default).
- `SORREL_HUB_SYNC_STORE=memory` — use ephemeral in-memory stores for both
  sync and metadata (the default inside tests via `createApp()`).

### Trusted grants (sync push/pull)

Mutating sync routes evaluate Core policy against a trusted grant map. The
server has no local bootstrap grants by default. For local development only,
the explicit opt-in below lets the CLI acting principal
`{"type":"user","id":"local"}` push/pull without a separate grant service:

- `grant_local_object_write` → `repo.object.write`
- `grant_local_ref_write` → `repo.ref.write`

Environment:

- `SORREL_HUB_BOOTSTRAP_GRANTS=1` — enable the two development-only,
  repo-wide bootstrap grants. No other value enables them.
- `SORREL_HUB_TRUSTED_GRANTS_FILE` — path to a JSON object of extra
  `id → grant` records merged on top of bootstrap grants.

The CLI sends matching `grantRefs` on `POST /objects` and `POST /refs/*`.
Because the bootstrap grants match every repository id, never enable them for
an internet-facing or multi-user Hub. They are a development convenience, not
production authentication or authorization provisioning.

For a local CLI-compatible development server:

```sh
SORREL_HUB_BOOTSTRAP_GRANTS=1 npm start
```

### Deployment

```sh
docker build -t sorrel-hub .
docker run --rm -p 3000:3000 \
  -e HOST=0.0.0.0 \
  -v hub-data:/app/data \
  sorrel-hub
```

Binding `0.0.0.0` is required for a published Docker port, but it does not
enable bootstrap grants or add authentication. Local Docker E2E that pushes
must additionally pass `-e SORREL_HUB_BOOTSTRAP_GRANTS=1`. Likewise, root-repo
E2E must opt in explicitly:

```sh
SORREL_HUB_BOOTSTRAP_GRANTS=1 npm test
```

The same variable must be forwarded when an E2E harness spawns
`scripts/listen.mjs`. Docker and root E2E opt-in is development-only; do not use
these settings as a deployment recipe. The alpha has no production auth.

The sync on-disk layout mirrors Core's `FileObjectStore` semantics:
content-addressed fanout (`<repo>/objects/<id[0..2]>/<id>`), atomic
temp-file + rename writes, digest-verified reads, and one JSON document per
ref under `<repo>/refs/`.

Product metadata (organizations, projects, repositories, proposals, review
comments, workflow runs, policies) is stored as one JSON document per record
under `<metadataDir>/<collection>/<id>.json`, also written atomically.

## License

Licensed under either the Apache License, Version 2.0
([`LICENSE-APACHE`](LICENSE-APACHE)) or the MIT License
([`LICENSE-MIT`](LICENSE-MIT)), at your option.

## API

### `GET /healthz`

Returns service health.

### `GET /projects`

Lists projects.

Optional query parameters:

- `organizationId` filters projects to one organization.

### `POST /projects`

Creates a project.

Required JSON fields:

- `organizationId`
- `name`

Optional JSON fields:

- `slug`
- `description`
- `status`
- `repositoryIds`
- `policyIds`
- `createdByPrincipal`
- `principalRefs`
- `policyRefs`
- `grantRefs`
- `policyDecisionRefs`
- `auditEventRefs`
- `metadata`

Example:

```sh
curl -X POST http://localhost:3000/projects \
  -H 'content-type: application/json' \
  -d '{"organizationId":"org_local","name":"Platform Collaboration","policyRefs":[{"kind":"Policy","id":"policy_project_access"}]}'
```

### Administration collections

Lightweight collection endpoints for administration data:

- `GET|POST /admin/organizations`
- `GET /admin/organizations/:id`
- `GET|POST /admin/repositories` (filters: `organizationId`, `projectId`)
- `GET /admin/repositories/:id`
- `GET|POST /admin/proposals` (filters: `projectId`, `repositoryId`, `syncRepoId`, `status`, `sourceLane`)
- `GET|PATCH /admin/proposals/:id` — detail; PATCH status (`draft`→`open`→`approved`/`rejected`/`merged`/`closed`) and editable fields
- `GET /admin/proposals/:id?include=comments` — proposal plus nested review comments
- `GET /admin/proposals/:id/comments` — comments only
- `GET|POST /admin/review-comments` (filters: `proposalId`, `state`)
- `GET|PATCH /admin/review-comments/:id` — resolve via `{ "state": "resolved" }`
- `GET|POST /admin/workflow-runs` (filters: `projectId`, `proposalId`, `status`)
- `GET|PATCH /admin/workflow-runs/:id` — status updates (`queued`→`in_progress`→`succeeded`/…)
- `GET|POST /admin/policies`
- `GET /admin/policies/:id`
- `GET /admin/sync-repos` — sync transport repos (`{ "repos": [ { "id", "refCount" } ] }`)

Proposal records may carry lane-submit fields: `syncRepoId`, `sourceLane`,
`targetLane`, `sourceSnapshot`, `targetSnapshot`.

### Collaboration (CLI / agent companions)

- `POST /collaboration/lane-submit` — create (or reuse) an open proposal for a
  lane tip. Required: `projectId`, `title`, `sourceLane`, `sourceSnapshot`.
  Optional: `syncRepoId`, `targetLane`, `authorPrincipal`, Core refs.
  Idempotent for the same `syncRepoId` + `sourceLane` + `sourceSnapshot` while
  status is `open` or `draft` (`{ data, reused }`).
- `GET /collaboration/proposal-summary?projectId=&syncRepoId=` — counts by
  status plus open/draft list.

### Projects

- `GET|POST /projects`
- `GET /projects/:id`

These endpoints accept the same Core/protocol reference fields used by projects:

- `principalRefs` and actor fields such as `ownerPrincipal`, `authorPrincipal`,
  `requestedByPrincipal`, or `runnerPrincipal`
- `policyRefs` for protocol `Policy` or `AgentPolicy` object references
- `grantRefs`, `policyDecisionRefs`, and `auditEventRefs` for Core-owned records

`/admin/policies` records Hub-side metadata and a `policyRef`; policy rules stay
owned by Core/protocol policy objects.

### Sync transport (`/{repoId}/...`)

Per-repo object and ref transport for Core snapshot graphs (content-addressed
BLAKE3 objects, filesystem-backed by default — see Persistence above):

- `GET /{repoId}/refs` — list ref names and snapshot ids (`{ repoId, refs }`)
- `POST /{repoId}/objects/missing` — negotiate missing object ids (`want`, `have`)
- `POST /{repoId}/objects` — upload objects (`repo.object.write` + acting
  principal + `grantRefs`; response `{ stored, skipped }`)
- `GET /{repoId}/objects/{id}` — download one object as `{ id, bytes }` (base64)
- `POST /{repoId}/refs/{name}` — advance a ref (`repo.ref.write`, closure +
  fast-forward + optimistic `expected` checks; response
  `{ name, snapshot, previous }`; ref names may contain `/`, e.g. `lane/main`)

The wire contract is the `sorrel-protocol` sync-transport spec
(`docs/sync-transport.md` there); error envelopes carry `code`, `message`, and
code-specific fields (`missing` for `closure_incomplete`, `current` for
`non_fast_forward` / `expected` mismatches).

Example push:

```sh
curl -sS -X POST "http://localhost:3000/repo_local/objects/missing" \
  -H 'content-type: application/json' \
  -d '{"want":["<snapshot-id>","<tree-id>","<blob-id>"],"have":["<snapshot-id>","<tree-id>","<blob-id>"]}'

curl -sS -X POST "http://localhost:3000/repo_local/objects" \
  -H 'content-type: application/json' \
  -H 'x-sorrel-acting-principal: {"type":"user","id":"user_pusher"}' \
  -d '{"grantRefs":[{"id":"grant_repo_object_write","source":"core"}],"objects":[{"id":"<blob-id>","bytes":"..."}]}'

curl -sS -X POST "http://localhost:3000/repo_local/refs/main" \
  -H 'content-type: application/json' \
  -H 'x-sorrel-acting-principal: {"type":"user","id":"user_pusher"}' \
  -d '{"snapshot":"<snapshot-id>","grantRefs":[{"id":"grant_repo_ref_write","source":"core"}]}'
```

## Domain models

Initial in-memory model factories live in `src/models.js` for:

- Organization
- Project
- Repository
- Proposal
- ReviewComment
- WorkflowRun
- Policy

Organizations, projects, repositories, proposals, review comments, workflow
runs, and policies carry Core principal/resource/policy references where useful.
