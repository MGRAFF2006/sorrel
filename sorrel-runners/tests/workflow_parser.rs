use sorrel_runners::workflow::{WorkflowError, WorkflowFile};
use sorrel_runners::{
    CAPABILITY_RUNNER_USE, CAPABILITY_WORKFLOW_RUN, CommandSpec, GrantStoreEvaluator, JobBundle,
    LocalProcessRunner, ObjectRef, PolicyDecision, RunStatus, Runner, RunnerCapabilities,
    RunnerError, Shell,
};

#[test]
fn parses_multi_job_workflow_in_topological_order() {
    let source = r#"
version: 1
workflows:
  test:
    jobs:
      e2e:
        command: echo e2e
        needs: [unit, lint]
      unit:
        command: echo unit
        needs: [lint]
      lint:
        command: echo lint
"#;

    let file = WorkflowFile::from_yaml(source).expect("workflow parses");
    let bundle = file.to_bundle("test").expect("workflow converts");

    let order: Vec<&str> = bundle.jobs.iter().map(|job| job.id.as_str()).collect();
    assert_eq!(order, vec!["lint", "unit", "e2e"]);

    // Commands map to portable sh shell specs.
    assert!(matches!(
        &bundle.jobs[0].command,
        CommandSpec::Shell {
            shell: Shell::Sh,
            command,
        } if command == "echo lint"
    ));

    // The workflow resource is set and the bundle declares both capabilities.
    assert_eq!(bundle.workflow, Some(ObjectRef::new("Workflow", "test")));
    assert!(
        bundle
            .required_capabilities
            .contains(&CAPABILITY_RUNNER_USE.to_owned())
    );
    assert!(
        bundle
            .required_capabilities
            .contains(&CAPABILITY_WORKFLOW_RUN.to_owned())
    );

    bundle.validate().expect("converted bundle is valid");
}

#[test]
fn cyclic_needs_is_rejected() {
    let source = r#"
version: 1
workflows:
  loop:
    jobs:
      a:
        command: echo a
        needs: [b]
      b:
        command: echo b
        needs: [a]
"#;

    let file = WorkflowFile::from_yaml(source).expect("workflow parses");
    let error = file
        .to_bundle("loop")
        .expect_err("cyclic needs should be rejected");

    assert!(
        matches!(
            error,
            RunnerError::Workflow(WorkflowError::CyclicNeeds { .. })
        ),
        "expected cyclic needs error, got {error:?}"
    );
}

#[test]
fn unknown_needs_target_is_rejected() {
    let source = r#"
version: 1
workflows:
  test:
    jobs:
      unit:
        command: echo unit
        needs: [does_not_exist]
"#;

    let file = WorkflowFile::from_yaml(source).expect("workflow parses");
    let error = file
        .to_bundle("test")
        .expect_err("unknown needs target should be rejected");

    assert!(
        matches!(
            error,
            RunnerError::Workflow(WorkflowError::UnknownNeeds { ref needed, .. })
                if needed == "does_not_exist"
        ),
        "expected unknown needs error, got {error:?}"
    );
}

#[test]
fn duplicate_job_reference_is_rejected() {
    let source = r#"
version: 1
workflows:
  test:
    jobs:
      lint:
        command: echo lint
      unit:
        command: echo unit
        needs: [lint, lint]
"#;

    let file = WorkflowFile::from_yaml(source).expect("workflow parses");
    let error = file
        .to_bundle("test")
        .expect_err("duplicate job reference should be rejected");

    assert!(
        matches!(
            error,
            RunnerError::Workflow(WorkflowError::DuplicateJob { ref job, .. })
                if job == "lint"
        ),
        "expected duplicate job error, got {error:?}"
    );
}

#[test]
fn empty_command_is_rejected() {
    let source = r#"
version: 1
workflows:
  test:
    jobs:
      unit:
        command: "   "
"#;

    let file = WorkflowFile::from_yaml(source).expect("workflow parses");
    let error = file
        .to_bundle("test")
        .expect_err("empty command should be rejected");

    assert!(
        matches!(
            error,
            RunnerError::Workflow(WorkflowError::EmptyCommand(ref job)) if job == "unit"
        ),
        "expected empty command error, got {error:?}"
    );
}

#[test]
fn empty_workflow_file_is_rejected() {
    let source = "version: 1\nworkflows: {}\n";
    let error = WorkflowFile::from_yaml(source).expect_err("empty file should be rejected");

    assert!(
        matches!(error, RunnerError::Workflow(WorkflowError::EmptyFile)),
        "expected empty file error, got {error:?}"
    );
}

#[test]
fn unknown_workflow_name_is_rejected() {
    let source = r#"
version: 1
workflows:
  test:
    jobs:
      unit:
        command: echo unit
"#;

    let file = WorkflowFile::from_yaml(source).expect("workflow parses");
    let error = file
        .to_bundle("missing")
        .expect_err("unknown workflow name should be rejected");

    assert!(
        matches!(
            error,
            RunnerError::Workflow(WorkflowError::UnknownWorkflow(ref name)) if name == "missing"
        ),
        "expected unknown workflow error, got {error:?}"
    );
}

#[test]
fn invalid_yaml_is_rejected() {
    let source = "version: 1\nworkflows:\n  test: [not a map\n";
    let error = WorkflowFile::from_yaml(source).expect_err("invalid yaml should be rejected");

    assert!(
        matches!(error, RunnerError::WorkflowParse(_)),
        "expected workflow parse error, got {error:?}"
    );
}

#[test]
fn parsed_workflow_runs_through_local_process_runner() {
    let source = r#"
version: 1
workflows:
  ci:
    jobs:
      build:
        command: printf 'built'
      check:
        command: printf 'checked'
        needs: [build]
"#;

    let file = WorkflowFile::from_yaml(source).expect("workflow parses");
    let bundle = file.to_bundle("ci").expect("workflow converts");

    let runner = LocalProcessRunner::default_local();
    let policy = allow_all(runner.capabilities(), &bundle);

    let result = runner.run(&bundle, &policy).expect("workflow should run");

    assert_eq!(result.status, RunStatus::Succeeded);
    assert_eq!(result.jobs.len(), 2);
    assert_eq!(result.jobs[0].job_id, "build");
    assert_eq!(result.jobs[0].stdout, "built");
    assert_eq!(result.jobs[1].job_id, "check");
    assert_eq!(result.jobs[1].stdout, "checked");
}

/// Builds an allow-all evaluator granting both runner.use and workflow.run for
/// the bundle's principal.
fn allow_all(capabilities: &RunnerCapabilities, bundle: &JobBundle) -> GrantStoreEvaluator {
    let runner = ObjectRef::new("Runner", capabilities.id.clone());
    let mut grants = vec![PolicyDecision::allow(
        CAPABILITY_RUNNER_USE,
        bundle.principal.clone(),
        runner,
        "test runner grant",
    )];
    if let Some(workflow) = &bundle.workflow {
        grants.push(PolicyDecision::allow(
            CAPABILITY_WORKFLOW_RUN,
            bundle.principal.clone(),
            workflow.clone(),
            "test workflow grant",
        ));
    }
    GrantStoreEvaluator::from_grants(grants)
}
