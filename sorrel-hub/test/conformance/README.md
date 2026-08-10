# Vendored policy conformance manifest

`policy-conformance.json` here is a vendored copy of the canonical conformance
manifest published by `sorrel-protocol`:

    sorrel-protocol/conformance/policy-conformance.json

Hub is an administration/API layer over Core policy semantics, not the source of
truth. `policy-conformance.test.js` proves Hub's grant-based guard agrees with
the canonical allow/deny decisions. See
`sorrel-protocol/docs/policy-conformance.md` for the contract.

## Staying in sync

`policy-conformance.meta.json` is the vendored sidecar from `sorrel-protocol`. It
records the manifest version and a SHA-256 over the manifest bytes. The
`conformance-sync.test.js` test recomputes the manifest hash and fails if it no
longer matches the sidecar, so a hand-edited or stale vendored manifest is caught
by `npm test`.

To refresh after the protocol manifest changes, re-export both files from a
`sorrel-protocol` checkout instead of editing by hand:

```bash
# from sorrel-protocol/
npm run export:conformance -- <path-to>/sorrel-hub/test/conformance
```

(or run the root `scripts/sync-conformance.sh`), then re-run `npm test`.
