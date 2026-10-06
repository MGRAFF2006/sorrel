#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use sorrel_runners::workflow::WorkflowFile;
use sorrel_runners::{
    CAPABILITY_RUNNER_USE, CAPABILITY_WORKFLOW_RUN, ContainerEngine, ContainerRunner,
    GrantStoreEvaluator, JobBundle, ObjectRef, PolicyDecision, RunStatus, Runner,
};

#[test]
fn container_workflows_stop_after_failed_prerequisites() {
    if let Some(directory) = std::env::var_os("SORREL_FAKE_CONTAINER_DIR") {
        check_bundles(Path::new(&directory));
        return;
    }

    // Isolate PATH in a child test process; other tests may use the real engine.
    let directory = std::env::temp_dir().join(format!(
        "sorrel-container-test-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&directory).unwrap();
    for engine in ["docker", "podman"] {
        let executable = directory.join(engine);
        fs::write(
            &executable,
            "#!/bin/sh\nfor command do :; done\nprintf '%s\\n' \"$command\" >> \"$SORREL_FAKE_CONTAINER_DIR/invocations\"\nif [ \"$command\" = fail ]; then exit 7; fi\nprintf completed\n",
        )
        .unwrap();
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o755)).unwrap();
    }
    let output = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "container_workflows_stop_after_failed_prerequisites",
            "--nocapture",
        ])
        .env("PATH", &directory)
        .env("SORREL_FAKE_CONTAINER_DIR", &directory)
        .output()
        .unwrap();
    fs::remove_dir_all(directory).unwrap();
    assert!(
        output.status.success(),
        "child test failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn check_bundles(directory: &Path) {
    for engine in [ContainerEngine::Docker, ContainerEngine::Podman] {
        let runner = ContainerRunner::new(engine, "fixture-image").unwrap();
        for (first_command, workflow, expected_jobs) in
            [("fail", true, 1), ("fail", false, 2), ("pass", true, 2)]
        {
            let mut bundle = WorkflowFile::from_yaml(&format!(
                "version: 1\nworkflows:\n  ci:\n    jobs:\n      build:\n        command: {first_command}\n      deploy:\n        command: deploy\n        needs: [build]\n"
            ))
            .unwrap()
            .to_bundle("ci")
            .unwrap();
            if !workflow {
                bundle.workflow = None;
                bundle.principal.workflow = None;
                bundle
                    .required_capabilities
                    .retain(|capability| capability != CAPABILITY_WORKFLOW_RUN);
            }
            let invocations = directory.join("invocations");
            fs::write(&invocations, "").unwrap();
            let result = runner
                .run(&bundle, &allow_bundle(&runner, &bundle))
                .unwrap();
            assert_eq!(result.jobs.len(), expected_jobs);
            assert_eq!(result.jobs[0].job_id, "build");
            assert_eq!(
                result.status,
                if first_command == "fail" {
                    RunStatus::Failed
                } else {
                    RunStatus::Succeeded
                }
            );
            assert_eq!(
                result.jobs[0].exit_code,
                Some(if first_command == "fail" { 7 } else { 0 })
            );
            let expected_invocations = if expected_jobs == 1 {
                format!("{first_command}\n")
            } else {
                assert_eq!(result.jobs[1].job_id, "deploy");
                assert_eq!(result.jobs[1].status, RunStatus::Succeeded);
                format!("{first_command}\ndeploy\n")
            };
            assert_eq!(
                fs::read_to_string(invocations).unwrap(),
                expected_invocations
            );
        }
    }
}

fn allow_bundle(runner: &ContainerRunner, bundle: &JobBundle) -> GrantStoreEvaluator {
    let mut grants = vec![PolicyDecision::allow(
        CAPABILITY_RUNNER_USE,
        bundle.principal.clone(),
        ObjectRef::new("Runner", runner.capabilities().id.clone()),
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
