# Contributing

Sorrel is a **single monorepo**. Read [`AGENTS.md`](AGENTS.md) and
[`docs/AGENT_WORKSPACE.md`](docs/AGENT_WORKSPACE.md) before changing code.

## Workflow

1. Create a feature branch from `main` in this repository.
2. Change packages in dependency order when needed (for example Core before CLI).
3. Run the affected package tests, lint, and formatting checks.
4. Open one PR against root `main`.
5. Run the root release, conformance, module, and E2E checks before merge.

Do not combine behavior changes with optimization-only work. Optimizations must
preserve protocol fixtures, CLI JSON contracts, and on-disk compatibility
documented by the affected package.

## Required root checks

```sh
npm run validate:release
npm run validate:conformance
npm run test:modules
npm test
```

Rust changes also require:

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

## Security

Follow [`SECURITY.md`](SECURITY.md). Never commit credentials, private keys, or
raw secret values.
