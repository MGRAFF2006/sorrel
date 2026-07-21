# Next agent prompt (2026-07-21)

Paste the block below to a fresh agent. It is self-contained.

---

```text
You are continuing work on Sorrel, an agent-native version-control system
coordinated from https://github.com/MGRAFF2006/sorrel (multi-repo via git
submodules). Work from a checkout of that root repo with submodules already
initialized.

## Goal

Two phases, in order:

### Phase A — Land the local happy-path + docs work onto each submodule’s `main`
There is substantial **uncommitted** work sitting in dirty submodule working
trees (and the root). Your first job is to turn that into proper commits on
branches, open/merge PRs (or merge to main per repo norms), then advance root
submodule pointers and commit the root docs/`docker-compose`/`README` updates.

Do **not** invent new features in Phase A. Preserve existing behavior; only
fix/commit/document as needed so main builds and the happy path still works.

### Phase B — Start roadmap item 3: Git import (one-way)
After Phase A is merged and root pointers are advanced: implement **one-way
`sorrel git import`** (Git repo → Sorrel snapshots/changes) in `sorrel-core`
and expose it from `sorrel-cli`. Do **not** start export, colocated mode,
agents, or SDKs in this pass.

Success looks like: from a normal Git checkout, `sorrel git import` produces a
`.sorrel/` workspace whose `log`/`status` reflect imported history for a
reasonable subset (at least linear history of commits → snapshots/changes),
with tests, and no reintroduction of a `sorrel-core-stub`.

## Critical context (read these first)

Root:
- `AGENTS.md` — submodule rules, Rust 1.85+, checks
- `docs/STATUS.md` — what works / what is missing (source of truth for status)
- `docs/GETTING_STARTED.md` — how to run CLI + Hub
- `ROADMAP.md` — sequenced plan (item 3 = Git bridge)
- `SORREL_PROGRESS.md` — live dashboard (update when you finish)
- `SORREL_AGENT_TASKS.md` — prior task-pack style; do not re-open finished tasks

Operating rules:
- Implementation lives in **submodule repos**. Root only holds docs, pointers,
  and orchestration files (`docker-compose.yml`, etc.).
- Never reintroduce `deps/sorrel-core-stub` or a Cargo `[patch]` for core.
  CLI must pin real `sorrel-core` by git `rev`.
- Core owns identity/permissions/policy/grants/`SecretRef`/audit. Hub/CLI/
  runners/vault consume them; do not invent a parallel authority model.
- Prefer small reviewable commits. Do not commit secrets. Do not force-push
  main. Do not skip hooks unless the user explicitly asks.
- Landing site production is **Cloudflare Pages** for `sorrel-web` (no build,
  publish `.`). Local `docker compose` `web` is preview only — do not treat it
  as replacing Cloudflare. Docs for visitors live in `sorrel-web/docs/*.md`
  and are mirrored under root `docs/`.

## What already works (do not break)

- `sorrel-core`: object store, snapshots, changes, merge_base/merge3,
  Conflict/MergeResult, merge_snapshots, transport helpers, stat-cache, policy
- `sorrel-cli`: persistent `.sorrel/` — init/status/change/diff/log/lane/
  merge(--abort)/grant/slice/workflow/remote/push/pull (pull restores worktree)
- Sync CLI ↔ Hub with bootstrap grants (`user:local` +
  `grant_local_object_write` / `grant_local_ref_write`)
- Hub FS sync + metadata; Hub understands Core protocol shapes in closure walk
  (`rootTree.id`, `parents[].id`, `entries[].object.id`); refs list is an
  **array**
- Hub UI read-only; Dockerfiles + root `docker compose` for hub + hub-web
- Landing updated with status/docs viewer under `/docs/`

Validate anytime with:
- Rust: `cargo test`, `cargo clippy --all-targets`, `cargo fmt --all -- --check`
- Node: `npm test` (+ `npm run validate` where defined)

Happy path smoke (after Hub on :3000):
```sh
cd sorrel-cli && cargo build && SORREL=target/debug/sorrel
# init → change → lane create/switch → merge → remote add → push → pull
# in a second dir with --repo-id; confirm files restore
docker compose up --build   # API :3000, UI :5180, landing preview :4173
```

## Uncommitted work you must land in Phase A

Submodules are on detached HEADs matching prior main SHAs but with local edits:

### `sorrel-cli` (was `5340f75`)
- `src/sync.rs` — send bootstrap `grantRefs` on object upload / ref advance;
  parse Hub refs as protocol **array** (compat with old map shape)
- `src/main.rs` — after pull, restore working tree to pulled snapshot
- `tests/sync.rs` — mock Hub returns refs array
- `SYNC.md`, `AGENTS.md` — docs (no stub)

### `sorrel-hub` (was `d8119b7`)
- `src/bootstrap-grants.js` + `server.js` wiring (`SORREL_HUB_BOOTSTRAP_GRANTS`,
  `SORREL_HUB_TRUSTED_GRANTS_FILE`)
- `src/sync-closure.js` — Core/protocol object ref shapes
- `src/app.js` — map `PolicyEvaluationError` → 403
- `Dockerfile`, `.dockerignore`, README deploy notes
- Tests: `test/bootstrap-grants.test.js`, `test/sync-closure-protocol.test.js`

### `sorrel-hub-web` (was `5cc7137`)
- Production-capable static+proxy server (`HOST` default `0.0.0.0`)
- `Dockerfile`, README/AGENTS deploy notes

### `sorrel-web` (was `6786303`) — Cloudflare-bound
- Landing status/roadmap/hero updates
- `docs/STATUS.md`, `docs/GETTING_STARTED.md`, `docs/index.html`, `docs/docs.js`
- Optional Dockerfile for local preview only
- **No Cloudflare settings change** (still: no build, publish `.`). Content
  must be pushed so Pages picks up `/docs/`.

### Root repo
- `docs/` (STATUS, GETTING_STARTED, README)
- `README.md` rewrite (status + quick start)
- `docker-compose.yml` (hub, hub-web, optional web preview)
- Dirty submodule pointers once submodule mains advance

Phase A procedure (per submodule):
1. Create a branch from current detached commit + your changes
2. Run that module’s tests; fix only regressions you introduce
3. Commit with a clear message; push; open PR with `gh`; merge when green
4. Checkout `main`, pull, confirm SHA
5. In root: update submodule pointer, commit docs/compose/README, open root PR

Suggested commit themes (adapt as needed):
- cli: “Fix Hub sync: grantRefs, refs array, restore worktree on pull”
- hub: “Bootstrap local sync grants; walk Core protocol closures; Dockerize”
- hub-web: “Make static+proxy server deployable”
- web: “Publish status/getting-started docs; refresh landing state”
- root: “Docs, compose stack, point submodules at happy-path mains”

## Phase B — Git import (after Phase A)

Repositories: primarily `sorrel-core`, then `sorrel-cli` (pin new core rev).
Optional tiny protocol note only if you add a new documented object/mapping
shape — prefer no schema churn on first pass.

### Product intent
Adoption path from ROADMAP §3: teams try Sorrel without leaving Git.
**This pass = one-way import only.**

### Suggested design (you may refine, but keep scope tight)
1. **Core library API** e.g. `git_import(store, options) -> ImportResult`
   - Read a Git repository (start with **libgit2 via `git2` crate** or a
     clearly justified alternative; avoid shelling out to `git` for the main
     path if a crate works cleanly)
   - Walk commits in topo/date order for the selected ref (default `HEAD`)
   - For each commit: materialize tree → Sorrel blobs/trees/snapshot; record a
     Change (or equivalent) linking parent snapshot → new snapshot
   - Keep a durable mapping table (e.g. under `.sorrel/git-map.json` or an
     object) from Git SHA → Sorrel snapshot id for later export/colocated work
2. **CLI**: `sorrel git import [PATH]` (default `.`)
   - Requires or creates a Sorrel workspace; refuse to clobber an unrelated
     dirty tree without a flag
   - `--ref`, `--limit` (optional) for safer demos
   - `--json` like other commands
3. **Tests**: temp Git repo with 2–3 commits, import, assert snapshot count,
   file contents via status/log or core reads; clippy/fmt clean
4. **Docs**: short section in `sorrel-cli/DEMO.md` or `sorrel-cli/GIT.md` +
   update root `docs/STATUS.md` / `sorrel-web/docs/STATUS.md` when import works
5. Out of scope: export, bidirectional sync, LFS, submodules, replace Git,
   Hub UI, auth

Pin `sorrel-cli`’s `sorrel-core` git `rev` to the merged import commit. Run
`cargo test --workspace` in CLI after the pin.

## Explicit non-goals for this agent

- Do not start `sorrel-agents`, SDKs, embedding/C ABI, desktop apps
- Do not build Hub proposal write UI or production auth
- Do not implement merge `--continue` unless you finish Phase B early and the
  user asks; prefer Git import
- Do not change Cloudflare project settings
- Do not leave useful work only on a feature branch — merge to submodule
  `main` and advance root pointers (see `SORREL_PROGRESS.md` repair checklist)

## Report back when done

1. Phase A: per-repo branch names, merged main SHAs, PR URLs
2. Root pointer commit SHA + what docs landed
3. Phase B: design sketch (1 short paragraph), core+cli SHAs, how to run import
4. Test commands + results
5. Updates made to `SORREL_PROGRESS.md` / `docs/STATUS.md` / landing docs
6. Anything blocked (private submodule access, git2 issues, etc.)
```

---
