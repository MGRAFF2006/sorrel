# Changelog

All notable coordinated Sorrel releases are documented here. Module-specific
details live in each `sorrel-*` repository's changelog.

## Unreleased

- Redesigned root README with Nord brand mark, wordmark, and social banner
  assets under `assets/`.
- Production authentication and stable embedding interfaces remain planned.

## 0.1.0-alpha.1 — 2026-07-30

First local-first developer preview:

- Content-addressed engine with snapshots, changes, lanes, stacks, policy
  evaluation, three-way merge, and first-class conflict objects.
- Persistent CLI workflow from `init` through changes, merge, Hub push/pull,
  and lane submission.
- Git import, export, and colocated bidirectional synchronization.
- FS-backed development Hub and writable browser companion.
- Protocol conformance fixtures shared by Core, CLI, Hub, runners, and vault.
- Experimental vault, runners, slices, agents, and JS/Rust SDK modules.

Important limitations:

- Hub authentication is not production-ready and the server is intended for
  localhost development only.
- `sorrel.protocol.v0` and on-disk formats may change before 1.0.
- Secret injection, stable embedding ABIs, hosted compute, and apps are not
  included.
