# Git bridge — import, export, and sync

Move history between a normal Git repository and a Sorrel workspace, and keep
colocated or external mirrors in sync with `sorrel git sync`.

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

A first colocated import refuses staged or unstaged tracked Git edits before
initializing Sorrel. Existing workspaces also refuse a dirty tree relative to
Sorrel HEAD or an existing `.sorrel/git-map.json` unless `--force` is passed.
Untracked collisions remain protected. Import checks out the selected ref;
ignore rules apply to subsequent recording, while files imported from Git stay
tracked.

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

## Bidirectional sync

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
- In an external mirror, exports and pushes update the active branch's files
  and index together. Tracked edits, staged changes, and incoming paths that
  collide with untracked or ignored files are refused before moving the branch.
  Unrelated untracked files and other checked-out branches are preserved.
- In a colocated checkout, pushes refuse staged Git changes before moving the
  branch, then refresh the index so `git status` stays clean. A failed index
  refresh is reported explicitly; inspect Git status before retrying.

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

- Supported normal/executable modes and full UTF-8 commit messages survive
  import/export. Non-UTF-8 messages, symlinks, and submodule gitlinks are rejected.
- Author/committer identity and timestamps are represented through Sorrel metadata;
  exports do not promise original commit bytes or SHAs for newly written snapshots.
  Tags, Git notes, signatures, hooks, and index staging are not Sorrel objects.
- The bridge does not replace existing Git CI; exported branches are ordinary Git.
- Run a two-agent example from [README.md](README.md#parallel-agents-in-an-existing-git-repository).
- Merge commits are imported/exported; Change base uses the first parent on import
- Mapping under `.sorrel/git-map.json` links Git SHAs ↔ Sorrel snapshot ids
