# Git bridge — import, export, and colocated sync

Move history between a normal Git repository and a Sorrel workspace, and keep
a colocated mirror in sync with `sorrel git sync`.

## Prerequisites

- A Git checkout (or bare repo) on disk
- `sorrel` built against a `sorrel-core` rev that includes `git_import` / `git_export`

```bash
cargo build
SORREL=target/debug/sorrel
```

## Import

From inside a Git working tree (or pass an explicit path):

```bash
cd /path/to/git-repo
$SORREL git import
# or: $SORREL git import /path/to/git-repo --ref HEAD --limit 10
```

What happens:

1. Creates `.sorrel/` if missing (`sorrel init`)
2. Walks commits reachable from `--ref` (default `HEAD`) via libgit2
3. Writes Sorrel blobs/trees/snapshots and Change objects
4. Advances HEAD to the tip snapshot and restores the working tree
5. Writes `.sorrel/git-map.json` (Git SHA → snapshot/change ids)
6. Appends `.sorrel/changes.index` so `sorrel log` shows imported history

Flags:

| Flag | Meaning |
| --- | --- |
| `--ref <name>` | Git ref to import (default `HEAD`) |
| `--limit <n>` | Import at most N newest commits |
| `--force` | Allow dirty worktree or overwrite existing `git-map.json` |
| `--json` | Structured output |

Refuse without `--force` when the working tree is dirty relative to HEAD, or when
`.sorrel/git-map.json` already exists.

## Export

Export the current HEAD (or `--snapshot`) into a Git repository:

```bash
$SORREL git export ./mirror.git --branch main
# or: $SORREL git export . --force   # update a colocated .git
```

What happens:

1. Walks Sorrel snapshot ancestors of the tip (parents before children)
2. Writes Git trees/commits for snapshots not already in `git-map.json`
3. Updates the named branch and refreshes `.sorrel/git-map.json`
4. Reuses mapped SHAs on subsequent exports (idempotent)

Flags:

| Flag | Meaning |
| --- | --- |
| `--branch <name>` | Branch to update (default `main`) |
| `--snapshot <id>` | Snapshot tip (default HEAD) |
| `--force` | Overwrite an existing branch when no map is present |
| `--json` | Structured output |

## Colocated sync

Once a mapping exists (created by `git import` or `git export`), `sorrel git
sync` keeps the two histories aligned in either direction:

```bash
# colocated: .sorrel/ and .git/ share one working tree
$SORREL git sync
# or a mirror in another directory
$SORREL git sync /path/to/mirror --branch main
```

What happens, depending on which side moved:

| State | Result |
| --- | --- |
| Neither side moved | `up-to-date`, nothing written |
| Git gained commits | Incremental import (only new commits), HEAD fast-forwards, `pulled` |
| Sorrel gained snapshots | Incremental export, branch advances, `pushed` |
| Both moved | New Git commits are imported and parked on lane `git/<branch>`; `diverged` |

On `diverged`, resolve with a normal merge and sync again:

```bash
$SORREL merge <laneId>   # lane id from the sync output (`git/<branch>`)
$SORREL git sync
```

Notes:

- Sync is fast-forward only on each side; it never rewrites existing Git
  commits or Sorrel snapshots.
- Bootstrap cases work too: a missing Git branch is created from Sorrel
  history, and a fresh (empty) Sorrel workspace adopts the Git history.
- Pulling refuses to overwrite uncommitted working-tree changes unless
  `--force` is passed.
- In a colocated checkout the Git index is refreshed after a push so
  `git status` stays clean.

Flags:

| Flag | Meaning |
| --- | --- |
| `--branch <name>` | Git branch to keep in sync (default `main`) |
| `--force` | Restore the working tree even when it has uncommitted changes |
| `--json` | Structured output (`status`: `up-to-date` / `pulled` / `pushed` / `diverged`) |

## Verify

```bash
$SORREL log
$SORREL status
$SORREL git export ./out.git --json
git -C ./out.git log --oneline
```

## Notes

- Symlinks and submodule gitlinks are rejected on import
- Merge commits are imported/exported; Change base uses the first parent on import
- Mapping under `.sorrel/git-map.json` links Git SHAs ↔ Sorrel snapshot ids
