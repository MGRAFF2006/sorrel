# Sorrel CLI — Persistent Local Demo (P0)

This walks the Quick Advance Goal: a single-user, on-disk version-control flow
where state persists across separate `sorrel` invocations. Everything below is
real (no mocks) and backed by the content-addressed `sorrel-core` engine.

## Build

```bash
cargo build            # from the sorrel-cli repo
SORREL=target/debug/sorrel
```

## 1. Initialize a repository

```bash
mkdir /tmp/sorrel-demo && cd /tmp/sorrel-demo
$SORREL init
# Initialized Sorrel repository repo_<hex> in .sorrel
```

This creates a real `.sorrel/` workspace:

```text
.sorrel/
  objects/        content-addressed object store (BLAKE3, two-char fanout)
  lanes/          lane registry (default `lane_main` plus any created lanes)
  heads/          per-lane head snapshot pointers (one file per lane id)
  manifest.json   repo identity + creation metadata + default lane
  HEAD            { lane, snapshot }   (atomically written)
  changes.index   JSON-lines map of snapshot id → change id
  MERGE_STATE     present only during a conflicted merge
```

## 2. Edit files and see real status

```bash
echo "line1" > a.txt
echo "line2" >> a.txt
$SORREL status
# Sorrel repository repo_<hex> on lane lane_main: dirty
```

`status --json` reports the actual added/modified/deleted paths against HEAD:

```bash
$SORREL status --json
```

## 3. Record a change (advances HEAD)

```bash
$SORREL change create -m "add a.txt"
# Created change <id> (1 path(s))

$SORREL status
# ... : clean
```

`change create` snapshots the working tree (excluding `.sorrel/`), diffs it
against HEAD, writes a real `Change` object, and advances HEAD (and the active
lane's `.sorrel/heads/<lane-id>` pointer). Recording with no changes is
rejected:

```bash
$SORREL change create -m "nothing"
# sorrel: no changes to record since HEAD   (exit 1)
```

## 4. Line-level diff

```bash
printf 'line1\nLINE2\nline3\n' > a.txt
$SORREL diff
# diff --sorrel a.txt (modified)
# @@ -1,2 +1,3 @@
#  line1
# -line2
# +LINE2
# +line3
```

`diff --json` returns structured hunks (each line tagged `context`/`added`/
`removed`); binary/non-UTF8 files report `"binary": true`.

## 5. History

```bash
$SORREL change create -m "edit a.txt"
echo b > b.txt
$SORREL change create -m "add b.txt"

$SORREL log
# <change>  system:sorrel  <timestamp>  add b.txt  (<snapshot>)
# <change>  system:sorrel  <timestamp>  edit a.txt  (<snapshot>)
# <change>  system:sorrel  <timestamp>  add a.txt  (<snapshot>)
# <snapshot>  initial snapshot
```

`log` walks the snapshot DAG (first-parent chain) from HEAD back to the initial
snapshot. Each `change create` appends a line to `.sorrel/changes.index` mapping
the resulting snapshot to its Change object, so `log` can show the short change
id, author, message, and timestamp. Snapshots without an index entry (the
initial snapshot, or repos created before the index existed) still print with
snapshot id + message only. `--limit N` caps the output; `--json` returns the
same fields per entry.

## 6. Lanes (list / create / switch)

Each lane keeps an independent head snapshot under `.sorrel/heads/<lane-id>`.
`lane list` shows every registered lane, its head, and which one is active:

```bash
$SORREL lane list
# * lane_main  main  <snapshot>
```

Create a feature lane (it starts at the current HEAD snapshot), switch to it on
a clean tree, record work there, then switch back — the working tree is restored
to that lane's head and `.sorrel/` is left untouched:

```bash
$SORREL lane create --name agent/feature
# Created lane agent/feature (<lane-id>)

$SORREL lane switch <lane-id>
# Switched to lane <lane-id> at <snapshot>

echo feature > feature.txt
$SORREL change create -m "feature work"

$SORREL lane switch lane_main
# feature.txt is gone; main's files are restored
```

Switching refuses a dirty working tree (and leaves HEAD / files unchanged):

```bash
echo dirty >> a.txt
$SORREL lane switch <lane-id>
# sorrel: working tree has uncommitted changes; ...   (exit 1)
```

## 7. Merge another lane into the active lane

`sorrel merge <lane-id>` merges the target lane's head into the active lane.
It resolves `ours` = active head, `theirs` = target head, and
`base = merge_base(ours, theirs)`.

**Fast-forward** — when the active lane has not diverged (`base == ours`), the
active head (and `HEAD`) advance to theirs and the working tree is restored.
JSON reports `"fastForward": true`:

```bash
$SORREL lane create --name agent/feature
$SORREL lane switch <lane-id>
echo feature > feature.txt
$SORREL change create -m "feature work"
$SORREL lane switch lane_main
$SORREL merge <lane-id>
# Fast-forwarded to <snapshot> (merged <lane-id>)
```

**Clean three-way merge** — disjoint edits on two lanes produce a merge Change
(`merge <lane-id>`), advance HEAD + the active lane head, and append
`.sorrel/changes.index` like `change create`:

```bash
# after branching at a shared base:
#   main edits a.txt, feature edits b.txt
$SORREL merge <lane-id>
# Merged <lane-id> (<change>; N path(s))
```

**Conflicts** — overlapping edits write conflict markers into the affected
files, leave HEAD unchanged, persist the MergeResult id at
`.sorrel/MERGE_STATE`, and exit nonzero. Abort restores the pre-merge tree:

```bash
$SORREL merge <lane-id>
# sorrel: merge conflicts in: a.txt; resolve or run `sorrel merge --abort`

cat a.txt
# <<<<<<< ours
# ...
# =======
# ...
# >>>>>>> theirs

$SORREL status
# ... : dirty

$SORREL merge --abort
# Merge aborted; restored working tree to <snapshot>
```

Merging a missing lane, merging a lane into itself, equal heads, or unrelated
histories (no merge base) each fail with a clear error. Conflict
resolution / `merge --continue` is not in this prototype yet.

## 8. Persistence across processes

Every command reads and writes real objects under `.sorrel/`. Open a new shell,
`cd` back into `/tmp/sorrel-demo`, and run `sorrel log` / `sorrel status` — the
same repository id, HEAD, and history are restored from disk.

## What is NOT in this prototype yet

- Git export / colocated mirror (one-way `sorrel git import` is available — see [`GIT.md`](GIT.md)).
- Conflict resolution / `merge --continue` (conflict markers + `--abort` are in).
- Lane stacks / submit workflows (list/create/switch + per-lane heads are in).
- Real secret injection.

For remotes / sync / Hub, see [`SYNC.md`](SYNC.md).
For Git import, see [`GIT.md`](GIT.md).
