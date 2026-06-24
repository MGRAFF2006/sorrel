# Sorrel Agent Task Board

This file is the orchestration handoff for agents working on Sorrel. It turns the architecture report into concrete work packages and defines the repository/submodule map needed to build the product.

## Current orchestrator status

- Main report branch: `cursor/version-control-report-eb91`
- Architecture report: `AGENT_NATIVE_VERSION_CONTROL_REPORT.md`
- Selected names:
  - `Sorrel Core`: protocol, storage engine, CLI, SDK, Git bridge, local runtime
  - `Sorrel Hub`: collaboration product, proposals, review, orgs, policies, secrets, workflow UI
- Attempted submodule:
  - `sorrel-web` at `https://github.com/MGRAFF2006/sorrel-web.git`
  - Status: blocked
  - Reason: repository exists but cursor bot could not push the first website commit (`403 Permission denied`)
  - Preserved starter website patch in this repo: `SORREL_WEB_STARTER.patch`
  - Original prepared commit message: `Add Sorrel website landing page`

To unblock `sorrel-web`, either:

1. Push an initial commit to `MGRAFF2006/sorrel-web` yourself, then run:

   ```bash
   git submodule add https://github.com/MGRAFF2006/sorrel-web.git sorrel-web
   git add .gitmodules sorrel-web
   git commit -m "Add sorrel-web submodule"
   ```

2. Or grant the agent/bot write access to `MGRAFF2006/sorrel-web`, then ask an agent to apply `SORREL_WEB_STARTER.patch`, push the prepared starter site, and add it as a submodule.

## Recommended submodule and repository map

Keep Sorrel modular. The report repository should become the coordination root that links the independently owned implementation repositories.

### Required now

| Submodule path | Repository name | Purpose | First owner task |
| --- | --- | --- | --- |
| `sorrel-web` | `sorrel-web` | Public website and eventually docs landing page. | Create/push initial static website, then add as submodule. |
| `sorrel-core` | `sorrel-core` | Rust core: object store, snapshots, changes, lanes, conflict objects, Git bridge. | Create a Rust workspace skeleton and core object model. |
| `sorrel-protocol` | `sorrel-protocol` | Shared schemas/specs: object types, JSON API, runner protocol, policy model. | Extract protocol definitions from the report into versioned specs. |

### Required after core starts

| Submodule path | Repository name | Purpose | First owner task |
| --- | --- | --- | --- |
| `sorrel-cli` | `sorrel-cli` | Human and agent CLI (`sorrel init`, `sorrel lane`, `sorrel slice`, `sorrel workflow`). | Build CLI shell around mocked protocol calls, then wire to `sorrel-core`. |
| `sorrel-hub` | `sorrel-hub` | Collaboration app/server: orgs, repos, proposals, review, merge queue, audit log. | Create app/API skeleton and data model. |
| `sorrel-runners` | `sorrel-runners` | Local, Docker, SSH, Kubernetes, WASM, browser, and REAPI-style runner adapters. | Define `JobBundle` and implement local process + Docker adapters. |
| `sorrel-vault` | `sorrel-vault` | Secrets and environment management, secret refs, grants, redaction, external adapters. | Define secret schema and local encrypted/dev backend. |

### Useful once the basics work

| Submodule path | Repository name | Purpose | First owner task |
| --- | --- | --- | --- |
| `sorrel-slices` | `sorrel-slices` | Entrypoint analyzers for JS/TS, Rust, Go, Python, Docker, Terraform. | Implement TypeScript dependency-closure analyzer. |
| `sorrel-agents` | `sorrel-agents` | Agent policy layers, task lanes, work claims, audit trails, MCP/JSON agent interface. | Define agent policy object and lane claim API. |
| `sorrel-sdk-js` | `sorrel-sdk-js` | TypeScript SDK for agents, web, and Node tools. | Generate SDK types from `sorrel-protocol`. |
| `sorrel-sdk-rust` | `sorrel-sdk-rust` | Rust SDK/client types for native tools and runner integrations. | Share protocol types with `sorrel-core`. |
| `sorrel-marketplace` | `sorrel-marketplace` | Later marketplace for analyzers, runner adapters, review bots, workflow modules. | Draft app manifest and capability model. |

## Submodule policy

Use submodules only for independently buildable repos. Do not create a repo until it has a clear owner and first deliverable.

Submodule naming rules:

- Path is lowercase and matches repo name.
- Each submodule has its own README with:
  - purpose
  - local setup
  - first milestone
  - agent instructions
- The root report repository remains the coordination source of truth.
- Do not duplicate implementation code between submodules.

Recommended add command:

```bash
git submodule add https://github.com/MGRAFF2006/<repo>.git <repo>
git add .gitmodules <repo>
git commit -m "Add <repo> submodule"
```

## Agent work packages

Each task below is designed to be given to a separate agent.

### T0 - Root coordination repo

Target repo: `Version-Controle`

Goal:

- Keep the report, task board, and submodule map current.

Deliverables:

- `AGENT_NATIVE_VERSION_CONTROL_REPORT.md` remains the product architecture source.
- `SORREL_AGENT_TASKS.md` remains the executable task board.
- `.gitmodules` tracks created Sorrel submodules.

Success checks:

- `git submodule status` works once submodules exist.
- New repos are only added after they have an initial commit.

### T1 - Sorrel Web

Target repo: `sorrel-web`

Goal:

- Build the public Sorrel website.

Deliverables:

- Static first version or lightweight app.
- Landing page for Sorrel Core and Sorrel Hub.
- Roadmap section and docs links.
- Clear local preview instructions.

Success checks:

- Website can be opened locally.
- No build dependency is required for the first version unless deliberately added.
- Repo has at least one pushed commit so it can be added as a submodule.

### T2 - Sorrel Protocol

Target repo: `sorrel-protocol`

Goal:

- Define stable contracts before implementation spreads across repos.

Deliverables:

- Object schemas:
  - `Blob`
  - `Tree`
  - `Change`
  - `Snapshot`
  - `Lane`
  - `Stack`
  - `Slice`
  - `Conflict`
  - `Resolution`
  - `SecretRef`
  - `Workflow`
  - `Runner`
  - `Policy`
  - `AgentPolicy`
- JSON examples for each object.
- Versioning rules for protocol changes.

Success checks:

- Schemas can be validated with one command.
- Example objects round-trip through validation.

### T3 - Sorrel Core object store

Target repo: `sorrel-core`

Goal:

- Build the storage foundation.

Deliverables:

- Rust workspace.
- `ObjectStore` trait.
- In-memory object store.
- Filesystem object store.
- BLAKE3 or SHA-256 object IDs.
- Basic content-addressed write/read tests.

Success checks:

- Unit tests prove deduplication.
- In-memory backend works without touching the filesystem.

### T4 - Snapshots and changes

Target repo: `sorrel-core`

Goal:

- Implement the minimum useful VCS model.

Deliverables:

- `Snapshot` object.
- `Change` object.
- Tree materialization.
- Diff from snapshot to snapshot.
- Apply change to snapshot.
- Operation log skeleton.

Success checks:

- Create two snapshots.
- Compute a diff.
- Apply diff and reproduce the second snapshot.

### T5 - CLI skeleton

Target repo: `sorrel-cli`

Goal:

- Create the command surface agents and humans will use.

Deliverables:

```bash
sorrel init
sorrel status
sorrel change create
sorrel change list
sorrel lane create
sorrel slice create
sorrel workflow run
```

Success checks:

- Every command supports `--json`.
- Mock mode works before full core integration.

### T6 - Git bridge

Target repo: `sorrel-core`

Goal:

- Make Git migration realistic.

Deliverables:

```bash
sorrel init --git-colocated
sorrel git import
sorrel git export
sorrel git sync
```

Success checks:

- Import a simple Git repo.
- Export Sorrel changes as normal Git commits.
- Round-trip without losing file contents.

### T7 - Lanes and stacks

Target repos: `sorrel-core`, `sorrel-cli`

Goal:

- Support parallel human/agent work.

Deliverables:

- Lane object.
- Stack object.
- Lane status.
- Basic path/symbol claim structure.
- Collision report format.

Success checks:

- Two lanes can change the same base snapshot.
- System reports overlapping paths.
- JSON output is stable for agents.

### T8 - Slices

Target repo: `sorrel-slices`

Goal:

- Extract shareable subprojects from entrypoints.

Deliverables:

- Slice object implementation.
- TypeScript entrypoint analyzer.
- Include/exclude rules.
- Permission projection design.
- Parent/child backlink format.

Success checks:

- Given a TypeScript entrypoint, produce a dependency closure.
- Produce a slice manifest.

### T9 - Workflows and runners

Target repo: `sorrel-runners`

Goal:

- Make workflows portable without hosted compute.

Deliverables:

- `JobBundle` schema.
- Local process runner.
- Docker/Podman runner.
- Runner capability descriptor.
- Rerun command design.

Success checks:

- Same job runs locally and in Docker.
- Inputs are content-addressed.
- Logs and artifacts are captured.

### T10 - Secrets and environments

Target repo: `sorrel-vault`

Goal:

- Make secrets first-class.

Deliverables:

- `sorrel.secrets.yml` schema.
- SecretRef object.
- Local dev backend.
- Redaction rules.
- Grant model for agents and runners.

Success checks:

- Import `.env` into local backend.
- Run command with injected env.
- Logs redact secret values.

### T11 - Sorrel Hub skeleton

Target repo: `sorrel-hub`

Goal:

- Start the collaboration product.

Deliverables:

- App/server skeleton.
- Organization/project/repo model.
- Proposal model.
- Review comment model.
- API routes for lanes, proposals, and workflow runs.

Success checks:

- Create a project.
- Create a proposal from a change.
- Fetch proposal as JSON.

### T12 - Agent control plane

Target repo: `sorrel-agents`

Goal:

- Give agents safe, structured interfaces.

Deliverables:

- Agent policy object.
- Instruction layering:
  - org
  - project
  - repo
  - path
  - task
  - agent overlay
- MCP or JSON-RPC tool surface.
- Claim/release API.

Success checks:

- Agent can claim a file or symbol.
- Policy denies an unauthorized operation.
- Tool output is structured JSON.

## First integration demo

The first end-to-end demo should prove:

1. Import an existing Git repo.
2. Create three lanes.
3. Make two lanes touch the same file.
4. Detect overlap.
5. Extract a TypeScript entrypoint as a slice.
6. Run a workflow locally and in Docker.
7. Export final result back to Git.
8. Show the work in Sorrel Hub.

## Orchestrator rules for future agents

- Keep tasks small and repo-scoped.
- Prefer protocol/schema work before UI.
- Every task must define:
  - target repo
  - deliverables
  - success checks
  - dependencies
- Do not add a submodule until its remote repo has an initial commit.
- Do not widen scope inside implementation agents; add follow-up tasks here instead.
