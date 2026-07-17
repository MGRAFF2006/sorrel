# Sorrel Agent Task Pack

Last updated: 2026-07-17 UTC

Small, self-contained work orders derived from [`ROADMAP.md`](ROADMAP.md).
Each task below is written as a **complete prompt you can paste to an agent
verbatim** — it carries all the context the agent needs and does not assume it
has read anything else.

> **Status 2026-07-17:** PROTO-1, CORE-1..4, CLI-1..3, HUB-1/2, and WEB-1 are
> **merged** (see `SORREL_PROGRESS.md`). CLI-3 needed an orchestrator fix: it
> was built against a local core stub instead of the real engine (fixed in
> `sorrel-cli` `5340f75`; the stub is deleted again — future CLI tasks must
> not reintroduce one). CI-1 and CI-2 did **not** land (no `.github/workflows/`
> on any main) and can be re-dispatched as written.

## How to use this pack

- **One agent per task, one task at a time per repository.** Tasks in
  different repositories can run in parallel. Tasks inside the same lane
  (CORE-1, CORE-2, ...) must run **in order**, each merged before the next
  starts, because they build on each other.
- Agents work **inside the named submodule repository only**. They must not
  touch other repos or the root repo's submodule pointers.
- When an agent finishes: review, merge its branch into that repo's `main`,
  then report back (template at the bottom). The orchestrator advances root
  pointers and updates `SORREL_PROGRESS.md`.
- The CORE lane must wait until the agent currently working in `sorrel-core`
  has reported and merged.

## Dependency map

```
PROTO-1 ──> CORE-3
CORE-1 ──> CORE-2 ──> CORE-3 ──> CORE-4 ──> CLI-3
CLI-1 ──> CLI-2                  (CLI-1/2 independent of CORE lane)
HUB-1 ──> HUB-2 ──> WEB-1
CI-1, CI-2: independent, anytime
```

---

## Lane PROTOCOL (repo: sorrel-protocol)

### PROTO-1 — Conflict and MergeResult object schemas

```text
Repository: https://github.com/MGRAFF2006/sorrel-protocol (work only in this repo)

Context: Sorrel is an agent-native version-control system. This repo holds the
canonical JSON Schemas for Sorrel objects in schemas/sorrel-object.schema.json,
with valid examples in examples/ and invalid examples in examples/invalid/.
Read AGENTS.md and README.md first. The Rust engine (a separate repo) is about
to get a three-way merge: when both sides modify the same file, it produces
first-class, content-addressed Conflict objects instead of failing.

Task: add two new object kinds to schemas/sorrel-object.schema.json, following
the exact style of the existing kinds (e.g. Snapshot, Change, Workspace):

1. "Conflict" — required: schemaVersion, kind ("Conflict"), repoId, path
   (the repo-relative file path), base/ours/theirs (each an object reference
   with a 64-hex "object" id for the blob, ours/theirs required, base optional
   for add/add conflicts), conflictType (enum: "content", "add_add",
   "modify_delete", "binary"). Optional: hunks (array of { baseStart,
   baseLines, oursLines: array of strings, theirsLines: array of strings }),
   resolution (a 64-hex object id of the resolved blob, absent while
   unresolved).
2. "MergeResult" — required: schemaVersion, kind ("MergeResult"), repoId,
   baseSnapshot, oursSnapshot, theirsSnapshot (64-hex ids), status (enum:
   "clean", "conflicted"), conflicts (array of 64-hex Conflict object ids,
   must be empty when status is "clean" — enforce with an if/then).
   Optional: mergedSnapshot (64-hex id, present when status is "clean").

Requirements:
- Register both kinds wherever the schema's top-level oneOf/kind dispatch
  lists the existing kinds, matching how other kinds are wired in.
- Ajv runs in STRICT mode here: inside any if/then, declare "type" and the
  referenced "properties" inline, otherwise compilation fails.
- Add examples/conflict.json and examples/merge-result.json (valid), and one
  invalid example under examples/invalid/ (e.g. a MergeResult with status
  "clean" but a non-empty conflicts array).
- Add docs/merge-conflicts.md (~half page): what the two objects are for and
  how the engine will use them.
- Do NOT touch conformance/policy-conformance.json or its sidecar.

Validate: npm install, then npm test and npm run validate must pass.

Deliver: one branch, small commits, do not merge yourself. Report: branch
name, final commit SHA, output of npm test and npm run validate.
```

---

## Lane CORE (repo: sorrel-core — WAIT until the current core agent has merged; then run in order)

### CORE-1 — Merge base (common ancestor) over the snapshot DAG

```text
Repository: https://github.com/MGRAFF2006/sorrel-core (work only in this repo)

Context: Sorrel is an agent-native version-control system; this repo is its
Rust engine. Snapshots are content-addressed objects with a `parents:
Vec<ObjectId>` field (see src/snapshot.rs; read_snapshot loads one from an
ObjectStore, src/store.rs). Read AGENTS.md first. Rust stable 1.85+, no new
dependencies.

Task: create src/dag.rs with a merge-base function over snapshot parents:

  pub fn merge_base(
      store: &impl ObjectStore,
      a: &ObjectId,
      b: &ObjectId,
  ) -> Result<Option<ObjectId>, DagError>

- Walk parents breadth-first from both snapshots; return the first common
  ancestor encountered. If several candidates tie at the same generation,
  return the one with the lexicographically smallest hex id so the result is
  deterministic. Return Ok(None) for unrelated histories.
- If a == b return that id. If one is an ancestor of the other, return the
  ancestor.
- DagError wraps store/read errors (missing object, decode failure) following
  the error style used in src/change.rs.
- Export the module and the public names from src/lib.rs like the other
  modules.

Tests (in the same file or tests/): linear history, two branches from a
common fork, unrelated roots (None), identical inputs, one-is-ancestor,
deterministic tie-break with two equal-distance ancestors. Build histories
with InMemoryObjectStore + write_snapshot (see existing tests in
src/snapshot.rs for the pattern).

Validate: cargo test, cargo clippy --all-targets (no warnings),
cargo fmt --all -- --check.

Deliver: one branch, do not merge yourself. Report: branch name, final commit
SHA, test/clippy/fmt output.
```

### CORE-2 — Dependency-free three-way line merge

```text
Repository: https://github.com/MGRAFF2006/sorrel-core (work only in this repo)

Context: Sorrel is an agent-native version-control system; this repo is its
Rust engine. Read AGENTS.md first. Rust stable 1.85+, NO new dependencies —
implement the diff yourself (the sibling CLI repo has a dependency-free LCS
line-diff in src/linediff.rs you can use as a reference for style, but write
your own code here).

Task: create src/merge3.rs, a pure text-merge module with no I/O:

  pub enum MergeOutcome {
      Merged(Vec<u8>),
      Conflicted { merged_with_markers: Vec<u8>, hunks: Vec<ConflictHunk> },
      Binary,
  }
  pub struct ConflictHunk {
      pub base_start: usize,
      pub base_lines: Vec<String>,
      pub ours_lines: Vec<String>,
      pub theirs_lines: Vec<String>,
  }
  pub fn merge3(base: &[u8], ours: &[u8], theirs: &[u8]) -> MergeOutcome

- If any input is not valid UTF-8, return Binary.
- Split into lines; compute base->ours and base->theirs edits via an LCS
  diff; regions changed on only one side take that side; identical changes on
  both sides merge cleanly; overlapping different changes produce a
  ConflictHunk and Git-style markers (<<<<<<< ours / ======= / >>>>>>> theirs)
  in merged_with_markers.
- Preserve the presence/absence of a trailing newline when the merge is clean.
- Export from src/lib.rs.

Tests: clean merge (non-overlapping edits both sides), identical edits both
sides, conflict (same region differs), ours-only change, theirs-only change,
delete vs edit of the same lines (conflict), binary input, empty base with
two different additions (conflict), trailing-newline preservation.

Validate: cargo test, cargo clippy --all-targets (no warnings),
cargo fmt --all -- --check.

Deliver: one branch, do not merge yourself. Report: branch name, final commit
SHA, test/clippy/fmt output.
```

### CORE-3 — First-class Conflict and MergeResult objects

```text
Repository: https://github.com/MGRAFF2006/sorrel-core (work only in this repo)

Context: Sorrel is an agent-native version-control system; this repo is its
Rust engine. Objects are content-addressed JSON documents written through an
ObjectStore (src/store.rs); see src/change.rs for how Change objects are
serialized (serde, schemaVersion "sorrel.protocol.v0", write/read helpers) —
mirror that pattern exactly. The canonical JSON shapes are defined in the
sorrel-protocol repo: docs/merge-conflicts.md plus the "Conflict" and
"MergeResult" kinds in schemas/sorrel-object.schema.json — fetch that file
from https://github.com/MGRAFF2006/sorrel-protocol (main branch) and match
the field names exactly. Read AGENTS.md first. No new dependencies.

Task: create src/conflict.rs with:
- `Conflict` and `MergeResult` structs matching the protocol schema
  (serde-serializable, camelCase field names like the schema).
- write_conflict / read_conflict, write_merge_result / read_merge_result
  helpers over `&impl ObjectStore`, following the same
  serialize-then-store-then-return-id pattern as write/read helpers in
  src/change.rs.
- A constructor that builds a `Conflict` from a path plus the ConflictHunk
  values produced by src/merge3.rs (from the previous task, already merged).
- Export from src/lib.rs.

Tests: round-trip both objects through InMemoryObjectStore (write, read back,
compare), stable/deterministic object ids for identical content, a
MergeResult with status "clean" has no conflicts and carries mergedSnapshot,
a conflicted one lists conflict ids.

Validate: cargo test, cargo clippy --all-targets (no warnings),
cargo fmt --all -- --check.

Deliver: one branch, do not merge yourself. Report: branch name, final commit
SHA, test/clippy/fmt output.
```

### CORE-4 — Snapshot-level three-way merge

```text
Repository: https://github.com/MGRAFF2006/sorrel-core (work only in this repo)

Context: Sorrel is an agent-native version-control system; this repo is its
Rust engine. Already merged and available: snapshot_diff (src/change.rs,
path-level added/modified/deleted between two snapshots), merge_base
(src/dag.rs), merge3 (src/merge3.rs, three-way line merge), Conflict /
MergeResult objects (src/conflict.rs). Trees/blobs/snapshots live in
src/snapshot.rs. Read AGENTS.md first. No new dependencies.

Task: create src/merge.rs with:

  pub fn merge_snapshots(
      store: &impl ObjectStore,
      base: &ObjectId, ours: &ObjectId, theirs: &ObjectId,
      options: &MergeOptions,   // author Principal, repo id, message
  ) -> Result<MergeResult, MergeError>

Algorithm (path-level, using snapshot_diff base->ours and base->theirs):
- Path changed on one side only: take that side's version.
- Path changed identically on both sides: take it.
- Path modified on both sides differently: run merge3 on the three blobs.
  Clean -> merged blob; Conflicted/Binary -> write a Conflict object
  (conflictType "content" or "binary"), keep OURS version in the merged tree.
- Added on both sides with different content: merge3 with empty base; on
  conflict, conflictType "add_add".
- Modified on one side and deleted on the other: Conflict with
  "modify_delete", keep the modified version.
- No conflicts: write the merged tree + a snapshot whose parents are
  [ours, theirs], return a clean MergeResult with mergedSnapshot.
- Any conflicts: do NOT write a merged snapshot; return a conflicted
  MergeResult listing the stored Conflict object ids.
- Export from src/lib.rs.

Tests (build small snapshots with InMemoryObjectStore, see src/snapshot.rs
tests for the pattern): clean merge of disjoint file edits, both-modified
same file clean (different regions), both-modified conflict, add/add
conflict, modify/delete conflict, binary conflict, parents of the merged
snapshot are [ours, theirs].

Validate: cargo test, cargo clippy --all-targets (no warnings),
cargo fmt --all -- --check.

Deliver: one branch, do not merge yourself. Report: branch name, final commit
SHA, test/clippy/fmt output.
```

---

## Lane CLI (repo: sorrel-cli — CLI-1/2 can start now; CLI-3 needs CORE-4)

### CLI-1 — Real `lane list` and `lane switch` with per-lane heads

```text
Repository: https://github.com/MGRAFF2006/sorrel-cli (work only in this repo)

Context: Sorrel is an agent-native version-control system; this repo is the
`sorrel` CLI, a persistent local VCS over the sorrel-core engine. Repo state
lives under .sorrel/: manifest.json, an atomically-written HEAD file
({ "lane": "<lane-id>", "snapshot": "<64-hex id>" }), and .sorrel/lanes/
(lane registry written by `lane create`). See src/repo.rs (all on-disk
helpers) and src/main.rs (command dispatch; every command supports --json).
Read AGENTS.md and DEMO.md first. Tests live in tests/json_output.rs and
build temp repos end to end — follow that pattern.

Task:
1. Add per-lane head storage: .sorrel/heads/<lane-id> files, same atomic
   write-temp-then-rename style as HEAD (helpers in src/repo.rs). On `init`,
   write the default lane's head there too. When HEAD advances (change
   create), also update the active lane's head file. Migration: if
   .sorrel/heads/ is missing but HEAD exists, create it lazily from HEAD.
2. `sorrel lane list` (new): read .sorrel/lanes/ + heads, print each lane's
   id, name, head snapshot, and mark the active one (from HEAD). --json
   included.
3. `sorrel lane switch <lane-id>` (new): fail with a clear error if the lane
   does not exist or if the working tree is dirty vs the current HEAD
   (reuse the existing status/dirty logic). Otherwise: restore the target
   lane's head snapshot into the working tree (the engine has
   restore_snapshot_to_directory; be careful to exclude .sorrel), then
   rewrite HEAD with the new lane + snapshot.
4. `lane create` should also write an initial head file for the new lane
   (pointing at the current HEAD snapshot).

Tests: lane list shows default lane after init; create + list shows both;
switch on a clean tree changes HEAD and restores files; switch with a dirty
tree fails without modifying anything; switch to a missing lane fails; two
lanes advance independent heads (create change on each, verify isolation).

Validate: cargo test --workspace, cargo clippy --all-targets (no warnings),
cargo fmt --all -- --check. Update DEMO.md with a short lane section.

Deliver: one branch, do not merge yourself. Report: branch name, final commit
SHA, test/clippy/fmt output.
```

### CLI-2 — Change index so `log` shows change ids, authors, messages

```text
Repository: https://github.com/MGRAFF2006/sorrel-cli (work only in this repo)

Context: Sorrel is an agent-native version-control system; this repo is the
`sorrel` CLI, a persistent local VCS over the sorrel-core engine. `sorrel log`
(src/main.rs) currently walks the snapshot DAG from HEAD and prints snapshot
ids only, because there is no index from snapshot to the Change object that
produced it. `change create` already records a real Change object (engine
type with id, author, message, timestamps). On-disk helpers live in
src/repo.rs; state is under .sorrel/. Read AGENTS.md and DEMO.md first.

Task:
1. On `change create`, append a record to .sorrel/changes.index (JSON lines:
   { "snapshot": "<64-hex>", "change": "<64-hex>" }). Write atomically
   (write whole file to temp + rename, or append with care) via a helper in
   src/repo.rs.
2. In `log`, for each snapshot in the walk, look up the change id in the
   index and load the Change object from the store to print: change id
   (short), author, message, timestamp, alongside the snapshot id. Snapshots
   without an index entry (e.g. the initial snapshot) print as before.
3. --json output gains the same fields per entry.
4. Lazy backfill is NOT required; missing index entries must never break log.

Tests (tests/json_output.rs pattern): log after two changes shows both
messages/authors in order; a repo initialized before the index existed
(simulate by deleting .sorrel/changes.index) still logs without error;
--json shape assertions.

Validate: cargo test --workspace, cargo clippy --all-targets (no warnings),
cargo fmt --all -- --check. Update DEMO.md's log sample output.

Deliver: one branch, do not merge yourself. Report: branch name, final commit
SHA, test/clippy/fmt output.
```

### CLI-3 — `sorrel merge <lane>` (BLOCKED until CORE-4 is merged)

```text
Repository: https://github.com/MGRAFF2006/sorrel-cli (work only in this repo)

Context: Sorrel is an agent-native version-control system; this repo is the
`sorrel` CLI over the sorrel-core engine (a git dependency pinned by rev in
Cargo.toml). The engine now provides merge_base (src/dag.rs), merge_snapshots
(src/merge.rs), and Conflict/MergeResult objects (src/conflict.rs) — bump the
sorrel-core git rev in Cargo.toml to the current sorrel-core main and run
cargo update -p sorrel-core. The CLI has per-lane heads under .sorrel/heads/
and `lane list/switch` (see src/repo.rs, src/main.rs). Read AGENTS.md first.

Task: add `sorrel merge <lane-id>` (with --json):
1. Resolve ours = active lane head, theirs = target lane head, base =
   merge_base(ours, theirs). Error clearly if the lane is missing, heads are
   equal (nothing to merge), or histories are unrelated (no base).
2. Fast-forward: if base == ours, just advance the active lane head + HEAD
   to theirs and restore the working tree. Report "fastForward": true.
3. Otherwise call merge_snapshots. On a clean result: restore the merged
   snapshot into the working tree (exclude .sorrel), advance HEAD + active
   lane head, record a Change ("merge <lane>") and update the changes index
   (.sorrel/changes.index) like change create does.
4. On conflicts: write conflict markers into the affected working-tree files
   (ours-with-markers content comes from the merge), do NOT advance HEAD,
   persist the MergeResult id to .sorrel/MERGE_STATE, and exit nonzero with
   a clear message listing conflicted paths. A follow-up `sorrel merge
   --abort` restores the pre-merge working tree and removes MERGE_STATE.
   (Conflict resolution/continue is a later task — do not build it.)

Tests: fast-forward, clean three-way merge (disjoint edits on two lanes),
conflicted merge leaves markers + MERGE_STATE + dirty status, --abort
restores cleanly, unrelated-history error, merge with self errors.

Validate: cargo test --workspace, cargo clippy --all-targets (no warnings),
cargo fmt --all -- --check. Add a merge section to DEMO.md.

Deliver: one branch, do not merge yourself. Report: branch name, final commit
SHA, test/clippy/fmt output, and the sorrel-core rev you pinned.
```

---

## Lane HUB (repo: sorrel-hub)

### HUB-1 — Persist product metadata to disk

```text
Repository: https://github.com/MGRAFF2006/sorrel-hub (work only in this repo)

Context: Sorrel Hub is the collaboration API server for the Sorrel
version-control system — Node >= 22, ES modules, ZERO runtime dependencies
(node: builtins only). Read AGENTS.md and README.md first. Sync objects/refs
already persist via src/fs-sync-store.js (atomic temp-file+rename writes,
percent-encoded path segments — reuse its helpers/patterns). Product
metadata (organizations, projects, repositories, proposals, review comments,
workflow runs, policies) still lives only in memory in src/store.js and is
lost on restart.

Task:
1. Create src/fs-metadata-store.js: persists each collection as one JSON
   document per record under <dataDir>/metadata/<collection>/<id>.json,
   written atomically (temp file + rename). On construction, load all
   records into the same Maps the in-memory store uses; every create also
   writes the file. Keep the exact same public methods as InMemoryStore
   (createProject, listProjects, etc.) — the routes must not change. The
   simplest correct approach: a subclass or wrapper of InMemoryStore that
   hydrates from disk in the constructor and persists on every create.
   Record ids are generated by src/models.js; encode ids into filenames with
   the same encodePathSegment used by fs-sync-store.js.
2. src/server.js: use the persistent store by default, honoring the existing
   SORREL_HUB_DATA_DIR (put metadata under <dataDir>/../metadata or a
   SORREL_HUB_METADATA_DIR override — keep it simple and document it) and
   SORREL_HUB_SYNC_STORE=memory should also keep metadata in memory.
   createApp() defaults (used by tests) stay fully in-memory.
3. Corrupt/unreadable record files are skipped with a console.warn, never a
   crash.

Tests (node --test, follow test/projects.test.js style): create a project
via HTTP, build a NEW store/app over the same directory, GET /projects still
returns it; same for one admin collection; duplicate-slug conflict still
enforced after reload; corrupt file on disk is skipped.

Validate: npm test (all existing tests must stay green).

Deliver: one branch, do not merge yourself. Report: branch name, final commit
SHA, npm test output.
```

### HUB-2 — List known sync repositories

```text
Repository: https://github.com/MGRAFF2006/sorrel-hub (work only in this repo)

Context: Sorrel Hub is the collaboration API server for the Sorrel
version-control system — Node >= 22, ES modules, zero runtime dependencies.
Read AGENTS.md and README.md first. The sync transport stores per-repo
objects/refs via src/sync-store.js (in-memory, a Map keyed by repoId) and
src/fs-sync-store.js (on disk: one percent-encoded directory per repo under
the data root; encodePathSegment is exported there — you will need a
matching decodePathSegment). Routes are dispatched in src/app.js;
/{repoId}/refs etc. live in src/routes/sync.js.

Task: add GET /admin/sync-repos returning
{ "repos": [ { "id": "<repoId>", "refCount": <n> } ] } sorted by id.
1. Add a listRepos() method to BOTH stores: in-memory returns its Map keys;
   fs store lists directories under the root and percent-DECODES the names
   back to repo ids (add and export decodePathSegment in fs-sync-store.js,
   with unit tests proving encode->decode round-trips arbitrary strings,
   including ids that need escaping).
2. refCount comes from listRefs(repoId).length.
3. Wire the route into the existing /admin/ dispatch (src/routes/admin.js),
   GET only, following the style of the other admin collection endpoints.
4. Document the endpoint in README.md's API section.

Tests: empty store returns []; after pushing objects/refs for two repos the
endpoint lists both with correct refCounts (do it over HTTP like
test/sync-transport.test.js does); fs-store variant round-trips a repo id
with special characters; decodePathSegment unit tests.

Validate: npm test (all existing tests must stay green).

Deliver: one branch, do not merge yourself. Report: branch name, final commit
SHA, npm test output.
```

---

## Lane HUB-WEB (repo: sorrel-hub-web — WEB-1 needs HUB-2 merged)

### WEB-1 — Sync status view

```text
Repository: https://github.com/MGRAFF2006/sorrel-hub-web (work only in this repo)

Context: this is the browser frontend for the Sorrel Hub collaboration API
(Sorrel is an agent-native version-control system). Framework-free: plain
HTML/CSS/ES modules in public/, a dev server (server/dev-server.mjs) that
serves public/ and proxies /api/* to the Hub API server. Node >= 22, zero
dependencies, tests via node --test (test/static-assets.test.mjs checks the
static files). Read AGENTS.md and README.md first. The Hub API (separate
repo, already merged) provides: GET /admin/sync-repos ->
{ "repos": [ { "id", "refCount" } ] } and GET /{repoId}/refs ->
{ "refs": [ { "name", "snapshot" } ] }.

Task: add a read-only "Sync" view alongside the existing Projects and
Administration views, following the exact structure/style the app already
uses in public/app.js and index.html:
1. Nav entry "Sync". The view fetches /api/admin/sync-repos and renders a
   table of repositories (id, ref count). Selecting a repo fetches
   /api/<repoId>/refs (URL-encode the repo id) and shows its refs: name +
   snapshot id (shortened to 12 chars with the full id in a title attribute).
2. Empty states ("No repositories have been synced yet", "No refs") and a
   simple error state when the API is unreachable — match how the existing
   views handle these.
3. No new build tooling, no frameworks, keep the existing visual style.
4. Update README.md's feature list.

Tests: extend the static-asset tests to assert the new view's markup/module
is served; if the existing test setup stubs API responses, add a rendering
test for the repo list; otherwise keep tests static like the current suite.

Validate: npm test. Also manually: npm start plus a running sorrel-hub
(npm start in that repo, default port 3000) and confirm the view renders
against a repo with pushed refs — describe what you saw in your report.

Deliver: one branch, do not merge yourself. Report: branch name, final commit
SHA, npm test output, manual-check notes.
```

---

## Lane CI (independent, anytime — one task per repo, trivial)

### CI-1 — GitHub Actions for the self-contained Node repos

```text
Repositories (do each in its own branch in its own repo):
  https://github.com/MGRAFF2006/sorrel-protocol
  https://github.com/MGRAFF2006/sorrel-hub
  https://github.com/MGRAFF2006/sorrel-hub-web
  https://github.com/MGRAFF2006/sorrel-vault
  https://github.com/MGRAFF2006/sorrel-slices

Context: Sorrel is split across repos; these five are Node packages with no
cross-repo dependencies. None has CI. Each defines npm scripts — check its
package.json: all have "test"; protocol and vault also have "validate";
slices has "lint".

Task: in each repo add .github/workflows/ci.yml:
- Trigger: push to main + pull_request.
- Node 22 (actions/setup-node@v4 with node-version: 22, actions/checkout@v4).
- Steps: npm ci when a package-lock.json exists, otherwise npm install;
  then npm test; then npm run validate / npm run lint where those scripts
  exist in that repo's package.json.
- Keep it to a single job named "test"; no matrix, no caching complexity.

Validate: run the same commands locally in each repo and confirm they pass;
YAML must parse (e.g. node -e with a YAML parser is NOT available — just be
careful, or validate on a scratch branch push).

Deliver: one branch per repo, do not merge yourself. Report per repo: branch
name, commit SHA, local command output.
```

### CI-2 — GitHub Actions for sorrel-core

```text
Repository: https://github.com/MGRAFF2006/sorrel-core (work only in this repo)

Context: Sorrel's Rust engine. Stable Rust 1.85+, self-contained (no
cross-repo git dependencies), tests + clippy + rustfmt are the standard
checks, and benches exist (cargo bench, dependency-free harness). No CI yet.

Task: add .github/workflows/ci.yml:
- Trigger: push to main + pull_request.
- ubuntu-latest, dtolnay/rust-toolchain@stable with components clippy,rustfmt.
- Steps: cargo build --all-targets; cargo test; cargo clippy --all-targets
  -- -D warnings; cargo fmt --all -- --check. Add Swatinem/rust-cache@v2.
- Do NOT run cargo bench in CI (perf budgets are machine-sensitive).
- Single job named "test".

Note: sorrel-cli and sorrel-runners are NOT part of this task — they pin
sorrel-core as a private git dependency and need a token strategy first.

Validate: run all four commands locally and confirm green.

Deliver: one branch, do not merge yourself. Report: branch name, commit SHA,
local command output.
```

---

## Later (needs a stronger agent — not in this pack)

- Git bridge (`sorrel git import`, roadmap item 3): large, cross-cutting.
- Hub proposals referencing lanes/stacks (roadmap item 5) — after CLI-1..3.
- Embedding surface / C ABI (roadmap item 6).
- CI for sorrel-cli / sorrel-runners: needs a PAT secret so Actions can fetch
  the private sorrel-core git dependency.

## Report-back template

When a task's branch is merged into its repo's `main`, report:

```
Task: <id, e.g. CORE-1>
Repo + branch: <repo> / <branch>
Merged main commit: <sha>
Checks: <commands + pass/fail>
Notes: <deviations, discovered debt, anything out of scope>
```

The orchestrator then advances the root submodule pointer, updates
`SORREL_PROGRESS.md`, and unblocks the next task in the lane.
