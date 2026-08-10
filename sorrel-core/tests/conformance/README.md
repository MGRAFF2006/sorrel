# Vendored policy conformance manifest

`policy-conformance.json` in this directory is a vendored copy of the canonical
conformance manifest published by `sorrel-protocol`:

    sorrel-protocol/conformance/policy-conformance.json

It is the language-neutral source of truth for policy decisions shared across
Sorrel consumers (Core, CLI embedded Core, Hub guard, Runners gate, Vault
adapter). See `sorrel-protocol/docs/policy-conformance.md` for the contract.

## Staying in sync

`policy-conformance.meta.json` is the vendored sidecar from `sorrel-protocol`. It
records the manifest version and a SHA-256 over the manifest bytes. The
`conformance_sync` test (`tests/conformance_sync.rs`) recomputes the manifest
hash and fails if it no longer matches the sidecar, so a hand-edited or stale
vendored manifest is caught by `cargo test`.

To refresh after the protocol manifest changes, re-export both files from a
`sorrel-protocol` checkout instead of editing by hand:

```bash
# from sorrel-protocol/
npm run export:conformance -- <path-to>/sorrel-core/tests/conformance
```

(or run the root `scripts/sync-conformance.sh`), then re-run `cargo test`. The
Core evaluator must still agree with every `expected` value.
