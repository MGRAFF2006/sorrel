# Sorrel Agent Task Pack

Last updated: 2026-07-30 UTC

Small, self-contained work orders derived from [`ROADMAP.md`](ROADMAP.md).
Each open task below is written as a **complete prompt you can paste to an
agent verbatim** — it carries all the context the agent needs and does not
assume it has read anything else.

> **Status 2026-07-30:** Wave 1 and Wave 2 are **done and merged** (see the
> completed ledger below); their prompts have been removed from this pack.
> Only the CI tasks remain open, and CI-1 has shrunk: `sorrel-hub` and
> `sorrel-hub-web` got their workflows, so CI-1 now covers only
> `sorrel-protocol`, `sorrel-vault`, and `sorrel-slices`.

## Completed (crossed off — prompts removed)

- ~~PROTO-1 — Conflict and MergeResult object schemas~~ (merged 2026-07-17)
- ~~CORE-1 — Merge base over the snapshot DAG~~ (merged 2026-07-17)
- ~~CORE-2 — Dependency-free three-way line merge~~ (merged 2026-07-17)
- ~~CORE-3 — First-class Conflict and MergeResult objects~~ (merged 2026-07-17)
- ~~CORE-4 — Snapshot-level three-way merge~~ (merged 2026-07-17)
- ~~CLI-1 — Real `lane list` / `lane switch` with per-lane heads~~ (merged 2026-07-17)
- ~~CLI-2 — Change index so `log` shows change ids/authors/messages~~ (merged 2026-07-17)
- ~~CLI-3 — `sorrel merge <lane>`~~ (merged 2026-07-17)
- ~~HUB-1 — Persist product metadata to disk~~ (merged 2026-07-17)
- ~~HUB-2 — List known sync repositories~~ (merged 2026-07-17)
- ~~WEB-1 — Sync status view~~ (merged 2026-07-17)
- ~~PROTO-2 — Conflict hunks carry base content~~ (schema `baseLines` is a
  string array; example + docs aligned)
- ~~CORE-5 — Align stored Conflict/MergeResult with the protocol schema~~
  (`src/conflict.rs` carries `repoId`, content refs, and the three input
  snapshot ids; root pointer advanced in `478797f`)
- ~~CLI-4 — `sorrel merge --continue`~~ (shipped with tests + root E2E
  coverage; DEMO.md updated)
- ~~HUB-3 — Configurable trusted grants~~ (shipped as
  `SORREL_HUB_TRUSTED_GRANTS_FILE` plus default-on local bootstrap grants,
  `SORREL_HUB_BOOTSTRAP_GRANTS` — see `sorrel-hub/src/bootstrap-grants.js`)
- ~~CLI-5 — Authorized push (grantRefs on mutating sync calls)~~ (the
  out-of-the-box push 403 is fixed: push/ref-advance send `grantRefs` for the
  Hub bootstrap grants; see `sorrel-cli/SYNC.md`. The per-remote
  `--grant-ref` flag variant was not needed and was not built.)
- ~~CI (partial) — `sorrel-hub` and `sorrel-hub-web`~~ (`.github/workflows/ci.yml`
  on both mains; root repo CI also landed)

## How to use this pack

- **One agent per task, one task at a time per repository.** Tasks in
  different repositories can run in parallel.
- Agents work **inside the named submodule repository only**. They must not
  touch other repos or the root repo's submodule pointers.
- When an agent finishes: review, merge its branch into that repo's `main`,
  then report back (template at the bottom). The orchestrator advances root
  pointers and updates the status docs.

## Dependency map

```
CI-1, CI-2: independent, anytime
```

---

## Lane CI (independent, anytime — one task per repo, trivial)

### CI-1 — GitHub Actions for the remaining self-contained Node repos

```text
Repositories (do each in its own branch in its own repo):
  https://github.com/MGRAFF2006/sorrel-protocol
  https://github.com/MGRAFF2006/sorrel-vault
  https://github.com/MGRAFF2006/sorrel-slices

Context: Sorrel is split across repos; these three are Node packages with no
cross-repo dependencies and no CI yet (sorrel-hub and sorrel-hub-web already
have workflows — do NOT touch them). Each defines npm scripts — check its
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

- Colocated Git mirror (roadmap item 3; one-way `git import` / `git export`
  already shipped).
- Embedding surface / C ABI (roadmap item 6).
- Production auth for the Hub (roadmap item 5 remainder).
- CI for sorrel-cli / sorrel-runners: needs a PAT secret so Actions can fetch
  the private sorrel-core git dependency.

## Report-back template

When a task's branch is merged into its repo's `main`, report:

```
Task: <id, e.g. CI-2>
Repo + branch: <repo> / <branch>
Merged main commit: <sha>
Checks: <commands + pass/fail>
Notes: <deviations, discovered debt, anything out of scope>
```

The orchestrator then advances the root submodule pointer and updates the
status docs ([`docs/STATUS.md`](docs/STATUS.md)).
