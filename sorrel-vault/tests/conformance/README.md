# Vendored policy conformance manifest

`policy-conformance.json` here is a vendored copy of the canonical conformance
manifest published by `sorrel-protocol`:

    sorrel-protocol/conformance/policy-conformance.json

Vault must only release secret values when an injected `corePolicy.evaluate()`
returns allow; local grant YAML alone never bypasses Core.
`tests/policy-conformance.test.mjs` proves this against the manifest. See
`sorrel-protocol/docs/policy-conformance.md` for the contract.

## Staying in sync

`policy-conformance.meta.json` is the vendored sidecar from `sorrel-protocol`. It
records the manifest version and a SHA-256 over the manifest bytes.
`tests/conformance-sync.test.mjs` recomputes the manifest hash and fails if it no
longer matches the sidecar, so a hand-edited or stale vendored manifest is caught
by `npm test`.

To refresh after the protocol manifest changes, re-export both files from a
`sorrel-protocol` checkout instead of editing by hand:

```bash
# from sorrel-protocol/
npm run export:conformance -- <path-to>/sorrel-vault/tests/conformance
```

(or run the root `scripts/sync-conformance.sh`), then re-run `npm test`.
