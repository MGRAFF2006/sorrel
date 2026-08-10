# Protocol compatibility

## `sorrel.protocol.v0`

`schemaVersion: "sorrel.protocol.v0"` identifies the pre-stable protocol
namespace. An object is compatible only when a consumer recognizes that
namespace and validates it with a schema bundle that accepts the object.

For `v0`, sharing the namespace does not guarantee that every package release
is mutually compatible. Producers and consumers should use the same
`@sorrel/protocol` release, or verify compatibility with the exact schemas,
examples, and policy-conformance metadata they vendor. Consumers must reject an
unknown `schemaVersion` unless the caller explicitly selects a supported
migration or compatibility mode.

Additive changes are expected to preserve existing objects where practical.
Examples include new optional fields and new object kinds. Consumers should
ignore unknown optional fields unless a schema or security boundary requires
strict rejection.

## Breaking changes

A change is breaking when an existing valid object or documented behavior can
no longer be consumed with the same meaning. This includes:

- adding or removing required fields;
- removing fields or enum values, changing field types, or narrowing accepted
  values;
- tightening validation so previously valid objects are rejected;
- changing object identity, content-hash, reference, policy-decision, or
  authorization semantics; and
- changing conformance cases in a way that reverses an expected decision.

Every breaking `v0` change must be explicit in the changelog and release notes.
The change must identify affected object kinds and consumers, explain why it is
breaking, and provide migration guidance. It must not be presented as a
transparent patch.

## Migration expectations

When persisted or exchanged objects need migration, the release introducing the
break must provide:

1. the exact source and target package/protocol versions;
2. deterministic field and semantic transformations;
3. representative before-and-after fixtures, including failure cases;
4. guidance for updating vendored schemas and conformance metadata; and
5. a rollback or backup expectation for persisted data.

Migration must be explicit and opt-in. Implementations must not silently
reinterpret an object under a newer schema or write mixed old/new forms as if
they were equivalent. A migration should validate the source before conversion
and validate the result against the target bundle.

Once a stable namespace is introduced, incompatible changes require a new
namespace (for example, `sorrel.protocol.v1` to `sorrel.protocol.v2`) and a new
schema `$id` path.
