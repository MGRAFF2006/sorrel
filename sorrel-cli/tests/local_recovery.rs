use assert_cmd::Command;
use serde_json::{json, Value};
use std::{fs, path::Path};

fn sorrel(root: &Path, args: &[&str]) -> Value {
    let output = Command::cargo_bin("sorrel")
        .unwrap()
        .current_dir(root)
        .args(args)
        .arg("--json")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn fails(root: &Path, args: &[&str], message: &str) {
    let head = fs::read(root.join(".sorrel/HEAD")).unwrap();
    let history = sorrel(root, &["operation", "log"]);
    let output = Command::cargo_bin("sorrel")
        .unwrap()
        .current_dir(root)
        .args(args)
        .arg("--json")
        .output()
        .unwrap();
    assert!(!output.status.success(), "{args:?}");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains(message),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stdout.is_empty());
    assert_eq!(fs::read(root.join(".sorrel/HEAD")).unwrap(), head);
    assert_eq!(sorrel(root, &["operation", "log"]), history);
}

fn record(root: &Path, text: &str) -> Value {
    fs::write(root.join("file.txt"), text).unwrap();
    sorrel(root, &["change", "create", "-m", text])
}

#[test]
fn recorded_checkpoint_and_undo_roundtrip_retains_later_history_and_lanes() {
    let workspace = tempfile::tempdir().unwrap();
    let root = workspace.path();
    sorrel(root, &["init"]);
    let first = record(root, "first");
    let checkpoint = sorrel(root, &["checkpoint", "create", "before"]);
    assert_eq!(
        checkpoint["checkpoint"]["head"]["snapshot"],
        first["object"]["resultingSnapshot"]["id"]
    );
    let second = record(root, "second");
    let lane = sorrel(root, &["lane", "create", "--name", "later"])["object"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let index = fs::read(root.join(".sorrel/changes.index")).unwrap();
    let lane_head = fs::read(root.join(".sorrel/heads").join(&lane)).unwrap();
    let undo = sorrel(root, &["undo"]);
    assert_eq!(undo["command"], "undo");
    assert_eq!(undo["status"], "restored");
    assert_eq!(
        undo["before"]["snapshot"],
        second["object"]["resultingSnapshot"]["id"]
    );
    assert_eq!(undo["after"], checkpoint["checkpoint"]["head"]);
    assert_eq!(fs::read(root.join("file.txt")).unwrap(), b"first");
    assert_eq!(fs::read(root.join(".sorrel/changes.index")).unwrap(), index);
    assert_eq!(
        fs::read(root.join(".sorrel/heads").join(&lane)).unwrap(),
        lane_head
    );
    sorrel(root, &["undo"]); // Undo is a new transition, so this restores second.
    assert_eq!(fs::read(root.join("file.txt")).unwrap(), b"second");
    sorrel(root, &["checkpoint", "restore", "before"]);
    assert_eq!(fs::read(root.join("file.txt")).unwrap(), b"first");
    let list = sorrel(root, &["checkpoint", "list"]);
    assert_eq!(list["count"], 1);
    assert_eq!(list["checkpoints"][0], checkpoint["checkpoint"]);
    let log = sorrel(root, &["operation", "log"]);
    assert_eq!(
        log["operations"][0]["operation"]["action"],
        "checkpoint restore"
    );
    assert_eq!(log["operations"][1]["operation"]["action"], "undo");
    assert_eq!(
        log["operations"][2]["operation"]["relatedOperation"],
        log["operations"][4]["id"]
    );
    assert_eq!(
        log["operations"].as_array().unwrap().last().unwrap()["operation"]["action"],
        "init"
    );
    let retained = second["object"]["resultingSnapshot"]["id"]
        .as_str()
        .unwrap();
    assert!(root
        .join(".sorrel/objects")
        .join(&retained[..2])
        .join(&retained[2..])
        .is_file());
}

#[test]
fn undo_lane_switch_restores_previous_lane_without_rewinding_incoming_head() {
    let workspace = tempfile::tempdir().unwrap();
    let root = workspace.path();
    sorrel(root, &["init"]);
    record(root, "main");
    let lane = sorrel(root, &["lane", "create", "--name", "incoming"])["object"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    sorrel(root, &["lane", "switch", &lane]);
    record(root, "incoming");
    sorrel(root, &["lane", "switch", "lane_main"]);
    let incoming = fs::read(root.join(".sorrel/heads").join(&lane)).unwrap();
    let result = sorrel(root, &["undo"]);
    assert_eq!(result["after"]["lane"], lane);
    assert_eq!(fs::read(root.join("file.txt")).unwrap(), b"incoming");
    assert_eq!(
        fs::read(root.join(".sorrel/heads").join(&lane)).unwrap(),
        incoming
    );
    assert_eq!(
        sorrel(root, &["operation", "log"])["operations"][1]["operation"]["action"],
        "lane switch"
    );
}

#[test]
fn guards_reject_dirty_pending_merge_collisions_and_invalid_names() {
    let workspace = tempfile::tempdir().unwrap();
    let root = workspace.path();
    sorrel(root, &["init"]);
    fails(root, &["undo"], "initialization");
    record(root, "first");
    sorrel(root, &["checkpoint", "create", "saved"]);
    for name in ["saved", "../outside", ".", "..", "bad/name"] {
        fails(
            root,
            &["checkpoint", "create", name],
            if name == "saved" {
                "already exists"
            } else {
                "invalid checkpoint"
            },
        );
    }
    fs::write(root.join("file.txt"), "unfinished").unwrap();
    for args in [
        &["undo"][..],
        &["checkpoint", "create", "dirty"],
        &["checkpoint", "restore", "saved"],
    ] {
        fails(root, args, "uncommitted changes");
    }
    assert_eq!(fs::read(root.join("file.txt")).unwrap(), b"unfinished");
    fs::write(root.join("file.txt"), "first").unwrap();
    fs::write(root.join(".sorrel/MERGE_STATE"), "malformed pending state").unwrap();
    for args in [
        &["undo"][..],
        &["checkpoint", "create", "pending"],
        &["checkpoint", "restore", "saved"],
    ] {
        fails(root, args, "pending merge");
    }
    fs::remove_file(root.join(".sorrel/MERGE_STATE")).unwrap();
    fails(
        root,
        &["checkpoint", "restore", "missing"],
        "does not exist",
    );
}

#[test]
fn restore_and_undo_preserve_ignored_destinations_and_preflight_metadata() {
    let workspace = tempfile::tempdir().unwrap();
    let root = workspace.path();
    sorrel(root, &["init"]);
    fs::write(root.join(".sorrelignore"), "data\n").unwrap();
    record(root, "first");
    sorrel(root, &["checkpoint", "create", "first"]);
    fs::write(root.join(".sorrelignore"), "").unwrap();
    fs::write(root.join("data"), "recorded").unwrap();
    record(root, "second");
    sorrel(root, &["checkpoint", "create", "second"]);
    sorrel(root, &["checkpoint", "restore", "first"]);
    fs::write(root.join("data"), "valuable ignored").unwrap();
    for args in [&["undo"][..], &["checkpoint", "restore", "second"]] {
        fails(root, args, "untracked or ignored");
        assert_eq!(fs::read(root.join("file.txt")).unwrap(), b"first");
        assert_eq!(fs::read(root.join("data")).unwrap(), b"valuable ignored");
    }
    fs::remove_file(root.join("data")).unwrap();
    fs::write(root.join(".env"), "ignored secret fixture").unwrap();
    sorrel(root, &["checkpoint", "restore", "second"]);
    assert_eq!(
        fs::read(root.join(".env")).unwrap(),
        b"ignored secret fixture"
    );
    // A broken publication destination must fail before restore changes files.
    fs::remove_file(root.join(".sorrel/heads/lane_main")).unwrap();
    fs::create_dir(root.join(".sorrel/heads/lane_main")).unwrap();
    Command::cargo_bin("sorrel")
        .unwrap()
        .current_dir(root)
        .args(["checkpoint", "restore", "first"])
        .assert()
        .failure();
    assert_eq!(fs::read(root.join("file.txt")).unwrap(), b"second");
}

#[test]
fn old_registries_are_optional_and_corrupt_history_fails_closed() {
    let workspace = tempfile::tempdir().unwrap();
    let root = workspace.path();
    sorrel(root, &["init"]);
    fs::remove_dir_all(root.join(".sorrel/operations")).unwrap();
    fs::remove_dir_all(root.join(".sorrel/heads")).unwrap();
    assert_eq!(sorrel(root, &["operation", "log"])["count"], 0);
    assert_eq!(sorrel(root, &["checkpoint", "list"])["count"], 0);
    record(root, "first");
    sorrel(root, &["undo"]);
    assert!(!root.join("file.txt").exists());
    let path = fs::read_dir(root.join(".sorrel/operations"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let mut record: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    record["action"] = json!("fabricated");
    fs::write(&path, serde_json::to_vec(&record).unwrap()).unwrap();
    for args in [&["operation", "log"][..], &["undo"]] {
        Command::cargo_bin("sorrel")
            .unwrap()
            .current_dir(root)
            .args(args)
            .assert()
            .failure();
    }
    assert!(!root.join("file.txt").exists());
}

#[test]
fn checkpoint_corruption_and_stale_operation_state_cannot_advance_head() {
    let workspace = tempfile::tempdir().unwrap();
    let root = workspace.path();
    sorrel(root, &["init"]);
    record(root, "first");
    sorrel(root, &["checkpoint", "create", "saved"]);
    record(root, "second");
    let cp = root.join(".sorrel/checkpoints/saved.json");
    let bytes = fs::read(&cp).unwrap();
    fs::write(&cp, b"{}").unwrap();
    fails(root, &["checkpoint", "restore", "saved"], "missing field");
    fs::write(cp, bytes).unwrap();
    let head = fs::read(root.join(".sorrel/HEAD")).unwrap();
    let old = sorrel(root, &["checkpoint", "list"])["checkpoints"][0]["head"]["snapshot"]
        .as_str()
        .unwrap()
        .to_owned();
    fs::write(
        root.join(".sorrel/heads/lane_main"),
        serde_json::to_vec(&json!({"snapshot": old})).unwrap(),
    )
    .unwrap();
    fails(root, &["undo"], "no longer matches");
    assert_eq!(fs::read(root.join(".sorrel/HEAD")).unwrap(), head);
    assert_eq!(fs::read(root.join("file.txt")).unwrap(), b"second");
}

#[test]
fn interrupted_operation_and_head_publication_recover_together() {
    let workspace = tempfile::tempdir().unwrap();
    let root = workspace.path();
    sorrel(root, &["init"]);
    record(root, "first");
    let before_head = fs::read(root.join(".sorrel/HEAD")).unwrap();
    let before_lane = fs::read(root.join(".sorrel/heads/lane_main")).unwrap();
    record(root, "second");
    let mut paths: Vec<_> = fs::read_dir(root.join(".sorrel/operations"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    paths.sort();
    let operation = paths.last().unwrap();
    let mut journal = Vec::new();
    for path in [
        operation.clone(),
        root.join(".sorrel/heads/lane_main"),
        root.join(".sorrel/HEAD"),
    ] {
        let bytes = fs::read(&path).unwrap();
        let temporary = path.parent().unwrap().join(".sorrel-recovery.tmp");
        fs::write(&temporary, &bytes).unwrap();
        journal.push(json!([
            path.strip_prefix(root.join(".sorrel")).unwrap(),
            temporary.strip_prefix(root.join(".sorrel")).unwrap(),
            sorrel_core::ObjectId::for_bytes(&bytes).to_hex()
        ]));
    }
    // Model a crash after the immutable operation rename but before refs.
    fs::remove_file(operation.parent().unwrap().join(".sorrel-recovery.tmp")).unwrap();
    fs::write(root.join(".sorrel/HEAD"), before_head).unwrap();
    fs::write(root.join(".sorrel/heads/lane_main"), before_lane).unwrap();
    fs::write(
        root.join(".sorrel/metadata-transaction.json"),
        serde_json::to_vec(&journal).unwrap(),
    )
    .unwrap();
    let status = sorrel(root, &["status"]);
    assert_eq!(status["worktree"]["dirty"], false);
    assert_eq!(sorrel(root, &["operation", "log"])["count"], 3);
    assert!(!root.join(".sorrel/metadata-transaction.json").exists());
    sorrel(root, &["undo"]);
    assert_eq!(fs::read(root.join("file.txt")).unwrap(), b"first");
}

#[cfg(unix)]
#[test]
fn operation_registry_symlinks_cannot_redirect_publication_or_recovery() {
    let workspace = tempfile::tempdir().unwrap();
    let root = workspace.path();
    let external = tempfile::tempdir().unwrap();
    sorrel(root, &["init"]);
    fs::remove_dir_all(root.join(".sorrel/operations")).unwrap();
    std::os::unix::fs::symlink(external.path(), root.join(".sorrel/operations")).unwrap();
    fs::write(root.join("file.txt"), "first").unwrap();
    let head = fs::read(root.join(".sorrel/HEAD")).unwrap();
    Command::cargo_bin("sorrel")
        .unwrap()
        .current_dir(root)
        .args(["change", "create", "-m", "first"])
        .assert()
        .failure();
    assert_eq!(fs::read(root.join(".sorrel/HEAD")).unwrap(), head);
    assert_eq!(fs::read_dir(external.path()).unwrap().count(), 0);
    fs::write(
        root.join(".sorrel/metadata-transaction.json"),
        serde_json::to_vec(&json!([[
            format!("operations/{:020}-{}.json", 1, "a".repeat(64)),
            "operations/.sorrel-test.tmp",
            "a".repeat(64)
        ]]))
        .unwrap(),
    )
    .unwrap();
    Command::cargo_bin("sorrel")
        .unwrap()
        .current_dir(root)
        .arg("status")
        .assert()
        .failure();
    assert_eq!(fs::read_dir(external.path()).unwrap().count(), 0);
}

#[test]
fn invalid_checkpoint_snapshot_paths_fail_before_worktree_changes() {
    use sorrel_core::{
        write_blob, write_snapshot, write_tree, EntryMode, EntryType, FileObjectStore, ObjectKind,
        ObjectRef, SnapshotOptions, TreeEntry,
    };
    let workspace = tempfile::tempdir().unwrap();
    let root = workspace.path();
    sorrel(root, &["init"]);
    record(root, "keep me");
    sorrel(root, &["checkpoint", "create", "invalid"]);
    let store = FileObjectStore::open_existing(root.join(".sorrel")).unwrap();
    let blob = write_blob(&store, b"incoming").unwrap();
    let child = write_tree(
        &store,
        vec![TreeEntry {
            name: "x.txt".into(),
            path: "outside.txt".into(),
            entry_type: EntryType::File,
            object: ObjectRef::new(ObjectKind::Blob, blob.id),
            mode: EntryMode::Normal,
            size: None,
            content_hash: None,
        }],
    )
    .unwrap();
    let tree = write_tree(
        &store,
        vec![TreeEntry {
            name: "a".into(),
            path: "a".into(),
            entry_type: EntryType::Directory,
            object: ObjectRef::new(ObjectKind::Tree, child.id),
            mode: EntryMode::Directory,
            size: None,
            content_hash: None,
        }],
    )
    .unwrap();
    let snapshot = write_snapshot(&store, tree.id, SnapshotOptions::new("repo_invalid")).unwrap();
    let path = root.join(".sorrel/checkpoints/invalid.json");
    let mut cp: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    cp["head"]["snapshot"] = json!(snapshot.id.to_hex());
    fs::write(path, serde_json::to_vec(&cp).unwrap()).unwrap();
    let head = fs::read(root.join(".sorrel/HEAD")).unwrap();
    let history = sorrel(root, &["operation", "log"]);
    Command::cargo_bin("sorrel")
        .unwrap()
        .current_dir(root)
        .args(["checkpoint", "restore", "invalid"])
        .assert()
        .failure();
    assert_eq!(fs::read(root.join("file.txt")).unwrap(), b"keep me");
    assert_eq!(fs::read(root.join(".sorrel/HEAD")).unwrap(), head);
    assert_eq!(sorrel(root, &["operation", "log"]), history);
    assert!(!root.join("a").exists());
    assert!(!root.join("outside.txt").exists());
}

fn corrupt_first_operation(root: &Path) {
    let path = fs::read_dir(root.join(".sorrel/operations"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    fs::write(path, b"{}").unwrap();
}

#[test]
fn corrupt_history_rejects_lane_switch_and_both_merge_paths_before_restore() {
    for divergent in [false, true] {
        let workspace = tempfile::tempdir().unwrap();
        let root = workspace.path();
        sorrel(root, &["init"]);
        record(root, "main");
        let lane = sorrel(root, &["lane", "create", "--name", "incoming"])["object"]["id"]
            .as_str()
            .unwrap()
            .to_owned();
        sorrel(root, &["lane", "switch", &lane]);
        record(root, "incoming");
        sorrel(root, &["lane", "switch", "lane_main"]);
        if divergent {
            fs::write(root.join("main.txt"), "main only").unwrap();
            sorrel(root, &["change", "create", "-m", "main only"]);
        }
        let head = fs::read(root.join(".sorrel/HEAD")).unwrap();
        let lane_head = fs::read(root.join(".sorrel/heads/lane_main")).unwrap();
        corrupt_first_operation(root);
        for args in [&["lane", "switch", &lane][..], &["merge", &lane]] {
            let output = Command::cargo_bin("sorrel")
                .unwrap()
                .current_dir(root)
                .args(args)
                .output()
                .unwrap();
            assert!(!output.status.success());
            assert!(String::from_utf8_lossy(&output.stderr).contains("missing field"));
            assert_eq!(fs::read(root.join("file.txt")).unwrap(), b"main");
            assert_eq!(fs::read(root.join(".sorrel/HEAD")).unwrap(), head);
            assert_eq!(
                fs::read(root.join(".sorrel/heads/lane_main")).unwrap(),
                lane_head
            );
            if divergent {
                assert_eq!(fs::read(root.join("main.txt")).unwrap(), b"main only");
            }
        }
    }
}

fn git(root: &Path, args: &[&str]) {
    let output = std::process::Command::new("git")
        .current_dir(root)
        .args(args)
        .env("GIT_AUTHOR_NAME", "humunkulud")
        .env("GIT_AUTHOR_EMAIL", "graff.mathis@gmail.com")
        .env("GIT_COMMITTER_NAME", "humunkulud")
        .env("GIT_COMMITTER_EMAIL", "graff.mathis@gmail.com")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn corrupt_history_rejects_git_import_and_sync_before_restore() {
    for import in [false, true] {
        let workspace = tempfile::tempdir().unwrap();
        let root = workspace.path();
        let mirror = tempfile::tempdir().unwrap();
        let git_root = mirror.path();
        sorrel(root, &["init"]);
        record(root, "first");
        if import {
            git(git_root, &["init", "--initial-branch", "main"]);
        } else {
            sorrel(
                root,
                &[
                    "git",
                    "export",
                    git_root.to_str().unwrap(),
                    "--branch",
                    "main",
                ],
            );
            git(git_root, &["checkout", "main"]);
        }
        fs::write(git_root.join("file.txt"), "incoming").unwrap();
        git(git_root, &["add", "file.txt"]);
        git(git_root, &["commit", "-m", "incoming"]);
        let head = fs::read(root.join(".sorrel/HEAD")).unwrap();
        let lane_head = fs::read(root.join(".sorrel/heads/lane_main")).unwrap();
        corrupt_first_operation(root);
        let args = if import {
            vec!["git", "import", git_root.to_str().unwrap(), "--force"]
        } else {
            vec![
                "git",
                "sync",
                git_root.to_str().unwrap(),
                "--branch",
                "main",
            ]
        };
        let output = Command::cargo_bin("sorrel")
            .unwrap()
            .current_dir(root)
            .args(args)
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("missing field"),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(fs::read(root.join("file.txt")).unwrap(), b"first");
        assert_eq!(fs::read(root.join(".sorrel/HEAD")).unwrap(), head);
        assert_eq!(
            fs::read(root.join(".sorrel/heads/lane_main")).unwrap(),
            lane_head
        );
    }
}

#[test]
fn recovery_rejects_inconsistent_operation_refs_before_any_publication() {
    for case in [
        "malformed-head",
        "wrong-head",
        "missing-head",
        "extra-head-field",
        "malformed-lane",
        "wrong-lane",
        "missing-lane",
        "extra-ref",
        "stale-current",
    ] {
        let workspace = tempfile::tempdir().unwrap();
        let root = workspace.path();
        sorrel(root, &["init"]);
        record(root, "first");
        let before_head = fs::read(root.join(".sorrel/HEAD")).unwrap();
        let before_lane = fs::read(root.join(".sorrel/heads/lane_main")).unwrap();
        record(root, "second");
        let mut paths: Vec<_> = fs::read_dir(root.join(".sorrel/operations"))
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect();
        paths.sort();
        let operation = paths.last().unwrap().clone();
        let mut journal = Vec::new();
        let mut staging = Vec::new();
        for path in [
            operation.clone(),
            root.join(".sorrel/heads/lane_main"),
            root.join(".sorrel/HEAD"),
        ] {
            let is_head = path.file_name().unwrap() == "HEAD";
            let is_lane = path.parent().unwrap().file_name().unwrap() == "heads";
            if (is_head && case == "missing-head") || (is_lane && case == "missing-lane") {
                continue;
            }
            let mut bytes = fs::read(&path).unwrap();
            if (is_head && case == "malformed-head") || (is_lane && case == "malformed-lane") {
                bytes = b"{}".to_vec();
            }
            if is_head && case == "wrong-head" {
                bytes = before_head.clone();
            }
            if is_lane && case == "wrong-lane" {
                bytes = before_lane.clone();
            }
            if is_head && case == "extra-head-field" {
                let mut value: Value = serde_json::from_slice(&bytes).unwrap();
                value["extra"] = json!(true);
                bytes = serde_json::to_vec(&value).unwrap();
            }
            let temporary = path.parent().unwrap().join(".sorrel-recovery.tmp");
            fs::write(&temporary, &bytes).unwrap();
            staging.push(temporary.clone());
            journal.push(json!([
                path.strip_prefix(root.join(".sorrel")).unwrap(),
                temporary.strip_prefix(root.join(".sorrel")).unwrap(),
                sorrel_core::ObjectId::for_bytes(&bytes).to_hex()
            ]));
        }
        if case == "extra-ref" {
            let bytes = fs::read(root.join(".sorrel/heads/lane_main")).unwrap();
            let temporary = root.join(".sorrel/heads/.sorrel-extra.tmp");
            fs::write(&temporary, &bytes).unwrap();
            staging.push(temporary);
            journal.push(json!([
                "heads/unrecorded",
                "heads/.sorrel-extra.tmp",
                sorrel_core::ObjectId::for_bytes(&bytes).to_hex()
            ]));
        }
        fs::remove_file(&operation).unwrap();
        fs::write(root.join(".sorrel/HEAD"), &before_head).unwrap();
        fs::write(root.join(".sorrel/heads/lane_main"), &before_lane).unwrap();
        if case == "stale-current" {
            let mut value: Value = serde_json::from_slice(&before_head).unwrap();
            value["snapshot"] = json!("f".repeat(64));
            fs::write(
                root.join(".sorrel/HEAD"),
                serde_json::to_vec(&value).unwrap(),
            )
            .unwrap();
        }
        let current_head = fs::read(root.join(".sorrel/HEAD")).unwrap();
        let journal_bytes = serde_json::to_vec(&journal).unwrap();
        fs::write(
            root.join(".sorrel/metadata-transaction.json"),
            &journal_bytes,
        )
        .unwrap();
        Command::cargo_bin("sorrel")
            .unwrap()
            .current_dir(root)
            .arg("status")
            .assert()
            .failure();
        assert_eq!(
            fs::read(root.join(".sorrel/HEAD")).unwrap(),
            current_head,
            "{case}"
        );
        assert_eq!(
            fs::read(root.join(".sorrel/heads/lane_main")).unwrap(),
            before_lane,
            "{case}"
        );
        assert_eq!(
            fs::read(root.join("file.txt")).unwrap(),
            b"second",
            "{case}"
        );
        assert_eq!(
            fs::read(root.join(".sorrel/metadata-transaction.json")).unwrap(),
            journal_bytes,
            "{case}"
        );
        assert!(!operation.exists(), "{case}");
        for path in staging {
            assert!(path.exists(), "{case}");
        }
    }
}
