//! Integration tests for `sorrel git export` and `sorrel stack`.

use assert_cmd::Command;
use serde_json::Value;
use std::path::Path;
use tempfile::TempDir;

fn command_json(cwd: &Path, args: &[&str]) -> Value {
    let output = Command::cargo_bin("sorrel")
        .expect("sorrel binary")
        .current_dir(cwd)
        .args(args)
        .output()
        .expect("run sorrel");
    assert!(
        output.status.success(),
        "sorrel {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("json stdout")
}

#[test]
fn git_export_writes_branch_and_map() {
    let temp = TempDir::new().expect("temp");
    let root = temp.path();
    command_json(root, &["init", "--json"]);
    std::fs::write(root.join("a.txt"), b"one\n").unwrap();
    command_json(root, &["change", "create", "-m", "first", "--json"]);
    std::fs::write(root.join("a.txt"), b"two\n").unwrap();
    command_json(root, &["change", "create", "-m", "second", "--json"]);

    let dest = root.join("exported.git");
    let exported = command_json(
        root,
        &[
            "git",
            "export",
            dest.to_str().unwrap(),
            "--branch",
            "export-main",
            "--json",
        ],
    );
    assert_eq!(exported["command"], "git export");
    assert_eq!(exported["status"], "exported");
    assert!(exported["createdCommits"].as_u64().unwrap() >= 2);
    assert_eq!(exported["branch"], "export-main");
    assert!(root.join(".sorrel/git-map.json").is_file());
    assert!(dest.join(".git").is_dir() || dest.join("HEAD").is_file());

    // Re-export should reuse map (0 new commits ideally, or at least succeed).
    let again = command_json(
        root,
        &[
            "git",
            "export",
            dest.to_str().unwrap(),
            "--branch",
            "export-main",
            "--json",
        ],
    );
    assert_eq!(again["status"], "exported");
    assert_eq!(again["createdCommits"], 0);
}

#[test]
fn stack_create_list_show() {
    let temp = TempDir::new().expect("temp");
    let root = temp.path();
    command_json(root, &["init", "--json"]);
    std::fs::write(root.join("x.txt"), b"x\n").unwrap();
    command_json(root, &["change", "create", "-m", "add x", "--json"]);

    let created = command_json(
        root,
        &["stack", "create", "--name", "stack/feature", "--json"],
    );
    assert_eq!(created["command"], "stack create");
    assert_eq!(created["status"], "created");
    let id = created["object"]["id"].as_str().expect("id").to_owned();

    let listed = command_json(root, &["stack", "list", "--json"]);
    assert_eq!(listed["count"], 1);

    let shown = command_json(root, &["stack", "show", &id, "--json"]);
    assert_eq!(shown["object"]["name"], "stack/feature");
    assert_eq!(shown["object"]["id"], id);
}
