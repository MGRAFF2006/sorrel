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

The `v0.1.0-alpha.2` server has development, WorkOS, and OIDC AuthAdapter seams,
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

Filesystem storage requires each complete percent-encoded component to fit
255 bytes: repository IDs and ref names include all escaped bytes; metadata IDs
also include the `.json` suffix. Oversized names return HTTP 400 with
`filesystem_name_too_long` before writing. Filesystems with lower component
limits can also return this error. The memory store keeps its existing identifier
contract; storage encoding and existing filenames are unchanged.

Unexpected HTTP 500 failures emit a server diagnostic with the HTTP method,
fixed category, and recognized filesystem error code. Raw exceptions and
request data are omitted; the client response remains redacted.

### Authentication adapters

`SORREL_HUB_AUTH` selects one of these request-authentication adapters:

- `dev` (default) provides no identity until `SORREL_HUB_LOCAL_DEMO=1` explicitly
  enables development headers and anonymous `user:local` demo sessions. It is
  restricted to loopback unless the insecure-demo bind override is explicit.
- `oidc` verifies RS256/ES256 Bearer JWTs using
  required `SORREL_OIDC_ISSUER` and `SORREL_OIDC_AUDIENCE`; keys are read from
  `<issuer>/.well-known/jwks.json`. Issuer-only configuration fails closed until
  an audience identifying this Hub is supplied. Keys are cached for ten minutes.
  An unknown signing-key ID triggers a shared refresh, limited to once per issuer
  URI every 30 seconds (including failed refreshes); HTTP fetches time out after
  five seconds. A failed refresh rejects the new key while previously cached keys
  remain usable until cache expiry. A rotation during cooldown may require a
  retry after the remaining cooldown.
- `workos` uses `WORKOS_API_KEY` and `WORKOS_CLIENT_ID` to verify AuthKit
  Bearer JWTs. It requires a string `client_id` exactly matching
  `WORKOS_CLIENT_ID` and reads keys from
  `<issuer>/sso/jwks/<encoded-client-id>`. The issuer defaults to
  `https://api.workos.com`; `WORKOS_ISSUER` overrides it. AuthKit tokens do not
  require `aud`; optional `WORKOS_AUDIENCE` adds an `aud` restriction without
  replacing the `client_id` check. See the
  [WorkOS session-token contract](https://workos.com/docs/reference/authkit/session-tokens).

OIDC and WorkOS tokens must contain a finite numeric `exp` claim. Generic OIDC
also requires an `aud` claim matching its configured audience (a string or
array of strings). Expiry retains the existing 60-second clock-skew allowance.
Tokens without expiry are rejected.

These adapters authenticate a principal; authorization still requires trusted
Core grant references. WorkOS remains an adapter skeleton without sealed
sessions, and the browser UI does not provide an IdP login flow in this alpha.

Every collaboration, metadata and sync read or write requires an authenticated
session and a native Core allow decision. Without one, private endpoints return
`401 authentication_required`. Verified AuthAdapter identity wins over acting
headers and claimed authors; organization ownership and mutation attribution are
bound to that identity. Only `/healthz`, `/capabilities` and `/session` discovery
remain public. No credentials are returned by discovery.

`SORREL_HUB_LOCAL_DEMO=1` with `auth=dev` is the explicit compatibility mode for
isolated local clients: absent headers act as `user:local`, and supplied acting
headers select a development identity. Actual configured Core grants are still
required. Set `SORREL_HUB_BOOTSTRAP_GRANTS=1` as well to provision the local demo
read/write grants. This flag never supplies a development identity in OIDC or
WorkOS mode.

### Private route capabilities

Capabilities are native Core strings. Resources are exact Core kind/id pairs;
an organization grant does not implicitly grant access to its projects, and a
project grant does not implicitly grant repository byte access. Operators may
use native resource id `*` deliberately. All effective grants and policies apply,
including denies that clients omit from references.

| Routes | Capability | Resource |
| --- | --- | --- |
| `GET /projects`, `/projects/:id` | `project.read` | Each stored project id |
| `POST /projects` | `project.create` | Requested organization namespace id |
| `POST /projects/:id/repositories` | `project.read`, `project.write`, `repo.read` | URL project id; requested sync repository id |
| Organization metadata GET / POST | `org.read` / `org.write` | Stored/new organization id |
| Repository metadata GET | `repo.read` | Stored Hub repository record id |
| Repository metadata POST | `project.read`, `policy.grant` | Actual stored parent project id; organization must match |
| Policy metadata GET / POST | `policy.read` / `policy.grant` | Stored parent project id, or organization namespace id |
| Proposal GET / POST / PATCH | `proposal.read` / `proposal.write` | Actual stored parent project id |
| Approve, reject or merge a proposal | Also `proposal.review` | Same actual project id |
| Review comments GET / POST / PATCH | `review.comment.read` / `review.comment.write` | Stored parent proposal's project id |
| Workflow runs GET / POST / PATCH | `workflow.run.read` / `workflow.run.write` | Actual stored parent project id |
| `/collaboration/lane-submit`, `/collaboration/proposal-summary` | `proposal.write` + `proposal.read` / `proposal.read` | Each actual project id |
| `GET /admin/sync-repos`, `/:repo/refs`, `/:repo/objects/:id`, `/:repo/tree`, `/:repo/files`, `POST /:repo/objects/missing` | `repo.read` | Each sync repository id |
| `GET /admin/proposals/:id/changes` | `proposal.read` + `repo.read` | Stored project id; stored sync repository id |
| `POST /:repo/objects`, `POST /:repo/refs/*` | `repo.object.write` / `repo.ref.write` | URL sync repository id |

Metadata creation and PATCH require the corresponding read capability as well
as write capability on their existing parent scope. New organizations and
projects use their creation capability. Comments additionally require reading
the actual parent proposal; runs linked to a proposal require `proposal.read`.
Proposal repository/workflow-run links require reading those actual targets and
matching their stored project. A Hub repository metadata id can differ from its
sync repository id: provision `repo.read` for both scopes when needed. None of
these metadata links supplies Core authority.

Lists, nested comments and summary counts contain only authorized records.
Unreadable records and records with missing or inconsistent local
project/proposal/repository parents return the same `404 not_found` response
as missing records. Organization namespace ids do not require local organization
records; external Core reference hydration remains separate. Sync authorization runs before looking up objects/refs, so
private and absent repository scopes have the same denial. Snapshot comparison
also requires whole-repository byte access. Subprocess errors propagate rather
than producing a successful empty list. Within one request, repeated identical
action/resource decisions are reused; request authorization references are
validated separately on sync and privileged administration mutations.

For example, an operator can configure this native project-reader record under
its matching key in `SORREL_HUB_TRUSTED_GRANTS_FILE`:

```json
{
  "grant_project_reader": {
    "schemaVersion": "sorrel.protocol.v0",
    "kind": "Grant",
    "id": "grant_project_reader",
    "principal": { "kind": "user", "id": "oidc:member-subject" },
    "capabilities": ["project.read", "proposal.read", "review.comment.read", "workflow.run.read"],
    "resource": { "kind": "project", "id": "proj_example" },
    "effect": "allow"
  }
}
```

Creating a project does not automatically provision grants for the new id.
Grant/policy metadata creation does not modify the configured effective records.

### Trusted grants (sync push/pull)

Mutating sync routes and privileged repository/policy administration evaluate
authorization through the packaged `sorrel-core-policy` Rust executable. Hub has
no JavaScript policy evaluator. The adapter calls Core `evaluate_policy` with all
configured trusted grants and policies, including denies omitted from request
references. Only a native Core `allow` decision authorizes the operation. The
server has no local bootstrap grants by default. For local development only,
the explicit opt-in below lets the CLI acting principal
`{"type":"user","id":"local"}` push/pull without a separate grant service:

- `grant_local_repo_read` → `repo.read`
- `grant_local_object_write` → `repo.object.write`
- `grant_local_ref_write` → `repo.ref.write`

Environment:

- `SORREL_HUB_BOOTSTRAP_GRANTS=1` — enable the development-only,
  repo-wide bootstrap grants. With `SORREL_HUB_LOCAL_DEMO=1`, also provision
  the native local organization/project collaboration grants. No other value enables them.
- `SORREL_HUB_TRUSTED_GRANTS_FILE` — path to a JSON object of extra
  `id → grant` records merged on top of bootstrap grants.
- `SORREL_HUB_TRUSTED_POLICIES_FILE` — path to a JSON object of native Core
  `id → policy` records. Every configured policy participates in evaluation.
- `SORREL_HUB_CORE_POLICY_BIN` — explicit executable path when using a packaged
  or separately built adapter. Local runs default to the workspace debug binary
  under `CARGO_TARGET_DIR` (or `target`); `npm run setup` and Hub `npm test` build it.

Trusted grants use native Core shapes: `schemaVersion: "sorrel.protocol.v0"`,
`kind: "Grant"`, `principal: { kind, id }`, string `capabilities`, a concrete
`resource: { kind, id }`, and an explicit `effect`. Concrete plural `resources`
are also supported. Older `action` / `principal.type` records require explicit
`effect` and `resource`; omitted effects or universal missing resources are rejected.
Grant `status`, `issuedAt`, `expiresAt`, and `revokedAt` are enforced in the
Core-owned adapter. Invalid dates/versions, unresolved protocol object references,
nonempty conditions, unsupported fields, and path-scoped resources fail closed.
Native policies use Core `resource`, `rules`, and optional `defaultDecision`.
Service IDs map to `service:<id>` and workflow IDs to `workflow:<id>` in Core's
service domain, consistently for native and legacy inputs, keeping them distinct.

The configured files are an operator trust boundary; Hub does not verify their
authority-chain signatures. Request `authorityRootRef` is metadata, not proof of
verified authority. Request policy references must resolve to configured policies;
request grant/policy payloads never supply authority. Each subprocess has a
five-second timeout and one-MiB request/response limit; at most 16 run concurrently.
An unavailable, invalid, overloaded, or timed-out adapter fails closed with
`503 policy_evaluation_failed`. Denials carry a native Core `PolicyDecision`.

The CLI sends matching `grantRefs` on `POST /objects` and `POST /refs/*`.
Because the bootstrap grants match every repository id, never enable them for
an internet-facing or multi-user Hub. They are a development convenience, not
production authentication or authorization provisioning.

For a local CLI-compatible development server:

```sh
SORREL_HUB_LOCAL_DEMO=1 SORREL_HUB_BOOTSTRAP_GRANTS=1 npm start
```

### Deployment

Coordinated releases publish the API image for Linux amd64 and arm64 as
`ghcr.io/mgraff2006/sorrel-hub:<VERSION>`. The same GitHub Release attaches a
Compose file for this API plus `sorrel-hub-web`, the image digests, and asset
checksums. See [`docs/GETTING_STARTED.md`](../docs/GETTING_STARTED.md#host-a-release-server)
for the verified download and hosting flow.

To build the API image from the current checkout instead:

```sh
docker build -t sorrel-hub --file Dockerfile ..
docker run --rm -p 3000:3000 \
  -e HOST=0.0.0.0 \
  -v hub-data:/app/data \
  sorrel-hub
```

The release image runs as an unprivileged user, persists only `/app/data`, and
has a built-in `/healthz` container health check.

Binding `0.0.0.0` is required for a published Docker port, but it does not
enable bootstrap grants or add authentication. Local Docker E2E that pushes
must additionally pass `-e SORREL_HUB_LOCAL_DEMO=1` and
`-e SORREL_HUB_BOOTSTRAP_GRANTS=1`. Likewise, root-repo
E2E must opt in explicitly:

```sh
SORREL_HUB_LOCAL_DEMO=1 SORREL_HUB_BOOTSTRAP_GRANTS=1 npm test
```

The same variable must be forwarded when an E2E harness spawns
`scripts/listen.mjs`. Docker and root E2E opt-in is development-only; do not use
these settings as a public deployment recipe. OIDC bearer verification is
available, but the alpha still lacks a complete production login/session flow.

The sync on-disk layout mirrors Core's `FileObjectStore` semantics:
content-addressed fanout (`<repo>/objects/<id[0..2]>/<id>`), atomic
temp-file + rename writes, digest-verified reads, and one JSON document per
ref under `<repo>/refs/`.

Ref reads validate the JSON record, its name against the filename, and its
64-character hexadecimal snapshot id. Corrupt records fail with a server error;
they are never treated as absent or overwritten by a ref advance. The file is
preserved for diagnosis. Restore a known-good ref record from a backup before
retrying the affected operation.

Product metadata (organizations, projects, repositories, proposals, review
comments, workflow runs, policies) is stored as one JSON document per record
under `<metadataDir>/<collection>/<id>.json`, also written atomically.
Filesystem-backed metadata becomes visible to requests only after persistence
succeeds; failed creates, updates, and project/repository links leave the prior
in-memory records intact so a failed request can be retried. Duplicate record
IDs return `409`.
On restart, metadata records load only when their canonical encoded filename
matches their valid record ID; malformed identities and alias filenames are
skipped with a diagnostic that omits record contents. Files remain untouched.
This identity check does not validate or repair every collection field; backups
with malformed field schemas still need separate validation before restoration.

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

- `slug` (optional string; omitted, null, or blank derives it from the name)
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
- `GET|PATCH /admin/proposals/:id` — detail; PATCH status (`draft`→`open`→`approved`/`rejected`/`merged`/`closed`) and editable fields. Repository, branch, lane, and snapshot inputs cannot change in an update involving approved or merged status (`400 model_validation_failed`); title and description remain editable. Reopen an approved proposal in a separate status-only update before replacing its inputs.
- `GET /admin/proposals/:id?include=comments` — proposal plus nested review comments
- `GET /admin/proposals/:id/comments` — comments only
- `GET|POST /admin/review-comments` (filters: `proposalId`, `state`)
- `GET|PATCH /admin/review-comments/:id` — resolve via `{ "state": "resolved" }`
- `GET|POST /admin/workflow-runs` (filters: `projectId`, `proposalId`, `status`)
- `GET|PATCH /admin/workflow-runs/:id` — status updates for one workflow attempt
- `GET|POST /admin/policies`

Metadata with a `projectId` must refer to an existing Hub project. Repository and
project-scoped policy organization IDs must match that project's organization
namespace. Proposal repository IDs and workflow-run proposal IDs must belong to
the same project; proposal workflow-run links must resolve in that project and
cannot identify a run attached to another proposal. Missing parents return
`404 not_found`; scope mismatches return `400 model_validation_failed` before
any record is saved. Project organization IDs remain namespace identifiers and
do not require a local organization record. Core references and external sync,
lane, snapshot, and provider IDs do not require local metadata records.
- `GET /admin/policies/:id`
- `GET /admin/sync-repos` — sync transport repos (`{ "repos": [ { "id", "refCount" } ] }`)

Workflow-run updates allow `queued` → `in_progress`, `failed`, or `cancelled`,
and `in_progress` → `succeeded`, `failed`, or `cancelled`. A terminal attempt
cannot switch to another status; create a new run record for another attempt.
Same-status updates and metadata/provider-id edits remain valid. Repeating a
terminal status preserves `startedAt`; `completedAt` is preserved unless an
explicit completion-time correction is supplied. Invalid transitions return
`400 model_validation_failed` without changing the record. POST still accepts
any known status for imported runs.

Proposal records may carry lane-submit fields: `syncRepoId`, `sourceLane`,
`targetLane`, `sourceSnapshot`, `targetSnapshot`.

### Collaboration (CLI / agent companions)

- `POST /collaboration/lane-submit` — create (or reuse) an open proposal for a
  lane tip. Required: `projectId`, `title`, `sourceLane`, `sourceSnapshot`.
  Optional: `syncRepoId`, `targetLane`, `authorPrincipal`, Core refs.
  Idempotent for the same `projectId` + `syncRepoId` + `sourceLane` + `sourceSnapshot` while
  status is `open` or `draft` (`{ data, reused }`).
- `GET /collaboration/proposal-summary?projectId=&syncRepoId=` — counts by
  status plus open/draft list.

### Projects

- `GET|POST /projects`
- `GET /projects/:id`

- `POST /projects/:id/repositories` with `{ "syncRepoId": "repo_…" }` links
  an already synchronized repository to a project. Repeating the link is
  idempotent; unknown repositories or projects return 404. The updated project
  is returned as `{ "data": ... }` with its `repositoryIds`.
- `GET /admin/proposals/:id/changes` compares the proposal's recorded
  `targetSnapshot` (before) and `sourceSnapshot` (after), returning
  `{ "data": { "repoId", "sourceSnapshot", "targetSnapshot", "changes" } }`.
  Each change includes `path`, `status` (`added`, `modified`, `deleted`), and
  `before`/`after` previews with `content`, optional `objectId`, and optional
  `reason`. Binary and files above 512 KiB have null content; total text is
  bounded to 4 MiB, snapshots to 10,000 entries, and comparisons to 500 changed
  files. Missing comparison snapshots return 409; excessive counts return 413.
  This endpoint never performs a merge or advances a ref. Proposal status
  updates likewise record metadata only.
- `/capabilities` includes `collaboration.proposalTransitions`, derived from
  the same state-transition rules that validate proposal mutations.

Admin proposal creation, updates, and lane submissions share verified
attribution and best-effort Convex mirroring. Lane-submit reuse is scoped to
the project as well as repository, lane, and source snapshot.

These endpoints accept the same Core/protocol reference fields used by projects:

- `principalRefs` and actor fields such as `ownerPrincipal`, `authorPrincipal`,
  `requestedByPrincipal`, or `runnerPrincipal`
- `policyRefs` for protocol `Policy` or `AgentPolicy` object references
- `grantRefs`, `policyDecisionRefs`, and `auditEventRefs` for Core-owned records

`/admin/policies` records Hub-side metadata and a `policyRef`; policy rules stay
owned by Core/protocol policy objects.

Typed metadata fields are validated before persistence. Supplied `createdAt` and
`updatedAt` values must be strings; absent/null values use the server timestamp.
Creation `metadata` must be an object (absent/null becomes `{}`), and its
extension keys and nested JSON values remain unrestricted. PATCH metadata must
be an object and merges into the existing metadata. Review-comment `line` is
optional and must be a positive safe integer; null clears it on PATCH. Policy
`enabled` must be a boolean; absent/null keeps the default `true`. Invalid
field types return `400 model_validation_failed` without changing memory or
disk records. This does not add date-format or workflow-transition rules.

### Sync transport (`/{repoId}/...`)

Per-repo object and ref transport for Core snapshot graphs (content-addressed
BLAKE3 objects, filesystem-backed by default — see Persistence above):

- `GET /{repoId}/refs` — list ref names and snapshot ids (`{ repoId, refs }`)
- `GET /{repoId}/tree?ref=main&path=src` — resolve a named ref and list the
  normalized entries in a protocol Tree; `path` is optional and relative to
  the snapshot root
- `GET /{repoId}/files?ref=main&path=README.md` — resolve a Sorrel Blob and
  return at most 512 KiB of valid UTF-8 text with snapshot context
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
Ref updates reject non-snapshot targets and malformed snapshot/tree links with
`422 invalid_sync_object` before changing the ref.

Tree and file reads reject absolute paths, traversal segments, backslashes,
non-protocol object kinds, non-Blob payloads, invalid UTF-8, and oversized text
previews. They expose structured repository content rather than raw object
bytes and follow the same read boundary as refs and object downloads.

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
