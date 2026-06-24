# Sorrel Progress Dashboard

Last updated: 2026-06-24 10:58 UTC

This is the root overview for Sorrel orchestration. Update this file whenever an agent reports completion, a PR is merged, or the execution plan changes.

## Current rule of operation

- The root repository stays on `main`.
- Each submodule repository should merge implementation work into its own `main`.
- The root repository should only point submodules at commits reachable from the corresponding submodule `main`.
- Agents should now run inside the submodule repos directly, not primarily from the root monorepo.
- After a submodule merge, update the root submodule pointer and open/merge a small root PR.

## Module status

| Module | Status | Latest known work | Notes |
| --- | --- | --- | --- |
| `sorrel-protocol` | Done / merged | Initial `sorrel.protocol.v0` schema package | Schemas, examples, validation, versioning rules. |
| `sorrel-core` | Done / merged / root pointer staged | Object store + snapshot + Change model | Verified `sorrel-core/main` at `af2505b`; this root branch updates the pointer from `f478c5d` to `af2505b`. |
| `sorrel-cli` | Done / merged / root pointer staged | Mocked CLI + local integration pass | Verified `sorrel-cli/main` at `7160391`; this root branch updates the pointer from `6b8cb6a` to `7160391`. |
| `sorrel-vault` | Done / merged | Secrets spec + local dev backend | `sorrel.secrets.yml`, SecretRef examples, grants, redaction, local backend. |
| `sorrel-runners` | Done / merged | Local runner prototype | JobBundle, capabilities, local runner, minimal Docker/Podman runner, JSONL logs. |
| `sorrel-slices` | Done / merged | TS/JS slice manifest prototype | Relative import dependency closure, package metadata, unresolved imports. |
| `sorrel-web` | Public product page shipped / root pointer staged | Polished static landing page | Committed to `sorrel-web/main` at `1862d5b`; validation passed with HTML asset check and local Python static server. |
| `sorrel-hub` | Done / merged / root pointer staged | Node HTTP app/server skeleton | Verified `sorrel-hub/main` at `c0707b7`; correct-repo implementation includes dependency-light Node HTTP server, models, in-memory store, project APIs, health route, README, and tests reported 5/5 passing. |
| `sorrel-agents` | Not started | Agent policy/control plane | Start after lanes/claims are clearer. |
| `sorrel-sdk-js` | Not started | TypeScript SDK | Start after protocol stabilizes around CLI/HUB needs. |
| `sorrel-sdk-rust` | Not started | Rust SDK | Start after core APIs settle. |

## Active agents

Reported running by user:

| Agent | Target | Goal | Dependency notes |
| --- | --- | --- | --- |
| K | root `AGENTS.md` | Durable instructions for future agents | Should replace stale setup notes and document submodule/private repo realities. |

## Blocked handoffs

| Module | Local branch/commit | Blocker | Recovery action |
| --- | --- | --- | --- |
| None active | Previous Core/CLI parent pointer pushes were blocked by token permissions. | Repaired in root branch `cursor/update-sorrel-web-product-page-c5ce` by pointing to commits reachable from each submodule `main`. | Merge the root pointer PR after review. |
| Historical only | old wrong-repo local branch `cursor/sorrel-hub-skeleton-18de` / `48583c2` | Superseded by correct-repo implementation. | No recovery needed unless useful code must be compared manually. |

## Immediate next completion checks

When an active agent reports completion, verify and record:

1. Submodule repo branch and commit.
2. Validation commands and result.
3. Parent/root PR link and commit.
4. Whether the submodule commit is merged into that submodule repo's `main`.
5. Whether the root submodule pointer points at that submodule `main` commit.

## Next planned agents

These are ready once this root pointer repair PR lands, or immediately if agents work directly in the submodule repos and verify `main` first.

### L - `sorrel-core` lanes and stacks

Goal:

- Implement Lane and Stack objects on top of Change and Snapshot.
- Focus on metadata, serialization, touched paths, and tests.
- Do not implement merge logic yet.

Depends on:

- Change model completed in `sorrel-core`; latest verified `sorrel-core/main` is `af2505b`.

### M - `sorrel-runners` workflow file parser

Goal:

- Add parsing for a simple `sorrel.workflow.yml`.
- Execute jobs through the existing `LocalProcessRunner`.
- Preserve portable JobBundle model.

Depends on:

- F / runner prototype.
- CLI integration completed in `sorrel-cli` PR #1 if CLI will expose the parser immediately.

### N - `sorrel-vault` CLI/dev integration

Goal:

- Add a small CLI or library API for:
  - importing `.env`
  - listing refs
  - granting access
  - redacting logs

Depends on:

- D / vault local backend.

## Backlog sequence

| Order | Work | Target | Blocked by |
| --- | --- | --- | --- |
| 1 | Merge root pointer/product-page PR | root repo | Pointers staged for Core/CLI/Hub/Web and dashboard updated. |
| 2 | Lanes/stacks | `sorrel-core` | Change model complete; latest verified main is `af2505b`. |
| 3 | Workflow file parser | `sorrel-runners` / `sorrel-cli` | Runner prototype and CLI integration complete. |
| 4 | Vault CLI/dev API | `sorrel-vault` | Vault backend complete. |
| 5 | Hub proposal/review expansion | `sorrel-hub` | Hub skeleton verified on `sorrel-hub/main` at `c0707b7`. |
| 6 | Agent control plane | `sorrel-agents` | Lanes/stacks + policy model. |
| 7 | Git bridge | `sorrel-core` / `sorrel-cli` | Change + lanes basics. |
| 8 | Merge/conflict model | `sorrel-core` | Change + lanes basics. |

## Do not start yet

- Marketplace.
- Full merge queue.
- Hosted compute.
- Production auth.
- Full Git bridge.
- Sophisticated conflict resolution.

These are later once Core, CLI, Vault, Runners, and Hub skeleton have a stable integration path.

## Submodule pointer repair checklist

Use this whenever an agent accidentally leaves useful work only on a submodule feature branch.

```bash
# Inside the submodule repo
git checkout main
git pull origin main
git fetch origin <feature-branch>
git merge --ff-only origin/<feature-branch> || git merge --no-ff origin/<feature-branch>
git push origin main

# Inside the root repo
git checkout main
git pull origin main
git -C <submodule> checkout main
git -C <submodule> pull origin main
git add <submodule>
git commit -m "Point <submodule> at main"
git push origin main
```

## Progress log

| Time UTC | Event |
| --- | --- |
| 2026-06-24 08:52 | `sorrel-protocol` completed and merged: initial protocol/spec package. |
| 2026-06-24 09:00 | `sorrel-core` object store foundation completed and merged. |
| 2026-06-24 09:24 | `sorrel-cli` mocked CLI skeleton completed and merged. |
| 2026-06-24 09:24 | `sorrel-vault` secrets spec/local backend completed and merged. |
| 2026-06-24 09:46 | `sorrel-core` snapshot model completed and merged. |
| 2026-06-24 09:46 | `sorrel-runners` local/container runner prototype completed and merged. |
| 2026-06-24 09:46 | `sorrel-slices` TS/JS slice manifest prototype completed and merged. |
| 2026-06-24 10:10 | Root/submodule branch policy clarified: root on `main`, submodule work merged into submodule `main`. |
| 2026-06-24 10:19 | Active batch reported running: H (`sorrel-core` changes), I (`sorrel-cli` integration), J (`sorrel-hub` skeleton), K (`AGENTS.md`). |
| 2026-06-24 10:26 | `sorrel-hub` skeleton implemented locally with Axum/models/routes/tests, but blocked from push due to private repo access. |
| 2026-06-24 10:45 | Correct `sorrel-hub` implementation completed with Node HTTP server, models, in-memory store, routes, README, and 5/5 tests. |
| 2026-06-24 10:45 | `sorrel-core` Change model completed and merged in submodule PR #1 at `64d9c26`; root pointer update blocked from push. |
| 2026-06-24 10:45 | `sorrel-cli` local integration pass completed and merged in submodule PR #1; root pointer update blocked from push. |
| 2026-06-24 10:58 | `sorrel-web` public product page completed and pushed to `sorrel-web/main` at `1862d5b`; static validation passed. |
| 2026-06-24 10:58 | Verified submodule mains and staged root pointer repairs: `sorrel-core` `af2505b`, `sorrel-cli` `7160391`, `sorrel-hub` `c0707b7`, `sorrel-web` `1862d5b`. |
