//! Real-process regressions for safe recording, selection, and recovery.

use assert_cmd::Command;
use serde_json::{json, Value};
use sorrel_core::{
    materialize_snapshot, read_snapshot_files, FileObjectStore, ObjectId, SnapshotOptions,
};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::Output,
};

fn output(root: &Path, args: &[&str]) -> Output {
    Command::cargo_bin("sorrel")
        .unwrap()
        .current_dir(root)
        .args(args)
        .arg("--json")
        .output()
        .unwrap()
}

fn run(root: &Path, args: &[&str]) -> Value {
    let result = output(root, args);
    assert!(
        result.status.success(),
        "sorrel {args:?}: stdout={} stderr={}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    serde_json::from_slice(&result.stdout).unwrap()
}

fn failure(root: &Path, args: &[&str]) -> Value {
    let result = output(root, args);
    assert!(
        !result.status.success(),
        "sorrel {args:?} unexpectedly succeeded"
    );
    let value: Value = serde_json::from_slice(&result.stdout)
        .expect("failed JSON commands emit structured stdout");
    assert_eq!(value["schemaVersion"], "sorrel.cli.v1");
    assert_eq!(value["status"], "error");
    assert!(value["error"]["code"]
        .as_str()
        .is_some_and(|code| !code.is_empty()));
    assert!(value["error"]["message"]
        .as_str()
        .is_some_and(|message| !message.is_empty()));
    value
}

fn write(root: &Path, name: &str, bytes: &[u8]) {
    let path = root.join(name);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
}

fn head_files(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let head: Value =
        serde_json::from_slice(&fs::read(root.join(".sorrel/HEAD")).unwrap()).unwrap();
    let id: ObjectId = head["snapshot"].as_str().unwrap().parse().unwrap();
    read_snapshot_files(&FileObjectStore::new(root.join(".sorrel")).unwrap(), &id).unwrap()
}

#[test]
fn nested_ignore_rules_negation_and_sorrelignore_select_only_expected_files() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    run(root, &["init"]);
    for (name, content) in [
        (".gitignore", "*.tmp\n!keep.tmp\ncache/\n"),
        (".sorrelignore", "private.txt\ncustom/\n"),
        ("nested/.gitignore", "*.bin\n!keep.bin\n"),
        ("nested/.sorrelignore", "*.local\n!allow.local\n"),
        ("nested/keep.tmp", "keep"),
        ("nested/drop.tmp", "drop"),
        ("nested/keep.bin", "keep"),
        ("nested/drop.bin", "drop"),
        ("nested/allow.local", "keep"),
        ("nested/drop.local", "drop"),
        ("private.txt", "private"),
        ("cache/file.txt", "cache"),
        ("custom/file.txt", "custom"),
        ("code.txt", "public"),
    ] {
        write(root, name, content.as_bytes());
    }
    run(root, &["change", "create", "-m", "selected files"]);
    let files = head_files(root);
    for name in [
        "nested/keep.tmp",
        "nested/keep.bin",
        "nested/allow.local",
        "code.txt",
    ] {
        assert!(
            files.contains_key(Path::new(name)),
            "missing included {name}"
        );
    }
    for name in [
        "nested/drop.tmp",
        "nested/drop.bin",
        "nested/drop.local",
        "private.txt",
        "cache/file.txt",
        "custom/file.txt",
    ] {
        assert!(
            !files.contains_key(Path::new(name)),
            "recorded ignored {name}"
        );
    }
    assert_eq!(run(root, &["status"])["worktree"]["dirty"], false);
}

#[test]
fn default_secret_and_generated_exclusions_apply_to_status_diff_and_recording() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    run(root, &["init"]);
    let omitted = [
        ".env",
        ".env.local",
        "nested/.env.production",
        "node_modules/package/index.js",
        "nested/target/object",
        "dist/bundle.js",
    ];
    for name in omitted {
        write(root, name, b"private or generated");
    }
    assert_eq!(run(root, &["status"])["worktree"]["dirty"], false);
    assert!(run(root, &["diff"])["files"].as_array().unwrap().is_empty());
    write(root, "code.txt", b"public");
    write(root, ".env.example", b"API_KEY=replace_me");
    run(root, &["change", "create", "-m", "public files"]);
    let files = head_files(root);
    assert_eq!(files.len(), 2);
    assert!(files.contains_key(Path::new("code.txt")));
    assert!(files.contains_key(Path::new(".env.example")));
    for name in omitted {
        assert!(!files.contains_key(Path::new(name)));
    }
}

#[test]
fn tracked_files_remain_tracked_inside_newly_ignored_directories() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    run(root, &["init"]);
    write(root, "cache/tracked.txt", b"before");
    run(root, &["change", "create", "-m", "track before ignore"]);
    write(root, ".gitignore", b"cache/\n");
    write(root, "cache/tracked.txt", b"after");
    write(root, "cache/new.txt", b"ignored");
    assert_eq!(run(root, &["status"])["worktree"]["dirty"], true);
    run(root, &["change", "create", "-m", "retain tracked edit"]);
    let files = head_files(root);
    assert_eq!(files[Path::new("cache/tracked.txt")], b"after");
    assert!(!files.contains_key(Path::new("cache/new.txt")));
}

#[test]
fn explicit_track_add_includes_ignored_files_and_directories_but_rejects_metadata() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    run(root, &["init"]);
    write(root, ".env", b"deliberately included local configuration");
    write(root, "target/fixture.txt", b"deliberately tracked fixture");
    let tracked = run(root, &["track", "add", ".env", "target"]);
    assert_eq!(tracked["paths"].as_array().unwrap().len(), 2);
    run(root, &["change", "create", "-m", "explicit exceptions"]);
    let files = head_files(root);
    assert!(files.contains_key(Path::new(".env")));
    assert!(files.contains_key(Path::new("target/fixture.txt")));
    assert!(!files.keys().any(|path| path.starts_with(".sorrel")));
    assert_eq!(
        failure(root, &["track", "add", ".sorrel"])["error"]["code"],
        "invalid_input"
    );
    assert_eq!(
        failure(root, &["track", "add", "../outside"])["error"]["code"],
        "invalid_input"
    );
}

#[test]
fn checkout_recovery_restores_previous_tree_and_head_without_losing_ignored_files() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let initialized = run(root, &["init"]);
    write(root, "shared.txt", b"before");
    write(root, "before-only.txt", b"before only");
    run(root, &["change", "create", "-m", "before checkout"]);
    let saved_head: Value =
        serde_json::from_slice(&fs::read(root.join(".sorrel/HEAD")).unwrap()).unwrap();
    let target_dir = tempfile::tempdir().unwrap();
    write(target_dir.path(), "shared.txt", b"after");
    write(target_dir.path(), "after-only.txt", b"after only");
    let store = FileObjectStore::new(root.join(".sorrel")).unwrap();
    let target = materialize_snapshot(
        &store,
        target_dir.path(),
        SnapshotOptions::new(initialized["repoId"].as_str().unwrap()),
    )
    .unwrap();
    let state = json!({ "schemaVersion":"sorrel.protocol.v0", "kind":"CheckoutRecovery", "current":saved_head["snapshot"], "target":target.id.to_string(), "head":saved_head, "mergeState":null });
    write(
        root,
        ".sorrel/CHECKOUT_STATE",
        &serde_json::to_vec(&state).unwrap(),
    );
    write(root, "shared.txt", b"after");
    write(root, "after-only.txt", b"after only");
    fs::remove_file(root.join("before-only.txt")).unwrap();
    write(root, ".env", b"unrelated private edit");
    assert_eq!(
        failure(root, &["status"])["error"]["code"],
        "recovery_required"
    );
    assert_eq!(run(root, &["recover"])["status"], "pending");
    assert!(root.join(".sorrel/CHECKOUT_STATE").exists());
    assert_eq!(run(root, &["recover", "--abort"])["status"], "aborted");
    assert_eq!(fs::read(root.join("shared.txt")).unwrap(), b"before");
    assert_eq!(
        fs::read(root.join("before-only.txt")).unwrap(),
        b"before only"
    );
    assert!(!root.join("after-only.txt").exists());
    assert_eq!(
        fs::read(root.join(".env")).unwrap(),
        b"unrelated private edit"
    );
    assert!(!root.join(".sorrel/CHECKOUT_STATE").exists());
    let restored: Value =
        serde_json::from_slice(&fs::read(root.join(".sorrel/HEAD")).unwrap()).unwrap();
    assert_eq!(restored, saved_head);
    assert_eq!(run(root, &["status"])["worktree"]["dirty"], false);
}

#[test]
fn interrupted_head_publication_is_completed_before_reading_repository_state() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let initialized = run(root, &["init"]);
    let source = tempfile::tempdir().unwrap();
    write(source.path(), "file.txt", b"published tree");
    let store = FileObjectStore::new(root.join(".sorrel")).unwrap();
    let snapshot = materialize_snapshot(
        &store,
        source.path(),
        SnapshotOptions::new(initialized["repoId"].as_str().unwrap()),
    )
    .unwrap();
    write(root, "file.txt", b"published tree");
    let transaction = json!({ "lane":"lane_main", "snapshot":snapshot.id.to_string() });
    // Simulate death after the lane ref changed, before HEAD was published.
    write(
        root,
        ".sorrel/heads/lane_main",
        &serde_json::to_vec(&json!({"snapshot":snapshot.id.to_string()})).unwrap(),
    );
    write(
        root,
        ".sorrel/HEAD_TRANSACTION",
        &serde_json::to_vec(&transaction).unwrap(),
    );
    let status = run(root, &["status"]);
    assert_eq!(status["headSnapshot"]["id"], snapshot.id.to_string());
    assert_eq!(status["worktree"]["dirty"], false);
    assert!(!root.join(".sorrel/HEAD_TRANSACTION").exists());
    let head: Value =
        serde_json::from_slice(&fs::read(root.join(".sorrel/HEAD")).unwrap()).unwrap();
    assert_eq!(head, transaction);
}

#[test]
fn interrupted_merge_head_transaction_replays_merge_state_cleanup() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let initialized = run(root, &["init"]);
    let head = initialized["headSnapshot"]["id"].as_str().unwrap();
    // A completed merge may die after writing refs but before clearing its old
    // conflict state. The ref journal must carry that remaining cleanup.
    write(root, ".sorrel/MERGE_STATE", b"stale conflict state");
    let transaction = json!({"lane":"lane_main", "snapshot":head, "clearMergeState":true});
    write(
        root,
        ".sorrel/HEAD_TRANSACTION",
        &serde_json::to_vec(&transaction).unwrap(),
    );
    assert_eq!(run(root, &["status"])["headSnapshot"]["id"], head);
    assert!(!root.join(".sorrel/MERGE_STATE").exists());
    assert!(!root.join(".sorrel/HEAD_TRANSACTION").exists());
    write(root, "next.txt", b"next change after completed merge");
    run(
        root,
        &["change", "create", "-m", "record after recovered merge"],
    );
    assert_eq!(run(root, &["status"])["worktree"]["dirty"], false);
}

#[test]
fn stale_lock_contents_and_concurrent_track_updates_preserve_all_paths() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    run(root, &["init"]);
    write(root, ".gitignore", b"*.fixture\n");
    write(root, ".sorrel/LOCK", b"stale process 999999\n");
    let names = (0..8).map(|n| format!("{n}.fixture")).collect::<Vec<_>>();
    for name in &names {
        write(root, name, b"fixture");
    }
    let barrier = std::sync::Barrier::new(names.len());
    std::thread::scope(|scope| {
        for name in &names {
            scope.spawn(|| {
                barrier.wait();
                run(root, &["track", "add", name]);
            });
        }
    });
    let tracked = run(root, &["track", "list"]);
    let actual = tracked["paths"].as_array().unwrap();
    assert_eq!(actual.len(), names.len());
    for name in names {
        assert!(actual.iter().any(|value| value == &name));
    }
}

#[test]
fn competing_records_of_one_edit_publish_one_change_and_consistent_refs() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    run(root, &["init"]);
    write(root, "file.txt", b"one edit");
    let barrier = std::sync::Barrier::new(6);
    let outcomes = std::thread::scope(|scope| {
        let handles = (0..6)
            .map(|_| {
                scope.spawn(|| {
                    barrier.wait();
                    output(root, &["change", "create", "-m", "same edit"])
                })
            })
            .collect::<Vec<_>>();
        handles
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .collect::<Vec<_>>()
    });
    assert_eq!(
        outcomes
            .iter()
            .filter(|output| output.status.success())
            .count(),
        1
    );
    let entries = fs::read_to_string(root.join(".sorrel/changes.index")).unwrap();
    assert_eq!(entries.lines().count(), 1);
    let head: Value =
        serde_json::from_slice(&fs::read(root.join(".sorrel/HEAD")).unwrap()).unwrap();
    let lane: Value =
        serde_json::from_slice(&fs::read(root.join(".sorrel/heads/lane_main")).unwrap()).unwrap();
    let indexed: Value = serde_json::from_str(entries.lines().next().unwrap()).unwrap();
    assert_eq!(head["snapshot"], lane["snapshot"]);
    assert_eq!(head["snapshot"], indexed["snapshot"]);
    assert_eq!(run(root, &["status"])["worktree"]["dirty"], false);
}

#[test]
fn sdk_and_cli_open_each_others_workspaces_and_preserve_detached_snapshots() {
    let sdk_dir = tempfile::tempdir().unwrap();
    let (sdk, initial) = sorrel_sdk::Workspace::init(sdk_dir.path(), "repo_sdk_cli").unwrap();
    assert_eq!(
        run(sdk_dir.path(), &["status"])["headSnapshot"]["id"],
        initial.id.to_string()
    );
    write(sdk_dir.path(), "cli.txt", b"created through CLI");
    let recorded = run(
        sdk_dir.path(),
        &["change", "create", "-m", "CLI records SDK workspace"],
    );
    let reopened = sorrel_sdk::Workspace::open(sdk_dir.path()).unwrap();
    assert_eq!(
        reopened.head_snapshot().unwrap().id.to_string(),
        recorded["object"]["resultingSnapshot"]["id"]
    );
    assert_eq!(sdk.repo_id(), reopened.repo_id());
    let cli_dir = tempfile::tempdir().unwrap();
    let initialized = run(cli_dir.path(), &["init"]);
    let embedded = sorrel_sdk::Workspace::open(cli_dir.path()).unwrap();
    assert_eq!(embedded.repo_id(), initialized["repoId"]);
    let original = embedded.head_snapshot().unwrap();
    write(cli_dir.path(), "sdk.txt", b"detached SDK object");
    let detached = embedded
        .snapshot_working_tree(original.id, "SDK snapshot")
        .unwrap();
    assert_ne!(detached.id, original.id);
    assert_eq!(
        run(cli_dir.path(), &["status"])["headSnapshot"]["id"],
        original.id.to_string()
    );
    assert_eq!(run(cli_dir.path(), &["status"])["worktree"]["dirty"], true);
}
