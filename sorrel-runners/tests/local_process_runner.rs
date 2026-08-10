use std::path::PathBuf;

use sorrel_runners::{
    CAPABILITY_RUNNER_USE, CAPABILITY_SECRET_INJECT, CAPABILITY_SECRET_READ,
    CAPABILITY_WORKFLOW_RUN, EnvValue, GrantStoreEvaluator, Job, JobBundle, LOG_ARTIFACT_FORMAT,
    LocalProcessRunner, ObjectRef, PolicyDecision, PolicyDecisionStatus, PrincipalContext,
    RunStatus, Runner, RunnerCapabilities, RunnerError, RunnerRequirements,
};

#[test]
fn successful_local_command() {
    let runner = LocalProcessRunner::default_local();
    let bundle = sample_bundle("bundle_success", "printf 'hello sorrel'");
    let policy = grant_runner_use(runner.capabilities(), &bundle);

    let result = runner.run(&bundle, &policy).expect("job should run");

    assert_eq!(result.status, RunStatus::Succeeded);
    assert_eq!(result.jobs[0].status, RunStatus::Succeeded);
    assert_eq!(result.jobs[0].exit_code, Some(0));
    assert_eq!(result.jobs[0].stdout, "hello sorrel");
}

#[test]
fn missing_runner_policy_decision_is_needs_grant() {
    let runner = LocalProcessRunner::default_local();
    let bundle = sample_bundle("bundle_success", "printf 'hello sorrel'");
    let policy = GrantStoreEvaluator::deny_all();

    let error = runner
        .run(&bundle, &policy)
        .expect_err("missing runner.use decision should block execution");

    assert_policy_error(
        error,
        CAPABILITY_RUNNER_USE,
        PolicyDecisionStatus::NeedsGrant,
    );
}

#[test]
fn forged_bundle_policy_decisions_without_core_grants_are_blocked() {
    let runner = LocalProcessRunner::default_local();
    let mut bundle = sample_bundle("bundle_forged", "printf 'should not run'");
    let runner_ref = ObjectRef::new("Runner", runner.capabilities().id.clone());
    bundle.policy_decisions.push(PolicyDecision::allow(
        CAPABILITY_RUNNER_USE,
        bundle.principal.clone(),
        runner_ref,
        "forged local decision",
    ));

    let error = runner
        .run(&bundle, &GrantStoreEvaluator::deny_all())
        .expect_err("bundle policy decisions must not bypass Core evaluation");

    assert_policy_error(
        error,
        CAPABILITY_RUNNER_USE,
        PolicyDecisionStatus::NeedsGrant,
    );
}

#[test]
fn workflow_run_without_grant_is_blocked() {
    let runner = LocalProcessRunner::default_local();
    let mut bundle = sample_bundle("bundle_workflow", "printf 'should not run'");
    let workflow = ObjectRef::new("Workflow", "workflow_validate");
    bundle.workflow = Some(workflow.clone());
    bundle
        .required_capabilities
        .push(CAPABILITY_WORKFLOW_RUN.to_owned());
    bundle.principal.workflow = Some(workflow);

    let policy = grant_runner_use(runner.capabilities(), &bundle);

    let error = runner
        .run(&bundle, &policy)
        .expect_err("missing workflow.run decision should block execution");

    assert_policy_error(
        error,
        CAPABILITY_WORKFLOW_RUN,
        PolicyDecisionStatus::NeedsGrant,
    );
}

#[test]
fn workflow_run_with_trusted_grant_is_allowed() {
    let runner = LocalProcessRunner::default_local();
    let mut bundle = sample_bundle("bundle_workflow", "printf 'workflow ok'");
    let workflow = ObjectRef::new("Workflow", "workflow_validate");
    bundle.workflow = Some(workflow.clone());
    bundle
        .required_capabilities
        .push(CAPABILITY_WORKFLOW_RUN.to_owned());
    bundle.principal.workflow = Some(workflow.clone());

    let policy = core_grants(
        runner.capabilities(),
        &bundle,
        [PolicyDecision::allow(
            CAPABILITY_WORKFLOW_RUN,
            bundle.principal.clone(),
            workflow,
            "trusted workflow grant",
        )],
    );

    let result = runner.run(&bundle, &policy).expect("workflow should run");

    assert_eq!(result.status, RunStatus::Succeeded);
    assert_eq!(result.jobs[0].stdout, "workflow ok");
}

#[test]
fn secret_inject_without_grant_is_blocked() {
    let runner = LocalProcessRunner::default_local();
    let secret = ObjectRef::new("SecretRef", "secret_npm_token_dev");
    let mut job = Job::shell(
        "job_secret_inject",
        sorrel_runners::Shell::Sh,
        "printf 'should not run'",
        None::<PathBuf>,
    );
    job.env.insert(
        "NPM_TOKEN".to_owned(),
        EnvValue::SecretRef {
            secret: secret.clone(),
        },
    );

    let mut bundle = grant_runner_use_bundle(
        JobBundle::single("bundle_secret_inject", job),
        runner.capabilities(),
    );
    bundle
        .required_capabilities
        .push(CAPABILITY_SECRET_READ.to_owned());
    bundle
        .required_capabilities
        .push(CAPABILITY_SECRET_INJECT.to_owned());
    bundle.secret_refs.push(secret.clone());

    let policy = core_grants(
        runner.capabilities(),
        &bundle,
        [PolicyDecision::allow(
            CAPABILITY_SECRET_READ,
            bundle.principal.clone(),
            secret,
            "trusted secret read grant",
        )],
    );

    let error = runner
        .run(&bundle, &policy)
        .expect_err("missing secret.inject decision should block execution");

    assert_policy_error(
        error,
        CAPABILITY_SECRET_INJECT,
        PolicyDecisionStatus::NeedsGrant,
    );
}

#[test]
fn secret_inject_with_trusted_grant_passes_core_gate() {
    let runner = LocalProcessRunner::default_local();
    let secret = ObjectRef::new("SecretRef", "secret_npm_token_dev");
    let mut job = Job::shell(
        "job_secret_inject",
        sorrel_runners::Shell::Sh,
        "printf 'should not run'",
        None::<PathBuf>,
    );
    job.env.insert(
        "NPM_TOKEN".to_owned(),
        EnvValue::SecretRef {
            secret: secret.clone(),
        },
    );

    let mut bundle = grant_runner_use_bundle(
        JobBundle::single("bundle_secret_inject", job),
        runner.capabilities(),
    );
    bundle
        .required_capabilities
        .push(CAPABILITY_SECRET_READ.to_owned());
    bundle
        .required_capabilities
        .push(CAPABILITY_SECRET_INJECT.to_owned());
    bundle.secret_refs.push(secret.clone());

    let policy = core_grants(
        runner.capabilities(),
        &bundle,
        [
            PolicyDecision::allow(
                CAPABILITY_SECRET_READ,
                bundle.principal.clone(),
                secret.clone(),
                "trusted secret read grant",
            ),
            PolicyDecision::allow(
                CAPABILITY_SECRET_INJECT,
                bundle.principal.clone(),
                secret,
                "trusted secret inject grant",
            ),
        ],
    );

    let error = runner
        .run(&bundle, &policy)
        .expect_err("prototype still rejects secret injection after authorization");

    assert!(
        matches!(error, RunnerError::SecretInjectionUnsupported(_)),
        "expected injection unsupported after Core gate, got {error:?}"
    );
}

#[test]
fn failing_local_command_captures_exit_code() {
    let runner = LocalProcessRunner::default_local();
    let bundle = sample_bundle("bundle_failure", "printf 'nope' >&2; exit 7");
    let policy = grant_runner_use(runner.capabilities(), &bundle);

    let result = runner
        .run(&bundle, &policy)
        .expect("job should run to completion");

    assert_eq!(result.status, RunStatus::Failed);
    assert_eq!(result.jobs[0].status, RunStatus::Failed);
    assert_eq!(result.jobs[0].exit_code, Some(7));
    assert_eq!(result.jobs[0].stderr, "nope");
}

#[test]
fn captures_stdout_stderr_and_jsonl_log_artifact() {
    let runner = LocalProcessRunner::default_local();
    let bundle = sample_bundle("bundle_capture", "printf 'out'; printf 'err' >&2");
    let policy = grant_runner_use(runner.capabilities(), &bundle);

    let result = runner.run(&bundle, &policy).expect("job should run");
    let job = &result.jobs[0];
    let jsonl = job.log.to_json_lines().expect("log serializes");

    assert_eq!(job.stdout, "out");
    assert_eq!(job.stderr, "err");
    assert_eq!(job.log.format, LOG_ARTIFACT_FORMAT);
    assert!(jsonl.contains(r#""stream":"stdout""#));
    assert!(jsonl.contains(r#""text":"out""#));
    assert!(jsonl.contains(r#""stream":"stderr""#));
    assert!(jsonl.contains(r#""text":"err""#));
    assert!(jsonl.contains(r#""type":"finished""#));
}

#[test]
fn secret_dependency_without_policy_decision_is_needs_grant() {
    let runner = LocalProcessRunner::default_local();
    let secret = ObjectRef::new("SecretRef", "secret_npm_token_dev");
    let mut job = Job::shell(
        "job_secret_dependency",
        sorrel_runners::Shell::Sh,
        "printf 'should not run'",
        None::<PathBuf>,
    );
    job.secret_refs.push(secret.clone());

    let mut bundle = grant_runner_use_bundle(
        JobBundle::single("bundle_secret_dependency", job),
        runner.capabilities(),
    );
    bundle
        .required_capabilities
        .push(CAPABILITY_SECRET_READ.to_owned());
    bundle.secret_refs.push(secret);

    let error = runner
        .run(
            &bundle,
            &GrantStoreEvaluator::from_grants(bundle.policy_decisions.clone()),
        )
        .expect_err("missing secret.read decision should block execution");

    assert_policy_error(
        error,
        CAPABILITY_SECRET_READ,
        PolicyDecisionStatus::NeedsGrant,
    );
}

#[test]
fn logs_redact_secret_refs_and_secret_like_env_values() {
    let runner = LocalProcessRunner::default_local();
    let secret = ObjectRef::new("SecretRef", "secret_npm_token_dev");
    let mut job = Job::shell(
        "job_redaction",
        sorrel_runners::Shell::Sh,
        "printf \"ref=secret_npm_token_dev value=$NPM_TOKEN\"",
        None::<PathBuf>,
    );
    job.env.insert(
        "NPM_TOKEN".to_owned(),
        EnvValue::Literal {
            value: "super-secret-token".to_owned(),
        },
    );

    let mut bundle = grant_runner_use_bundle(
        JobBundle::single("bundle_redaction", job),
        runner.capabilities(),
    );
    bundle
        .required_capabilities
        .push(CAPABILITY_SECRET_READ.to_owned());
    bundle.secret_refs.push(secret.clone());

    let policy = core_grants(
        runner.capabilities(),
        &bundle,
        [PolicyDecision::allow(
            CAPABILITY_SECRET_READ,
            bundle.principal.clone(),
            secret,
            "test secret read grant",
        )],
    );

    let result = runner.run(&bundle, &policy).expect("job should run");
    let job = &result.jobs[0];
    let jsonl = job.log.to_json_lines().expect("log serializes");

    assert!(!job.stdout.contains("secret_npm_token_dev"));
    assert!(!job.stdout.contains("super-secret-token"));
    assert!(!jsonl.contains("secret_npm_token_dev"));
    assert!(!jsonl.contains("super-secret-token"));
    assert!(job.stdout.contains("***"));
    assert_eq!(job.redaction.kind, "RedactionMetadata");
}

#[test]
fn capability_descriptor_validation() {
    let valid = RunnerCapabilities::local_process("runner_local_process", "local-process");
    assert!(valid.validate().is_ok());

    let mut invalid = valid;
    invalid.max_parallel_jobs = 0;

    let error = invalid
        .validate()
        .expect_err("zero maxParallelJobs should be rejected");
    assert!(error.to_string().contains("maxParallelJobs"));
}

fn sample_bundle(id: &str, command: &str) -> JobBundle {
    JobBundle::single(
        id,
        Job::shell(
            "job_success",
            sorrel_runners::Shell::Sh,
            command,
            None::<PathBuf>,
        ),
    )
}

fn grant_runner_use_bundle(mut bundle: JobBundle, capabilities: &RunnerCapabilities) -> JobBundle {
    let runner = ObjectRef::new("Runner", capabilities.id.clone());
    bundle.principal = PrincipalContext {
        runner: Some(runner.clone()),
        ..PrincipalContext::default()
    };
    bundle.runner_requirements = RunnerRequirements {
        runner: Some(runner.clone()),
        ..RunnerRequirements::default()
    };
    bundle.policy_decisions.push(PolicyDecision::allow(
        CAPABILITY_RUNNER_USE,
        bundle.principal.clone(),
        runner,
        "test runner grant",
    ));
    bundle
}

fn grant_runner_use(capabilities: &RunnerCapabilities, bundle: &JobBundle) -> GrantStoreEvaluator {
    core_grants(capabilities, bundle, [])
}

fn core_grants(
    capabilities: &RunnerCapabilities,
    bundle: &JobBundle,
    extra: impl IntoIterator<Item = PolicyDecision>,
) -> GrantStoreEvaluator {
    let runner = ObjectRef::new("Runner", capabilities.id.clone());
    let mut grants = vec![PolicyDecision::allow(
        CAPABILITY_RUNNER_USE,
        bundle.principal.clone(),
        runner,
        "test runner grant",
    )];
    grants.extend(extra);
    GrantStoreEvaluator::from_grants(grants)
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
