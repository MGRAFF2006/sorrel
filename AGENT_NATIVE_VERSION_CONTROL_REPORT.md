# Forge Core and Forge Hub: Agent-Native Version Control and Collaboration Report

## Executive summary

Forge Core and Forge Hub are the recommended working names for a new version-control and collaboration system built for modern software work: humans, parallel AI agents, cloud workspaces, local-first development, secrets, portable workflows, and selective sharing of unfinished work.

The core idea is not to build "Git but nicer." The better architecture is a layered system:

1. A new VCS core based on content-addressed objects, changes, lanes, slices, and first-class conflicts.
2. A Git bridge so teams can import, export, mirror, and leave without lock-in.
3. A collaboration forge with reviews, stacks, proposals, issues, CI, policies, and agent coordination.
4. A secret and environment layer built into the workflow.
5. A portable workflow and runner protocol that supports local and user-owned remote compute.
6. An optional marketplace for integrations, workflow modules, analyzers, runners, and agent tools.

The framework and protocol should be independent. Hosted products, marketplaces, and UI layers should sit on top of the open core rather than define it.

## Naming recommendation

### Recommended working names: Forge Core and Forge Hub

Forge Core is the protocol, storage engine, local CLI, SDK, and interoperability layer. Forge Hub is the collaboration product built on top of it: reviews, proposals, issues, runners, secrets, policies, organizations, and marketplace.

Why these are good working names:

- "Forge" fits code collaboration, creation, and shared engineering work.
- The split keeps the framework independent from the hosted product.
- "Core" makes the protocol/runtime boundary clear.
- "Hub" makes the collaboration surface clear without forcing hosted compute into the initial product.
- The names leave room for sub-brands:
  - Forge Core
  - Forge Hub
  - Forge Runners
  - Forge Slices
  - Forge Vault

Potential CLI names:

```bash
forge
fg
forge-core
```

### Names checked and naming cautions

This was only a quick collision check, not a trademark clearance.

| Name | Reason to avoid |
| --- | --- |
| ForgeCore / Forge Core | Preferred despite some unrelated uses; "forge" is crowded in developer tooling, so legal/domain checks are still needed. |
| Forge | Too broad by itself; use Forge Core and Forge Hub as the specific names. |
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

Forge Core and Forge Hub are stronger than the more artificial coined-name alternatives because they communicate the product shape immediately. The naming risk is crowding, not meaning. If legal and domain checks are acceptable, they should remain the primary names.

## Product thesis

Git was designed for human-driven source-code history. Modern development increasingly includes:

- many parallel AI agents
- cloud and ephemeral workspaces
- in-memory/browser execution
- generated code
- secrets and environment files
- large binaries and non-code assets
- hidden work in progress
- cross-repo and subproject sharing
- stack-based review
- local and remote execution options

Forge Core should treat these as primary design constraints, not integrations added after the fact.

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

- Forge Core should have storage adapters from day one:
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

Forge Core should have a Rust core with CLI, server, Node/WASM bindings, and a TypeScript SDK.

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

- Filesystem backend with `.forge/` beside the working tree.
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
SecretRef   reference to external/encrypted secret
Policy      permissions and workflow rules
AgentNote   instruction/policy overlay for agents
Workflow    portable job DAG
Runner      execution target with capabilities
```

Use BLAKE3 or SHA-256 for native object IDs. Preserve Git SHA compatibility through mapping tables.

### Layer 3: Change model

Git commits are snapshots. Forge Core should treat changes as primary.

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
forge init --git-colocated
```

Creates:

```text
.git/
.forge/
```

Users can still use Git. Forge Core imports Git commits and exports Forge Core changes as normal Git commits.

### Native mode

```bash
forge clone forge://org/project
```

Uses the native object model.

### Git remote bridge

```bash
git remote add forge forge://org/project
git push forge main
```

or:

```bash
forge git export --to origin/main
```

### One-way mirror

For early adoption:

- team works in Forge Core
- Forge Core exports to GitHub or GitLab
- existing CI and deployments continue

### Round-trip exit

Teams must be able to leave:

```bash
forge export --git ./repo.git
```

No lock-in should be a product principle.

## Collaboration platform

Forge Hub is the GitHub-like collaboration product built on top of Forge Core's portable collaboration objects.

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
- secret manager
- environments
- marketplace

Canonical project data should be exportable and syncable, not trapped in a hosted database only.

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
forge lane create agent/refactor-auth
forge slice create --entry packages/auth/src/index.ts --name auth-lib
forge workflow run test
forge workflow run test --runner ssh-buildbox
```

### JSON API

```http
POST /repos/:repo/lanes
POST /repos/:repo/slices
POST /repos/:repo/workflows/:workflow/runs
```

### SDK

```ts
const lane = await forge.lanes.create("agent/refactor-auth")

const slice = await forge.slices.create({
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
forge slice create \
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
    repo: forge://org/auth-lib
    ref: main
    mode: live
```

Best for active shared components.

#### Pinned link

```yaml
dependencies:
  auth-lib:
    repo: forge://org/auth-lib
    change: chg_abc123
    mode: pinned
```

Best for reproducible production dependencies.

#### Vendored link

```yaml
dependencies:
  auth-lib:
    repo: forge://org/auth-lib
    change: chg_abc123
    mode: vendored
```

Best when consumers should not need remote access after import.

#### Virtual link

The parent repo shows the slice as a normal directory, but storage, review, and history know it is linked.

Best long-term UX, but requires native Forge Core tooling.

### Permission carryover

Rule:

```text
new repo permissions = source permissions intersected with explicit share target
```

Example:

```bash
forge slice create \
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
  sourceRepo: forge://org/app
  sourcePath: packages/auth
  sourceChange: chg_123
```

In the old repo:

```yaml
links:
  packages/auth:
    repo: forge://org/auth-lib
    mode: live
    createdFrom: chg_123
```

Sync commands:

```bash
forge slice sync auth-lib --from-parent
forge slice sync auth-lib --to-parent
forge slice history auth-lib
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
forge slice create \
  --entry packages/auth/src/index.ts \
  --include packages/auth/tests \
  --exclude "**/*.snap"
```

## Permissions model

Git has weak permission granularity. Forge Core should support object-level policy.

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

Capabilities:

```text
repo.read
repo.write
path.read
path.write
change.create
change.read_private
proposal.view
proposal.review
proposal.merge
secret.read
secret.inject
agent.launch
agent.read_logs
agent.modify_instructions
runner.use
workflow.run
marketplace.install
```

Visibility states:

```text
private       only creator or assigned agents
team          visible to selected team
review        visible to reviewers
public        visible to everyone with repo access
redacted      metadata visible, diff hidden
```

Private work should be enforced by object-level authorization and, where needed, encryption rather than UI hiding only.

## Secret management

Secrets must be first-class.

Never commit raw secrets. Repos should contain:

```text
.env.schema
.env.example
forge.secrets.yml
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

- Forge Core Vault
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
- redaction in logs
- preview environment secrets
- secret diffs without values

Commands:

```bash
forge secrets import .env --env dev
forge run --env dev npm test
forge agent start --task fix-tests --secrets test-only
```

For agents, prefer secret handles over raw values.

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
.forge/agent-policy.lock
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

Forge Core should not offer hosted compute at the beginning. Instead, build a portable execution protocol.

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
forge workflow run test
```

Run elsewhere:

```bash
forge workflow run test --runner k8s-prod
forge workflow run test --runner ssh-mac-mini
forge workflow run test --runner local-docker
```

Same workflow, different execution target.

### Runner types

#### Local process runner

```bash
forge runner local
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
forge runner docker
```

Pros:

- reproducible
- good isolation
- works for most CI-like tasks

Cons:

- Linux-container assumptions on macOS/Windows

#### SSH runner

```bash
forge runner register ssh://buildbox.internal
```

Pros:

- easy remote execution
- works with existing machines

Cons:

- requires hardening

#### Kubernetes runner

```bash
forge runner register k8s://cluster/prod-builds
```

Pros:

- autoscaling
- isolation
- good for teams with infrastructure

Cons:

- more operational complexity

#### GitHub Actions / GitLab CI adapter

```bash
forge workflow export --target github-actions
forge workflow export --target gitlab-ci
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
forge runner register \
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
forge workflow run test --local
forge workflow rerun run_123 --runner k8s
forge workflow rerun run_123 --runner ssh-gpu-box
```

Same inputs. Same command. Different runner.

### Agent workflow integration

Agents can ask:

```bash
forge workflow suggest
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
forge workflow run unit --lane agent/refactor-auth
```

The run is attached to the agent lane, change, and proposal.

### Workflow permissions

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

Forge Core must support:

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

### Phase 1: Core engine

Build:

- Rust object store.
- Filesystem backend.
- In-memory backend.
- Snapshot objects.
- Change objects.
- Basic diff/apply.
- Operation log and undo.
- First-class conflict objects.

Commands:

```bash
forge init
forge status
forge change create
forge change list
forge switch
forge diff
forge undo
```

Success criteria:

- Tracks files without Git.
- Runs entirely in memory.
- Supports undo.
- Represents conflict objects.

### Phase 2: Git bridge

Build:

```bash
forge init --git-colocated
forge git import
forge git export
forge git sync
```

Success criteria:

- Existing Git repo can be used.
- Forge Core changes export as normal Git commits.
- Team can leave without data loss.

### Phase 3: Lanes and stacks

Build:

```bash
forge lane create agent-17/fix-tests
forge stack create
forge stack submit
forge lane status
```

Success criteria:

- Multiple lanes can edit the same base.
- Stacks are first-class.
- Rebase/merge preserves reviewable changes.

### Phase 4: Slices

Build:

```bash
forge slice create --entry packages/auth/src/index.ts --name auth-lib
forge slice inspect auth-lib
forge slice sync auth-lib --from-parent
forge slice sync auth-lib --to-parent
```

Success criteria:

- Entrypoint extraction works.
- New repo carries permissions safely.
- Parent and child stay linked.
- Git export remains possible.

### Phase 5: Workflows and runners

Build:

```bash
forge workflow run test --local
forge workflow rerun run_123 --runner ssh-buildbox
forge runner register ssh://buildbox.internal
```

Success criteria:

- Same workflow runs locally and remotely.
- Job inputs are content-addressed.
- Secret refs are resolved only by authorized runners.
- Runs attach to lanes, changes, proposals, and agents.

### Phase 6: Forge Hub server

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

### Phase 7: Secrets and environments

Build:

```bash
forge secrets import .env
forge secrets grant agent-17 --env dev
forge run --env dev npm test
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
4. Forge Core detects overlapping symbols.
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

Build Forge Core in this order:

1. Core object store, changes, snapshots, operation log, and in-memory backend.
2. Git bridge.
3. Lanes and stacks.
4. Slices from entrypoints with permission projection.
5. Portable workflows and user-owned runners.
6. Forge collaboration server.
7. Secrets, agent policy, and marketplace.

This keeps the hardest technical bets close to the core while delaying hosted compute and marketplace complexity until the protocol is proven.
