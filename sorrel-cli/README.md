# sorrel-cli

Sorrel's command-line interface for persistent, agent-native version control.
The binary is named `sorrel`.

## Architecture

The CLI depends on in-tree [`sorrel-core`](../sorrel-core) via the root Cargo
workspace path dependency. Core provides the content-addressed object store,
snapshots, changes, lanes, stacks, merge primitives, Git import/export, and
policy object types. CLI-specific repository registries, Hub transport, line
diffs, policy compatibility, and workflow host adapters live here.

Workflow parsing and local execution use `sorrel-runners`; `cli_runner` contains
thin adapters preserving CLI JSON. Workflow authorization uses native Core
policy. Secret handles are listed via `sorrel secret`; host-side resolution goes
through SecretSpec and resolved values stay outside portable bundles.

Legacy top-level `jobs` files remain supported alongside named `workflows`.
Use `--workflow <name>` when a file has several workflows. Running a job first
runs its `needs` dependencies; a failed dependency stops execution. Literal
`env` entries are applied, and authorized secret references support environment
aliases. Legacy `shell` strings retain their `<shell> -c` invocation.

Workflow failures, invalid/missing files or jobs, and policy denials return
nonzero process status after printing JSON. A failed child preserves its exit
code. Human output includes redacted output and a `sorrel run logs <id>` hint;
JSON includes `runId` when persisted. Failure to save logs is reported as
`run_log_failed`, even when the child succeeded.

`tests/policy_conformance.rs` checks the CLI policy evaluator against the
vendored `sorrel-protocol` conformance manifest.

## Merge status

`status --json` reports recorded pending merge conflicts in `worktree.conflicts`
and whether a merge awaits completion in `worktree.mergeInProgress`. The count
includes conflicts without text markers and stays pending after manual editing
until `merge --continue` or `merge --abort`. `status` still exits successfully
when it can report the state; `status` and `worktree.dirty` continue to describe
changes against HEAD. Human output includes a pending-merge hint.

## Workspace file selection

`status`, `diff`, and `change create` honor nested `.gitignore` and
`.sorrelignore` rules. Ordinary files already tracked at HEAD remain tracked even
when ignored later. `.env`, `.env.*`, and configured local dotenv-provider paths
are protected before contents enter the object store; `.env.example` remains an
ordinary file. Ignore negation cannot reinclude protected secret files.

For legacy workspaces that already track secrets, see the
[workspace selection and recovery contract](../docs/ARCHITECTURE.md#change-lane-and-merge-flow).

Workspace commands acquire an advisory operating-system lock before reading
or changing metadata. If another command is using the same workspace, retry
after its busy error; the lock releases automatically when that process exits.
Use separate workspaces for concurrent edits. Workflow child processes run
outside the lock so they can invoke Sorrel. Head and change-index publication
is journaled and an interrupted commit finishes on the next locked command.

## Features

- Persistent repositories: `init`, `status`, `diff`, `log`, and
  `change create` / `change list`.
- Parallel work and integration: `lane create` / `list` / `switch` / `submit`,
  `stack create` / `list` / `show`, and three-way `merge` with
  `--continue` / `--abort`. Conflicted merges apply clean changes alongside
  conflict markers; continuing preserves those changes, and aborting restores
  the original tree. Incoming tracked files remain included even when the
  receiving lane's ignore rules exclude them.
- Git bridge: history `git import`, `git export`, and bidirectional,
  fast-forward-oriented `git sync` for colocated or separate mirrors. See
  [`GIT.md`](GIT.md).
- Hub sync: `remote add` / `list`, `push`, and `pull`; lane submission can push
  and open a Hub collaboration proposal. See [`SYNC.md`](SYNC.md).
- Persisted slice manifests, grants, and secret-reference handles.
- Policy evaluation and policy-change checks.
- Workflow validation and policy-gated local job execution from
  `sorrel.workflow.yml`.

Every command accepts the global `--json` flag and emits structured JSON.
Repository state is real and persisted under `.sorrel/`; commands do not return
fabricated domain data. Empty changes are rejected. See [`DEMO.md`](DEMO.md)
for an end-to-end local walkthrough.

### On-disk layout

A workspace lives in a `.sorrel/` directory next to the working tree:

```text
.sorrel/
  objects/        content-addressed object store (BLAKE3 ids, two-char fanout)
  lanes/          persisted lane records
  heads/          per-lane snapshot pointers
  stacks/         persisted stack records
  grants/         persisted permission grants
  secrets/        persisted secret-reference handles
  slices/         persisted slice manifests
  manifest.json   repo identity + creation metadata + default lane
  HEAD            current lane + head snapshot pointer (atomically written)
  changes.index   snapshot-to-change index
  remotes.json    configured Hub remotes
```

`manifest.json`:

```json
{
  "schemaVersion": "sorrel.protocol.v0",
  "kind": "Workspace",
  "repoId": "repo_<hex>",
  "createdAt": "2026-06-26T12:00:00Z",
  "defaultLane": { "id": "lane_main", "name": "main" }
}
```

`HEAD`:

```json
{ "lane": "lane_main", "snapshot": "<64-hex object id>" }
```

## Workflow file

Create `sorrel.workflow.yml` in the repository root (or pass `--file`):

```yaml
version: 1
id: workflow_validate_protocol

jobs:
  test:
    command: echo workflow-ok
    shell: sh
    secrets:
      - secret_npm_token_dev
    env:
      NPM_TOKEN: "secret:secret_npm_token_dev"
```

Validate the file:

```bash
sorrel workflow validate
sorrel workflow validate --file ./sorrel.workflow.yml --json
```

Run a named job locally:

```bash
sorrel workflow run test
sorrel workflow run test --json
```

Policy gates still apply to workflow execution. A run is denied when the CLI
agent principal lacks grants for `workflow.run`, `runner.use`, or any declared
secret permissions. Workflow parsing only records `secretRefs`; it does not
resolve values while parsing. At execution time, the CLI resolves authorized
references through SecretSpec, injects them into the local process, and redacts
the persisted run output. Workflow grant scopes enforce exact `ref`/`path`
identifiers (or `*`) and the selected `environment`. Unsupported scope fields or
patterns are rejected; narrow existing grants to these supported constraints.
Core deny decisions take precedence over allow grants.

## Examples

```bash
sorrel init
sorrel init --json
```

```json
{
  "command": "init",
  "mocked": false,
  "repoId": "repo_a834b552a41b9e09",
  "sorrelDir": ".sorrel",
  "initialized": true,
  "status": "initialized",
  "createdAt": "2026-06-26T12:00:00Z",
  "defaultLane": { "id": "lane_main", "name": "main" },
  "headSnapshot": {
    "kind": "Snapshot",
    "id": "9c20ff158056dea97c3855573c471e7ebcaf828a9224292869a158a86c4b5d41"
  }
}
```

```bash
sorrel status --json
```

```json
{
  "command": "status",
  "mocked": false,
  "repoId": "repo_a834b552a41b9e09",
  "sorrelDir": ".sorrel",
  "initialized": true,
  "status": "ready",
  "currentLane": { "kind": "Lane", "id": "lane_main" },
  "headSnapshot": {
    "kind": "Snapshot",
    "id": "9c20ff158056dea97c3855573c471e7ebcaf828a9224292869a158a86c4b5d41"
  }
}
```

```bash
sorrel change create -m "Document Sorrel" --json
sorrel change list --json
sorrel lane create --name agent/docs --json
sorrel slice create --name auth-lib --source-path packages/auth --entrypoint packages/auth/src/index.ts --json
sorrel workflow validate --json
sorrel workflow run test --json
```

## Local/headless policy usage

Policy evaluation works without Sorrel Hub and is suitable for local agents,
scripts, and tests.

Evaluate a path write for an agent:

```bash
sorrel policy evaluate \
  --principal agent:docs \
  --action path.write \
  --resource path:docs/README.md \
  --json
```

Evaluate a workflow run:

```bash
sorrel policy evaluate \
  --principal agent:docs \
  --action workflow.run \
  --resource workflow:workflow_validate_protocol \
  --json
```

Check a secret injection request. Core returns `needs_grant`, which lets a
headless caller prompt for or create a scoped grant before materializing any
value:

```bash
sorrel policy evaluate \
  --principal agent:docs \
  --action secret.inject \
  --resource secret:secret_database_url_dev \
  --environment dev \
  --json
```

Evaluate a signed policy change before it would be applied:

```bash
sorrel policy change apply \
  --actor agent:agent_17 \
  --target-principal agent:agent_17 \
  --capability secret.inject \
  --capability policy.grant \
  --signature sig_agent_17 \
  --json
```

Core denies self-grants when the actor lacks delegated `policy.grant` authority
under the previous effective policy.

Create and inspect persisted grants:

```bash
sorrel grant create \
  --action secret.inject \
  --agent agent_mock_cli \
  --workflow workflow_validate_vault \
  --runner runner_local_process \
  --secret secret_database_url_dev \
  --environment dev \
  --json

sorrel grant list --json
```

Resolve and inject secrets via SecretSpec (after a grant):

```bash
sorrel secret sync
sorrel secret set secret_database_url_dev --value 'postgres://localhost/dev'
sorrel secret run --provider dotenv:.env -- printenv DATABASE_URL
sorrel workflow run test
sorrel run list
sorrel env ensure
```

Sorrel supplies an operation-specific SecretSpec access reason by default.
Set `SECRETSPEC_REASON` to a more specific non-empty reason when your audit
policy requires caller context.

List secret handles without resolving values:

```bash
sorrel secret list --json
```

## Development

```bash
cargo test --workspace
cargo clippy --bin sorrel -- -D warnings
cargo fmt --all -- --check
```
