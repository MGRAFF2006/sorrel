# Sorrel Progress Dashboard

Last updated: 2026-06-24 14:17 UTC

This is the root overview for Sorrel orchestration. Update this file whenever an agent reports completion, a PR is merged, or the execution plan changes.

## Current rule of operation

- The root repository stays on `main`.
- Each submodule repository should merge implementation work into its own `main`.
- The root repository should only point submodules at commits reachable from the corresponding submodule `main`.
- Agents should now run inside the submodule repos directly, not primarily from the root monorepo.
- After a submodule merge, update the root submodule pointer and open/merge a small root PR.

## Current architecture correction

Sorrel Core must include headless identity, permissions, grants, policy decisions, redaction, `SecretRef`, and audit semantics from the foundation. Sorrel Hub is the collaboration product and administration surface on top of those Core semantics, not the owner of the only permission model.

Decentralized does not mean permissionless. Core policy/grant/authority changes must be signed `PolicyChange` objects evaluated against the previous effective policy. A principal cannot grant itself power unless it already has delegated authority such as `policy.grant`, `policy.delegate`, or `authority.admin`; peers, runners, Hub, remotes, and vaults should reject locally edited or forged permission state.

The initial compatibility pass has now landed across the separate submodule repositories. The current coordination priority is to repair root pointers to those submodule-main commits, then do a focused unification/conformance pass so Core policy behavior does not drift across consumers:

1. Repair root submodule pointers from the inaccessible PR #25 gitlinks to real submodule `main` commits.
2. Use protocol fixtures as cross-language conformance tests for `AuthorityRoot`, `PolicyChange`, and policy decisions.
3. Reduce evaluator drift between Rust Core, embedded CLI code, Hub's JS mirror, runner evaluators, and Vault adapters.
4. Resume workflow/vault/Hub expansion only after the policy authority contract is consistent.

## Module status

| Module | Status | Latest known work | Notes |
| --- | --- | --- | --- |
| `sorrel-protocol` | Done / merged / root pointer repair staged | Protocol package + Core permission spine + AuthorityRoot/PolicyChange schemas | Submodule PR #1 merged at `5c43054`; local validation passed with `npm test` (61 passed). This branch repairs the root pointer to `5c43054`. |
| `sorrel-core` | Done / merged / root pointer repair staged | Object store + snapshot + Change + policy evaluator + lane/stack metadata + authority hardening | Submodule PR #2 merged at `5e84f0f`; local validation passed with `cargo test` (40 passed). This branch repairs the root pointer to `5e84f0f`. |
| `sorrel-cli` | Done / merged / root pointer repair staged | Mocked CLI + Core-backed policy evaluation and policy-change command surfaces | Submodule PR #2 merged at `0717f6f`; local validation passed with `cargo test --workspace` (17 CLI integration + 5 embedded core tests). This branch repairs the root pointer to `0717f6f`. |
| `sorrel-vault` | Done / merged / root pointer repair staged | Secrets spec/local backend + trusted Core evaluate for `secret.read` / `secret.inject` | Submodule PR #1 merged at `f1ed7cd`; local validation passed with `npm test`. This branch repairs the root pointer to `f1ed7cd`. |
| `sorrel-runners` | Done / merged / root pointer repair staged | Local/container runner + Core permission gate before JobBundle execution | Submodule PR #1 merged at `38dcef3`; local validation passed with `cargo test` (12 passed). This branch repairs the root pointer to `38dcef3`. |
| `sorrel-slices` | Done / merged | TS/JS slice manifest prototype | Relative import dependency closure, package metadata, unresolved imports. |
| `sorrel-web` | Public product page hosted / merged | Polished static landing page + Core-native permissions copy | Root points to `sorrel-web/main` at `db12183`. |
| `sorrel-hub` | Done / merged / root pointer repair staged | Node HTTP app/server skeleton + Core policy refs/admin guard | Submodule PR #2 merged at `aae580a`; local validation passed with `npm test` (13 passed). This branch repairs the root pointer to `aae580a`. |
| `sorrel-agents` | Not started | Agent policy/control plane | Start after lanes/claims are clearer. |
| `sorrel-sdk-js` | Not started | TypeScript SDK | Start after protocol stabilizes around CLI/HUB needs. |
| `sorrel-sdk-rust` | Not started | Rust SDK | Start after core APIs settle. |

## Active agents

Reported running by user:

| Agent | Target | Goal | Dependency notes |
| --- | --- | --- | --- |
| None active | - | - | Webpage agent completed and merged in `sorrel-web` PR #1. |

## Blocked handoffs

| Module | Local branch/commit | Blocker | Recovery action |
| --- | --- | --- | --- |
| Root submodule pointers | root PR #25 / `a5dc79c` | PR #25 was merged with six inaccessible submodule gitlinks (`not our ref`) for protocol/core/CLI/Hub/runners/vault. | Repair staged in `cursor/repair-authority-hardening-pointers-c5ce`, pointing to actual submodule `main` commits: protocol `5c43054`, core `5e84f0f`, CLI `0717f6f`, Hub `aae580a`, runners `38dcef3`, vault `f1ed7cd`. |
| Root PR #24 | `cursor/submodule-authority-prompts-43b8` | Prompt-only PR is now stale because the authority hardening work already ran in the separate submodule repositories. | Close or supersede PR #24 unless durable prompt docs are explicitly wanted. |
| Historical only | old wrong-repo local branch `cursor/sorrel-hub-skeleton-18de` / `48583c2` | Superseded by correct-repo implementation. | No recovery needed unless useful code must be compared manually. |

## Immediate next completion checks

When an active agent reports completion, verify and record:

1. Submodule repo branch and commit.
2. Validation commands and result.
3. Parent/root PR link and commit.
4. Whether the submodule commit is merged into that submodule repo's `main`.
5. Whether the root submodule pointer points at that submodule `main` commit.

## Next planned agents

These are ready for agents working directly in the submodule repos after verifying each submodule `main`.

### O - repair root pointers after authority hardening

Goal:

- Replace the inaccessible root PR #25 gitlinks with actual submodule `main` commits.
- Update the root dashboard and mark PR #24 stale/superseded.
- Verify each repaired root gitlink is reachable from the corresponding submodule `origin/main`.

Depends on:

- Authority-hardening submodule PRs merged in the separate repositories.

### P - unify Core policy evaluator usage across consumers

Goal:

- Review the new authority-hardening implementations and remove drift between duplicated policy evaluators/adapters.
- `sorrel-cli` currently embeds a `crates/sorrel-core` path dependency.
- `sorrel-hub` mirrors Core evaluation in JavaScript until a package is available.
- `sorrel-runners` uses a `CorePermissionEvaluator` trait and in-memory grant test double.
- `sorrel-vault` uses injected `corePolicy.evaluate()` / local-dev adapter.
- Decide the short-term shared contract: protocol fixtures + test vectors, a generated JS SDK, or a small shared package.

Depends on:

- Root pointer repair landing.
- Current authority-hardening commits being reachable from submodule `main`.

### L - `sorrel-core` lanes and stacks

Goal:

- Implement Lane and Stack objects on top of Change and Snapshot after the permission spine exists.
- Include owner principal, visibility, policy refs, grant refs, touched resources, and audit hooks from the start.
- Focus on metadata, serialization, touched paths/resources, and tests.
- Do not implement merge logic yet.

Depends on:

- Change model completed in `sorrel-core`; latest verified `sorrel-core/main` is `ca82981`.
- Core permission spine compatibility pass.

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
| 1 | Merge root pointer repair for authority-hardening work | root repo | Root PR #25 points at inaccessible gitlinks; repair branch points to real submodule `main` commits. |
| 2 | Close/supersede stale prompt PR #24 | root repo | Authority hardening already ran in separate repositories. |
| 3 | Policy evaluator unification pass | `sorrel-core`, `sorrel-cli`, `sorrel-hub`, `sorrel-runners`, `sorrel-vault` | Avoid long-term drift between embedded/mirrored/evaluator-adapter implementations. |
| 4 | Protocol/Core compatibility test vectors | `sorrel-protocol`, `sorrel-core`, JS consumers | Use protocol fixtures as cross-language conformance tests for `PolicyChange` and authority decisions. |
| 5 | Workflow file parser with policy inputs | `sorrel-runners` / `sorrel-cli` | Runner prototype and CLI integration complete; use Core workflow.run/runner.use decisions. |
| 6 | Vault CLI/dev API on Core grants | `sorrel-vault` | Vault backend complete; map grants/redaction to Core policy. |
| 7 | Hub proposal/review expansion consuming Core policy | `sorrel-hub` | Hub skeleton verified on `sorrel-hub/main` at `aae580a`; policy should be Core-owned. |
| 8 | Agent control plane | `sorrel-agents` | Lanes/stacks + Core policy model. |
| 9 | Git bridge | `sorrel-core` / `sorrel-cli` | Change + lanes basics. |
| 10 | Merge/conflict model | `sorrel-core` | Change + lanes basics. |

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
| 2026-06-24 10:59 | Opened root PR #13 from `cursor/update-sorrel-web-product-page-c5ce` to update the product-page pointer, repair completed submodule pointers, and refresh the progress dashboard. |
| 2026-06-24 11:12 | Architecture correction recorded: Core owns headless identity, permissions, grants, policies, redaction, `SecretRef`, and audit semantics; Hub consumes/administers them. Added `SORREL_AGENT_PROMPTS.md` for adaptation and next agents. |
| 2026-06-24 11:17 | Root PR #13 merged, so root `main` now points Core/CLI/Hub/Web at verified submodule-main commits. |
| 2026-06-24 11:20 | Resolved PR #14 conflict against merged PR #13, preserving completed pointer repair status and Core-permissions priority correction. |
| 2026-06-24 11:27 | User reported starting the prompt-pack agents. Added decentralized authority/self-escalation requirements and a `sorrel-web` prompt for updating the landing page with Core-native permissions messaging. |
| 2026-06-24 11:45 | User reported all prompt-pack agents finished but opened root PRs instead of completing submodule-main handoffs. Manually merged/pushed repaired submodule mains: protocol `f5cf9cd`, core `ca82981`, CLI `d33ae00`, Hub `3278782`, runners `5028df5`, vault `fa16a55`. Removed stale `SORREL_AGENT_PROMPTS.md`; webpage agent reported running. |
| 2026-06-24 11:46 | User reported `sorrel-web` PR #1 merged. Staged root `sorrel-web` pointer to `db12183`, which adds Core-native permissions landing-page copy. |
| 2026-06-24 11:56 | User merged root PR #21 and closed superseded root PRs #15-#20. Verified no open root PRs and root `main` points to repaired submodule-main commits. |
| 2026-06-24 14:11 | User reported authority-hardening agents were split into separate repositories. Verified merged submodule PRs: protocol #1 `5c43054`, core #2 `5e84f0f`, CLI #2 `0717f6f`, Hub #2 `aae580a`, runners #1 `38dcef3`, vault #1 `f1ed7cd`. Found root PR #25 had inaccessible gitlinks and needs repair; root PR #24 prompt docs are stale. |
| 2026-06-24 14:17 | Staged root pointer repair to actual submodule `main` commits and locally revalidated: protocol `npm test`, core `cargo test`, CLI `cargo test --workspace`, Hub `npm test`, runners `cargo test`, vault `npm test` all passed. |
