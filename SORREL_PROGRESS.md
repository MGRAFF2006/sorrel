# Sorrel Progress Dashboard

Last updated: 2026-06-24 11:56 UTC

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

This means the next implementation priority is a compatibility pass across completed foundations before deeper lanes/stacks work:

1. Define protocol schemas for principals, capabilities, resources, grants, policies, decisions, secret refs, redaction, and audit events.
2. Add authority roots, signed policy changes, delegation rules, and self-escalation denial.
3. Add matching Core model types and a minimal deterministic in-memory policy evaluator.
4. Adapt CLI, Vault, Runners, Slices, and Hub assumptions to consume the shared Core permission vocabulary.
5. Resume lanes/stacks with owner principal, visibility, policy refs, grant refs, touched resources, and audit hooks included from the start.

## Module status

| Module | Status | Latest known work | Notes |
| --- | --- | --- | --- |
| `sorrel-protocol` | Done / merged | Protocol package + Core permission spine schemas | Root points to `sorrel-protocol/main` at `f5cf9cd`; validation passed with `npm run validate`. |
| `sorrel-core` | Done / merged | Object store + snapshot + Change + policy evaluator + lane/stack metadata | Root points to `sorrel-core/main` at `ca82981`; `cargo test` passed 28/28. |
| `sorrel-cli` | Done / merged | Mocked CLI + headless policy/grant/secret command surfaces | Root points to `sorrel-cli/main` at `d33ae00`; `cargo test` passed 14/14 integration tests. |
| `sorrel-vault` | Done / merged | Secrets spec/local backend + Core policy decisions for grants/redaction | Root points to `sorrel-vault/main` at `fa16a55`; `npm test` passed. |
| `sorrel-runners` | Done / merged | Local/container runner + Core policy-gated JobBundle/secret refs | Root points to `sorrel-runners/main` at `5028df5`; `cargo test` passed 7/7 integration tests. |
| `sorrel-slices` | Done / merged | TS/JS slice manifest prototype | Relative import dependency closure, package metadata, unresolved imports. |
| `sorrel-web` | Public product page hosted / merged | Polished static landing page + Core-native permissions copy | Root points to `sorrel-web/main` at `db12183`. |
| `sorrel-hub` | Done / merged | Node HTTP app/server skeleton + Core policy refs | Root points to `sorrel-hub/main` at `3278782`; `npm test` passed 10/10. |
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
| None active | Root PRs #15-#20 were superseded and closed; root PR #21 was merged. | Submodule work now lives on each corresponding submodule `main`, and root points to those commits. | No action needed. |
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

### O - verify repaired Core permission compatibility

Goal:

- Review repaired submodule-main commits for consistency across protocol/core/CLI/Vault/Runners/Hub.
- Confirm authority roots, signed policy changes, scoped delegation, and explicit self-grant/self-escalation denial are either implemented or tracked as next gaps.
- Treat this as the next focused review/hardening pass, not a pointer repair.

Depends on:

- Root `main` now points to repaired submodule mains.

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
| 1 | Verify/self-escalation hardening pass | `sorrel-protocol`, `sorrel-core`, consumers | Ensure signed `PolicyChange`/authority rules are implemented, not just documented. |
| 2 | Workflow file parser with policy inputs | `sorrel-runners` / `sorrel-cli` | Runner prototype and CLI integration complete; use Core workflow.run/runner.use decisions. |
| 3 | Vault CLI/dev API on Core grants | `sorrel-vault` | Vault backend complete; map grants/redaction to Core policy. |
| 4 | Hub proposal/review expansion consuming Core policy | `sorrel-hub` | Hub skeleton verified on `sorrel-hub/main` at `3278782`; policy should be Core-owned. |
| 5 | Agent control plane | `sorrel-agents` | Lanes/stacks + Core policy model. |
| 6 | Git bridge | `sorrel-core` / `sorrel-cli` | Change + lanes basics. |
| 7 | Merge/conflict model | `sorrel-core` | Change + lanes basics. |

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
