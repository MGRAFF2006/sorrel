# Workflow-file compatibility

This document describes the `sorrel.workflow.yml` contract supported by
`sorrel-runners` `0.1.0-alpha.2`. The format and its Rust data model are alpha
interfaces and may change incompatibly before a stable release. Pin the exact
prerelease version when relying on them.

## Supported contract

The supported top-level format version is `1`:

```yaml
version: 1
workflows:
  check:
    jobs:
      lint:
        command: cargo clippy
      test:
        command: cargo test
        needs: [lint]
```

A file contains a non-empty `workflows` map, or the legacy top-level `jobs`
map with optional `id` (default `workflow_local`). Mixing both shapes is
rejected. Each selected workflow must
contain at least one named job, and each job must have a non-empty `command`.
The parser accepts these job fields:

| Field | Alpha behavior |
| --- | --- |
| `command` | Required string executed with `sh` by default. |
| `shell` | Optional executable string; invokes `<shell> -c` for legacy compatibility. |
| `secrets` | Optional SecretRef ID list resolved by the trusted host. |
| `needs` | Optional job-id list. Dependencies must exist in the same workflow and be acyclic. |
| `inputs` | Optional string list converted directly to read-only local paths; glob expansion is not provided. |
| `platform` | Optional `runtime`, `os`, `arch`, and `image` metadata. `runtime: container` plus `image` records routing metadata only; it does not select a runner. |
| `env` | Optional map of literals, `secret:<id>` strings, or SecretRef objects. Local host-resolved injection is authorized and redacted. |

Jobs are converted to a `JobBundle` in deterministic topological order
(ascending job id when multiple jobs are ready). Conversion declares
`runner.use` and `workflow.run`; callers must provide authorization through a
Core permission evaluator before execution.

The `version` field must be an integer and alpha producers must emit `1`.
Values other than `1` are outside the compatibility contract even if a given
alpha parser happens to deserialize them.

## Execution limits

- `LocalProcessRunner` and `ContainerRunner` are experimental and for
  development/testing only.
- Workflow conversion does not select a process or container runner.
- Local execution is not sandboxed. Container execution shells out to a
  user-owned Docker or Podman engine and is not a production security boundary.
- `LocalProcessRunner::run_with_env` accepts host-resolved environment values
  after authorization. Values never enter the bundle and secret outputs/logs
  are redacted. The runner does not consult providers. Default `run` and
  container execution reject unresolved injection.
- Workflow bundles stop after a failed job; independent non-workflow bundles
  retain their existing run-all behavior.
- Matrices, triggers, multi-step jobs, services, conditions, retries, timeouts,
  caches, artifacts, hosted runners, Kubernetes, and SSH execution are not part
  of the version 1 alpha contract.

Unknown fields and semantics not listed here have no compatibility guarantee.
Consumers should reject or avoid them rather than relying on incidental parser
behavior.
