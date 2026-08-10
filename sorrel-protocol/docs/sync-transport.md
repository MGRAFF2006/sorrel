# Sorrel sync transport (push / pull)

This document specifies the **wire protocol** a Sorrel client (e.g. `sorrel-cli`)
and a Sorrel remote (e.g. `sorrel-hub`) use to **push** and **pull**
content-addressed objects and to advance **refs** (lanes / HEAD).

It is the network analogue of the local engine: the same `Blob`/`Tree`/
`Snapshot`/`Change` objects, the same content addressing (BLAKE3), the same
policy spine. The remote is a **transport + ref store**, not a new authority:

- The remote stores and serves content-addressed objects and named refs.
- Every mutation is gated by **Core policy** (the remote re-checks allow/deny for
  the action it administers; signature trust stays Core's job).
- The remote **re-verifies** that each uploaded object's id equals
  `BLAKE3(bytes)`. Forged or mislabeled objects are rejected. Canonical project
  data stays portable and syncable — never trapped in a hosted database only.

This is intentionally close to Git's "objects + refs negotiation" but uses
Sorrel's content-addressed object ids and policy model.

## Concepts

- **Object id**: a **64-character lowercase hexadecimal** string — the BLAKE3
  content id of an engine object (`Blob`, `Tree`, `Snapshot`, `Change`, `Lane`,
  ...), exactly as produced by the local store. Uppercase hex, missing
  characters, or non-hex digits are rejected (`invalid_request`). This is the
  same hash the engine uses for `contentHash` with `algorithm: "blake3"`; on the
  wire the id is the bare 64-hex value with no prefix.
- **Ref**: a named pointer in a repo to a snapshot id, e.g. `HEAD` or
  `lane/main`. Refs are the only mutable server state; objects are immutable.
- **Closure**: the transitive set of objects reachable from a snapshot id
  (snapshot → root tree → subtrees → blobs, plus parent snapshots and their
  trees/blobs, recursively). Pushing a ref requires its full closure to be
  present on the remote first. Non-snapshot objects (`Change`, `Lane`, `Stack`,
  `Conflict`, `MergeResult`, `Resolution`, ...) are not part of the snapshot
  closure; clients upload them explicitly when a product flow needs them on the
  remote.

## HTTP conventions

- All endpoints are **JSON over HTTP** under a repo scope `/{repoId}`.
- Request and response bodies use `Content-Type: application/json`.
- `repoId` matches the protocol `RepositoryId` pattern (e.g. `repo_main`,
  `repo_4a91c0de`).
- Path parameters `{id}` are object ids (64-hex lowercase BLAKE3). Path
  parameters `{name}` are ref names (e.g. `HEAD`, `lane/main`); URL-encode `/`
  as needed.
- Errors use the shared error envelope (below). Authenticated principal and
  policy context are carried by the remote's auth layer; the remote **MUST**
  evaluate Core policy for every mutating call (see [Policy actions](#policy-actions)).

### Skeleton auth

Until a production auth layer is wired, skeleton remotes (e.g. early Hub builds)
**MAY** accept an acting principal via the request header. The value is a JSON
object with the principal `type` and `id`:

```http
x-sorrel-acting-principal: {"type":"user","id":"local"}
```

The remote maps this header to a `Principal` for Core policy evaluation. This is
**skeleton auth only** — not a Hub-owned permission model and not suitable for
production. Production remotes MUST replace it with a real auth layer that
binds requests to principals without trusting client-supplied headers alone.

## Policy actions

For each **mutating** endpoint the remote MUST call Core `evaluate()` with a
concrete capability action and a repo-scoped resource before performing the
write. Deny → `policy_denied` (`403`).

| Endpoint | Capability action | Resource scope |
| --- | --- | --- |
| `POST /{repoId}/objects` | `repo.object.write` | `{ "kind": "repo", "id": "<repoId>" }` |
| `POST /{repoId}/refs/{name}` | `repo.ref.write` | `{ "kind": "repo", "id": "<repoId>" }` |

Read-only endpoints (`GET` refs/objects, `POST /objects/missing`) do not require
a mutating policy check in v0; remotes MAY still audit them.

Conformance vectors for both actions live in
`conformance/policy-conformance.json` (`repo_object_write_*`, `repo_ref_write_*`).

Evaluators receive the usual `(principal, capability, resource)` tuple. Example
resource scope for `repo_main`:

```json
{ "kind": "repo", "id": "repo_main" }
```

### Grant references on mutating requests

Mutating request bodies **MUST** include a `grantRefs` array so the remote can
hydrate the client's grants and evaluate Core policy. Each entry is a grant id
plus a source hint (where the remote should look up the grant):

```json
"grantRefs": [{ "id": "grant_repo_object_write", "source": "core" }]
```

Clients send grant refs; the remote resolves them to full `Grant` objects and
calls `evaluate()` before performing the write. Omitting `grantRefs` on a
mutating call is an `invalid_request` for conforming remotes in v0.

## Conformance

Implementations **MUST** re-verify content addressing on every object transfer:

- **Upload** (`POST /objects`): for each entry, recompute `BLAKE3(bytes)` and
  reject when it does not equal the claimed `id` (`object_id_mismatch`).
- **Download** (`GET /objects/{id}`): the client MUST recompute
  `BLAKE3(bytes)` and reject when it does not equal the response `id`.

This matches the local engine store contract. Skipping verification breaks
portability and allows forged objects into a repo.

## Endpoints

### 1. List refs — `GET /{repoId}/refs`

Returns every named ref and its current snapshot id.

**Request:** no body.

**Response `200`:**

```json
{
  "repoId": "repo_main",
  "refs": [
    { "name": "HEAD", "snapshot": "a1b2c3d4e5f6789012345678901234567890abcdef1234567890abcdef123456" },
    { "name": "lane/main", "snapshot": "b2c3d4e5f6789012345678901234567890abcdef1234567890abcdef12345678" }
  ]
}
```

| Status | When |
| --- | --- |
| `200` | Success. `refs` may be empty for a newly created remote repo. |
| `400` | Malformed `repoId` (`invalid_request`). |

---

### 2. Negotiate missing objects — `POST /{repoId}/objects/missing`

The client declares what it wants present on the remote and what it already
believes the remote has. The remote returns the object ids still needed to
complete the **closure** of `want`.

- **`want`**: one or more **snapshot ids** (64-hex). The remote walks each
  snapshot's transitive closure and unions the result.
- **`have`**: zero or more **object ids** of any kind (`Blob`, `Tree`,
  `Snapshot`, `Change`, ...) — an optimization hint. Objects listed in `have`
  are treated as already present on the remote and excluded from `missing`.
  For **push**, the client typically includes the remote ref's current snapshot
  id in `have`. For **pull**, the client includes its local object ids.

**Request:**

```json
{
  "want": ["b2c3d4e5f6789012345678901234567890abcdef1234567890abcdef12345678"],
  "have": ["a1b2c3d4e5f6789012345678901234567890abcdef1234567890abcdef123456"]
}
```

`have` is optional and defaults to `[]`.

**Response `200`:**

```json
{
  "missing": [
    "c3d4e5f6789012345678901234567890abcdef1234567890abcdef1234567890",
    "d4e5f6789012345678901234567890abcdef1234567890abcdef1234567890ab"
  ]
}
```

`missing` is ordered arbitrarily; the client may upload in any order. An empty
array means the remote already has the full closure.

For **pull**, the same endpoint is used: the client sends the remote ref
snapshot as `want` and its own local object ids as `have`; the response is the
set to **download**.

| Status | When |
| --- | --- |
| `200` | Success. |
| `400` | Empty `want`, malformed ids, or unknown snapshot in closure walk (`invalid_request`). |

---

### 3. Upload objects — `POST /{repoId}/objects`

Body is a batch of raw objects. Each entry carries the claimed id and the raw
bytes (standard base64, no line breaks). The remote MUST recompute
`BLAKE3(bytes)` and reject any entry whose id does not match
(`object_id_mismatch`). Writes are content-addressed and idempotent — objects
already present are reported in `skipped`.

**Policy:** `repo.object.write` (see [Policy actions](#policy-actions)).

**Request:**

```json
{
  "grantRefs": [{ "id": "grant_repo_object_write", "source": "core" }],
  "objects": [
    {
      "id": "c3d4e5f6789012345678901234567890abcdef1234567890abcdef1234567890",
      "bytes": "eyJzY2hlbWFWZXJzaW9uIjoic29ycmVsLnByb3RvY29sLnYwIiwia2luZCI6IlRyZWUifQ=="
    }
  ]
}
```

**Response `200`:**

```json
{
  "stored": ["c3d4e5f6789012345678901234567890abcdef1234567890abcdef1234567890"],
  "skipped": []
}
```

| Status | When |
| --- | --- |
| `200` | Batch accepted (individual entries may be `stored` or `skipped`). |
| `400` | Malformed body, invalid base64, or bad id format (`invalid_request`). |
| `400` | Claimed id does not match `BLAKE3(bytes)` (`object_id_mismatch`). |
| `403` | Core policy denied the write (`policy_denied`). |

---

### 4. Download an object — `GET /{repoId}/objects/{id}`

Returns one object by content id.

**Request:** no body. `{id}` must be 64-hex lowercase.

**Response `200`:**

```json
{
  "id": "c3d4e5f6789012345678901234567890abcdef1234567890abcdef1234567890",
  "bytes": "eyJzY2hlbWFWZXJzaW9uIjoic29ycmVsLnByb3RvY29sLnYwIiwia2luZCI6IlRyZWUifQ=="
}
```

The client MUST verify `BLAKE3(bytes) == id` on receipt (see [Conformance](#conformance)).

| Status | When |
| --- | --- |
| `200` | Object found. |
| `400` | Malformed `{id}` (`invalid_request`). |
| `404` | No object with that id (`object_not_found`). |

---

### 5. Advance a ref — `POST /{repoId}/refs/{name}`

Moves a ref to a new snapshot id. The remote MUST:

1. Evaluate Core policy for `repo.ref.write` (deny → `policy_denied`).
2. Verify the new snapshot's **closure is fully present** on the remote
   (`closure_incomplete` with the missing ids otherwise).
3. Enforce the fast-forward rule unless `force` is `true`: the new snapshot must
   be a descendant of the current ref snapshot in the snapshot parent graph
   (`non_fast_forward` otherwise).
4. If `expected` is not `null`, verify it equals the ref's current snapshot
   (optimistic concurrency). A mismatch is `invalid_request` with the actual
   snapshot in `current`.

**Request:**

```json
{
  "grantRefs": [{ "id": "grant_repo_ref_write", "source": "core" }],
  "snapshot": "b2c3d4e5f6789012345678901234567890abcdef1234567890abcdef12345678",
  "expected": "a1b2c3d4e5f6789012345678901234567890abcdef1234567890abcdef123456",
  "force": false
}
```

- `expected`: current snapshot id the client last saw, or `null` to skip the
  check (e.g. creating a new ref).
- `force`: when `true`, skip the fast-forward ancestry check (non-ancestor
  updates allowed). Policy still applies; closure must still be complete.

**Response `200`:**

```json
{
  "name": "lane/main",
  "snapshot": "b2c3d4e5f6789012345678901234567890abcdef1234567890abcdef12345678",
  "previous": "a1b2c3d4e5f6789012345678901234567890abcdef1234567890abcdef123456"
}
```

`previous` is `null` when the ref did not exist before this call.

| Status | When |
| --- | --- |
| `200` | Ref updated. |
| `400` | Malformed body or `expected` does not match current ref (`invalid_request`; may include `"current"`). |
| `403` | Core policy denied the write (`policy_denied`). |
| `404` | Ref name unknown when `expected` is non-null and ref is missing (`unknown_ref`). |
| `409` | New snapshot closure not fully on remote (`closure_incomplete`). |
| `409` | New snapshot is not a descendant and `force` is `false` (`non_fast_forward`). |

---

## Operation flows

**push** (client → remote):

1. `GET /{repoId}/refs` to learn the remote ref snapshot (include it in `have`).
2. `POST /{repoId}/objects/missing` with `want=[localSnapshot]`,
   `have=[remoteSnapshot, …localHints]`.
3. `POST /{repoId}/objects` to upload the returned `missing` set (any order;
   the remote validates ids).
4. `POST /{repoId}/refs/{name}` with `snapshot=localSnapshot` and
   `expected=remoteSnapshot`.

**pull** (remote → client):

1. `GET /{repoId}/refs`.
2. `POST /{repoId}/objects/missing` with `want=[remoteSnapshot]`,
   `have=[localObjectIds…]` to learn what to download.
3. `GET /{repoId}/objects/{id}` for each missing id (verify content id).
4. Update the local lane/HEAD pointer to `remoteSnapshot`.

**clone** = create an empty local repo, then **pull** the default lane.

See [examples/sync/push-flow.json](../examples/sync/push-flow.json) for a
concrete push walkthrough with request/response bodies.

## Error envelope

All error responses use `Content-Type: application/json` and this shape:

```json
{
  "error": {
    "code": "<code>",
    "message": "human readable"
  }
}
```

Additional top-level keys on the error object are allowed for specific codes
(documented below). Clients should read `error.code` first.

### `policy_denied` — `403`

Core policy evaluation returned deny (or `needs_grant` / `require-approval` —
treated as deny for sync writes).

```json
{
  "error": {
    "code": "policy_denied",
    "message": "principal lacks repo.object.write on repo_main"
  }
}
```

### `object_id_mismatch` — `400`

Uploaded bytes do not hash to the claimed id.

```json
{
  "error": {
    "code": "object_id_mismatch",
    "message": "object c3d4e5f6…7890: BLAKE3(bytes) does not match claimed id"
  }
}
```

### `object_not_found` — `404`

No object with the requested id.

```json
{
  "error": {
    "code": "object_not_found",
    "message": "no object with id c3d4e5f6789012345678901234567890abcdef1234567890abcdef1234567890"
  }
}
```

### `closure_incomplete` — `409`

Ref advance blocked because the new snapshot's closure is not fully stored.

```json
{
  "error": {
    "code": "closure_incomplete",
    "message": "snapshot b2c3d4e5…5678 closure is missing 2 object(s)",
    "missing": [
      "c3d4e5f6789012345678901234567890abcdef1234567890abcdef1234567890",
      "d4e5f6789012345678901234567890abcdef1234567890abcdef1234567890ab"
    ]
  }
}
```

### `non_fast_forward` — `409`

Ref advance blocked because the new snapshot is not a descendant of the current
ref snapshot and `force` is `false`.

```json
{
  "error": {
    "code": "non_fast_forward",
    "message": "snapshot b2c3d4e5…5678 is not a descendant of ref lane/main",
    "current": "a1b2c3d4e5f6789012345678901234567890abcdef1234567890abcdef123456"
  }
}
```

### `unknown_ref` — `404`

Ref name does not exist when required (e.g. `expected` is set but the ref was
never created).

```json
{
  "error": {
    "code": "unknown_ref",
    "message": "ref lane/feature does not exist"
  }
}
```

### `invalid_request` — `400`

Malformed input: bad id format, empty `want`, invalid JSON, `expected` mismatch,
etc.

```json
{
  "error": {
    "code": "invalid_request",
    "message": "want must contain at least one snapshot id"
  }
}
```

When `expected` does not match the current ref:

```json
{
  "error": {
    "code": "invalid_request",
    "message": "ref lane/main expected snapshot does not match current",
    "current": "a1b2c3d4e5f6789012345678901234567890abcdef1234567890abcdef123456"
  }
}
```

## Remote storage (deployment-agnostic)

The remote's object store is defined by behavior, not by a single backend, so
Sorrel can deploy in many environments (single host, container, cloud). A
conforming remote MUST provide, behind one interface:

- `has(id) -> bool`
- `get(id) -> bytes` (content-verified on read)
- `put(bytes) -> id` (content-addressed; rejects on mismatch when an id is
  asserted)
- `listRefs() / getRef(name) / setRef(name, snapshot, expected, force)`

The reference Hub implementation ships a filesystem-backed store first; an
in-memory store and cloud/object-store backends are valid alternatives as long
as they satisfy this contract and the content-verification + policy rules above.

## Shadow mode (linked instances) — PLANNED, not yet implemented

To support high availability, the spec reserves a **shadow mode** for linked
remotes/instances: two or more remotes are configured as peers over the same
repos so that if one goes down another can take over serving push/pull.

Intended (future) shape — **do not implement yet**:

- Remotes advertise peers and a replication role (`primary` / `shadow`).
- Object writes replicate to shadows (objects are immutable + content-addressed,
  so replication is convergent and conflict-free).
- Ref advances use the same fast-forward + policy rules; a shadow promoted to
  primary continues serving from the last replicated ref state.
- Clients may be handed a peer list and fail over transparently on the next
  request.

Only the object/ref transport above is in scope for the initial implementation.
Shadow/failover is documented here so backends and ref handling are designed
with replication in mind, but it is explicitly deferred.
