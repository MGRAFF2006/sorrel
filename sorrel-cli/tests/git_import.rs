//! Integration tests for `sorrel git import`.

use assert_cmd::Command;
use serde_json::Value;
use std::path::Path;
use std::process::Command as StdCommand;
use tempfile::TempDir;

fn git(cwd: &Path, args: &[&str]) {
    let status = StdCommand::new("git")
        .args(args)
        .current_dir(cwd)
        .env("GIT_AUTHOR_NAME", "Importer")
        .env("GIT_AUTHOR_EMAIL", "importer@example.com")
        .env("GIT_COMMITTER_NAME", "Importer")
        .env("GIT_COMMITTER_EMAIL", "importer@example.com")
        .status()
        .expect("spawn git");
    assert!(status.success(), "git {args:?} failed");
}

fn make_git_repo() -> TempDir {
    let dir = TempDir::new().expect("tempdir");
    let root = dir.path();
    git(root, &["init"]);
    git(root, &["config", "user.email", "importer@example.com"]);
    git(root, &["config", "user.name", "Importer"]);

    std::fs::write(root.join("readme.txt"), b"hello\n").unwrap();
    git(root, &["add", "readme.txt"]);
    git(root, &["commit", "-m", "add readme"]);

    std::fs::write(root.join("readme.txt"), b"hello world\n").unwrap();
    std::fs::write(root.join("note.txt"), b"note\n").unwrap();
    git(root, &["add", "readme.txt", "note.txt"]);
    git(root, &["commit", "-m", "update readme"]);

    dir
}

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
fn git_import_creates_workspace_and_log_history() {
    let git_dir = make_git_repo();
    let root = git_dir.path();

    let imported = command_json(root, &["git", "import", "--json"]);
    assert_eq!(imported["command"], "git import");
    assert_eq!(imported["status"], "imported");
    assert_eq!(imported["importedCommits"], 2);
    assert_eq!(imported["createdWorkspace"], true);
    assert!(root.join(".sorrel").is_dir());
    assert!(root.join(".sorrel/git-map.json").is_file());

    let log = command_json(root, &["log", "--json"]);
    let entries = log["entries"].as_array().expect("entries");
    assert!(entries.len() >= 2);

    let status = command_json(root, &["status", "--json"]);
    assert_eq!(status["worktree"]["dirty"], false);

    let readme = std::fs::read_to_string(root.join("readme.txt")).expect("readme");
    assert_eq!(readme, "hello world\n");
    assert!(root.join("note.txt").is_file());
}

#[test]
fn git_import_refuses_second_import_without_force() {
    let git_dir = make_git_repo();
    let root = git_dir.path();
    command_json(root, &["git", "import", "--json"]);

    Command::cargo_bin("sorrel")
        .expect("sorrel binary")
        .current_dir(root)
        .args(["git", "import", "--json"])
        .assert()
        .failure();

    let again = command_json(root, &["git", "import", "--force", "--json"]);
    assert_eq!(again["status"], "imported");
}

#[test]
fn git_import_respects_limit() {
    let git_dir = make_git_repo();
    let root = git_dir.path();
    let imported = command_json(root, &["git", "import", "--limit", "1", "--json"]);
    assert_eq!(imported["importedCommits"], 1);
    assert_eq!(imported["commits"][0]["message"], "update readme");
}
