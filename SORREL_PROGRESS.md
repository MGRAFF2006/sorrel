# Sorrel Progress Dashboard

Last updated: 2026-06-24 10:26 UTC

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
| `sorrel-core` | Done / merged | Object store + snapshot model | BLAKE3 IDs, object stores, blobs, trees, snapshots, materialization/readback/restore. |
| `sorrel-cli` | Done / merged | Mocked CLI skeleton | `sorrel` binary; commands support `--json`; no `sorrel-core` dependency yet. |
| `sorrel-vault` | Done / merged | Secrets spec + local dev backend | `sorrel.secrets.yml`, SecretRef examples, grants, redaction, local backend. |
| `sorrel-runners` | Done / merged | Local runner prototype | JobBundle, capabilities, local runner, minimal Docker/Podman runner, JSONL logs. |
| `sorrel-slices` | Done / merged | TS/JS slice manifest prototype | Relative import dependency closure, package metadata, unresolved imports. |
| `sorrel-web` | Seeded | Static landing page | Continue independently; does not block core architecture. |
| `sorrel-hub` | Blocked / local only | Axum app/server skeleton | Implemented locally at `cursor/sorrel-hub-skeleton-18de` commit `48583c2`, but not pushed because the agent could not access `MGRAFF2006/sorrel-hub`. |
| `sorrel-agents` | Not started | Agent policy/control plane | Start after lanes/claims are clearer. |
| `sorrel-sdk-js` | Not started | TypeScript SDK | Start after protocol stabilizes around CLI/HUB needs. |
| `sorrel-sdk-rust` | Not started | Rust SDK | Start after core APIs settle. |

## Active agents

Reported running by user:

| Agent | Target | Goal | Dependency notes |
| --- | --- | --- | --- |
| H | `sorrel-core` | First Change model | Builds on object store, blobs, trees, snapshots, and `sorrel-protocol`. |
| I | `sorrel-cli` | Integrate CLI with real local modules where feasible | Preserve existing mocked JSON output compatibility. |
| K | root `AGENTS.md` | Durable instructions for future agents | Should replace stale setup notes and document submodule/private repo realities. |

## Blocked handoffs

| Module | Local branch/commit | Blocker | Recovery action |
| --- | --- | --- | --- |
| `sorrel-hub` | `cursor/sorrel-hub-skeleton-18de` / `48583c2` | Agent could not push to `https://github.com/MGRAFF2006/sorrel-hub.git` (`Repository not found`). | Run a new agent directly inside the accessible `sorrel-hub` repo, or manually push the local commit if available, then merge into `sorrel-hub/main` and update the root submodule pointer. |

## Immediate next completion checks

When an active agent reports completion, verify and record:

1. Submodule repo branch and commit.
2. Validation commands and result.
3. Parent/root PR link and commit.
4. Whether the submodule commit is merged into that submodule repo's `main`.
5. Whether the root submodule pointer points at that submodule `main` commit.

## Next planned agents

Start these only after the active batch lands.

### L - `sorrel-core` lanes and stacks

Goal:

- Implement Lane and Stack objects on top of Change and Snapshot.
- Focus on metadata, serialization, touched paths, and tests.
- Do not implement merge logic yet.

Depends on:

- H / Change model.

### M - `sorrel-runners` workflow file parser

Goal:

- Add parsing for a simple `sorrel.workflow.yml`.
- Execute jobs through the existing `LocalProcessRunner`.
- Preserve portable JobBundle model.

Depends on:

- F / runner prototype.
- I / CLI integration if CLI will expose the parser immediately.

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
| 1 | Change model | `sorrel-core` | Snapshot model complete. |
| 2 | CLI real integration | `sorrel-cli` | Core snapshots, slices, runners. |
| 3 | Hub skeleton | `sorrel-hub` | Protocol complete. |
| 4 | Lanes/stacks | `sorrel-core` | Change model. |
| 5 | Workflow file parser | `sorrel-runners` / `sorrel-cli` | Runner prototype. |
| 6 | Vault CLI/dev API | `sorrel-vault` | Vault backend. |
| 7 | Agent control plane | `sorrel-agents` | Lanes/stacks + policy model. |
| 8 | Git bridge | `sorrel-core` / `sorrel-cli` | Change + lanes basics. |
| 9 | Merge/conflict model | `sorrel-core` | Change + lanes basics. |
| 10 | Sorrel Hub review/proposals | `sorrel-hub` | Hub skeleton + Change model. |

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
