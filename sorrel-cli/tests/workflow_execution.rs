use std::{fs, path::Path};

use assert_cmd::Command;
use serde_json::Value;
use tempfile::TempDir;

fn run(root: &Path, args: &[&str]) -> (std::process::Output, Value) {
    let output = Command::cargo_bin("sorrel")
        .unwrap()
        .current_dir(root)
        .args(args)
        .output()
        .unwrap();
    let json = serde_json::from_slice(&output.stdout).unwrap();
    (output, json)
}

#[test]
fn named_workflow_runs_only_selected_job_dependencies_and_literal_environment() {
    let root = TempDir::new().unwrap();
    fs::write(
        root.path().join("sorrel.workflow.yml"),
        r#"
version: 1
workflows:
  build:
    jobs:
      prepare:
        command: printf prepared > ready.txt
      test:
        command: test -f ready.txt && printf '%s' "$MODE"
        needs: [prepare]
        env:
          MODE: production
      unrelated:
        command: touch should-not-run
  other:
    jobs:
      test:
        command: exit 9
"#,
    )
    .unwrap();
    let (ambiguous, json) = run(root.path(), &["workflow", "validate", "--json"]);
    assert!(!ambiguous.status.success());
    assert!(json["error"]["message"]
        .as_str()
        .unwrap()
        .contains("--workflow"));
    let (output, json) = run(
        root.path(),
        &["workflow", "run", "test", "--workflow", "build", "--json"],
    );
    assert!(output.status.success());
    assert_eq!(json["workflow"]["id"], "build");
    assert_eq!(json["job"]["stdout"], "production");
    assert!(root.path().join("ready.txt").exists());
    assert!(!root.path().join("should-not-run").exists());
}

#[test]
fn failed_dependency_returns_child_exit_code_and_does_not_execute_dependent() {
    let root = TempDir::new().unwrap();
    fs::write(root.path().join("sorrel.workflow.yml"), "version: 1\njobs:\n  prepare:\n    command: exit 7\n  test:\n    command: touch should-not-run\n    needs: [prepare]\n").unwrap();
    let (output, json) = run(root.path(), &["workflow", "run", "test", "--json"]);
    assert_eq!(output.status.code(), Some(7));
    assert_eq!(json["status"], "failed");
    assert_eq!(json["job"]["exitCode"], 7);
    assert!(!root.path().join("should-not-run").exists());
}

#[test]
fn invalid_dependencies_and_missing_jobs_produce_json_and_nonzero_exit() {
    let root = TempDir::new().unwrap();
    for jobs in ["  test:\n    command: echo test\n    needs: [missing]\n", "  test:\n    command: echo test\n    needs: [other]\n  other:\n    command: echo other\n    needs: [test]\n"] {
        fs::write(root.path().join("sorrel.workflow.yml"), format!("version: 1\njobs:\n{jobs}")).unwrap();
        let (output, json) = run(root.path(), &["workflow", "validate", "--json"]);
        assert!(!output.status.success());
        assert_eq!(json["status"], "invalid");
    }
    fs::write(
        root.path().join("sorrel.workflow.yml"),
        "jobs:\n  test:\n    command: echo test\n",
    )
    .unwrap();
    let (output, json) = run(root.path(), &["workflow", "run", "missing", "--json"]);
    assert!(!output.status.success());
    assert_eq!(json["error"]["availableJobs"], serde_json::json!(["test"]));
}

#[test]
fn policy_and_log_storage_failures_are_visible_and_never_report_success() {
    let root = TempDir::new().unwrap();
    assert!(run(root.path(), &["init", "--json"]).0.status.success());
    fs::write(
        root.path().join("sorrel.workflow.yml"),
        "jobs:\n  test:\n    command: touch executed\n",
    )
    .unwrap();
    fs::write(root.path().join(".sorrel/grants"), "not a directory").unwrap();
    let (output, json) = run(root.path(), &["workflow", "run", "test", "--json"]);
    assert!(!output.status.success());
    assert_eq!(json["error"]["kind"], "policy_load_failed");
    assert!(!root.path().join("executed").exists());
    fs::remove_file(root.path().join(".sorrel/grants")).unwrap();
    fs::write(root.path().join(".sorrel/runs"), "not a directory").unwrap();
    let (output, json) = run(root.path(), &["workflow", "run", "test", "--json"]);
    assert!(!output.status.success());
    assert_eq!(json["error"]["kind"], "run_log_failed");
    assert_eq!(json["job"]["exitCode"], 0);
    assert!(root.path().join("executed").exists());
}
