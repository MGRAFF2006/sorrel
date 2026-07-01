# Agent workspace model (unified checkout)

Sorrel uses **git submodules** for publishing (each component has its own repo and
`main` branch), but agents should treat the **root checkout as one workspace**.

**Canonical protocol spec:** [`sorrel-protocol/docs/workspace-links.md`](../sorrel-protocol/docs/workspace-links.md)
defines `Workspace.componentLinks` — **`member`** (branch-tracked) vs
**`dependency`** (revision/tag-pinned). This file maps that model to git/Cargo
workflows in the umbrella repo.

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

## Monorepo members vs foreign dependencies

Sorrel uses two linking models. They are **not** the same thing.

| Kind | Protocol `role` | Examples | Git / Cargo analogue | Update model |
| --- | --- | --- | --- | --- |
| **Monorepo member** | **`member`** | `sorrel-core`, `sorrel-cli`, … | Submodule **`branch = main`**; Cargo **`path`** / `[patch]` | `tracking.mode: branch` — see protocol spec |
| **Foreign dependency** | **`dependency`** | Published CLI engine pin | Cargo **`git` + `rev`**; fixed submodule commit | `tracking.mode: revision` or `tag` |

You are right: first-party modules should behave like parts of one product (follow a
branch). External consumers need frozen versions (pin a commit).

### Git caveat (honest)

The root repo **always stores a commit SHA** per submodule in its tree — git has no
“branch pointer” slot in the parent. The counterpart to “pin to branch not commit”
is:

1. **`branch = main` in `.gitmodules`** — tells git which branch tip to use with
   `--remote`.
2. **`git submodule update --remote`** — moves checkouts to `origin/main`.
3. **Commit in root** — records that snapshot when you choose to (CI/release), not
   after every edit.

Day to day, submodule dirs stay on **`main`**; root pointer updates are periodic,
not per agent task. The Sorrel protocol expresses the same distinction as
[`componentLinks`](../sorrel-protocol/docs/workspace-links.md) on a `Workspace`
(`role: member` → branch tracking, `role: dependency` → revision pin).

A **true** single-repo monorepo (no submodules) would drop gitlinks entirely;
that is a larger migration. Until then, branch-tracked submodules are the git-native
version of “monorepo member.”

### Foreign-style pins (when to use)

- Publishing `sorrel-cli` for users who only clone `sorrel-cli` → `sorrel-core` git
  **`rev`** in `Cargo.toml`.
- Root **`main`** when you want a reproducible umbrella snapshot (release, CI on root).
- **Not** for agents editing across `sorrel-core` + `sorrel-cli` in this checkout.

### Workspace setup (monorepo member mode)

```bash
git submodule update --init --recursive
git submodule update --remote --recursive   # follow branch = main from .gitmodules

# CLI builds against sibling core (path dep), not a fetched git rev:
# sorrel-cli/Cargo.toml (workspace only):
#   [patch."https://github.com/MGRAFF2006/sorrel-core"]
#   sorrel-core = { path = "../sorrel-core" }
```

Or use the helper:

```bash
./scripts/sync-submodule-pointers.sh          # --remote + stage gitlink drift
./scripts/sync-submodule-pointers.sh --check  # verify only
```

### Release snapshot (optional pin)

When root `main` should record exact versions: run the sync script, review
`git diff --cached`, commit. Remove `[patch]` in `sorrel-cli` and set `rev` to the
released core SHA for **external** CLI consumers.

---

## Agent rules (read this first)

1. **Work from the root** — `cd` to the root `sorrel` repo. Open and edit submodule
   paths directly (`sorrel-cli/src/main.rs`, `sorrel-core/src/stat_cache.rs`, …).
2. **Monorepo members track `main`** — sorrel-* submodules use `branch = main` in
   `.gitmodules`. Use `git submodule update --remote`, not manual SHA hunting.
   For CLI↔core, use `[patch] path = "../sorrel-core"` while developing (foreign
   `rev` pin is for published CLI only).
3. **Cross-repo changes in one task are normal** — e.g. protocol conformance export
   + hub/cli vendored copies in a single agent run. Defer root pointer updates to
   the end via `./scripts/sync-submodule-pointers.sh`.
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
./scripts/sync-submodule-pointers.sh
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

**Outside this monorepo:** `sorrel-cli` pins `sorrel-core` by git `rev` (reproducible).

**Inside this workspace (floating):** prefer `[patch]` → `path = "../sorrel-core"`
so CLI always builds against the sibling checkout. Remove `[patch]` and set `rev`
to the merged core SHA before a release/root PR.

If `[patch]` is unavailable (e.g. network-only clone), merge core to `main` then:
`rev = "<SHA>"` and `cargo update -p sorrel-core` in `sorrel-cli/`.

## Docs map

| File | Purpose |
| --- | --- |
| `AGENTS.md` | Root agent entry + toolchain |
| `SORREL_PROGRESS.md` | Live status and next work |
| `SORREL_PROTOTYPE_PLAN.md` | Build phases |
| `docs/AGENT_WORKSPACE.md` | This file — umbrella git/Cargo mapping |
| `sorrel-protocol/docs/workspace-links.md` | **Protocol spec** — `member` vs `dependency` |
| `<submodule>/AGENTS.md` | Module-specific checks and boundaries |
