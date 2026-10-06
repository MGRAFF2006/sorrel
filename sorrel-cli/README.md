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
Inherited process variables remain available to jobs. Values under secret-like
keys (`TOKEN`, `SECRET`, `PASSWORD`, or `KEY` by default) join the existing
runner masking terms, including for devenv output, before JSON or persisted
stream logs are returned. These terms are kept in memory only and follow the
bundle masking settings. JSON includes `runId` when persisted. Failure to save logs is reported as
`run_log_failed`, even when the child succeeded.

`tests/policy_conformance.rs` checks the CLI policy evaluator against the
vendored `sorrel-protocol` conformance manifest.

`diff` compares complete line segments, including LF/CRLF endings and an
unterminated final line. Human output marks missing final newlines and changed
CRLF lines. JSON hunk lines retain terminator-free `text` and include
`lineEnding: "crlf"` or `"none"` when applicable; omitted `lineEnding` means LF.
LCS reconstruction uses linear auxiliary memory while retaining the same edit
choices. CPU time remains quadratic in the old/new line counts; large files
can still take longer to compare.

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

Use `sorrel path explain <workspace-relative-path>` (or add `--json`) to inspect
`included`, `tracked`, `ignored`, `protected`, and `metadata` flags without
reading the target's contents or creating snapshots, blobs, or cache entries.
Tracked ordinary files can be both ignored and included; protected files remain
excluded. `included` means selection eligibility, including for missing paths;
`exists`, `isDirectory`, and `supportedType` describe filesystem metadata.
Reserved `.git` and `.sorrel` paths are classified without traversal; these three
fields are `null` (`unknown` in human output) because their type and existence
are not inspected, including when `.git` is a worktree pointer file.
The command also works before `init`, without creating `.sorrel/`.

Explanation reads current ignore/provider configuration and immutable HEAD
snapshot/tree metadata. It does not replay pending metadata transactions or
acquire the writer lock, so the result is a moment-in-time observation rather
than an atomic view of concurrent changes. Parent components and absolute paths
are rejected, symlink ancestors are not traversed, and unsupported leaf types
are reported as excluded. No rule editing or preset configuration is included.

For legacy workspaces that already track secrets, see the
[workspace selection and recovery contract](../docs/ARCHITECTURE.md#change-lane-and-merge-flow).

`status` compares against temporary preview objects under `.sorrel/tmp`, then
removes them without adding objects to `.sorrel/objects`. It saves cache entries
for unchanged committed files; changed files are hashed again until recorded.
Existing unreferenced objects are retained. An abruptly terminated status can
leave a temporary preview directory, but does not publish its objects or HEAD.

Workspace commands acquire an advisory operating-system lock before reading
or changing metadata. If another command is using the same workspace, retry
after its busy error; the lock releases automatically when that process exits.
Use separate workspaces for concurrent edits. Workflow child processes run
outside the lock so they can invoke Sorrel. Head and change-index publication
is journaled and an interrupted commit finishes on the next locked command.

On Unix, metadata staging flushes contents and directory entries before the
journal is published; target renames are flushed before journal removal, whose
directory entry is then flushed as well. A failure after publication reports
uncertain durability and preserves the journal and staged data while target
publication is incomplete. Recovery retries barriers even for targets already
renamed. If the final journal-removal flush fails, the targets are already
flushed; the next locked command retries the root barrier, and a journal that
reappears after restart can be replayed safely.

HEAD and lane-head publication flush their validated snapshot closure, including
terminal blobs and ancestor history. Change-index publication and journal recovery
also flush referenced changes, their parent changes, and linked base/result
snapshot closures. This can add work proportional to the closure and history.
Recovery rechecks all changes present in a staged changes index, including history.
A failed closure check publishes no further pointers and leaves pending recovery data intact.
Directory-entry flushing is a Unix contract; Windows retains file flushes and
atomic replacement without the same directory guarantee. Device-level power-loss
behavior has not been verified.

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

`diff` includes each changed entry's `oldMode` and `newMode` in JSON:
`normal`, `executable`, or `directory`, with `null` on the missing side of an
addition or deletion. Human output shows mode transitions before content hunks.
Executable-only and empty-directory changes remain visible with no text hunks;
an unchanged binary file's mode change does not report a content change.

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

The CLI accepts only `schemaVersion: "sorrel.protocol.v0"`. Missing, malformed,
or unknown versions fail before command execution or metadata recovery; no
migration or rewrite occurs. Use a version-compatible Sorrel release to inspect
such a repository. Optional fields in supported v0 manifests remain intact.

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

Create a value-free request for an operator to approve:

```bash
sorrel grant create --request-only \
  --action secret.inject --agent agent_mock_cli \
  --workflow workflow_validate_vault --runner runner_local_process \
  --secret secret_database_url_dev --environment dev --json > grant-request.json
```

`--request-only` emits a canonical `scope`, its full BLAKE3 `grantId`, and native
Core `proposedGrants` templates. It never persists a grant. Without approval
inputs, ordinary `grant create` reports `needs_grant` and also does not persist.
Repeat `--agent`, `--workflow`, or `--runner` for multiple allowed identities;
values are sorted and deduplicated. SecretRef IDs are literal and cannot contain `*`. Omitting workflow or runner restrictions
permits any workflow or runner, including direct CLI use; a workflow-restricted
grant cannot authorize a direct `secret get`, `check`, `set`, or `run` invocation.
Direct CLI operations identify their runner as `runner_local_process`.

The operator provides two external JSON inputs:

- `--authority-context`: an operator-trusted object with `authorityRoot`
  (native `AuthorityRoot`), `previousGrants` (native `Grant` array), and `context`
  (native `PolicyChangeContext`). Keep signing material outside the workspace
  in protected local configuration. This input is never persisted or printed.
- `--policy-change`: a signed native `PolicyChange` with action `grant`, the
  expected previous/current policy root, and exactly the request's
  `proposedGrants`. The actor needs `policy.grant` or `authority.admin` under the
  previous effective policy; possessing a signing key alone is insufficient.

Use the native Core signing API to sign the complete change. Each proposal ID
is `<grantId>_<recipient-index>`; the signed ID binds all recipients, the secret,
capabilities, environment, and workflow/runner restrictions. Do not edit the
proposal IDs or substitute scope fields after approval. `secret.inject`
explicitly includes `secret.read` in its approved capabilities. Scope hashing
uses compact UTF-8 JSON in the field order `action`, `agents`, `workflows`,
`runners`, `secret`, `environment`; consume the CLI's generated templates to
avoid reproducing serialization conventions.

```bash
sorrel grant create \
  --action secret.inject --agent agent_mock_cli \
  --workflow workflow_validate_vault --runner runner_local_process \
  --secret secret_database_url_dev --environment dev \
  --authority-context "$OPERATOR_AUTHORITY_CONTEXT" \
  --policy-change "$SIGNED_POLICY_CHANGE" --json
sorrel grant list --json

# Reverify native approval against explicit operator input on each use.
SORREL_AUTHORITY_CONTEXT="$OPERATOR_AUTHORITY_CONTEXT" sorrel workflow run test
```

Stored grants contain only the value-free scope and signed change, never signing
keys or an automatically trusted context path. Each use rechecks signatures,
actor authority, the expected policy root, and exact scope binding against
`SORREL_AUTHORITY_CONTEXT`. Consumption also evaluates each actual recipient's
secret capability through native Core with previous grants plus approved issued
grants; Deny, Redact, and Review effects block use even when issuance is approved. Missing/invalid approval, changed authority context,
legacy decision-only grants, and environment/workflow/runner mismatches fail
closed. Reissue grants when the trusted policy context changes. The workflow
adapter currently identifies its environment as `dev` and its runner as
`runner_local_process`; these are the dimensions enforced here. SecretRef provider
environments must match the workflow execution environment before resolution. The native
Core signing API remains the project's headless authority implementation; this
flow does not introduce a production public-key trust service.

For an explicitly mocked local demo, use a separate opt-in:

```bash
sorrel grant create --local-demo --secret secret_database_url_dev --json
SORREL_LOCAL_DEMO=1 sorrel secret run --provider dotenv:.env -- your-command
```

Demo records report `mocked: true` and are ignored without
`SORREL_LOCAL_DEMO=1`. Do not enable this opt-in in an authoritative workflow.

Resolve and inject secrets via SecretSpec after an approved grant for the actual operation.
For the direct commands below, approve a separate request without `--workflow`;
the earlier workflow-restricted grant only covers `workflow_validate_vault`:

```bash
sorrel secret sync
SORREL_AUTHORITY_CONTEXT="$OPERATOR_AUTHORITY_CONTEXT" sorrel secret set secret_database_url_dev
SORREL_AUTHORITY_CONTEXT="$OPERATOR_AUTHORITY_CONTEXT" sorrel secret run --provider dotenv:.env -- your-command
SORREL_AUTHORITY_CONTEXT="$OPERATOR_AUTHORITY_CONTEXT" sorrel workflow run test
sorrel run list
sorrel env ensure
```

Sorrel supplies an operation-specific SecretSpec access reason by default.
Set `SECRETSPEC_REASON` to a more specific non-empty reason when your audit
policy requires caller context.

Secret resolution and checks use only the selected handles, grouped by provider
and environment profile. Missing unselected secrets do not block a selected
handle. Mixed selections report `"mixed"` in the provider or profile string;
check reports retain each secret's provider attribution. An explicit provider
override applies to every selected profile. Selecting distinct handles with
the same environment-variable name fails before reading values. Composed
secrets must include their dependencies in the same selected provider/profile
group; an excluded dependency is rejected before provider access.

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
