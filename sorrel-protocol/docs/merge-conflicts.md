# Merge conflicts

When the Sorrel engine performs a three-way merge and both sides touch the same
path, it does **not** fail the merge. Instead it materializes first-class,
content-addressed objects that tools and agents can inspect, store, and resolve.

## `Conflict`

A **`Conflict`** records a single path-level disagreement:

- **`path`** — repo-relative file path that diverged.
- **`base` / `ours` / `theirs`** — content refs (`{ "object": "<64-hex>" }`) to
  the blob on each side. At least one of `ours` / `theirs` is required; the
  deleted side is omitted for `modify_delete` conflicts, and `base` is omitted
  for add/add conflicts where there was no common ancestor blob.
- **`conflictType`** — `content`, `add_add`, `modify_delete`, or `binary`.
- **`hunks`** (optional) — textual regions for content conflicts (`baseStart`
  0-based line index, plus `baseLines` / `oursLines` / `theirsLines` string
  arrays holding the actual lines on each side).
- **`resolution`** (optional) — 64-hex blob id of the resolved content; absent
  while the conflict is unresolved.

Conflict objects are identified by their BLAKE3 content id (same 64-hex scheme
as other engine objects), not by a separate string `id` field.

## `MergeResult`

A **`MergeResult`** summarizes one three-way merge of snapshots:

- **`baseSnapshot` / `oursSnapshot` / `theirsSnapshot`** — 64-hex snapshot ids.
- **`status`** — `clean` or `conflicted`.
- **`conflicts`** — array of 64-hex `Conflict` object ids. When `status` is
  `clean`, this array **must** be empty.
- **`mergedSnapshot`** (optional) — 64-hex id of the merged tree snapshot; set
  when the merge completed cleanly.

## Engine usage

1. Compute the three-way merge against `base` / `ours` / `theirs` snapshots.
2. For each conflicting path, write a `Conflict` object and collect its content
   id.
3. Emit a `MergeResult` with `status: "conflicted"` and those ids (or
   `status: "clean"`, empty `conflicts`, and `mergedSnapshot` when automatic).
4. Resolvers (human or agent) produce a resolved blob, set `Conflict.resolution`,
   and eventually produce a new snapshot that incorporates all resolutions.

See [`examples/conflict.json`](../examples/conflict.json) and
[`examples/merge-result.json`](../examples/merge-result.json).
