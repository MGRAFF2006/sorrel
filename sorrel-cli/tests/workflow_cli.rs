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

#[test]
fn secret_sync_set_get_and_workflow_inject() {
    let temp_dir = TempDir::new().expect("temp dir is available");
    let root = temp_dir.path();

    command_json(root, &["init", "--json"]);

    fs::write(
        root.join("sorrel.secrets.yml"),
        r#"
schemaVersion: sorrel.vault.v0
kind: SecretSpec
secretRefs:
  - id: secret_demo_token
    name: DEMO_TOKEN
    provider: dotenv
    uri: dotenv:.env
    environment: dev
    required: true
    description: Demo token for CLI tests
"#,
    )
    .expect("secrets yaml writes");

    let sync = command_json(root, &["secret", "sync", "--json"]);
    assert_eq!(sync["command"], "secret sync");
    assert_eq!(sync["count"], 1);
    assert!(root.join("secretspec.toml").is_file());

    command_json(
        root,
        &[
            "grant",
            "create",
            "--json",
            "--action",
            "secret.inject",
            "--secret",
            "secret_demo_token",
            "--agent",
            "agent_mock_cli",
            "--environment",
            "dev",
        ],
    );

    let set = command_json(
        root,
        &[
            "secret",
            "set",
            "secret_demo_token",
            "--value",
            "demo-secret-value",
            "--provider",
            "dotenv:.env",
            "--json",
        ],
    );
    assert_eq!(set["stored"], true);
    assert!(root.join(".env").is_file());

    let get = command_json(
        root,
        &[
            "secret",
            "get",
            "secret_demo_token",
            "--reveal",
            "--provider",
            "dotenv:.env",
            "--json",
        ],
    );
    assert_eq!(get["revealed"], true);
    assert_eq!(get["value"], "demo-secret-value");

    write_workflow(
        root,
        r#"
version: 1
id: workflow_secret_inject
jobs:
  test:
    command: printf '%s' "$DEMO_TOKEN"
    secrets:
      - secret_demo_token
"#,
    );

    let run = command_json(root, &["workflow", "run", "test", "--json"]);
    assert_eq!(run["status"], "completed");
    assert_eq!(run["backend"], "local-fallback");
    assert_eq!(run["job"]["injectedSecrets"], json!(["DEMO_TOKEN"]));
    // Value must be redacted from captured logs.
    assert_ne!(run["job"]["stdout"], "demo-secret-value");
    assert!(run["job"]["stdout"]
        .as_str()
        .unwrap_or_default()
        .contains("<sorrel:redacted secret_demo_token>"));

    let runs = command_json(root, &["run", "list", "--json"]);
    assert_eq!(runs["command"], "run list");
    assert!(runs["count"].as_u64().unwrap_or(0) >= 1);
    let run_id = runs["runs"][0]["id"].as_str().expect("run id");
    let show = command_json(root, &["run", "show", run_id, "--json"]);
    assert_eq!(show["run"]["id"], run_id);
    let logs = command_json(root, &["run", "logs", run_id, "--json"]);
    assert_eq!(logs["command"], "run logs");
    assert!(!logs["events"].as_array().unwrap().is_empty());

    let follow = Command::cargo_bin("sorrel")
        .expect("sorrel binary is available")
        .current_dir(root)
        .args(["run", "logs", run_id, "--follow"])
        .output()
        .expect("follow command runs");
    assert!(!follow.status.success());
    assert!(String::from_utf8_lossy(&follow.stderr).contains("following is not implemented"));
}

#[test]
fn env_init_and_info_report_local_fallback() {
    let temp_dir = TempDir::new().expect("temp dir is available");
    let root = temp_dir.path();
    let init = command_json(root, &["env", "init", "--json"]);
    assert_eq!(init["command"], "env init");
    assert!(root.join("devenv.nix").is_file());
    assert!(root.join("devenv.yaml").is_file());
    let info = command_json(root, &["env", "info", "--json"]);
    assert_eq!(info["status"]["backend"], "local-fallback");
    let ensure = command_json(root, &["env", "ensure", "--json"]);
    assert_eq!(ensure["command"], "env ensure");
    assert_eq!(ensure["backend"], "local-fallback");
}
