# Sorrel Roadmap

Last updated: 2026-10-15

The launch target is a hosted collaboration product for teams working with
coding agents: **recoverable work, isolated execution, and consequential review**.
Current behavior belongs in [docs/STATUS.md](docs/STATUS.md); releases and
changelogs record shipped work. These are future milestones, not shipped claims.

## 1. Validate an invite-only hosted pilot

Finish credentialed WorkOS AuthKit validation at a real HTTPS origin: login,
callback, expiry, refresh, logout, invitation and revoked access. Configure exact
Core organization/project/repository grants. Test two teams through the browser
proxy, including private metadata, counters, objects, reviews and CLI uploads.
Exercise restart and backup restoration on the intended host. Verify actual
release assets; an intended build matrix does not establish download availability.

Acceptance: an invited teammate signs in, finds the shared project, connects a
disposable checkout, uploads a lane and reviews recorded changes without editing
policy JSON. Managed CLI login must replace manual bearer-token provisioning.
Desktop remote-Hub selection and secure credentials follow the verified session
path. The old alpha artifacts do not contain new source changes.

## 2. Make daily agent work recoverable and isolated

1. Extend recorded-state recovery to interrupted working-tree restores. Preserve
   unfinished work and define filesystem-failure rollback before promising
   universal undo.
2. Extend fixed-lane isolated directories with explicit directory lifecycle and
   ownership transfer; assess linked Git/SDK support and finer locks from real use.
3. Extend literal path recording to selective hunks. Leave unrelated unfinished
   work untouched.
4. Connect advisory claims to authenticated remote coordination and explicit
   cleanup/retention. Keep expiry separate from authorization and execution
   isolation; local leases must not look like a sandbox or permission boundary.

Acceptance: two agents work separately, submit focused changes and recover from
an abandoned or incorrect operation without losing each other's files. Add
mutation previews, doctor/integrity checks and storage accounting using the same
contracts. Retain recovery records through migrations and future garbage collection.

## 3. Make reviews consequential

Implement **Integrate proposal** through Core together with approvals and checks
bound to exact source and target snapshots. Compare the expected target head
atomically; changed snapshots or stale approvals/checks prevent integration.
Report conflicts before advancing refs and record a recovery checkpoint.

Acceptance: one permitted action advances the intended repository, retries are
safe, and a stale review cannot integrate different code. Until this exists,
“Mark as merged” remains a manual review record.

## 4. Give agents a small, useful integration surface

Start a narrow MCP adapter over tested CLI/Core contracts: status, diff, lane
operations, submission, checks and run results. Add handoff bundles pinned to a
base snapshot, with task context, paths, open questions and verification results.
Explain policy denials with effective scope, expiry and the next approval step;
explanations never grant access.

Then add stacked-change append/amend/restack/submit and show affected dependent
reviews. Materialize inspectable context slices pinned to snapshots, reporting
unresolved imports and exclusions before bidirectional slice sync. Tie workflow
results to tested snapshots; add log following, authorized streaming and cancellation.

## 5. Expand into a dependable team product

Add self-service team/workspace provisioning through previously authorized Core
authority, invitations, scoped agent credentials, revocation and an operator audit
trail. Add pagination, tenant quotas, abuse controls and versioned migrations with
preflight backups. Test disaster recovery. Garbage collection needs a preview
and retention for checkpoints, active reviews and in-flight work.

Sign native distribution after remote sessions and recovery settle. Stabilize an
embedding API when concrete MCP/SDK/native integrations demonstrate the needed
boundary. Keep Convex migration, hosted compute and marketplace work behind
observed pilot demand.

## Market validation and launch

[Git worktrees](https://git-scm.com/docs/git-worktree),
[Jujutsu](https://docs.jj-vcs.dev/latest/git-compatibility/) and
[GitButler](https://docs.gitbutler.com/ai-agents/parallel-agents) already address
Git-compatible parallel workflows (sources checked 2026-10-14). Sorrel must
show a complete delegation → verification → review → integration → recovery loop.
A new object model and the phrase “agent-native” do not establish demand.

Recruit 3–5 agent-tool teams as design partners. Trial their disposable repositories;
record first successful upload/review, time to that outcome, where help was needed
and return use the next week. Interview users who stop as well as those who succeed.
Keep private source, tokens and prompts out of marketing analytics.

After the pilot gates pass, publish a real workflow recording, honest release
announcement and reproducible setup. Invite feedback through existing GitHub issue
forms. Share the demo in relevant developer/agent-tool communities under their
current posting rules. Broad Show HN/Product Hunt promotion follows a usable
integration/recovery story. Keep ad spend at zero until users complete onboarding
and return without hands-on operator help.

## Performance

Measure pilot repositories before making speed claims. Preserve existing benchmark
baselines and warm status/history budgets; investigate packing, indexes, large
blobs and lazy fetch when observed workloads identify the limiting operation.
