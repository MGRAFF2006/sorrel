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

fn git(cwd: &Path, args: &[&str]) -> String {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(cwd)
        .env("GIT_AUTHOR_NAME", "Exporter")
        .env("GIT_AUTHOR_EMAIL", "exporter@example.com")
        .env("GIT_COMMITTER_NAME", "Exporter")
        .env("GIT_COMMITTER_EMAIL", "exporter@example.com")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

fn export_fixture() -> (TempDir, TempDir) {
    let source = TempDir::new().unwrap();
    let dest = TempDir::new().unwrap();
    command_json(source.path(), &["init", "--json"]);
    std::fs::write(source.path().join("a.txt"), b"one\n").unwrap();
    command_json(
        source.path(),
        &["change", "create", "-m", "first", "--json"],
    );
    command_json(
        source.path(),
        &["git", "export", dest.path().to_str().unwrap(), "--json"],
    );
    (source, dest)
}

fn assert_export_refused(source: &Path, dest: &Path) {
    let before = git(dest, &["rev-parse", "refs/heads/main"]);
    let mapping = std::fs::read(source.join(".sorrel/git-map.json")).unwrap();
    let output = Command::cargo_bin("sorrel")
        .unwrap()
        .current_dir(source)
        .args(["git", "export", dest.to_str().unwrap(), "--json"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("not a fast-forward"));
    assert_eq!(git(dest, &["rev-parse", "refs/heads/main"]), before);
    assert_eq!(
        std::fs::read(source.join(".sorrel/git-map.json")).unwrap(),
        mapping
    );
}

#[test]
fn git_export_checks_actual_tip_after_git_only_commits() {
    let (source, dest) = export_fixture();
    git(
        dest.path(),
        &["commit", "--allow-empty", "-m", "Git-only work"],
    );
    let git_only_tip = git(dest.path(), &["rev-parse", "HEAD"]);
    std::fs::write(source.path().join("a.txt"), b"Sorrel work\n").unwrap();
    command_json(
        source.path(),
        &["change", "create", "-m", "Sorrel work", "--json"],
    );
    assert_export_refused(source.path(), dest.path());

    let forced = command_json(
        source.path(),
        &[
            "git",
            "export",
            dest.path().to_str().unwrap(),
            "--force",
            "--json",
        ],
    );
    let exported = forced["headGitSha"].as_str().unwrap();
    assert_ne!(exported, git_only_tip);
    assert_eq!(
        git(dest.path(), &["rev-parse", "refs/heads/main"]),
        exported
    );
    assert_eq!(
        git(dest.path(), &["show", &format!("{exported}:a.txt")]),
        "Sorrel work"
    );
}

#[test]
fn git_export_refuses_unrelated_destination_despite_stale_map() {
    let (source, _previous_dest) = export_fixture();
    let dest = TempDir::new().unwrap();
    git(dest.path(), &["init", "--initial-branch=main"]);
    std::fs::write(dest.path().join("other.txt"), b"unrelated work\n").unwrap();
    git(dest.path(), &["add", "other.txt"]);
    git(dest.path(), &["commit", "-m", "unrelated"]);
    assert_export_refused(source.path(), dest.path());
    assert_eq!(
        std::fs::read(dest.path().join("other.txt")).unwrap(),
        b"unrelated work\n"
    );
}

#[test]
fn git_export_allows_real_fast_forward_with_mapping() {
    let (source, dest) = export_fixture();
    let old = git(dest.path(), &["rev-parse", "refs/heads/main"]);
    std::fs::write(source.path().join("a.txt"), b"two\n").unwrap();
    command_json(
        source.path(),
        &["change", "create", "-m", "second", "--json"],
    );
    let result = command_json(
        source.path(),
        &["git", "export", dest.path().to_str().unwrap(), "--json"],
    );
    let tip = git(dest.path(), &["rev-parse", "refs/heads/main"]);
    assert_ne!(old, tip);
    assert_eq!(result["headGitSha"], tip);
    git(dest.path(), &["merge-base", "--is-ancestor", &old, &tip]);
    assert_eq!(git(dest.path(), &["show", "main:a.txt"]), "two");
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
