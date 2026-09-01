# sorrel-runners

Sorrel module: `sorrel-runners`.

> **Alpha status (`0.1.0-alpha.2`):** local process and local Docker/Podman
> execution are experimental and intended only for development and testing.
> Do not use either runner for production workloads or untrusted jobs. Secret
> injection is unsupported: secret references are modeled for policy and
> redaction, but this crate never resolves or injects secret values.

This package contains the first portable workflow runner prototype for Sorrel.
It is intentionally user-owned execution only: local host processes and a
minimal Docker/Podman adapter. It does not provide hosted compute, Kubernetes,
SSH, or secret injection (SecretSpec injection lands in `sorrel-cli` first).

The alpha workflow-file contract and compatibility limits are documented in
[`docs/COMPATIBILITY.md`](docs/COMPATIBILITY.md). See
[`CHANGELOG.md`](CHANGELOG.md) for release notes.

## Prototype scope

- `JobBundle`: a reusable execution bundle containing one or more jobs.
- `JobBundle` Core context: required capabilities, principal context, runner
  requirements, SecretRef dependencies, and local policy decisions.
- `RunnerCapabilities`: a Sorrel protocol-compatible runner descriptor.
- `LocalProcessRunner`: runs jobs as local processes and captures stdout,
  stderr, exit status, and a JSON-lines log artifact.
- `ContainerRunner`: minimal Docker/Podman adapter that runs the same bundle
  shape in a local container engine.
- Inputs can be modeled as content-addressed object references or simple local
  paths. Secret values are not injected; only placeholder references are modeled.
- Captured logs are redacted for declared SecretRefs and secret-like env values.

The log artifact format is newline-delimited JSON:

```text
application/vnd.sorrel.runner.log+jsonl;version=0
```

Each line is a `started`, `stream`, or `finished` record.

## Run a job locally (experimental, development only)

```rust
use std::path::PathBuf;

use sorrel_runners::{
    CorePermissionEvaluator, GrantStoreEvaluator, Job, JobBundle, LocalProcessRunner, ObjectRef,
    PolicyDecision, PrincipalContext, Runner, RunnerRequirements, Shell, CAPABILITY_RUNNER_USE,
};

fn main() -> sorrel_runners::Result<()> {
    let runner = LocalProcessRunner::default_local();
    let runner_ref = ObjectRef::new("Runner", runner.capabilities().id.clone());
    let mut bundle = JobBundle::single(
        "bundle_validate",
        Job::shell(
            "job_validate",
            Shell::Sh,
            "printf 'hello from sorrel\n'",
            None::<PathBuf>,
        ),
    );
    bundle.principal = PrincipalContext {
        runner: Some(runner_ref.clone()),
        ..PrincipalContext::default()
    };
    bundle.runner_requirements = RunnerRequirements {
        runner: Some(runner_ref.clone()),
        ..RunnerRequirements::default()
    };
    bundle.policy_decisions.push(PolicyDecision::allow(
        CAPABILITY_RUNNER_USE,
        bundle.principal.clone(),
        runner_ref.clone(),
        "local development runner grant",
    ));
    let policy = GrantStoreEvaluator::from_grants([PolicyDecision::allow(
        CAPABILITY_RUNNER_USE,
        bundle.principal.clone(),
        runner_ref,
        "local development runner grant",
    )]);

    let result = runner.run(&bundle, &policy)?;

    assert_eq!(result.jobs[0].exit_code, Some(0));
    print!("{}", result.jobs[0].stdout);

    Ok(())
}
```

## Core policy spine

Runner execution is local, but it now requires Core-shaped authorization data in
the `JobBundle` before any process or container command starts:

- `requiredCapabilities` declares the Core capabilities the bundle needs, such
  as `runner.use`, `workflow.run`, `secret.read`, and `secret.inject`.
- `principal` carries the `AgentPolicy`/`Workflow`/`Runner` context used as the
  policy subject.
- `runnerRequirements` pins the expected runner id, labels, platform
  capabilities, and/or secret-handling mode.
- `secretRefs` declares bundle-level SecretRef dependencies. Job-level
  `secretRefs` and `EnvValue::SecretRef` entries must be listed here.
- `policyDecisions` carries Core `PolicyDecision` records for audit and replay. Runners
  re-evaluate authorization through a Core permission evaluator and do not trust
  bundle-attached decisions or local runner config alone.

This crate does not implement hosted compute, production auth, or external
secret providers. The policy decisions are a local/testable envelope so callers
can prove that unauthorized runner or secret access is blocked before the runner
touches the host/container process boundary.

### Policy conformance

`tests/policy_conformance.rs` runs the canonical `sorrel-protocol` policy
conformance manifest (vendored at `tests/conformance/policy-conformance.json`)
against the `CorePermissionEvaluator` gate for `runner.use`, `workflow.run`,
`secret.read`, and `secret.inject`, and asserts that forged bundle-attached
decisions cannot bypass Core evaluation. When the protocol manifest changes,
re-copy it and re-run `cargo test`.

Secret dependency policy uses the shared capability names:

- Declared SecretRef dependencies require `secret.read`.
- `EnvValue::SecretRef` dependencies require `secret.inject`.
- This prototype still rejects actual secret injection after authorization
  because both bundled local runner presets advertise `secretHandling: none`.

## Log redaction

Each bundle carries `redaction` metadata compatible with the vault masking
model. Before results are returned, stdout/stderr and JSONL log records are
scrubbed for:

- declared SecretRef ids
- job-level SecretRef ids
- literal env values whose keys match `TOKEN`, `SECRET`, `PASSWORD`, or `KEY`
  by default

The raw values are not stored in `JobRunResult`.

## Describe runner capabilities

```rust
use sorrel_runners::{ContainerEngine, RunnerCapabilities};

let local = RunnerCapabilities::local_process("runner_local_process", "local-process");
local.validate()?;

let container = RunnerCapabilities::local_container(
    "runner_local_podman",
    "local-podman",
    ContainerEngine::Podman,
    "alpine:3.20",
);
container.validate()?;

println!("{}", serde_json::to_string_pretty(&local)?);
```

Example local descriptor:

```json
{
  "schemaVersion": "sorrel.protocol.v0",
  "kind": "Runner",
  "id": "runner_local_process",
  "name": "local-process",
  "mode": "local",
  "runnerType": "process",
  "platform": {
    "runtime": "shell",
    "os": "linux",
    "arch": "x64",
    "capabilities": ["process", "stdout", "stderr"]
  },
  "isolation": "host",
  "maxParallelJobs": 1,
  "labels": ["local", "process"],
  "endpoint": "runner://local/process",
  "trust": {
    "attestation": "none",
    "secretHandling": "none"
  },
  "status": "online"
}
```

## Rerun the same bundle

`JobBundle` is serializable, so a caller can store it and rerun the exact same
job definition later:

```rust
use sorrel_runners::{GrantStoreEvaluator, JobBundle, LocalProcessRunner, Runner};

fn rerun(serialized_bundle: &str) -> sorrel_runners::Result<()> {
    let bundle: JobBundle = serde_json::from_str(serialized_bundle)?;
    let runner = LocalProcessRunner::default_local();
    let policy = GrantStoreEvaluator::from_grants(bundle.policy_decisions.clone());

    let first = runner.run(&bundle, &policy)?;
    let second = runner.run(&bundle, &policy)?;

    assert_eq!(first.jobs[0].exit_code, second.jobs[0].exit_code);
    Ok(())
}
```

## Docker/Podman prototype (experimental, development only)

The container adapter is experimental and intended only for local development.
It shells out to a local engine owned by the user, mounts the job working
directory at `/workspace`, adds simple local-path input mounts under `/inputs`,
forwards literal environment variables, and captures output in the same
`RunResult` and JSON-lines log format as the local process runner. It is not a
production isolation or security boundary.

```rust
use std::path::PathBuf;

use sorrel_runners::{
    ContainerEngine, ContainerRunner, GrantStoreEvaluator, Job, JobBundle, ObjectRef,
    PolicyDecision, Runner, Shell, CAPABILITY_RUNNER_USE,
};

let runner = ContainerRunner::new(ContainerEngine::Docker, "alpine:3.20")?;
let bundle = JobBundle::single(
    "bundle_container_smoke",
    Job::shell(
        "job_container_smoke",
        Shell::Sh,
        "echo running inside container",
        Some(PathBuf::from(".")),
    ),
);

let result = runner.run(
    &bundle,
    &GrantStoreEvaluator::from_grants([PolicyDecision::allow(
        CAPABILITY_RUNNER_USE,
        bundle.principal.clone(),
        ObjectRef::new("Runner", runner.capabilities().id.clone()),
        "local container runner grant",
    )]),
)?;
assert_eq!(result.jobs[0].exit_code, Some(0));
```

## Development

```sh
cargo fmt --all -- --check
cargo clippy --all-targets
cargo test
```

## License

Licensed under either [Apache License, Version 2.0](LICENSE-APACHE) or the
[MIT License](LICENSE-MIT), at your option.
