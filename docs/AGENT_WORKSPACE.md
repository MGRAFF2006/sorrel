# Agent workspace model

Sorrel is coordinated from one root checkout, but implementation lives in
independent `sorrel-*` Git repositories mounted as submodules.

## Two layers

| Layer | Responsibility |
| --- | --- |
| Root `sorrel/` | Architecture/status docs, full-stack tests, submodule pointers |
| `sorrel-*` submodules | Implementation, module tests, module branches and releases |

Agents can edit any submodule from the root filesystem workspace; separate
clones are unnecessary. Git operations still belong to the owning repository:

```sh
git -C sorrel-cli status
git -C sorrel-core status
git status                 # root pointer/docs state
```

## Change workflow

1. Create a feature branch in each affected submodule.
2. Implement and run that module's checks.
3. Commit, push, review, and merge the submodule branch into its `main`.
4. Advance the corresponding root gitlink(s).
5. Run `npm run test:modules` and `npm test` from the root.
6. Commit the root pointer and documentation updates.

Cross-repository changes must respect dependency order. For example, merge
`sorrel-core` first, then update the exact `sorrel-core` revision in
`sorrel-cli`, merge the CLI, and finally advance both root pointers.

## Gitlink and branch behavior

Git always stores an exact commit SHA for each submodule in the root tree.
`branch = main` in `.gitmodules` declares the upstream branch used when looking
for pointer updates; it does not make the root gitlink float automatically.

Use the helper after submodule work is merged:

```sh
./scripts/sync-submodule-pointers.sh --check  # report root vs origin/main drift
./scripts/sync-submodule-pointers.sh          # fetch and stage updated gitlinks
git diff --cached --submodule=short
```

The helper stages `origin/main` pointers without switching active submodule
branches or modifying their working trees.

## Dependency pins

First-party package dependencies remain reproducible exact revisions where
declared (for example, `sorrel-cli` pins `sorrel-core` by Git revision). Do not
replace those declarations with local path patches in committed code.

When an upstream module changes:

1. Merge and push the upstream module.
2. Update the dependent module's revision and lockfile.
3. Run the dependent module's full checks.
4. Merge the dependent module.
5. Advance the root pointers.

## Validation

From the root:

```sh
npm run test:modules
npm test
```

Within Rust modules:

```sh
cargo build
cargo test
cargo clippy --all-targets
cargo fmt --all -- --check
```

Within Node modules, run `npm test` plus `npm run validate` or `npm run lint`
where defined.

See [`AGENTS.md`](../AGENTS.md) for the current module map and mandatory
workspace rules.
