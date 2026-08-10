# Contributing

Sorrel is split across independent repositories coordinated by this root
checkout. Read [`AGENTS.md`](AGENTS.md) and
[`docs/AGENT_WORKSPACE.md`](docs/AGENT_WORKSPACE.md) before changing code.

## Workflow

1. Create a feature branch in each affected module.
2. Change the upstream dependency first (for example Core before CLI).
3. Run that module's tests, lint, and formatting checks.
4. Commit and review each module independently.
5. Advance root gitlinks only after module commits are final.
6. Run the root release, conformance, module, and E2E checks.

Do not combine behavior changes with optimization-only work. Optimizations must
preserve protocol fixtures, CLI JSON contracts, and on-disk compatibility
documented by the affected module.

## Required root checks

```sh
npm run validate:release
npm run validate:conformance
npm run test:modules
npm test
```

Rust changes also require:

```sh
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --all -- --check
```

## Security

Follow [`SECURITY.md`](SECURITY.md). Never commit credentials, private keys, or
raw secret values.
