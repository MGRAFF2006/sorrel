# Sorrel Roadmap

Last updated: 2026-10-02

Forward work only. Release history lives in [`CHANGELOG.md`](CHANGELOG.md);
current capabilities and limits live in [`docs/STATUS.md`](docs/STATUS.md).

## Product direction

Prove this workflow first: **parallel agents produce isolated, recoverable,
reviewable work in an existing Git repository**.

Retain the native engine as the implementation baseline. Evaluate its value
through that workflow before considering a Git-backed rewrite. Git compatibility
is the adoption and exit path. Further app, hosting, environment-provider, and
embedding expansion comes after the local workflow earns its complexity.

## 1. Make the local workflow routine

The foundational implementation is in the current source tree; see
[`docs/STATUS.md`](docs/STATUS.md). Use isolated workspaces and owner-side
review/integration for real Sorrel tasks. Record friction through focused issues
rather than a parallel planning system.

- Improve active-work discoverability and review from actual two-agent use.
- Measure workspace creation and warm status on a real repository before adding
  shared stores, indexes or packfiles.
- Exercise interrupted-operation recovery across supported operating systems.
- Define workspace and Hub format upgrades before the next incompatible format.

Acceptance: a human and two agents can complete ordinary changes, inspect and
integrate them, recover failures, and repeat after restarting processes.

## 2. Extend dependable Git adoption and exit

- Preserve existing Git CI and ordinary Git consumption of exported branches.
- Prioritize fidelity gaps from real use: author/committer metadata, tags,
  signatures, symlinks/submodules, and encoding support.
- Expand staged-index/divergence and incremental recovery scenarios without
  promising byte-identical commits or unsupported object kinds.

Acceptance: supported files/modes/messages survive a two-agent import/integrate/
export round-trip; unsupported history fails explicitly without lost work.

## 3. Complete production collaboration boundaries

- Hydrate native Core policy rules and signed previous authority instead of
  extending another independent evaluator. Current Hub guards are a limited
  fail-closed subset, not complete policy parity.
- Complete WorkOS sealed sessions and the browser IdP login flow.
- Define multi-writer metadata persistence before horizontally scaling Hub.
- Keep Convex optional/internal and align future public subscriptions with
  session and repository scopes.

Acceptance: a production login, private reads, writes, grant lifecycle, and signed
policy mutation work under one authority contract; persistence is coherent
under the intended deployment topology.

## 4. Expand from demonstrated friction

Choose the next feature from actual use of the local workflow: stacked-change operations,
one local API, an integrated slice workflow, or a Hub view of real agent activity.
Slice manifests must not claim live linking or permission projection until those
behaviors exist end to end.

Defer additional native-app scope, Convex migration, hosted compute, marketplace,
extra embedding transports, and secret-provider breadth. Preserve existing
companions and local environment fallback; none is required to prove the local VCS.

## Performance

Measure on the same machine against [`benchmarks/README.md`](benchmarks/README.md).
Targets remain warm 10k-file status below 100 ms and 1k-change log below 50 ms.
Correctness comes first; add packfiles, indexes, lazy fetch, or chunking only for
measured bottlenecks.
