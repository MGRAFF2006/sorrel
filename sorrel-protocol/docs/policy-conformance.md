# Policy evaluator conformance

Sorrel has several policy/authorization consumers that must agree on the same
decisions for the same inputs:

- `sorrel-core` — the canonical Rust authority/permission evaluator.
- `sorrel-cli` — owns a CLI-facing evaluator with different type shapes while
  consuming `sorrel-core` as a workspace dependency for engine behavior.
- `sorrel-hub` — a JavaScript administration guard mirroring Core semantics.
- `sorrel-runners` — a `CorePermissionEvaluator` gate before bundle execution.
- `sorrel-vault` — an injected `corePolicy.evaluate()` adapter for secrets.

Without a shared contract these implementations drift. This package provides the
shared contract.

## Short-term shared contract (decision)

**Protocol fixtures are the canonical conformance source.**

We considered three options:

1. Protocol fixtures as canonical conformance tests. *(chosen)*
2. Generated JS/Rust fixtures from `sorrel-protocol`.
3. A shared SDK/package path.

Option 1 is the lightest and matches the current state of the repos: the
authority/permission decision fixtures already live in
`sorrel-protocol/examples`. Options 2 and 3 add build/release wiring that is not
justified while Core is still embedded in the CLI and Hub/Vault mirror Core in
JavaScript.

The contract is expressed as a single language-neutral manifest:

```
conformance/policy-conformance.json
```

It contains two sets of vectors:

- `permissionDecisions` — direct `(principal, capability, resource, grants)` to
  `allow | deny | needs_grant` cases, covering `path.write`, `workflow.run`,
  `secret.read`, and `secret.inject`.
- `policyChanges` — signed `PolicyChange` evaluation cases producing a
  `(trust, outcome)` pair, covering denied self-grant, unsigned/forged changes,
  delegated grant allowed, scope-broadening denied, and authority rotation
  thresholds.

Each case lists the `sourceFixtures` it derives from so the vectors stay aligned
with the example objects under `examples/`.

## How consumers use it

Each consumer copies (vendors) `conformance/policy-conformance.json` into its own
test tree and adds a thin mapping layer from the manifest case shape to its own
evaluator inputs, then asserts the evaluator returns the manifest `expected`
value. Because the manifest is small and stable, vendoring is acceptable until a
release/package path exists. When a consumer updates its vendored copy it must
re-run conformance.

## Keeping vendored copies in sync (no manual drift)

Vendoring used to mean a consumer's copy could silently drift from the canonical
manifest. To make drift a **test failure** instead of a silent gap, the protocol
publishes a tiny sidecar next to the manifest:

```
conformance/policy-conformance.meta.json
```

It records the manifest version (`manifestVersion`, derived from the manifest
`id`), its `schemaVersion`, and a SHA-256 over the exact manifest bytes:

```json
{
  "kind": "PolicyConformanceMeta",
  "manifestFile": "policy-conformance.json",
  "manifestVersion": "policy_conformance_v0",
  "schemaVersion": "sorrel.protocol.v0",
  "sha256": "<64 hex chars>"
}
```

### Protocol side (source of truth)

- The sidecar is generated from the manifest, never hand-edited.
- Regenerate it after any manifest change:

  ```bash
  npm run sync:meta
  ```

- `npm run validate` (and CI) runs `node scripts/conformance-meta.mjs --check`,
  which fails if the sidecar is missing or stale. The `npm test` suite also
  asserts the sidecar matches the manifest.

### Consumer side (vendored copies)

Each consumer vendors **both** files into its conformance directory:

```
<consumer>/.../conformance/policy-conformance.json
<consumer>/.../conformance/policy-conformance.meta.json
```

Each consumer adds a lightweight self-contained check (a normal test) that
computes the SHA-256 of its vendored manifest and asserts it equals the
`sha256` in its vendored sidecar, and that `manifestVersion` / `schemaVersion`
match. Because both files are vendored together, the check runs offline with no
cross-repo dependency: if someone edits the vendored manifest by hand without
re-exporting, the SHA-256 no longer matches the sidecar and the test fails.

### Refreshing a vendored copy

From a side-by-side checkout (e.g. the root monorepo or sibling clones), the
protocol can push both files into a consumer's vendored directory:

```bash
# from sorrel-protocol/
npm run export:conformance -- ../sorrel-core/tests/conformance
npm run export:conformance -- ../sorrel-cli/tests/conformance
npm run export:conformance -- ../sorrel-hub/test/conformance
npm run export:conformance -- ../sorrel-runners/tests/conformance
npm run export:conformance -- ../sorrel-vault/tests/conformance
```

The root repo also ships `scripts/sync-conformance.sh`, which runs the export
for every consumer checkout at once. Both paths are optional conveniences; a
maintainer can also copy the two files by hand. After refreshing, re-run the
consumer's conformance/sync tests so the new vectors are enforced.

Mapping notes per consumer:

- **Core / CLI**: map `permissionDecisions` and `policyChanges` into each
  package's native evaluator types. `needs_grant`/`deny` are both "not
  allowed"; both evaluators distinguish them, so assert the exact decision.
- **Hub**: map `permissionDecisions` to the JS guard's `evaluate()`; `allow`
  must map to `allowed: true`, everything else to `allowed: false`. Hub is an
  administration layer, not the source of truth.
- **Runners**: map `permissionDecisions` for `workflow.run`, `secret.read`,
  `secret.inject`, and `runner.use` to the `CorePermissionEvaluator` gate, and
  assert that forged bundle-attached decisions cannot bypass Core evaluation.
- **Vault**: map `secret.read` and `secret.inject` cases to
  `corePolicy.evaluate()`, and assert local grant YAML alone returns
  `needs_grant` (no bypass).

## Known gaps

- The protocol `PolicyChange`/`AuthorityRoot` JSON shape and the Core Rust struct
  shape differ. Consumers use a small mapping layer rather than deserializing the
  fixtures directly. This is documented per consumer and is acceptable for v0.
- Production cryptography is intentionally out of scope. Signature trust is
  modeled deterministically (signed/unsigned/forged flags), matching the current
  evaluator pattern.
- The CLI-facing policy model and Core's native policy model still use
  different Rust type shapes. Both consume these fixtures, but converging the
  CLI on Core's decision objects would also change the CLI JSON contract.
