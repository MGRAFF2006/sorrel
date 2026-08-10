use assert_cmd::Command;
use serde_json::{json, Value};
use std::fs;
use std::path::Path;
use tempfile::TempDir;

const FIXTURE_WORKFLOW: &str = include_str!("fixtures/sorrel.workflow.yml");

fn write_workflow(dir: &Path, contents: &str) {
    fs::write(dir.join("sorrel.workflow.yml"), contents).expect("workflow file writes");
}

#[test]
fn workflow_validate_supports_json_output() {
    let temp_dir = TempDir::new().expect("temp dir is available");
    write_workflow(temp_dir.path(), FIXTURE_WORKFLOW);

    let output = command_json(temp_dir.path(), &["workflow", "validate", "--json"]);

    assert_eq!(output["command"], "workflow validate");
    assert_eq!(output["status"], "valid");
    assert_eq!(output["workflow"]["id"], "workflow_validate_protocol");
    assert_eq!(output["workflow"]["version"], 1);
    assert_eq!(output["workflow"]["jobs"][0]["name"], "test");
    assert_eq!(output["workflow"]["jobs"][0]["command"], "echo workflow-ok");
}

#[test]
fn workflow_validate_reports_missing_workflow_file() {
    let temp_dir = TempDir::new().expect("temp dir is available");

    let output = command_json(temp_dir.path(), &["workflow", "validate", "--json"]);
    assert_eq!(output["status"], "not_found");
    assert_eq!(output["error"]["kind"], "workflow_file_not_found");
}

#[test]
fn workflow_validate_reports_parse_errors() {
    let temp_dir = TempDir::new().expect("temp dir is available");
    write_workflow(temp_dir.path(), "version: 1\njobs: {}\n");

    let output = command_json(temp_dir.path(), &["workflow", "validate", "--json"]);
    assert_eq!(output["status"], "invalid");
    assert_eq!(output["error"]["kind"], "workflow_invalid_document");
}

#[test]
fn workflow_run_supports_json_output() {
    let temp_dir = TempDir::new().expect("temp dir is available");
    write_workflow(temp_dir.path(), FIXTURE_WORKFLOW);

    let output = command_json(temp_dir.path(), &["workflow", "run", "test", "--json"]);
    assert_eq!(output["command"], "workflow run");
    assert_eq!(output["status"], "completed");
    assert_eq!(output["workflow"]["id"], "workflow_validate_protocol");
    assert_eq!(output["job"]["name"], "test");
    assert!(output["job"]["stdout"]
        .as_str()
        .expect("stdout is a string")
        .contains("workflow-ok"));
    assert_eq!(output["bundle"]["runnerId"], "runner_local_process");
    assert_eq!(output["bundle"]["secretRefs"], json!([]));
}

#[test]
fn workflow_run_reports_missing_job() {
    let temp_dir = TempDir::new().expect("temp dir is available");
    write_workflow(temp_dir.path(), FIXTURE_WORKFLOW);

    let output = command_json(temp_dir.path(), &["workflow", "run", "missing", "--json"]);
    assert_eq!(output["status"], "not_found");
    assert_eq!(output["error"]["kind"], "job_not_found");
    assert_eq!(output["error"]["availableJobs"], json!(["test"]));
}

#[test]
fn workflow_run_denies_execution_without_grants() {
    let temp_dir = TempDir::new().expect("temp dir is available");
    write_workflow(temp_dir.path(), FIXTURE_WORKFLOW);

    let mut command = Command::cargo_bin("sorrel").expect("sorrel binary is available");
    command
        .current_dir(temp_dir.path())
        .env("SORREL_WORKFLOW_POLICY", "restrictive")
        .args(["workflow", "run", "test", "--json"]);

    let assert = command.assert().success();
    let output: Value =
        serde_json::from_slice(&assert.get_output().stdout).expect("workflow denial emits json");

    assert_eq!(output["status"], "denied");
    assert_eq!(output["decision"]["action"], "workflow.run");
    assert_eq!(output["decision"]["result"], "deny");
}

#[test]
fn workflow_run_preserves_secret_refs_without_values() {
    let temp_dir = TempDir::new().expect("temp dir is available");
    let workflow = r#"
version: 1
id: workflow_with_secrets
jobs:
  test:
    command: echo secret-test
    secrets:
      - secret_npm_token_dev
"#;
    write_workflow(temp_dir.path(), workflow);

    let output = command_json(temp_dir.path(), &["workflow", "validate", "--json"]);
    assert_eq!(
        output["workflow"]["jobs"][0]["secretRefs"],
        json!(["secret_npm_token_dev"])
    );
}

fn command_json(cwd: &Path, args: &[&str]) -> Value {
    let mut command = Command::cargo_bin("sorrel").expect("sorrel binary is available");
    command.current_dir(cwd).args(args);

    let assert = command.assert().success();
    serde_json::from_slice(&assert.get_output().stdout).expect("command emits json")
}
