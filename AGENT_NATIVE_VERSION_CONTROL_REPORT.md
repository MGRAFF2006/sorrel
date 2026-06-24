# Sorrel Core and Sorrel Hub: Agent-Native Version Control and Collaboration Report

## Executive summary

Sorrel Core and Sorrel Hub are the final selected names for a new version-control and collaboration system built for modern software work: humans, parallel AI agents, cloud workspaces, local-first development, secrets, portable workflows, and selective sharing of unfinished work.

The core idea is not to build "Git but nicer." The better architecture is a layered system:

1. A new VCS core based on content-addressed objects, changes, lanes, slices, and first-class conflicts.
2. Core-native identity, permissions, grants, policy evaluation, audit records, and secret/environment references that work without Sorrel Hub.
3. A Git bridge so teams can import, export, mirror, and leave without lock-in.
4. A collaboration product with reviews, stacks, proposals, issues, CI, policy administration, and agent coordination.
5. A secret and environment layer built into the workflow and governed by Core grants.
6. A portable workflow and runner protocol that supports local and user-owned remote compute.
7. An optional marketplace for integrations, workflow modules, analyzers, runners, and agent tools.

The framework and protocol should be independent. Hosted products, marketplaces, and UI layers should sit on top of the open core rather than define identity, permission, policy, or secret semantics.

## Naming recommendation

### Recommended working names: Sorrel Core and Sorrel Hub

Sorrel Core is the protocol, storage engine, policy engine, local CLI, SDK, and interoperability layer. Sorrel Hub is the collaboration product built on top of it: reviews, proposals, issues, runners, policy administration, organizations, and marketplace.

Why these are good final names:

- "Sorrel" is short, memorable, and calmer than a hard technical acronym.
- The split keeps the framework independent from the hosted product.
- "Core" makes the protocol/runtime boundary clear.
- "Hub" makes the collaboration surface clear without forcing hosted compute into the initial product.
- The matching Core and Hub domains are available/acquired for the product split.
- The names leave room for sub-brands:
  - Sorrel Core
  - Sorrel Hub
  - Sorrel Runners
  - Sorrel Slices
  - Sorrel Vault

Potential CLI names:

```bash
sorrel
srl
sorrel-core
```

### Names checked and naming cautions

This was only a quick collision check, not a trademark clearance.

| Name | Reason to avoid |
| --- | --- |
| Sorrel Core / Sorrel Hub | Final selected names; still requires normal legal/trademark clearance. |
| ForgeCore / Forge Core | Liked, but the related `.com` domains were unavailable or crowded. |
| Forge | Too broad by itself and crowded in developer tooling. |
| Weave / GitWeave / ChangeWeave | Crowded around agent VCS, semantic merge, and Git submodule alternatives. |
| Loom | Used for Git worktree/multirepo tooling and older patch-stack tooling. |
| Patchlane | Existing npm CLI for managing custom patches on forks. |
| Lanes / Laneset | Lanes is already an AI-agent workspace product. |
| Cairn | Existing VCS and agent-code projects use the name. |
| Tendril | Existing AI coding orchestrators use the name. |
| Arc / Arclet | Arc is already a checkpoint-based VCS and Arcanist's CLI is `arc`. |
| Vetra / Vellora | Existing software companies/products use the names. |
| Ravelin | Existing fraud/risk company; not VCS, but a strong brand collision. |
| Mergent | Existing business data provider and AI code review assistant. |
| Meldra | No direct VCS hit found, but close to Meld, a well-known diff/merge tool. |

### Naming note

Sorrel Core and Sorrel Hub are the final selected names. The architecture keeps the same product split: Sorrel Core is the independent protocol/runtime, including headless identity, permissions, grants, policies, and audit semantics. Sorrel Hub is the collaboration layer built on top.

## Product thesis

Git was designed for human-driven source-code history. Modern development increasingly includes:

- many parallel AI agents
- cloud and ephemeral workspaces
- in-memory/browser execution
- generated code
- secrets and environment files
- permissions for humans, agents, runners, workflows, and marketplace tools
- large binaries and non-code assets
- hidden work in progress
- cross-repo and subproject sharing
- stack-based review
- local and remote execution options

Sorrel Core should treat these as primary design constraints, not integrations added after the fact.

## Competitive research summary

### Graphite

Strengths:

- Excellent stacked pull request workflow.
- Better review UX than vanilla GitHub.
- Stack-aware merge queue.
- AI-assisted code review.
- Easy adoption because it sits on GitHub/Git.

Weaknesses:

- Still fundamentally Git/GitHub-bound.
- Does not change the underlying version-control model.
- Collaboration state remains tied to a platform.

Lessons:

- Stacks must be first-class.
- Review should optimize for small dependent changes.
- Adoption improves when teams can keep GitHub/Git during migration.

### Jujutsu / jj

Strengths:

- Git-compatible.
- Great local UX.
- Automatic snapshots.
- Operation log and universal undo.
- Conflicts can be stored as commit states.

Weaknesses:

- Focuses more on local workflow than the full collaboration platform.
- Git backend still shapes many constraints.

Lessons:

- Git compatibility is essential.
- Operation history and undo should be core.
- Branches should become less central than changes.

### Sapling

Strengths:

- Built for huge monorepos.
- Great stacked-change workflow.
- Strong usability focus.
- Proven ideas from Meta-scale development.

Weaknesses:

- Most powerful features depend on server and virtual filesystem pieces.
- Some assumptions are corporate monorepo oriented.

Lessons:

- Separate UX from storage format.
- Huge-repo support requires server intelligence and virtualized working copies.

### Pijul

Strengths:

- Patch-theory model.
- Independent changes can commute.
- Conflicts are first-class, not failed merges.
- Conflict resolutions become reusable changes.

Weaknesses:

- Smaller ecosystem.
- Different mental model.
- Less mature tooling and adoption than Git.

Lessons:

- Changes should be reusable, partly commutative operations.
- Conflict resolution should be versioned and reusable.

### Radicle

Strengths:

- Local-first and peer-to-peer.
- Issues, patches, and discussions are versioned collaborative objects.
- Cryptographic identity.
- Less platform lock-in.

Weaknesses:

- Smaller network.
- Less centralized convenience.
- Less mature CI/CD and enterprise workflow support than GitHub/GitLab.

Lessons:

- Collaboration objects should be portable.
- Hosted UI can exist, but canonical project data should not be trapped in SaaS tables only.

### Fossil

Strengths:

- Version control plus issues, wiki, forum, and web UI in one system.
- Self-contained.
- Offline-friendly.
- Coherent links between code and project metadata.

Weaknesses:

- Niche ecosystem.
- Less aligned with modern GitHub-style contribution workflows.
- Not designed for cloud-agent workflows.

Lessons:

- Issues, reviews, docs, and code metadata should be first-class project data.
- The system still needs to interoperate with existing ecosystems.

### Perforce, Plastic SCM, and Unity Version Control

Strengths:

- Large binary support.
- File locking.
- Game and media workflows.
- Granular path permissions.

Weaknesses:

- Often centralized.
- Operational cost and complexity.
- Less friendly to distributed stacked code workflows.

Lessons:

- Native large-file support matters.
- Soft and hard locks remain necessary for non-mergeable assets.
- Path-level ACLs are table stakes for enterprise adoption.

### isomorphic-git and LightningFS

Strengths:

- Prove Git-like operations can run in JS/browser contexts.
- Support pluggable filesystems.
- Enable in-memory or IndexedDB-backed operation.

Weaknesses:

- Git still expects filesystem-shaped behavior.
- Browser persistence and crash consistency are hard.

Lessons:

- Sorrel Core should have storage adapters from day one:
  - native filesystem
  - in-memory
  - IndexedDB / OPFS
  - SQLite
  - object storage
  - remote content-addressed storage

### CRDT systems: Yjs, Automerge, Loro

Strengths:

- Real-time multi-user editing.
- Offline-first convergence.
- Good fit for collaborative metadata and live documents.

Weaknesses:

- CRDTs alone are not a full VCS.
- Access control, persistence, compaction, and schema migration are difficult.

Lessons:

- Use CRDTs for comments, task boards, live editor sessions, and mutable collaboration metadata.
- Do not make the whole VCS "just a CRDT."

## Core architecture

Sorrel Core should have a Rust core with CLI, server, Node/WASM bindings, and a TypeScript SDK.

### Layer 1: Storage engine

The core must not require a normal filesystem.

Storage interfaces:

```ts
interface ObjectStore {
  read(id: ObjectId): Promise<Bytes>
  write(bytes: Bytes): Promise<ObjectId>
  has(id: ObjectId): Promise<boolean>
}

interface Workspace {
  readFile(path: Path): Promise<Bytes>
  writeFile(path: Path, bytes: Bytes): Promise<void>
  delete(path: Path): Promise<void>
  list(path: Path): Promise<FileEntry[]>
}
```

Backends:

- Filesystem backend with `.sorrel/` beside the working tree.
- Bare backend for servers, CI, bots, and merge queues.
- In-memory backend for JS agents, tests, browser sandboxes, and ephemeral previews.
- Browser backend using IndexedDB or OPFS.
- Cloud object backend using S3, R2, GCS, Azure Blob, or equivalent.
- SQLite backend for a single-file local repository with atomic local metadata.

### Layer 2: Object model

Core object types:

```text
Blob        raw bytes or chunk references
Tree        path -> file, symlink, or subtree entries
Change      semantic patch/change object
Snapshot    materialized tree state
Stack       ordered or dependency-linked set of changes
Lane        isolated workstream for a human or agent
Slice       shareable extracted subproject
Ref         named pointer/bookmark
Workspace   mutable view/lane
Conflict    first-class unresolved merge state
Resolution  reusable conflict-resolution change
Review      portable review metadata
Issue       portable issue metadata
Principal   user, agent, service, team, org, runner, or app identity reference
Authority   trust root or delegated policy authority
Grant       scoped permission assignment or delegation
SecretRef   reference to external/encrypted secret
Policy      portable authorization, visibility, workflow, and redaction rules
PolicyChange signed proposal/transaction that changes grants, policy, or authority
Decision    auditable policy evaluation result
AgentNote   instruction/policy overlay for agents
Workflow    portable job DAG
Runner      execution target with capabilities
AuditEvent  append-only security/workflow event
```

Use BLAKE3 or SHA-256 for native object IDs. Preserve Git SHA compatibility through mapping tables.

### Layer 2.5: Core identity, permissions, and policy spine

Permissions are not a Sorrel Hub add-on. Sorrel Core must define the portable identity, capability, grant, policy, decision, and audit objects that every UI, CLI, runner, vault backend, and hosted product consumes. A repository should be usable headlessly through the CLI or SDK while still enforcing the same access model that Hub would show in a web UI.

Core policy requirements:

- Work without Sorrel Hub, a hosted database, or production auth.
- Treat humans, agents, services, runners, workflow jobs, teams, organizations, and marketplace apps as principals.
- Authorize object reads before returning private/redacted objects, not only before rendering them.
- Express path, symbol, change, lane, stack, slice, proposal, workflow, runner, environment, and secret scopes.
- Store grant and policy objects as portable Sorrel data where possible, with stable export/import semantics.
- Keep raw secret values outside the object graph; store only `SecretRef`, schema, grant, redaction, and audit metadata.
- Produce deterministic policy decisions that can be tested in memory and replayed for audit.
- Treat policy mutation as a privileged action governed by the previous effective policy, not by the proposed new policy.
- Reject self-grants, self-escalation, and unsigned authority changes unless an already-authorized authority delegated that power.
- Allow Hub to add authentication, UI, administration, and team workflows without changing Core semantics.

Canonical policy evaluation shape:

```json
{
  "principal": "agent:agent_17",
  "action": "path.write",
  "resource": {
    "scope": "path",
    "repo": "repo_api",
    "path": "packages/auth/src/session.ts"
  },
  "context": {
    "lane": "lane_agent_17_fix_tests",
    "workflowRun": null,
    "requestedAt": "2026-06-24T00:00:00Z"
  },
  "decision": "allow",
  "matchedPolicy": "pol_auth_agent_edits",
  "auditEvent": "audit_123"
}
```

Minimal decisions:

```text
allow
deny
redact
needs_grant
needs_review
```

This spine should land before deeper lanes/stacks, workflow execution, vault integration, and Hub expansion so those modules share one authorization vocabulary.

### Layer 2.6: Decentralized authority and self-escalation prevention

A decentralized Sorrel repository cannot rely on a central server to prevent someone from editing a local file. It must instead make permission changes verifiable. Anyone can mutate bytes in their own clone, but other peers, runners, Hub, CI, and upstream repositories must reject policy state that is not authorized by the existing authority graph.

Core invariants:

- Policy, grant, and authority updates are signed `PolicyChange` objects, not ordinary untrusted file edits.
- A `PolicyChange` is evaluated against the effective policy before that change is applied.
- A principal cannot grant itself new capabilities unless it already has a delegated capability such as `policy.grant` or `authority.admin` for that resource.
- A principal cannot broaden the scope of a delegated grant beyond the scope it received.
- Root authority rotation requires the current root authority rule, such as owner signature, maintainer threshold, or explicit recovery policy.
- Grants are monotonic only through valid authority chains; a forged or locally edited grant is visible as invalid/untrusted.
- Runners, vaults, Hub, and remotes verify policy signatures and decisions before accepting changes, injecting secrets, running privileged workflows, or displaying private objects.
- Forks may choose their own authority roots for their own namespace, but cannot impersonate upstream authority when proposing back.

Canonical authority root:

```json
{
  "id": "auth_repo_main",
  "scope": { "scope": "repo", "repo": "repo_api" },
  "authorities": [
    { "principal": "user:alice", "weight": 1 },
    { "principal": "user:bob", "weight": 1 },
    { "principal": "user:carol", "weight": 1 }
  ],
  "threshold": 2,
  "capabilities": ["policy.grant", "policy.revoke", "policy.update", "authority.rotate"],
  "createdAt": "2026-06-24T00:00:00Z",
  "signature": "sig_root..."
}
```

Example rejected self-escalation:

```json
{
  "type": "PolicyChange",
  "actor": "agent:agent_17",
  "operation": "grant",
  "grant": {
    "principal": "agent:agent_17",
    "capabilities": ["secret.inject", "policy.grant"],
    "resources": [{ "scope": "repo", "repo": "repo_api" }]
  },
  "decision": "deny",
  "reason": "actor lacks policy.grant on repo_api under the previous effective policy"
}
```

This is the difference between "decentralized" and "permissionless mutation." Sorrel can be local-first and peer-verifiable while still refusing unauthorized policy state.

### Layer 3: Change model

Git commits are snapshots. Sorrel Core should treat changes as primary.

Example:

```json
{
  "id": "chg_123",
  "author": "user_or_agent_id",
  "parents": ["chg_a", "chg_b"],
  "baseSnapshot": "snap_123",
  "operations": [],
  "dependencies": [],
  "touches": {
    "paths": [],
    "symbols": [],
    "secrets": [],
    "schemas": []
  },
  "visibility": "private",
  "createdAt": "2026-06-24T00:00:00Z",
  "signedBy": "identity_key"
}
```

Important design choice:

- Store snapshots for fast checkout, diff, and export.
- Store operations/patches for merge, review, stack manipulation, and AI-agent workflows.

## Git migration and compatibility

Adoption requires a bridge.

### Git-colocated mode

```bash
sorrel init --git-colocated
```

Creates:

```text
.git/
.sorrel/
```

Users can still use Git. Sorrel Core imports Git commits and exports Sorrel Core changes as normal Git commits.

### Native mode

```bash
sorrel clone sorrel://org/project
```

Uses the native object model.

### Git remote bridge

```bash
git remote add sorrel sorrel://org/project
git push sorrel main
```

or:

```bash
sorrel git export --to origin/main
```

### One-way mirror

For early adoption:

- team works in Sorrel Core
- Sorrel Core exports to GitHub or GitLab
- existing CI and deployments continue

### Round-trip exit

Teams must be able to leave:

```bash
sorrel export --git ./repo.git
```

No lock-in should be a product principle.

## Collaboration platform

Sorrel Hub is the GitHub-like collaboration product built on top of Sorrel Core's portable collaboration, identity, policy, and audit objects. Hub can provide auth, administration UI, hosted storage, notifications, and team workflows, but it must not define a separate permission model that Core cannot run headlessly.

Core features:

- organizations
- projects
- repositories
- lanes
- stacks
- proposals
- reviews
- issues
- discussions
- docs/wiki
- CI/workflows
- merge queue
- agent jobs
- policy and grant administration
- secret/environment administration backed by Core `SecretRef` and grant semantics
- environments
- marketplace

Canonical project data should be exportable and syncable, not trapped in a hosted database only. Hub-only metadata should be an optimization or product feature, not the source of truth for whether a principal can read, write, run, inject, review, merge, or redact Sorrel objects.

### Proposal model

Replace PRs as the only review concept with proposals:

```text
Proposal
  contains one or more Changes
  may be stacked
  may be private, team-visible, review-visible, or public
  has review policy
  has CI policy
  has merge policy
```

Support:

- stacked proposals
- draft/private proposals
- dependent proposals
- partial review
- reviewer-specific state
- review comments anchored to path, object IDs, semantic symbol, and stable span hashes

### Merge queue

Rules:

- verify exact object IDs, not branch names
- test exact final snapshot
- support stack-aware landing
- auto-rebase descendants
- preserve conflict-resolution changes
- require policy approvals
- block if required secrets or environments are unavailable
- record provenance

## Agent-first interface

Agents should not scrape human terminal output. Provide three stable interfaces.

### CLI

```bash
sorrel lane create agent/refactor-auth
sorrel slice create --entry packages/auth/src/index.ts --name auth-lib
sorrel workflow run test
sorrel workflow run test --runner ssh-buildbox
```

### JSON API

```http
POST /repos/:repo/lanes
POST /repos/:repo/slices
POST /repos/:repo/workflows/:workflow/runs
```

### SDK

```ts
const lane = await sorrel.lanes.create("agent/refactor-auth")

const slice = await sorrel.slices.create({
  entrypoint: "packages/auth/src/index.ts",
  name: "auth-lib",
  carryPermissions: true
})
```

Every operation should return structured results:

```json
{
  "ok": true,
  "createdChange": "chg_123",
  "touchedSymbols": ["createSession", "validateToken"],
  "conflicts": [],
  "nextActions": ["run:test", "submit:proposal"]
}
```

## Parallel human and AI workflow

### Lanes

Every human or agent gets a lane.

```text
main
 |-- alice/login-ui
 |-- agent-17/fix-tests
 |-- agent-22/refactor-api
 `-- agent-35/update-docs
```

A lane is not just a branch. It includes:

- workspace overlay
- active changes
- task metadata
- claimed files and symbols
- secret grants
- environment definition
- agent instructions
- review visibility
- merge policy

### Same-file parallel work

Use a multi-level merge strategy:

1. Text operation merge with stable anchors.
2. Semantic merge using tree-sitter/LSP symbols.
3. Dependency-aware merge for API, test, config, and schema impact.
4. First-class conflict objects.
5. Reusable conflict-resolution changes.
6. Advisory soft locks for code and hard locks for configured binary assets.

The system should warn:

```text
agent-17 and agent-22 are both editing auth.ts::createSession
```

But it should only block when policy requires it.

## Shareable slices

Slices are better submodules.

The user should be able to say:

```bash
sorrel slice create \
  --entry packages/auth/src/index.ts \
  --name auth-lib \
  --visibility team
```

The system should:

1. Detect the dependency closure from the entrypoint.
2. Create a new repository.
3. Preserve relevant history/change graph.
4. Carry over applicable permissions.
5. Copy secret schemas, not secret values.
6. Link the new repo back to the original location.
7. Leave a pointer in the old repo.
8. Allow bidirectional sync later.

### Slice object

```json
{
  "type": "Slice",
  "id": "slice_auth_lib",
  "sourceRepo": "repo_main",
  "sourcePath": "packages/auth",
  "entrypoints": ["packages/auth/src/index.ts"],
  "targetRepo": "repo_auth_lib",
  "sourceChange": "chg_123",
  "permissionsPolicy": "projected",
  "linkMode": "live",
  "createdBy": "alice"
}
```

### Link modes

#### Live link

```yaml
dependencies:
  auth-lib:
    repo: sorrel://org/auth-lib
    ref: main
    mode: live
```

Best for active shared components.

#### Pinned link

```yaml
dependencies:
  auth-lib:
    repo: sorrel://org/auth-lib
    change: chg_abc123
    mode: pinned
```

Best for reproducible production dependencies.

#### Vendored link

```yaml
dependencies:
  auth-lib:
    repo: sorrel://org/auth-lib
    change: chg_abc123
    mode: vendored
```

Best when consumers should not need remote access after import.

#### Virtual link

The parent repo shows the slice as a normal directory, but storage, review, and history know it is linked.

Best long-term UX, but requires native Sorrel Core tooling.

### Permission carryover

Rule:

```text
new repo permissions = source permissions intersected with explicit share target
```

Example:

```bash
sorrel slice create \
  --entry packages/billing \
  --name billing-sdk \
  --share team:payments
```

If `packages/billing` was visible only to `payments` and `platform-admins`, then the new repo cannot accidentally become org-public.

Carry over:

- path permissions
- reviewer rules
- proposal rules
- agent permissions
- secret references
- workflow policies
- audit history
- ownership metadata

Do not carry over raw secrets. Carry over secret declarations only.

### Backlinks

In the new repo:

```yaml
origin:
  sourceRepo: sorrel://org/app
  sourcePath: packages/auth
  sourceChange: chg_123
```

In the old repo:

```yaml
links:
  packages/auth:
    repo: sorrel://org/auth-lib
    mode: live
    createdFrom: chg_123
```

Sync commands:

```bash
sorrel slice sync auth-lib --from-parent
sorrel slice sync auth-lib --to-parent
sorrel slice history auth-lib
```

### Entrypoint dependency closure

For JS/TS, the slice analyzer should:

- parse imports
- follow `package.json`
- follow `tsconfig`
- include relevant tests
- include local workspace dependencies
- include needed assets
- include secret schemas
- include workflow fragments

For other languages, use analyzers:

```yaml
analyzers:
  - typescript
  - rust
  - go
  - python
  - java
  - docker
  - terraform
```

Allow overrides:

```bash
sorrel slice create \
  --entry packages/auth/src/index.ts \
  --include packages/auth/tests \
  --exclude "**/*.snap"
```

## Permissions model

Git has weak permission granularity. Sorrel Core should support object-level policy as a core runtime feature, not a Hub-only product feature. Every module that reads, writes, runs, injects secrets, exposes review state, or launches agents should be able to ask the same Core policy engine for a decision.

Headless invariant:

```text
same repository + same principal + same grant/policy objects + same action = same decision
```

That invariant must hold from the CLI, SDK, local runner, test harness, Hub server, or future hosted product.

Principals:

```text
user
agent
service
team
org
runner
workflow
marketplace app
anonymous/public viewer
authority
```

Scopes:

- org
- project
- repo
- path
- file
- symbol
- change
- stack
- proposal
- comment thread
- secret
- environment
- agent
- runner
- workflow
- marketplace app
- authority

Capabilities:

```text
repo.read
repo.write
path.read
path.write
symbol.read
symbol.write
change.create
change.read
change.read_private
change.update_visibility
lane.create
lane.read
lane.write
stack.create
stack.submit
slice.create
slice.export
proposal.view
proposal.review
proposal.merge
secret.read
secret.inject
environment.read
environment.use
agent.launch
agent.read_logs
agent.modify_instructions
runner.use
workflow.run
marketplace.install
policy.read
policy.grant
policy.revoke
policy.update
policy.delegate
authority.admin
authority.rotate
```

Grant object:

```json
{
  "id": "grant_123",
  "principal": "agent:agent_17",
  "capabilities": ["path.write", "workflow.run"],
  "resources": [
    { "scope": "path", "path": "packages/auth/**" },
    { "scope": "workflow", "name": "test" }
  ],
  "constraints": {
    "lane": "lane_agent_17_fix_tests",
    "expiresAt": "2026-06-24T04:00:00Z",
    "requiresReviewFor": ["proposal.merge"]
  },
  "issuedBy": "user:alice",
  "createdAt": "2026-06-24T00:00:00Z",
  "signature": "sig_..."
}
```

Policy changes:

```text
PolicyChange
  actor: Principal
  operation: grant | revoke | update_policy | delegate | rotate_authority
  previousPolicyRoot: ObjectId
  proposedPolicyRoot: ObjectId
  signatures: one or more authority signatures
  evaluatedAgainst: previous effective policy
  decision: allow | deny | needs_review
```

Security-critical rule:

```text
Never evaluate a permission change using the permissions created by that same change.
```

The evaluator must first ask whether the actor already has authority to make the change under the previous effective policy. This prevents an agent, fork, local clone, or compromised tool from editing its own policy object and having that edit accepted by peers.

Policy decision object:

```json
{
  "id": "decision_123",
  "principal": "agent:agent_17",
  "action": "secret.inject",
  "resource": "secret://project/api/dev/DATABASE_URL",
  "decision": "needs_grant",
  "reason": "No matching grant for secret injection in this lane",
  "matchedPolicies": [],
  "auditEvent": "audit_456"
}
```

Visibility states:

```text
private       only creator or assigned agents
team          visible to selected team
review        visible to reviewers
public        visible to everyone with repo access
redacted      metadata visible, diff hidden
```

Private work should be enforced by object-level authorization and, where needed, encryption rather than UI hiding only. Redacted objects should remain useful for coordination by exposing safe metadata, such as IDs, dependencies, touched paths, or required review state, while hiding protected content.

## Secret management

Secrets must be first-class, but raw secret values must not become Sorrel objects. Sorrel Core owns the portable `SecretRef`, environment schema, grant, redaction, and audit semantics. `sorrel-vault` and external vaults resolve values only after Core policy allows the requesting principal, workflow, runner, and environment context.

Never commit raw secrets. Repos should contain:

```text
.env.schema
.env.example
sorrel.secrets.yml
```

Example:

```yaml
environments:
  dev:
    DATABASE_URL:
      type: url
      required: true
      source: secret://project/api/dev/DATABASE_URL
    STRIPE_KEY:
      type: secret
      required: true
      source: secret://org/payments/dev/STRIPE_KEY
```

Actual values can live in:

- Sorrel Core Vault
- HashiCorp Vault
- Infisical
- Doppler
- AWS/GCP/Azure secret stores
- SOPS-encrypted files for lightweight self-hosted use

Features:

- env-file import
- secret scanning
- rotation history
- access audit log
- short-lived credentials
- OIDC federation
- per-agent grants
- per-workflow grants
- per-runner grants
- policy-engine decisions before injection
- redaction in logs
- preview environment secrets
- secret diffs without values

Commands:

```bash
sorrel secrets import .env --env dev
sorrel run --env dev npm test
sorrel agent start --task fix-tests --secrets test-only
```

For agents, prefer secret handles over raw values. Agent prompts, lanes, proposals, workflow bundles, and logs should carry `SecretRef` handles and redaction metadata, never the value itself.

## Agent instruction system

Instruction layers:

```text
Org global instructions
  -> Project instructions
    -> Repo instructions
      -> Path-specific instructions
        -> Task instructions
          -> Agent-specific overlay
```

Materialized locally as:

```text
AGENTS.md
AGENTS.local.md
.sorrel/agent-policy.lock
```

Canonical policy object:

```json
{
  "type": "AgentPolicy",
  "scope": "repo:/frontend",
  "rules": [
    "Run pnpm test before proposing changes",
    "Do not edit generated files",
    "Use design tokens from packages/ui"
  ],
  "patches": [
    "agent-specific overrides"
  ],
  "approvedBy": ["team-lead"]
}
```

This creates one control plane for:

- what agents may read
- what agents may edit
- which files or symbols they claim
- which secrets they can use
- which commands they can run
- which instructions they follow
- which changes they produced

## Workflow and runner system

Sorrel Core should not offer hosted compute at the beginning. Instead, build a portable execution protocol.

The platform stores:

- workflow definitions
- job DAGs
- caches
- logs
- artifacts
- permissions
- runner registrations

Users provide compute.

### Workflow file

```yaml
version: 1

workflows:
  test:
    jobs:
      unit:
        command: pnpm test
        inputs:
          - src/**
          - package.json
          - pnpm-lock.yaml
        platform:
          runtime: node
          os: any
          arch: any

      e2e:
        command: pnpm test:e2e
        needs: [unit]
        platform:
          runtime: container
          image: mcr.microsoft.com/playwright:v1
```

Run locally:

```bash
sorrel workflow run test
```

Run elsewhere:

```bash
sorrel workflow run test --runner k8s-prod
sorrel workflow run test --runner ssh-mac-mini
sorrel workflow run test --runner local-docker
```

Same workflow, different execution target.

### Runner types

#### Local process runner

```bash
sorrel runner local
```

Pros:

- simple
- fast startup
- good for development

Cons:

- weak isolation

#### Local container runner

Docker or Podman.

```bash
sorrel runner docker
```

Pros:

- reproducible
- good isolation
- works for most CI-like tasks

Cons:

- Linux-container assumptions on macOS/Windows

#### SSH runner

```bash
sorrel runner register ssh://buildbox.internal
```

Pros:

- easy remote execution
- works with existing machines

Cons:

- requires hardening

#### Kubernetes runner

```bash
sorrel runner register k8s://cluster/prod-builds
```

Pros:

- autoscaling
- isolation
- good for teams with infrastructure

Cons:

- more operational complexity

#### GitHub Actions / GitLab CI adapter

```bash
sorrel workflow export --target github-actions
sorrel workflow export --target gitlab-ci
```

Pros:

- adoption-friendly
- uses existing infrastructure

Cons:

- some platform lock-in remains

#### Buildkite-style hybrid runner

Control plane schedules. Customer-owned agents pull jobs.

Pros:

- good for enterprises
- data stays on user infrastructure
- good for special hardware

Cons:

- requires runner management

#### Bazel REAPI-compatible runner

Use a remote execution API style:

- content-addressed storage
- action cache
- worker capabilities
- platform properties

Pros:

- very scalable
- strong caching
- proven model

Cons:

- more complex than simple CI

#### WASM runner

Best for marketplace plugins, analyzers, formatters, and lightweight tasks.

Pros:

- portable
- secure
- good sandboxing

Cons:

- not suitable for arbitrary builds

#### Browser runner

Best for:

- docs preview
- lint subset
- formatting
- dependency graph
- simple tests

Pros:

- zero server
- good for demos and lightweight agents

Cons:

- limited compute and filesystem

### Specialized platform runners

Capability labels should go beyond OS:

```yaml
platform:
  capabilities:
    - gpu:nvidia
    - cuda:12
    - ios-simulator
    - android-emulator
    - windows-signing
    - macos-codesign
    - arm64
    - wasm
    - browser
    - fpga
```

Workflows should target capabilities, not hardcoded machines.

### Runner registration

```bash
sorrel runner register \
  --name mac-mini-1 \
  --labels macos,arm64,ios-simulator,codesign \
  --mode pull
```

Runner descriptor:

```json
{
  "id": "runner_mac_mini_1",
  "mode": "local | pull | push",
  "platform": {
    "os": "macos",
    "arch": "arm64",
    "runtimes": ["shell", "xcode", "node"],
    "capabilities": ["ios-simulator", "codesign"]
  },
  "isolation": "host",
  "maxParallelJobs": 2
}
```

### Local-to-remote reroute

A workflow run should be portable as a job bundle:

```text
JobBundle
  inputs: content-addressed files
  command: exact command
  env: secret refs, not values
  platform requirements
  cache keys
  expected outputs
```

Reroute:

```bash
sorrel workflow run test --local
sorrel workflow rerun run_123 --runner k8s
sorrel workflow rerun run_123 --runner ssh-gpu-box
```

Same inputs. Same command. Different runner.

### Agent workflow integration

Agents can ask:

```bash
sorrel workflow suggest
```

Response:

```json
{
  "recommended": [
    {
      "workflow": "unit",
      "reason": "You changed packages/auth/src/session.ts"
    },
    {
      "workflow": "lint",
      "reason": "TypeScript files changed"
    }
  ]
}
```

Agents can run:

```bash
sorrel workflow run unit --lane agent/refactor-auth
```

The run is attached to the agent lane, change, and proposal.

### Workflow permissions

Workflow permissions are Core policy inputs. Runners should not decide privilege from local config alone; they should receive a `JobBundle`, principal context, required capabilities, and secret/environment refs, then ask Core policy whether the run, runner use, and secret injection are allowed.

```yaml
permissions:
  workflows:
    deploy-prod:
      run: [team:release]
    test:
      run: [repo:write]

  runners:
    mac-codesign:
      use: [team:ios]
    gpu-large:
      use: [team:ml]

  secrets:
    APPLE_CERT:
      inject:
        - workflow: ios-release
          runner: mac-codesign
```

This prevents random agents from running privileged jobs.

## Marketplace

Build a marketplace later, but keep it independent from the core framework.

Sell or install:

- CI steps
- code review bots
- AI agents
- merge strategies
- language analyzers
- tree-sitter packs
- secret backends
- deployment integrations
- issue tracker integrations
- cloud workspace templates
- compliance packs
- repository templates
- visual diff tools
- binary asset tools
- game development plugins
- data-versioning plugins
- workflow runners

Marketplace apps should declare permissions:

```yaml
name: semantic-merge-ts
permissions:
  - repo.read
  - change.read
  - change.suggest
runtime:
  sandbox: wasm
billing:
  type: usage
```

Use signed packages and sandboxed execution. Marketplace apps should mutate core state only through auditable capabilities.

## Modern filesystem support

Sorrel Core must support:

- Linux
- macOS
- Windows
- case-sensitive and case-insensitive filesystems
- Unicode normalization differences
- symlinks
- executable bits
- file modes
- large files
- sparse checkouts
- virtual filesystems
- file watchers
- atomic writes
- long paths on Windows
- generated-file markers

Checkout profile:

```yaml
checkout:
  platform: auto
  casePolicy: forbid-conflicts
  unicodePolicy: normalize-nfc
  symlinks: materialize-or-copy
  largeFiles: lazy
```

## Large files and binaries

Native support should include:

- chunked blob storage
- deduplication
- resumable upload/download
- lazy fetch
- file locking
- media previews
- binary diff plugins
- path-based retention policies
- CDN-backed artifact delivery

Binary lock policy:

```yaml
locks:
  required:
    - "*.psd"
    - "*.blend"
    - "*.uasset"
```

## Computing and workspace alternatives

### Local filesystem

Pros:

- familiar
- fast
- works with existing tools

Cons:

- harder to isolate agents
- platform differences
- secret leakage risk

Use for human development.

### In-memory workspace

Pros:

- very fast
- great for agents and tests
- easy cleanup
- works in JS and browser runtimes

Cons:

- volatile
- large repos need lazy loading
- some tools expect real paths

Use for AI agents, previews, browser demos, and test sandboxes.

### Container workspace

Pros:

- reproducible
- good isolation
- works with CI

Cons:

- filesystem overhead
- startup time
- careful secret handling required

Use for standard cloud and local agent execution.

### VM or microVM workspace

Pros:

- stronger isolation
- better for untrusted agents
- full OS compatibility

Cons:

- more expensive
- slower startup

Use for enterprise or security-heavy agent execution.

### WASM sandbox

Pros:

- excellent plugin isolation
- portable
- good for marketplace apps

Cons:

- not suitable for every build tool
- filesystem and network APIs need capability design

Use for plugins, analyzers, and merge tools.

### Browser / OPFS / IndexedDB

Pros:

- no install
- good demos and lightweight editing
- can support offline-first UX

Cons:

- persistence complexity
- browser storage limits
- native toolchains unavailable unless paired with remote compute

Use for web editor and lightweight collaboration.

### Cloud object store

Pros:

- scales
- cheap for blobs
- good for distributed agents

Cons:

- needs metadata database or index
- latency if not cached

Use for hosted storage and user-owned storage backends.

## MVP plan

MVP priority correction: identity, permissions, grants, policy decisions, secret/environment references, redaction metadata, and audit events are part of the Core foundation. Later phases can deepen UX and integrations, but they must not be the first place these concepts appear.

### Phase 1: Core engine and permission spine

Build:

- Rust object store.
- Filesystem backend.
- In-memory backend.
- Snapshot objects.
- Change objects.
- Minimal Principal, Grant, Policy, Decision, SecretRef, and AuditEvent objects.
- Deterministic in-memory policy evaluator.
- Object read/write authorization hooks for private/redacted metadata.
- Basic diff/apply.
- Operation log and undo.
- First-class conflict objects.

Commands:

```bash
sorrel init
sorrel status
sorrel change create
sorrel change list
sorrel policy evaluate --principal agent:demo --action path.write --path src/lib.rs
sorrel grant create --principal agent:demo --capability path.write --path src/**
sorrel switch
sorrel diff
sorrel undo
```

Success criteria:

- Tracks files without Git.
- Runs entirely in memory.
- Evaluates permissions without Sorrel Hub.
- Represents private/redacted object visibility through Core decisions.
- Supports undo.
- Represents conflict objects.

### Phase 1b: Adapt existing module foundations to the permission spine

Build:

- Protocol schemas for principals, capabilities, resource scopes, grants, policies, decisions, audit events, `SecretRef`, and redaction markers.
- Core model types and serialization for the same vocabulary.
- CLI commands that expose headless policy inspection/evaluation before Hub exists.
- Vault and runner adapters that use Core grants/decisions rather than local-only authorization shapes.
- Hub model updates so organizations, projects, repositories, proposals, and workflow runs reference Core principals/policies instead of inventing product-only permissions.

Success criteria:

- Existing protocol/core/CLI/vault/runner/slice/Hub skeletons can compile/test with the shared permission vocabulary.
- A local test can ask whether an agent may write a path, run a workflow, and inject a secret without starting Hub.
- Existing secret grants and runner capabilities map onto Core grant/policy objects.

### Phase 2: Git bridge

Build:

```bash
sorrel init --git-colocated
sorrel git import
sorrel git export
sorrel git sync
```

Success criteria:

- Existing Git repo can be used.
- Sorrel Core changes export as normal Git commits.
- Team can leave without data loss.

### Phase 3: Lanes and stacks

Build:

```bash
sorrel lane create agent-17/fix-tests
sorrel stack create
sorrel stack submit
sorrel lane status
```

Success criteria:

- Multiple lanes can edit the same base.
- Stacks are first-class.
- Lanes and stacks carry owner principal, visibility, policy refs, grant refs, touched resources, and audit hooks.
- Rebase/merge preserves reviewable changes.

### Phase 4: Slices

Build:

```bash
sorrel slice create --entry packages/auth/src/index.ts --name auth-lib
sorrel slice inspect auth-lib
sorrel slice sync auth-lib --from-parent
sorrel slice sync auth-lib --to-parent
```

Success criteria:

- Entrypoint extraction works.
- New repo carries permissions safely.
- Parent and child stay linked.
- Git export remains possible.

### Phase 5: Workflows and runners

Build:

```bash
sorrel workflow run test --local
sorrel workflow rerun run_123 --runner ssh-buildbox
sorrel runner register ssh://buildbox.internal
```

Success criteria:

- Same workflow runs locally and remotely.
- Job inputs are content-addressed.
- Secret refs are resolved only by authorized runners.
- Runs attach to lanes, changes, proposals, and agents.

### Phase 6: Sorrel Hub server

Build:

- auth
- org/project/repo model
- proposal/review UI
- stack-aware merge queue
- comments
- workflow hooks
- audit log

Success criteria:

- GitHub-like collaboration works.
- Private draft proposals work.
- Review comments survive rebases.

### Phase 7: Secrets and environments UX/adapters

The Core `SecretRef`, grant, policy decision, redaction, and audit model exists earlier. This phase adds richer CLI ergonomics, external backend adapters, and operational workflows.

Build:

```bash
sorrel secrets import .env
sorrel secrets grant agent-17 --env dev
sorrel run --env dev npm test
```

Success criteria:

- No raw secrets in repo.
- Agents get scoped secrets.
- Logs are redacted.
- Env schemas are versioned.

### Phase 8: Agent control plane

Build:

- agent lanes
- task assignment
- file/symbol claims
- policy overlays
- global/repo/path AGENTS rules
- active work dashboard

Success criteria:

- Multiple agents can work safely.
- Maintainers can see who touches what.
- Agents receive only allowed instructions and secrets.

### Phase 9: Marketplace

Build:

- signed app packages
- permission manifests
- billing hooks
- install/uninstall
- sandbox runtime
- SDK

Success criteria:

- Third parties can build integrations without coupling to hosted product internals.

## First demo target

The first convincing demo should show:

1. Import an existing Git repo.
2. Create three agent lanes.
3. Two agents edit the same file.
4. Sorrel Core detects overlapping symbols.
5. One change merges cleanly.
6. One produces a first-class conflict object.
7. Conflict resolution is saved and reused.
8. Export final result back to Git.
9. Extract `packages/auth/src/index.ts` as a shareable slice.
10. Carry permissions and link the new repo back to the old path.
11. Run the same workflow locally and on an SSH runner.
12. Repeat one workflow against an in-memory workspace.

If that demo works, the foundation is strong enough to justify the larger platform.

## Key risks

1. Merge correctness.
   - Do not promise "no conflicts."
   - Promise better conflict representation, reuse, and coordination.

2. Git compatibility.
   - Mandatory for adoption.
   - Scope carefully at first.

3. Performance.
   - Requires content-addressed storage, packfiles/chunking, indexes, lazy loading, and caching.

4. Permissions and encryption.
   - Hiding WIP securely is harder than hiding UI rows.
   - Needs object-level authorization and possibly encryption.

5. Secrets in agent workflows.
   - Biggest security risk.
   - Must be designed before agent execution becomes powerful.

6. Marketplace security.
   - Plugins need capability sandboxing from day one.

7. Runner trust.
   - User-owned runners need identity, attestation, isolation levels, and secret policies.

## Final recommendation

Build Sorrel Core in this order:

1. Core object store, changes, snapshots, operation log, and in-memory backend.
2. Git bridge.
3. Lanes and stacks.
4. Slices from entrypoints with permission projection.
5. Portable workflows and user-owned runners.
6. Sorrel Hub collaboration server.
7. Secrets, agent policy, and marketplace.

This keeps the hardest technical bets close to the core while delaying hosted compute and marketplace complexity until the protocol is proven.
