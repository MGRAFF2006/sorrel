use assert_cmd::Command;
use serde_json::Value;
use sorrel_cli::repo::WorkspaceLock;
use std::fs;

#[test]
fn competing_command_fails_without_changing_workspace_and_can_retry() {
    let workspace = tempfile::tempdir().unwrap();
    let root = workspace.path().join(".sorrel");
    let guard = WorkspaceLock::acquire(&root).unwrap();
    let output = Command::cargo_bin("sorrel")
        .unwrap()
        .current_dir(workspace.path())
        .arg("init")
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("workspace is busy"));
    assert!(!root.join("manifest.json").exists());
    drop(guard);
    Command::cargo_bin("sorrel")
        .unwrap()
        .current_dir(workspace.path())
        .arg("init")
        .assert()
        .success();
}

#[test]
fn failed_change_index_write_does_not_advance_either_head() {
    let workspace = tempfile::tempdir().unwrap();
    Command::cargo_bin("sorrel")
        .unwrap()
        .current_dir(workspace.path())
        .arg("init")
        .assert()
        .success();
    let root = workspace.path().join(".sorrel");
    let head = fs::read(root.join("HEAD")).unwrap();
    let value: Value = serde_json::from_slice(&head).unwrap();
    let lane_path = root.join("heads").join(value["lane"].as_str().unwrap());
    let lane_head = fs::read(&lane_path).unwrap();
    fs::write(workspace.path().join("new.txt"), "new content").unwrap();
    fs::create_dir(root.join("changes.index")).unwrap();
    Command::cargo_bin("sorrel")
        .unwrap()
        .current_dir(workspace.path())
        .args(["change", "create", "-m", "new"])
        .assert()
        .failure();
    assert_eq!(fs::read(root.join("HEAD")).unwrap(), head);
    assert_eq!(fs::read(lane_path).unwrap(), lane_head);
    assert!(!root.join("metadata-transaction.json").exists());
}
