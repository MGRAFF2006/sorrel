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
    assert_eq!(synced["commits"][0]["message"], "git side\n");

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
    let error: Value = serde_json::from_slice(&output.stdout).expect("structured sync failure");
    assert_eq!(error["schemaVersion"], "sorrel.cli.v1");
    assert_eq!(error["status"], "error");
    assert_eq!(error["error"]["code"], "dirty_worktree");
    assert!(error["error"]["message"]
        .as_str()
        .unwrap()
        .contains("uncommitted changes"));
}

#[test]
fn sync_refuses_staged_git_index_when_pushing_native_history() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    git(root, &["init"]);
    std::fs::write(root.join("native.txt"), b"base native\n").unwrap();
    std::fs::write(root.join("staged.txt"), b"base Git\n").unwrap();
    git(root, &["add", "native.txt", "staged.txt"]);
    git(root, &["commit", "-m", "Git baseline"]);
    git(root, &["branch", "-M", "main"]);
    sorrel_json(root, &["git", "import", "--json"]);

    std::fs::write(root.join("native.txt"), b"recorded in Sorrel\n").unwrap();
    sorrel_json(
        root,
        &["change", "create", "-m", "native history ahead", "--json"],
    );
    // Keep this separate from the recorded native edit: Git's staged content
    // has not entered Sorrel history and must survive a mirror push refusal.
    std::fs::write(root.join("staged.txt"), b"uncommitted staged Git work\n").unwrap();
    git(root, &["add", "staged.txt"]);
    let index_before = git_stdout(root, &["write-tree"]);
    let branch_before = git_stdout(root, &["rev-parse", "refs/heads/main"]);
    let head_before = std::fs::read(root.join(".sorrel/HEAD")).unwrap();

    let output = Command::cargo_bin("sorrel")
        .unwrap()
        .current_dir(root)
        .args(["git", "sync", "--json"])
        .output()
        .unwrap();
    assert!(
        !output.status.success(),
        "mirror push must preserve staged Git work"
    );
    let error: Value =
        serde_json::from_slice(&output.stdout).expect("structured index guard failure");
    assert_eq!(error["schemaVersion"], "sorrel.cli.v1");
    assert_eq!(error["status"], "error");
    assert_eq!(error["error"]["code"], "dirty_git_index");
    assert!(error["error"]["message"]
        .as_str()
        .unwrap()
        .contains("staged"));
    assert_eq!(git_stdout(root, &["write-tree"]), index_before);
    assert_eq!(
        git_stdout(root, &["rev-parse", "refs/heads/main"]),
        branch_before
    );
    assert_eq!(
        std::fs::read(root.join(".sorrel/HEAD")).unwrap(),
        head_before
    );
    assert_eq!(
        std::fs::read(root.join("staged.txt")).unwrap(),
        b"uncommitted staged Git work\n"
    );
    assert_eq!(
        std::fs::read(root.join("native.txt")).unwrap(),
        b"recorded in Sorrel\n"
    );
}

#[test]
fn external_sync_updates_worktree_index_and_preserves_untracked_files() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    sorrel_json(root, &["init", "--json"]);
    std::fs::write(root.join("a.txt"), "one\n").unwrap();
    std::fs::write(root.join("deleted.txt"), "remove me\n").unwrap();
    sorrel_json(root, &["change", "create", "-m", "first", "--json"]);
    let mirror_dir = TempDir::new().unwrap();
    let mirror = mirror_dir.path().join("mirror");
    sorrel_json(root, &["git", "export", mirror.to_str().unwrap(), "--json"]);
    std::fs::write(mirror.join("scratch.txt"), "untracked notes\n").unwrap();
    std::fs::write(root.join("a.txt"), "two\n").unwrap();
    std::fs::remove_file(root.join("deleted.txt")).unwrap();
    sorrel_json(root, &["change", "create", "-m", "second", "--json"]);
    // A metadata-directory alias must still be recognized as an external mirror.
    let mirror_git = mirror.join(".git");
    let pushed = sorrel_json(
        root,
        &["git", "sync", mirror_git.to_str().unwrap(), "--json"],
    );
    assert_eq!(pushed["status"], "pushed");
    assert_eq!(
        std::fs::read_to_string(mirror.join("a.txt")).unwrap(),
        "two\n"
    );
    assert!(!mirror.join("deleted.txt").exists());
    assert_eq!(
        std::fs::read_to_string(mirror.join("scratch.txt")).unwrap(),
        "untracked notes\n"
    );
    assert_eq!(
        git_stdout(&mirror, &["status", "--porcelain"]),
        "?? scratch.txt\n"
    );
    assert_eq!(
        sorrel_json(root, &["git", "sync", mirror.to_str().unwrap(), "--json"])["status"],
        "up-to-date"
    );

    std::fs::write(root.join("a.txt"), "three\n").unwrap();
    sorrel_json(root, &["change", "create", "-m", "third", "--json"]);
    sorrel_json(root, &["git", "export", mirror.to_str().unwrap(), "--json"]);
    assert_eq!(
        std::fs::read_to_string(mirror.join("a.txt")).unwrap(),
        "three\n"
    );
    assert_eq!(
        git_stdout(&mirror, &["status", "--porcelain"]),
        "?? scratch.txt\n"
    );
}

#[test]
fn external_sync_refuses_dirty_checkout_without_advancing_either_head() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    sorrel_json(root, &["init", "--json"]);
    std::fs::write(root.join("a.txt"), "one\n").unwrap();
    sorrel_json(root, &["change", "create", "-m", "first", "--json"]);
    let mirror_dir = TempDir::new().unwrap();
    let mirror = mirror_dir.path().join("mirror");
    sorrel_json(root, &["git", "export", mirror.to_str().unwrap(), "--json"]);
    std::fs::write(root.join("a.txt"), "two\n").unwrap();
    sorrel_json(root, &["change", "create", "-m", "second", "--json"]);
    std::fs::write(mirror.join("a.txt"), "unfinished mirror edit\n").unwrap();
    let branch_before = git_stdout(&mirror, &["rev-parse", "HEAD"]);
    let index_before = std::fs::read(mirror.join(".git/index")).unwrap();
    let owner_before = std::fs::read(root.join(".sorrel/HEAD")).unwrap();
    Command::cargo_bin("sorrel")
        .unwrap()
        .current_dir(root)
        .args(["git", "sync", mirror.to_str().unwrap(), "--json"])
        .assert()
        .failure();
    assert_eq!(git_stdout(&mirror, &["rev-parse", "HEAD"]), branch_before);
    assert_eq!(
        std::fs::read(mirror.join(".git/index")).unwrap(),
        index_before
    );
    assert_eq!(
        std::fs::read(root.join(".sorrel/HEAD")).unwrap(),
        owner_before
    );
    assert_eq!(
        std::fs::read_to_string(mirror.join("a.txt")).unwrap(),
        "unfinished mirror edit\n"
    );
}

#[test]
fn colocated_metadata_alias_sync_keeps_native_work_and_refreshes_git_index() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    colocated_workspace(root);
    std::fs::write(root.join("a.txt"), "two\n").unwrap();
    sorrel_json(root, &["change", "create", "-m", "second", "--json"]);
    assert_eq!(
        sorrel_json(root, &["git", "sync", ".git", "--json"])["status"],
        "pushed"
    );
    assert_eq!(
        std::fs::read_to_string(root.join("a.txt")).unwrap(),
        "two\n"
    );
    assert!(git_stdout(root, &["diff", "--name-only"]).is_empty());
    assert!(git_stdout(root, &["diff", "--cached", "--name-only"]).is_empty());
}
