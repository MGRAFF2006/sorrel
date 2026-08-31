# Contributing

Sorrel is a **single monorepo**. Implementation packages live as normal
directories under this checkout (not submodules or gitlinks). Read
[`AGENTS.md`](AGENTS.md) and [`docs/AGENT_WORKSPACE.md`](docs/AGENT_WORKSPACE.md)
before changing code.

## Workflow

1. Create a feature branch from `main` in this repository.
2. Change dependencies first when needed (for example Core before CLI).
3. Run that package's tests, lint, and formatting checks.
4. Prefer small, reviewable commits in the root repo.
5. Add user-visible behavior to `CHANGELOG.md` and each affected package
   changelog under `Unreleased`.
6. Open one PR against root `main` and run the complete repository gate before
   merge.

Do not combine behavior changes with optimization-only work. Optimizations must
preserve protocol fixtures, CLI JSON contracts, and on-disk compatibility
documented by the affected package.

## Required root checks

```sh
npm run setup       # once per fresh checkout
npm run check
```

During iteration, use `npm run check:quick` plus
`npm run test:module -- <module>`. See
[`docs/AGENT_WORKSPACE.md`](docs/AGENT_WORKSPACE.md) for the dependency map and
generated-file rules.

GitHub Releases and changelogs are the project history. Keep `ROADMAP.md`
forward-looking and do not add separate task-pack, progress-dashboard, or
feature-audit Markdown files.

Rust changes also require:

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

## Security

Follow [`SECURITY.md`](SECURITY.md). Never commit credentials, private keys, or
raw secret values. The Hub alpha uses development auth by default — do not bind
`auth=dev` or bootstrap grants to a non-loopback interface without an explicit
`SORREL_HUB_ALLOW_INSECURE_DEV_AUTH=1` override.
