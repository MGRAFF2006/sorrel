//! Integration tests for `sorrel git sync` (colocated Git mirror).

use assert_cmd::Command;
use serde_json::Value;
use std::path::Path;
use tempfile::TempDir;

fn sorrel_json(cwd: &Path, args: &[&str]) -> Value {
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

fn git(cwd: &Path, args: &[&str]) {
    let status = std::process::Command::new("git")
        .args(args)
        .current_dir(cwd)
        .env("GIT_AUTHOR_NAME", "Mirror")
        .env("GIT_AUTHOR_EMAIL", "mirror@example.com")
        .env("GIT_COMMITTER_NAME", "Mirror")
        .env("GIT_COMMITTER_EMAIL", "mirror@example.com")
        .status()
        .expect("spawn git");
    assert!(status.success(), "git {args:?} failed");
}

fn git_stdout(cwd: &Path, args: &[&str]) -> String {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .expect("spawn git");
    assert!(output.status.success(), "git {args:?} failed");
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// Sets up a colocated workspace: one Sorrel change exported into `./.git`.
fn colocated_workspace(root: &Path) {
    sorrel_json(root, &["init", "--json"]);
    std::fs::write(root.join("a.txt"), b"one\n").unwrap();
    sorrel_json(root, &["change", "create", "-m", "first", "--json"]);
    sorrel_json(root, &["git", "export", ".", "--branch", "main", "--json"]);
}

#[test]
fn sync_pulls_new_git_commits_and_fast_forwards() {
    let temp = TempDir::new().expect("temp");
    let root = temp.path();
    colocated_workspace(root);

    std::fs::write(root.join("b.txt"), b"bee\n").unwrap();
    git(root, &["add", "b.txt"]);
    git(root, &["commit", "-m", "git side"]);

    let synced = sorrel_json(root, &["git", "sync", "--json"]);
    assert_eq!(synced["command"], "git sync");
    assert_eq!(synced["status"], "pulled");
    assert_eq!(synced["importedCommits"], 1);
    assert_eq!(synced["commits"][0]["message"], "git side");

    // HEAD advanced to the imported snapshot and the worktree kept both files.
    let status = sorrel_json(root, &["status", "--json"]);
    assert_eq!(status["worktree"]["dirty"], false);
    assert!(root.join("a.txt").is_file());
    assert!(root.join("b.txt").is_file());

    // A second sync is a no-op.
    let again = sorrel_json(root, &["git", "sync", "--json"]);
    assert_eq!(again["status"], "up-to-date");
}

#[test]
fn sync_pushes_new_snapshots_to_git() {
    let temp = TempDir::new().expect("temp");
    let root = temp.path();
    colocated_workspace(root);

    std::fs::write(root.join("c.txt"), b"sea\n").unwrap();
    sorrel_json(root, &["change", "create", "-m", "sorrel side", "--json"]);

    let synced = sorrel_json(root, &["git", "sync", "--json"]);
    assert_eq!(synced["status"], "pushed");
    assert!(synced["createdCommits"].as_u64().unwrap() >= 1);

    let log = git_stdout(root, &["log", "--oneline", "main"]);
    assert!(log.contains("sorrel side"), "git log missing commit: {log}");

    // The colocated index was refreshed: nothing tracked is modified/staged.
    let porcelain = git_stdout(root, &["status", "--porcelain"]);
    let tracked_changes: Vec<&str> = porcelain
        .lines()
        .filter(|line| !line.starts_with("??"))
        .collect();
    assert!(
        tracked_changes.is_empty(),
        "unexpected tracked changes after push: {tracked_changes:?}"
    );

    let again = sorrel_json(root, &["git", "sync", "--json"]);
    assert_eq!(again["status"], "up-to-date");
}

#[test]
fn sync_pulls_into_fresh_workspace() {
    let temp = TempDir::new().expect("temp");
    let root = temp.path();
    git(root, &["init"]);
    std::fs::write(root.join("a.txt"), b"one\n").unwrap();
    git(root, &["add", "a.txt"]);
    git(root, &["commit", "-m", "seeded in git"]);
    // `git init` may pick a non-main default branch; normalize.
    git(root, &["branch", "-M", "main"]);

    sorrel_json(root, &["init", "--json"]);
    let synced = sorrel_json(root, &["git", "sync", "--json"]);
    assert_eq!(synced["status"], "pulled");
    assert_eq!(synced["importedCommits"], 1);
    assert!(root.join("a.txt").is_file());

    let status = sorrel_json(root, &["status", "--json"]);
    assert_eq!(status["worktree"]["dirty"], false);
}

#[test]
fn sync_diverged_parks_lane_then_merge_and_push() {
    let temp = TempDir::new().expect("temp");
    let root = temp.path();
    sorrel_json(root, &["init", "--json"]);
    std::fs::write(root.join("a.txt"), b"one\n").unwrap();
    sorrel_json(root, &["change", "create", "-m", "first", "--json"]);

    // Mirror lives outside the workspace so snapshots do not capture it.
    let mirror_temp = TempDir::new().expect("mirror temp");
    let mirror = mirror_temp.path().join("mirror");
    sorrel_json(
        root,
        &[
            "git",
            "export",
            mirror.to_str().unwrap(),
            "--branch",
            "main",
            "--json",
        ],
    );

    // Git side gains a commit…
    std::fs::write(mirror.join("b.txt"), b"bee\n").unwrap();
    git(&mirror, &["add", "b.txt"]);
    git(&mirror, &["commit", "-m", "git side"]);

    // …and the Sorrel side gains an independent change.
    std::fs::write(root.join("c.txt"), b"sea\n").unwrap();
    sorrel_json(root, &["change", "create", "-m", "sorrel side", "--json"]);

    let synced = sorrel_json(root, &["git", "sync", mirror.to_str().unwrap(), "--json"]);
    assert_eq!(synced["status"], "diverged");
    assert_eq!(synced["importedCommits"], 1);
    assert_eq!(synced["lane"]["name"], "git/main");
    let lane_id = synced["lane"]["id"].as_str().expect("lane id").to_owned();

    // The parked lane merges like any other lane (paths do not conflict).
    let merged = sorrel_json(root, &["merge", &lane_id, "--json"]);
    assert_eq!(merged["status"], "merged");
    assert!(root.join("b.txt").is_file());
    assert!(root.join("c.txt").is_file());

    // The next sync pushes the merge result back to Git.
    let pushed = sorrel_json(root, &["git", "sync", mirror.to_str().unwrap(), "--json"]);
    assert_eq!(pushed["status"], "pushed");
    let log = git_stdout(&mirror, &["log", "--oneline", "main"]);
    assert!(log.contains("sorrel side"), "git log missing commit: {log}");
    assert!(log.contains("git side"), "git log missing commit: {log}");
}

#[test]
fn sync_refuses_dirty_worktree_on_pull() {
    let temp = TempDir::new().expect("temp");
    let root = temp.path();
    sorrel_json(root, &["init", "--json"]);
    std::fs::write(root.join("a.txt"), b"one\n").unwrap();
    sorrel_json(root, &["change", "create", "-m", "first", "--json"]);

    let mirror_temp = TempDir::new().expect("mirror temp");
    let mirror = mirror_temp.path().join("mirror");
    sorrel_json(
        root,
        &[
            "git",
            "export",
            mirror.to_str().unwrap(),
            "--branch",
            "main",
            "--json",
        ],
    );

    std::fs::write(mirror.join("b.txt"), b"bee\n").unwrap();
    git(&mirror, &["add", "b.txt"]);
    git(&mirror, &["commit", "-m", "git side"]);

    // Uncommitted local edit → pull must refuse without --force.
    std::fs::write(root.join("a.txt"), b"edited\n").unwrap();
    let output = Command::cargo_bin("sorrel")
        .expect("sorrel binary")
        .current_dir(root)
        .args(["git", "sync", mirror.to_str().unwrap(), "--json"])
        .output()
        .expect("run sorrel");
    assert!(
        !output.status.success(),
        "sync should refuse a dirty worktree"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("uncommitted changes"),
        "unexpected error: {stderr}"
    );
}
