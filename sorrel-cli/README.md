# sorrel-cli

Sorrel's command-line interface for persistent, agent-native version control.
The binary is named `sorrel`.

## Architecture

The CLI depends on in-tree [`sorrel-core`](../sorrel-core) via the root Cargo
workspace path dependency. Core provides the content-addressed object store,
snapshots, changes, lanes, stacks, merge primitives, Git import/export, and
policy object types. CLI-specific repository registries, Hub transport, line
diffs, policy evaluation, and workflow parsing/local execution live here.

Workflow execution currently uses an in-tree `cli_runner` (intentional
**DEBT-1** — unify with `sorrel-runners` after secret injection). Secret
handles are listed via `sorrel secret`; resolve/inject goes through SecretSpec
providers under Core grants (see root `ROADMAP.md`).

`tests/policy_conformance.rs` checks the CLI policy evaluator against the
vendored `sorrel-protocol` conformance manifest.

## Features

- Persistent repositories: `init`, `status`, `diff`, `log`, and
  `change create` / `change list`.
- Parallel work and integration: `lane create` / `list` / `switch` / `submit`,
  `stack create` / `list` / `show`, and three-way `merge` with
  `--continue` / `--abort`.
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
the persisted run output.

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
