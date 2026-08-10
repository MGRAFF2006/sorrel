# Sorrel Protocol

This package contains the first JSON Schema specification bundle for Sorrel
protocol objects. The schemas define the portable contracts shared by Sorrel
Core, Sorrel Hub, SDKs, runners, vault integrations, and agent tooling.

## Package layout

```text
sorrel-protocol/
  schemas/
    sorrel-object.schema.json
  examples/
    blob.json
    tree.json
    snapshot.json
    change.json
    lane.json
    stack.json
    slice.json
    conflict.json
    merge-result.json
    resolution.json
    secret-ref.json
    workflow.json
    runner.json
    policy.json
    principal-agent.json
    capability-path-write.json
    resource-docs-path.json
    grant-agent-docs-write.json
    policy-decision-agent-docs-write.json
    audit-event-secret-injection.json
    agent-policy.json
    authority-root-repo-main.json
    policy-authority-root.json
    policy-change-delegated-grant-allowed.json
    policy-change-self-grant-denied.json
    policy-change-scope-broadening-denied.json
    policy-change-authority-rotation.json
    policy-change-unsigned-untrusted.json
    capability-policy-grant.json
    capability-policy-delegate.json
    capability-authority-admin.json
    capability-authority-rotate.json
    invalid/
  docs/
    validation.md
    merge-conflicts.md
```

The schema bundle currently covers:

- `Blob`
- `Tree`
- `Snapshot`
- `Change`
- `Lane`
- `Stack`
- `Slice`
- `Conflict`
- `MergeResult`
- `Resolution`
- `Principal`
- `Capability`
- `ResourceRef`
- `Grant`
- `SecretRef`
- `Workflow`
- `Runner`
- `Policy`
- `PolicyDecision`
- `AuditEvent`
- `AgentPolicy`
- `AuthorityRoot`
- `PolicyChange`
- `Workspace` (local repo manifest + optional `componentLinks`)

## Workspace component links

See [docs/workspace-links.md](docs/workspace-links.md) for **`member`** (branch-tracked
monorepo modules) vs **`dependency`** (revision/tag-pinned) links. Example:
[examples/workspace.json](examples/workspace.json).

## Merge conflicts

Three-way merges that touch the same path produce content-addressed `Conflict`
objects and a summarizing `MergeResult` instead of failing. See
[docs/merge-conflicts.md](docs/merge-conflicts.md).


## Decentralized authority

`AuthorityRoot` anchors decentralized governance for a scope with weighted
threshold authorities and a linked root `Policy`. `PolicyChange` proposes
updates from a `previousPolicyRoot` to a `proposedPolicyRoot` with typed
operations (`policy.grant`, `policy.delegate`, `authority.admin`,
`authority.rotate`) and optional authority signatures.

Core evaluates every `PolicyChange` against `evaluationPolicyRoot`, which must
reference the previous effective policy root. Permissions introduced by the
proposed change must not be used during evaluation.

Run schema and semantic tests:

```bash
npm run validate
npm test
```

## Headless Core permission spine

The permission schemas describe portable authorization data owned by headless
Sorrel Core:

- `Principal` identifies a Sorrel actor such as an agent, runner, workflow,
  team, service, user, or system process.
- `Capability` names an action Core can authorize, such as `path.write`,
  `workflow.run`, or `secret.inject`.
- `ResourceRef` points at a protocol scope or object without loading the target.
- `Grant` links principals, capabilities, and resources with an authorization
  effect and lifecycle status.
- `Policy` groups grant-like rules for a scope.
- `PolicyDecision` records Core's evaluation of a request.
- `AuditEvent` records authorization-relevant events.
- `SecretRef` and `RedactionMetadata` carry secret handles and redaction
  instructions without raw secret material.

These objects are not a production login system and do not model hosted identity
providers. Sorrel Hub consumes, displays, indexes, and routes these Core-owned
authorization objects; Hub does not own the permission model or become the
identity source of truth.

## Protocol versioning

Sorrel protocol objects carry two version identifiers:

1. `schemaVersion` on every object, for example `sorrel.protocol.v0`.
2. The JSON Schema `$id`, for example
   `https://schemas.sorrel.dev/protocol/v0/sorrel-object.schema.json`.

`schemaVersion` is the compatibility boundary for persisted and exchanged
objects. Tooling must reject objects with an unknown `schemaVersion` unless the
caller explicitly enables a migration or compatibility mode.

See [docs/COMPATIBILITY.md](docs/COMPATIBILITY.md) for the
`sorrel.protocol.v0` compatibility boundary, explicit breaking-change policy,
and migration expectations.

### Version policy

This first package uses `sorrel.protocol.v0`, which is pre-stable. During `v0`,
schemas may change while the object model is being proven, but changes should
still be documented in commits and reflected in examples.

Once `sorrel.protocol.v1` exists:

- Breaking changes require a new protocol namespace, such as
  `sorrel.protocol.v2`, and a new schema `$id` path.
- Non-breaking changes may add optional fields, enum values, or new object
  types without changing existing required fields.
- Removing a field, changing a field type, changing object identity semantics,
  or tightening validation in a way that rejects previously valid objects is
  breaking.
- Consumers should validate against the exact major protocol version they
  persist or exchange.

### Object identity and references

The schemas intentionally separate object identity from storage hashing:

- `id` is the stable Sorrel object identifier used by APIs and examples.
- `contentHash` records content-addressed storage hashes where applicable.
- References between objects use `{ "kind": "...", "id": "..." }` so readers
  can validate the expected target type without loading the target object.

Future implementations may use BLAKE3, SHA-256, Git SHA compatibility mappings,
or other storage-specific indexes behind these protocol fields.

## Validation

Install dependencies and validate the schema bundle plus examples:

```bash
npm install
npm run validate
```

Validate only example objects:

```bash
npm run validate:examples
```

Valid examples live directly under `examples/`. Negative fixtures live under
`examples/invalid/` and must fail schema validation.

See [docs/validation.md](docs/validation.md) for direct validator commands, CI
guidance, and notes for SDK implementers.

## Policy conformance sync

`conformance/policy-conformance.json` is the canonical, language-neutral policy
conformance manifest vendored by every consumer (Core, CLI, Hub, Runners,
Vault). A sidecar `conformance/policy-conformance.meta.json` records the manifest
version and a SHA-256 so vendored copies can detect drift as a test failure.

```bash
npm run sync:meta                                  # regenerate the sidecar after editing the manifest
npm run validate:conformance                        # fail if the sidecar is stale (also part of npm run validate)
npm run export:conformance -- <consumer-dir>        # copy manifest + sidecar into a consumer checkout
```

See [docs/policy-conformance.md](docs/policy-conformance.md) for the full sync
workflow and the root-level `scripts/sync-conformance.sh` helper.

## Sync transport (push / pull)

See [docs/sync-transport.md](docs/sync-transport.md) for the canonical
**push/pull** wire protocol between a Sorrel client (`sorrel-cli`) and a Sorrel
remote (`sorrel-hub`): five JSON-over-HTTP endpoints for ref listing,
`want`/`have` closure negotiation (snapshot ids in `want`, any object ids in
`have`), object upload/download with mandatory BLAKE3 re-verification, and
policy-gated (`repo.object.write`, `repo.ref.write`) fast-forward ref advancement.
Object ids are 64-hex lowercase BLAKE3 content ids matching the local engine.
A full push walkthrough lives in [examples/sync/push-flow.json](examples/sync/push-flow.json).
The remote is a transport + ref store, not a new authority. **Shadow mode** for
linked instances / failover is reserved but not yet implemented.
