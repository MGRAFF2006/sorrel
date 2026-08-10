use std::path::PathBuf;
use std::process::Command;

use sorrel_runners::{
    CAPABILITY_RUNNER_USE, ContainerEngine, ContainerRunner, GrantStoreEvaluator, Job, JobBundle,
    LocalProcessRunner, ObjectRef, PolicyDecision, PolicyDecisionStatus, PrincipalContext,
    RunStatus, Runner, RunnerError, RunnerRequirements,
};

fn docker_available() -> bool {
    Command::new("docker")
        .args(["info"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

#[test]
fn container_runner_capabilities_validate() {
    let runner = ContainerRunner::new(ContainerEngine::Docker, "alpine:3.20")
        .expect("container runner constructs");
    assert!(runner.capabilities().id.contains("docker"));
    assert_eq!(runner.capabilities().name, "local-docker");
}

#[test]
fn container_runner_policy_gate_blocks_without_grant() {
    let runner = ContainerRunner::new(ContainerEngine::Docker, "alpine:3.20").unwrap();
    let bundle = sample_bundle("bundle_deny", "echo blocked");
    let error = runner
        .run(&bundle, &GrantStoreEvaluator::deny_all())
        .expect_err("missing runner.use must block before docker spawn");
    assert_policy_error(
        error,
        CAPABILITY_RUNNER_USE,
        PolicyDecisionStatus::NeedsGrant,
    );
}

#[test]
fn container_runner_executes_echo_when_docker_present() {
    if !docker_available() {
        eprintln!("skipping: docker not available");
        return;
    }

    let runner = ContainerRunner::new(ContainerEngine::Docker, "alpine:3.20").unwrap();
    let mut bundle = sample_bundle("bundle_docker_ok", "echo hello-container");
    let runner_ref = ObjectRef::new("Runner", runner.capabilities().id.clone());
    bundle.principal = PrincipalContext {
        runner: Some(runner_ref.clone()),
        ..PrincipalContext::default()
    };
    bundle.runner_requirements = RunnerRequirements {
        runner: Some(runner_ref.clone()),
        ..RunnerRequirements::default()
    };
    let policy = GrantStoreEvaluator::from_grants(vec![PolicyDecision::allow(
        CAPABILITY_RUNNER_USE,
        bundle.principal.clone(),
        runner_ref,
        "test container grant",
    )]);
    let result = runner.run(&bundle, &policy).expect("docker job should run");
    assert_eq!(result.status, RunStatus::Succeeded);
    assert!(
        result.jobs[0].stdout.contains("hello-container"),
        "stdout was: {}",
        result.jobs[0].stdout
    );
}

#[test]
fn local_and_container_runners_share_policy_surface() {
    let local = LocalProcessRunner::default_local();
    let container = ContainerRunner::new(ContainerEngine::Docker, "alpine:3.20").unwrap();
    assert_ne!(local.capabilities().id, container.capabilities().id);
}

fn sample_bundle(id: &str, command: &str) -> JobBundle {
    JobBundle::single(
        id,
        Job::shell(
            "job_container",
            sorrel_runners::Shell::Sh,
            command,
            None::<PathBuf>,
        ),
    )
}

fn assert_policy_error(error: RunnerError, capability: &str, status: PolicyDecisionStatus) {
    match error {
        RunnerError::PolicyDenied {
            capability: actual_capability,
            status: actual_status,
            ..
        } => {
            assert_eq!(actual_capability, capability);
            assert_eq!(actual_status, status);
        }
        other => panic!("expected policy error, got {other:?}"),
    }
}
