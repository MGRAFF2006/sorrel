# Agent workspace model (unified checkout)

Sorrel uses **git submodules** for publishing (each component has its own repo and
`main` branch), but agents should treat the **root checkout as one workspace**.

You do **not** need a separate agent session per submodule. Edit any file under
`sorrel-protocol/`, `sorrel-core/`, `sorrel-cli/`, etc. from the parent tree.
Paths are normal files on disk.

## What “one workspace” means

| Layer | Role |
| --- | --- |
| **Root `sorrel/`** | Orchestration: architecture docs, progress dashboard, `scripts/`, submodule **pointers** |
| **`sorrel-*` directories** | Real implementation (each is a nested git repo) |

**Publishing** is still per submodule (separate remotes). **Editing** is from the
root tree.

## Agent rules (read this first)

1. **Work from the root** — `cd` to the root `sorrel` repo. Open and edit submodule
   paths directly (`sorrel-cli/src/main.rs`, `sorrel-core/src/stat_cache.rs`, …).
2. **Never re-vendor or stub engine crates** — `sorrel-cli` depends on
   `sorrel-core` via **git `rev` in `Cargo.toml`**. Do not add `[patch]` tables or
   `deps/sorrel-core-stub/` unless explicitly asked for offline CI only.
3. **Cross-repo changes in one task are normal** — e.g. protocol conformance export
   + hub/cli vendored copies + core pin bump in a single agent run.
4. **Commit in two layers** when you change submodule code:
   - **Inside each touched submodule:** commit on a branch, push, merge to that
     submodule’s `main` (or open a PR).
   - **In the root:** `git add <submodule>` (pointer) + any root files
     (`SORREL_PROGRESS.md`, docs), commit, push.
5. **Discover which git repo a path belongs to:**
   ```bash
   git -C sorrel-cli rev-parse --show-toplevel   # → .../sorrel-cli
   git -C . rev-parse --show-toplevel            # → .../sorrel (root)
   ```
6. **Run checks in the module that owns the code** — `cargo test` in `sorrel-cli/`,
   `npm test` in `sorrel-hub/`, etc. Root has no unified build.
7. **Conformance sync from root** (when protocol manifest changes):
   ```bash
   ./scripts/sync-conformance.sh
   ./scripts/sync-conformance.sh --check
   ```
   Then re-run each consumer’s tests.

## Submodule commit helper (from root)

```bash
# Example: commit CLI work without cd confusion
git -C sorrel-cli status -sb
git -C sorrel-cli add -A
git -C sorrel-cli commit -m "cli: wire stat-cache"
git -C sorrel-cli push origin HEAD

# After submodule main is updated, advance root pointer
git -C sorrel-cli checkout main && git -C sorrel-cli pull
git add sorrel-cli SORREL_PROGRESS.md
git commit -m "Point sorrel-cli at main (stat-cache wire)"
git push origin main
```

If `git add sorrel-cli` hangs, use:
`GIT_FS_MONITOR_ENABLED=false git add sorrel-cli`

## What agents must not do

- Do not clone sibling repos elsewhere and edit copies — edit the submodule paths
  in the root checkout.
- Do not commit submodule **file** changes only in the root repo (root commits
  only **pointer** SHAs for submodules, not the file contents).
- Do not pin `sorrel-core` to a fake/placeholder `rev` when the real crate is
  available in `sorrel-core/`.

## Relationship to `sorrel-cli` git dependencies

Released `sorrel-cli` pins `sorrel-core` by git SHA for consumers outside this
monorepo. **Inside this workspace**, after changing core, always:

1. Merge `sorrel-core` to its `main`
2. Set `sorrel-cli/Cargo.toml` `rev = "<that merge SHA>"`
3. `cargo update -p sorrel-core` in `sorrel-cli/` (or refresh `Cargo.lock`)

Optional local dev override (not for merge): `[patch]` path to `../sorrel-core` —
only if the team agrees; default is git `rev` matching `main`.

## Docs map

| File | Purpose |
| --- | --- |
| `AGENTS.md` | Root agent entry + toolchain |
| `SORREL_PROGRESS.md` | Live status and next work |
| `SORREL_PROTOTYPE_PLAN.md` | Build phases |
| `docs/AGENT_WORKSPACE.md` | This file — unified workspace rules |
| `<submodule>/AGENTS.md` | Module-specific checks and boundaries |
