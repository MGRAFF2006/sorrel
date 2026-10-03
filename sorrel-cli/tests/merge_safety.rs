use assert_cmd::Command;
use serde_json::Value;
use std::path::Path;
use tempfile::TempDir;

fn run(root: &Path, args: &[&str]) -> Value {
    let output = Command::cargo_bin("sorrel")
        .unwrap()
        .current_dir(root)
        .args(args)
        .arg("--json")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn conflicted_merge() -> (TempDir, String) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    run(root, &["init"]);
    std::fs::create_dir(root.join("notes")).unwrap();
    for (name, bytes) in [
        ("notes/tracked.txt", "baseline notes\n"),
        ("conflict.txt", "base\n"),
        ("modified.txt", "base\n"),
        ("deleted.txt", "base\n"),
        ("disjoint.txt", "first\nsecond\nthird\nfourth\n"),
    ] {
        std::fs::write(root.join(name), bytes).unwrap();
    }
    run(root, &["change", "create", "-m", "base"]);
    let lane = run(root, &["lane", "create", "--name", "feature"])["object"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    std::fs::write(root.join("conflict.txt"), "ours\n").unwrap();
    std::fs::write(root.join("disjoint.txt"), "FIRST\nsecond\nthird\nfourth\n").unwrap();
    run(root, &["change", "create", "-m", "ours"]);
    run(root, &["lane", "switch", &lane]);
    std::fs::write(root.join("conflict.txt"), "theirs\n").unwrap();
    std::fs::write(root.join("added.txt"), "theirs added\n").unwrap();
    std::fs::write(root.join("modified.txt"), "theirs modified\n").unwrap();
    std::fs::remove_file(root.join("deleted.txt")).unwrap();
    std::fs::write(root.join("disjoint.txt"), "first\nsecond\nthird\nFOURTH\n").unwrap();
    run(root, &["change", "create", "-m", "theirs"]);
    run(root, &["lane", "switch", "lane_main"]);
    Command::cargo_bin("sorrel")
        .unwrap()
        .current_dir(root)
        .args(["merge", &lane])
        .assert()
        .failure();
    assert!(root.join(".sorrel/MERGE_STATE").is_file());
    (dir, lane)
}

fn assert_clean_changes(root: &Path) {
    assert_eq!(
        std::fs::read_to_string(root.join("added.txt")).unwrap(),
        "theirs added\n"
    );
    assert_eq!(
        std::fs::read_to_string(root.join("modified.txt")).unwrap(),
        "theirs modified\n"
    );
    assert!(!root.join("deleted.txt").exists());
    assert_eq!(
        std::fs::read_to_string(root.join("disjoint.txt")).unwrap(),
        "FIRST\nsecond\nthird\nFOURTH\n"
    );
}

#[test]
fn conflicted_merge_retains_all_clean_changes_when_continued() {
    let (dir, _) = conflicted_merge();
    let root = dir.path();
    assert_clean_changes(root);
    std::fs::write(root.join("conflict.txt"), "resolved\n").unwrap();
    let merged = run(root, &["merge", "--continue"]);
    assert_eq!(merged["continued"], true);
    assert_clean_changes(root);
    assert_eq!(run(root, &["status"])["worktree"]["dirty"], false);
}

#[test]
fn abort_restores_clean_changes_and_conflict_to_ours() {
    let (dir, _) = conflicted_merge();
    let root = dir.path();
    std::fs::write(root.join("scratch.txt"), "unrelated new work\n").unwrap();
    std::fs::write(root.join("notes/resolution.txt"), "keep these notes\n").unwrap();
    run(root, &["merge", "--abort"]);
    assert_eq!(
        std::fs::read_to_string(root.join("scratch.txt")).unwrap(),
        "unrelated new work\n"
    );
    assert_eq!(
        std::fs::read_to_string(root.join("notes/resolution.txt")).unwrap(),
        "keep these notes\n"
    );
    assert!(!root.join("added.txt").exists());
    assert_eq!(
        std::fs::read_to_string(root.join("modified.txt")).unwrap(),
        "base\n"
    );
    assert_eq!(
        std::fs::read_to_string(root.join("deleted.txt")).unwrap(),
        "base\n"
    );
    assert_eq!(
        std::fs::read_to_string(root.join("conflict.txt")).unwrap(),
        "ours\n"
    );
    assert_eq!(
        std::fs::read_to_string(root.join("disjoint.txt")).unwrap(),
        "FIRST\nsecond\nthird\nfourth\n"
    );
    assert_eq!(run(root, &["status"])["worktree"]["dirty"], true);
}
