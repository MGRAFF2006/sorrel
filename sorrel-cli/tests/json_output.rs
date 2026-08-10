use assert_cmd::Command;
use serde_json::{json, Value};
use sorrel_core::{
    write_snapshot, write_tree, EntryMode, EntryType, FileObjectStore, ObjectKind, ObjectRef,
    ObjectStore, SnapshotOptions, TreeEntry,
};
use std::path::{Path, PathBuf};
use tempfile::TempDir;

const PROTOCOL_VERSION: &str = "sorrel.protocol.v0";
const MOCK_TIMESTAMP: &str = "2026-06-24T09:00:00Z";

#[test]
fn init_writes_real_persistent_workspace() {
    let temp_dir = TempDir::new().expect("temp dir is available");
    let value = command_json(temp_dir.path(), &["init", "--json"]);

    assert_eq!(value["command"], "init");
    assert_eq!(value["mocked"], false);
    assert_eq!(value["sorrelDir"], ".sorrel");
    assert_eq!(value["initialized"], true);
    assert_eq!(value["status"], "initialized");
    assert_eq!(value["defaultLane"]["id"], "lane_main");
    assert_eq!(value["defaultLane"]["name"], "main");
    assert_nonempty_str(&value["repoId"]);
    assert!(value["repoId"]
        .as_str()
        .is_some_and(|id| id.starts_with("repo_")));
    assert_content_id(&value["headSnapshot"]["id"]);
    assert_nonempty_str(&value["createdAt"]);

    // Real on-disk state exists.
    assert!(temp_dir.path().join(".sorrel/manifest.json").is_file());
    assert!(temp_dir.path().join(".sorrel/HEAD").is_file());
    assert!(temp_dir.path().join(".sorrel/objects").is_dir());
}

#[test]
fn init_is_idempotent_and_does_not_clobber() {
    let temp_dir = TempDir::new().expect("temp dir is available");
    let first = command_json(temp_dir.path(), &["init", "--json"]);
    let second = command_json(temp_dir.path(), &["init", "--json"]);

    assert_eq!(second["status"], "already_initialized");
    // The repository identity is preserved across a redundant init.
    assert_eq!(second["repoId"], first["repoId"]);
}

#[test]
fn status_reports_real_persisted_state_for_initialized_workspace() {
    let temp_dir = TempDir::new().expect("temp dir is available");
    let init = command_json(temp_dir.path(), &["init", "--json"]);

    // A freshly initialized, empty working tree is clean.
    let value = command_json(temp_dir.path(), &["status", "--json"]);
    assert_eq!(value["command"], "status");
    assert_eq!(value["mocked"], false);
    assert_eq!(value["initialized"], true);
    assert_eq!(value["status"], "clean");
    assert_eq!(value["worktree"]["dirty"], false);
    assert_eq!(value["currentLane"]["id"], "lane_main");
    // status must reflect the SAME repo + HEAD that init persisted.
    assert_eq!(value["repoId"], init["repoId"]);
    assert_eq!(value["headSnapshot"]["id"], init["headSnapshot"]["id"]);
}

#[test]
fn status_detects_dirty_working_tree() {
    let temp_dir = TempDir::new().expect("temp dir is available");
    command_json(temp_dir.path(), &["init", "--json"]);
    std::fs::write(temp_dir.path().join("a.txt"), b"hello\n").expect("write file");

    let value = command_json(temp_dir.path(), &["status", "--json"]);
    assert_eq!(value["status"], "dirty");
    assert_eq!(value["worktree"]["dirty"], true);
    let added = value["worktree"]["changes"]["added"]
        .as_array()
        .expect("added is an array");
    assert!(added.iter().any(|path| path == "a.txt"));
}

#[test]
fn status_writes_stat_cache_and_reuses_it_on_unchanged_resnapshot() {
    let temp_dir = TempDir::new().expect("temp dir is available");
    command_json(temp_dir.path(), &["init", "--json"]);
    std::fs::write(temp_dir.path().join("tracked.txt"), b"cached bytes\n").expect("write file");

    // First status materializes the working tree and must persist the cache.
    let first = command_json(temp_dir.path(), &["status", "--json"]);
    assert_eq!(first["status"], "dirty");
    let cache_path = temp_dir.path().join(".sorrel/stat-cache.json");
    assert!(
        cache_path.is_file(),
        "status must persist .sorrel/stat-cache.json"
    );

    // The cache records the tracked file with the v0 schema version.
    let cache: Value = serde_json::from_slice(&std::fs::read(&cache_path).expect("read cache"))
        .expect("cache json");
    assert_eq!(cache["schemaVersion"], PROTOCOL_VERSION);
    assert!(
        cache["entries"].get("tracked.txt").is_some(),
        "cache should contain the tracked file entry"
    );

    // Re-running status with an unchanged working tree must still succeed and
    // report the same dirty result (cache hit path exercised).
    let second = command_json(temp_dir.path(), &["status", "--json"]);
    assert_eq!(second["status"], "dirty");
    assert_eq!(
        second["headSnapshot"]["id"], first["headSnapshot"]["id"],
        "unchanged re-snapshot must not move HEAD"
    );
    let added = second["worktree"]["changes"]["added"]
        .as_array()
        .expect("added is an array");
    assert!(added.iter().any(|path| path == "tracked.txt"));

    // Recording the change persists the cache too, and a follow-up status is
    // clean, proving the cache stays consistent after a snapshot advance.
    command_json(
        temp_dir.path(),
        &["change", "create", "-m", "add tracked", "--json"],
    );
    assert!(cache_path.is_file(), "change create must persist the cache");
    let after = command_json(temp_dir.path(), &["status", "--json"]);
    assert_eq!(after["status"], "clean");
}

#[test]
fn status_reports_uninitialized_workspace() {
    let temp_dir = TempDir::new().expect("temp dir is available");
    let value = command_json(temp_dir.path(), &["status", "--json"]);

    assert_eq!(value["command"], "status");
    assert_eq!(value["initialized"], false);
    assert_eq!(value["status"], "uninitialized");
    assert_eq!(value["currentLane"], Value::Null);
    assert_eq!(value["headSnapshot"], Value::Null);
}

#[test]
fn repo_state_persists_across_separate_processes() {
    let temp_dir = TempDir::new().expect("temp dir is available");

    // Process 1: init.
    let init = command_json(temp_dir.path(), &["init", "--json"]);
    let repo_id = init["repoId"].as_str().expect("repoId is a string");
    let head = init["headSnapshot"]["id"]
        .as_str()
        .expect("head id is a string");

    // Process 2 (fresh invocation): status reads persisted state from disk.
    let status = command_json(temp_dir.path(), &["status", "--json"]);
    assert_eq!(status["repoId"], repo_id);
    assert_eq!(status["headSnapshot"]["id"], head);
}

#[test]
fn change_create_records_real_change_and_advances_head() {
    let temp_dir = TempDir::new().expect("temp dir is available");
    let init = command_json(temp_dir.path(), &["init", "--json"]);
    std::fs::write(temp_dir.path().join("a.txt"), b"hello\n").expect("write file");

    let value = command_json(
        temp_dir.path(),
        &["change", "create", "-m", "add a.txt", "--json"],
    );
    assert_eq!(value["command"], "change create");
    assert_eq!(value["mocked"], false);
    assert_eq!(value["status"], "created");
    assert_content_id(&value["object"]["id"]);
    assert_eq!(value["object"]["message"], "add a.txt");
    assert_eq!(value["object"]["changedPaths"], 1);
    let added = value["object"]["diff"]["added"]
        .as_array()
        .expect("added is an array");
    assert!(added.iter().any(|path| path == "a.txt"));

    // HEAD advanced: base is the init snapshot, result is a new snapshot.
    assert_eq!(
        value["object"]["baseSnapshot"]["id"],
        init["headSnapshot"]["id"]
    );
    assert_ne!(
        value["object"]["resultingSnapshot"]["id"],
        init["headSnapshot"]["id"]
    );

    // After recording, the working tree is clean again.
    let status = command_json(temp_dir.path(), &["status", "--json"]);
    assert_eq!(status["status"], "clean");
    assert_eq!(
        status["headSnapshot"]["id"],
        value["object"]["resultingSnapshot"]["id"]
    );
}

#[test]
fn change_create_rejects_empty_change() {
    let temp_dir = TempDir::new().expect("temp dir is available");
    command_json(temp_dir.path(), &["init", "--json"]);

    // No working-tree edits since HEAD -> nothing to record -> failure.
    Command::cargo_bin("sorrel")
        .expect("sorrel binary is available")
        .current_dir(temp_dir.path())
        .args(["change", "create", "-m", "nothing", "--json"])
        .assert()
        .failure();
}

#[test]
fn diff_reports_line_level_hunks_for_modified_text() {
    let temp_dir = TempDir::new().expect("temp dir is available");
    std::fs::write(temp_dir.path().join("a.txt"), b"line1\nline2\nline3\n").expect("write");
    command_json(temp_dir.path(), &["init", "--json"]);
    command_json(
        temp_dir.path(),
        &["change", "create", "-m", "add", "--json"],
    );

    // Modify one line and add one line.
    std::fs::write(
        temp_dir.path().join("a.txt"),
        b"line1\nLINE2\nline3\nline4\n",
    )
    .expect("write");

    let value = command_json(temp_dir.path(), &["diff", "--json"]);
    assert_eq!(value["command"], "diff");
    assert_eq!(value["mocked"], false);
    let files = value["files"].as_array().expect("files is an array");
    let file = files
        .iter()
        .find(|file| file["path"] == "a.txt")
        .expect("a.txt is in the diff");
    assert_eq!(file["kind"], "modified");
    assert_eq!(file["binary"], false);
    let hunks = file["hunks"].as_array().expect("hunks is an array");
    assert!(!hunks.is_empty());
    let lines = hunks[0]["lines"].as_array().expect("lines is an array");
    assert!(lines
        .iter()
        .any(|line| line["kind"] == "removed" && line["text"] == "line2"));
    assert!(lines
        .iter()
        .any(|line| line["kind"] == "added" && line["text"] == "LINE2"));
    assert!(lines
        .iter()
        .any(|line| line["kind"] == "added" && line["text"] == "line4"));
}

#[test]
fn diff_reports_no_changes_when_clean() {
    let temp_dir = TempDir::new().expect("temp dir is available");
    command_json(temp_dir.path(), &["init", "--json"]);

    let value = command_json(temp_dir.path(), &["diff", "--json"]);
    assert_eq!(value["command"], "diff");
    assert_eq!(
        value["files"].as_array().expect("files is an array").len(),
        0
    );
}

#[test]
fn log_walks_change_history_from_head() {
    let temp_dir = TempDir::new().expect("temp dir is available");
    command_json(temp_dir.path(), &["init", "--json"]);
    std::fs::write(temp_dir.path().join("a.txt"), b"a\n").expect("write");
    let first = command_json(
        temp_dir.path(),
        &["change", "create", "-m", "add a", "--json"],
    );
    std::fs::write(temp_dir.path().join("b.txt"), b"b\n").expect("write");
    let second = command_json(
        temp_dir.path(),
        &["change", "create", "-m", "add b", "--json"],
    );

    let value = command_json(temp_dir.path(), &["log", "--json"]);
    assert_eq!(value["command"], "log");
    let entries = value["entries"].as_array().expect("entries is an array");
    // initial snapshot + two changes, most recent first.
    assert_eq!(entries.len(), 3);
    assert_eq!(entries[0]["message"], "add b");
    assert_eq!(entries[1]["message"], "add a");
    assert_eq!(entries[2]["message"], "initial snapshot");
    assert_eq!(entries[2]["root"], true);

    // Indexed changes expose change id, author, and timestamp from the Change object.
    assert_eq!(entries[0]["change"]["id"], second["object"]["id"]);
    assert_eq!(entries[1]["change"]["id"], first["object"]["id"]);
    assert_content_id(&entries[0]["change"]["id"]);
    assert_content_id(&entries[1]["change"]["id"]);
    assert_nonempty_str(&entries[0]["author"]);
    assert_nonempty_str(&entries[1]["author"]);
    assert_eq!(entries[0]["author"], entries[1]["author"]);
    assert_nonempty_str(&entries[0]["createdAt"]);
    assert_nonempty_str(&entries[1]["createdAt"]);
    assert_eq!(
        entries[0]["snapshot"]["id"],
        second["object"]["resultingSnapshot"]["id"]
    );
    assert_eq!(
        entries[1]["snapshot"]["id"],
        first["object"]["resultingSnapshot"]["id"]
    );

    // The initial snapshot has no index entry / Change object.
    assert_eq!(entries[2]["change"], Value::Null);
    assert_eq!(entries[2]["author"], Value::Null);
}

#[test]
fn log_survives_missing_changes_index() {
    let temp_dir = TempDir::new().expect("temp dir is available");
    command_json(temp_dir.path(), &["init", "--json"]);
    std::fs::write(temp_dir.path().join("a.txt"), b"a\n").expect("write");
    command_json(
        temp_dir.path(),
        &["change", "create", "-m", "add a", "--json"],
    );
    std::fs::write(temp_dir.path().join("b.txt"), b"b\n").expect("write");
    command_json(
        temp_dir.path(),
        &["change", "create", "-m", "add b", "--json"],
    );

    // Simulate a repo initialized before the index existed.
    let index_path = temp_dir.path().join(".sorrel/changes.index");
    assert!(
        index_path.is_file(),
        "change create must write changes.index"
    );
    std::fs::remove_file(&index_path).expect("delete changes.index");

    let value = command_json(temp_dir.path(), &["log", "--json"]);
    assert_eq!(value["command"], "log");
    let entries = value["entries"].as_array().expect("entries is an array");
    assert_eq!(entries.len(), 3);
    assert_eq!(entries[0]["message"], "add b");
    assert_eq!(entries[1]["message"], "add a");
    assert_eq!(entries[2]["message"], "initial snapshot");
    // Without the index, change/author are null; log still succeeds.
    assert_eq!(entries[0]["change"], Value::Null);
    assert_eq!(entries[0]["author"], Value::Null);
    assert_eq!(entries[1]["change"], Value::Null);
    assert_eq!(entries[1]["author"], Value::Null);
}

#[test]
fn change_create_appends_changes_index() {
    let temp_dir = TempDir::new().expect("temp dir is available");
    command_json(temp_dir.path(), &["init", "--json"]);
    std::fs::write(temp_dir.path().join("a.txt"), b"a\n").expect("write");
    let created = command_json(
        temp_dir.path(),
        &["change", "create", "-m", "add a", "--json"],
    );

    let index_path = temp_dir.path().join(".sorrel/changes.index");
    assert!(index_path.is_file());
    let text = std::fs::read_to_string(&index_path).expect("read index");
    let line = text.lines().next().expect("one index line");
    let record: Value = serde_json::from_str(line).expect("index line is json");
    assert_eq!(
        record["snapshot"],
        created["object"]["resultingSnapshot"]["id"]
    );
    assert_eq!(record["change"], created["object"]["id"]);
}

#[test]
fn log_respects_limit() {
    let temp_dir = TempDir::new().expect("temp dir is available");
    command_json(temp_dir.path(), &["init", "--json"]);
    std::fs::write(temp_dir.path().join("a.txt"), b"a\n").expect("write");
    command_json(
        temp_dir.path(),
        &["change", "create", "-m", "add a", "--json"],
    );

    let value = command_json(temp_dir.path(), &["log", "--limit", "1", "--json"]);
    let entries = value["entries"].as_array().expect("entries is an array");
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0]["message"], "add a");
}

#[test]
fn change_list_walks_recorded_changes() {
    let temp_dir = TempDir::new().expect("temp dir is available");
    command_json(temp_dir.path(), &["init", "--json"]);
    std::fs::write(temp_dir.path().join("a.txt"), b"hello\n").expect("write file");
    command_json(
        temp_dir.path(),
        &["change", "create", "-m", "add a.txt", "--json"],
    );

    let value = command_json(temp_dir.path(), &["change", "list", "--json"]);
    assert_eq!(value["command"], "change list");
    assert_eq!(value["mocked"], false);
    assert_eq!(value["count"], 1);
    let objects = value["objects"].as_array().expect("objects is an array");
    assert_eq!(objects.len(), 1);
    assert_eq!(objects[0]["kind"], "Change");
    assert_eq!(objects[0]["message"], "add a.txt");
    assert_content_id(&objects[0]["resultingSnapshot"]["id"]);
}

#[test]
fn change_list_is_empty_for_fresh_repo() {
    let temp_dir = TempDir::new().expect("temp dir is available");
    command_json(temp_dir.path(), &["init", "--json"]);
    let value = command_json(temp_dir.path(), &["change", "list", "--json"]);
    assert_eq!(value["count"], 0);
    assert!(value["objects"].as_array().expect("array").is_empty());
}

#[test]
fn lane_create_persists_real_lane() {
    let temp_dir = TempDir::new().expect("temp dir is available");
    let init = command_json(temp_dir.path(), &["init", "--json"]);

    let value = command_json(
        temp_dir.path(),
        &["lane", "create", "--name", "agent/docs", "--json"],
    );
    assert_eq!(value["command"], "lane create");
    assert_eq!(value["mocked"], false);
    assert_eq!(value["status"], "created");
    assert_eq!(value["object"]["kind"], "Lane");
    assert_eq!(value["object"]["name"], "agent/docs");
    assert_content_id(&value["object"]["id"]);
    assert_content_id(&value["object"]["baseSnapshot"]["id"]);

    // The lane is persisted on disk under .sorrel/lanes/.
    let lane_id = value["object"]["id"].as_str().expect("lane id");
    assert!(temp_dir
        .path()
        .join(format!(".sorrel/lanes/{lane_id}.json"))
        .is_file());
    // New lanes also get an initial per-lane head at the current HEAD snapshot.
    let head_path = temp_dir.path().join(format!(".sorrel/heads/{lane_id}"));
    assert!(head_path.is_file());
    let head_file: Value =
        serde_json::from_slice(&std::fs::read(&head_path).expect("read lane head"))
            .expect("lane head json");
    assert_eq!(head_file["snapshot"], init["headSnapshot"]["id"]);
}

#[test]
fn lane_list_shows_default_lane_after_init() {
    let temp_dir = TempDir::new().expect("temp dir is available");
    let init = command_json(temp_dir.path(), &["init", "--json"]);

    let value = command_json(temp_dir.path(), &["lane", "list", "--json"]);
    assert_eq!(value["command"], "lane list");
    assert_eq!(value["mocked"], false);
    assert_eq!(value["count"], 1);
    assert_eq!(value["activeLane"]["id"], "lane_main");

    let objects = value["objects"].as_array().expect("objects array");
    assert_eq!(objects.len(), 1);
    assert_eq!(objects[0]["id"], "lane_main");
    assert_eq!(objects[0]["name"], "main");
    assert_eq!(objects[0]["active"], true);
    assert_eq!(objects[0]["headSnapshot"]["id"], init["headSnapshot"]["id"]);
    assert!(temp_dir.path().join(".sorrel/heads/lane_main").is_file());
}

#[test]
fn lane_create_and_list_shows_both_lanes() {
    let temp_dir = TempDir::new().expect("temp dir is available");
    command_json(temp_dir.path(), &["init", "--json"]);
    let created = command_json(
        temp_dir.path(),
        &["lane", "create", "--name", "agent/feature", "--json"],
    );
    let feature_id = created["object"]["id"]
        .as_str()
        .expect("lane id")
        .to_owned();

    let value = command_json(temp_dir.path(), &["lane", "list", "--json"]);
    assert_eq!(value["count"], 2);
    assert_eq!(value["activeLane"]["id"], "lane_main");

    let objects = value["objects"].as_array().expect("objects array");
    let main = objects
        .iter()
        .find(|lane| lane["id"] == "lane_main")
        .expect("default lane present");
    let feature = objects
        .iter()
        .find(|lane| lane["id"] == feature_id)
        .expect("created lane present");
    assert_eq!(main["active"], true);
    assert_eq!(feature["active"], false);
    assert_eq!(feature["name"], "agent/feature");
    assert_content_id(&feature["headSnapshot"]["id"]);
}

#[test]
fn lane_switch_on_clean_tree_changes_head_and_restores_files() {
    let temp_dir = TempDir::new().expect("temp dir is available");
    command_json(temp_dir.path(), &["init", "--json"]);

    std::fs::write(temp_dir.path().join("main.txt"), b"on main\n").expect("write");
    let main_change = command_json(
        temp_dir.path(),
        &["change", "create", "-m", "add main.txt", "--json"],
    );
    let main_snapshot = main_change["object"]["resultingSnapshot"]["id"]
        .as_str()
        .expect("main snapshot")
        .to_owned();

    let created = command_json(
        temp_dir.path(),
        &["lane", "create", "--name", "agent/feature", "--json"],
    );
    let feature_id = created["object"]["id"]
        .as_str()
        .expect("lane id")
        .to_owned();

    let switched = command_json(temp_dir.path(), &["lane", "switch", &feature_id, "--json"]);
    assert_eq!(switched["command"], "lane switch");
    assert_eq!(switched["status"], "switched");
    assert_eq!(switched["lane"]["id"], feature_id);
    assert_eq!(switched["headSnapshot"]["id"], main_snapshot);

    // Edit only on the feature lane, then record.
    std::fs::write(temp_dir.path().join("feature.txt"), b"on feature\n").expect("write");
    let feature_change = command_json(
        temp_dir.path(),
        &["change", "create", "-m", "add feature.txt", "--json"],
    );
    let feature_snapshot = feature_change["object"]["resultingSnapshot"]["id"]
        .as_str()
        .expect("feature snapshot")
        .to_owned();
    assert_ne!(feature_snapshot, main_snapshot);

    // Switch back to main: HEAD moves and feature.txt disappears.
    let back = command_json(temp_dir.path(), &["lane", "switch", "lane_main", "--json"]);
    assert_eq!(back["lane"]["id"], "lane_main");
    assert_eq!(back["headSnapshot"]["id"], main_snapshot);

    let head: Value = serde_json::from_slice(
        &std::fs::read(temp_dir.path().join(".sorrel/HEAD")).expect("read HEAD"),
    )
    .expect("HEAD json");
    assert_eq!(head["lane"], "lane_main");
    assert_eq!(head["snapshot"], main_snapshot);
    assert!(temp_dir.path().join("main.txt").is_file());
    assert!(!temp_dir.path().join("feature.txt").exists());

    let status = command_json(temp_dir.path(), &["status", "--json"]);
    assert_eq!(status["status"], "clean");
    assert_eq!(status["currentLane"]["id"], "lane_main");
    assert_eq!(status["headSnapshot"]["id"], main_snapshot);
}

#[test]
fn lane_switch_with_dirty_tree_fails_without_modifying_state() {
    let temp_dir = TempDir::new().expect("temp dir is available");
    command_json(temp_dir.path(), &["init", "--json"]);
    let created = command_json(
        temp_dir.path(),
        &["lane", "create", "--name", "agent/feature", "--json"],
    );
    let feature_id = created["object"]["id"]
        .as_str()
        .expect("lane id")
        .to_owned();

    std::fs::write(temp_dir.path().join("dirty.txt"), b"uncommitted\n").expect("write");
    let head_before = std::fs::read(temp_dir.path().join(".sorrel/HEAD")).expect("read HEAD");
    let main_head_before =
        std::fs::read(temp_dir.path().join(".sorrel/heads/lane_main")).expect("read main head");

    let output = Command::cargo_bin("sorrel")
        .expect("sorrel binary is available")
        .current_dir(temp_dir.path())
        .args(["lane", "switch", &feature_id, "--json"])
        .output()
        .expect("run lane switch");
    assert!(
        !output.status.success(),
        "dirty switch should fail: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("uncommitted"),
        "stderr should mention uncommitted changes: {stderr}"
    );

    let head_after = std::fs::read(temp_dir.path().join(".sorrel/HEAD")).expect("read HEAD");
    let main_head_after =
        std::fs::read(temp_dir.path().join(".sorrel/heads/lane_main")).expect("read main head");
    assert_eq!(
        head_before, head_after,
        "dirty switch must not rewrite HEAD"
    );
    assert_eq!(
        main_head_before, main_head_after,
        "dirty switch must not rewrite lane heads"
    );
    assert!(temp_dir.path().join("dirty.txt").is_file());

    let status = command_json(temp_dir.path(), &["status", "--json"]);
    assert_eq!(status["currentLane"]["id"], "lane_main");
    assert_eq!(status["status"], "dirty");
}

#[test]
fn lane_switch_to_missing_lane_fails() {
    let temp_dir = TempDir::new().expect("temp dir is available");
    command_json(temp_dir.path(), &["init", "--json"]);
    let head_before = std::fs::read(temp_dir.path().join(".sorrel/HEAD")).expect("read HEAD");

    let output = Command::cargo_bin("sorrel")
        .expect("sorrel binary is available")
        .current_dir(temp_dir.path())
        .args(["lane", "switch", "lane_does_not_exist", "--json"])
        .output()
        .expect("run lane switch");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("does not exist"),
        "stderr should mention missing lane: {stderr}"
    );

    let head_after = std::fs::read(temp_dir.path().join(".sorrel/HEAD")).expect("read HEAD");
    assert_eq!(head_before, head_after);
}

#[test]
fn lane_heads_advance_independently() {
    let temp_dir = TempDir::new().expect("temp dir is available");
    command_json(temp_dir.path(), &["init", "--json"]);

    std::fs::write(temp_dir.path().join("shared.txt"), b"base\n").expect("write");
    let base = command_json(
        temp_dir.path(),
        &["change", "create", "-m", "add shared", "--json"],
    );
    let base_snapshot = base["object"]["resultingSnapshot"]["id"]
        .as_str()
        .expect("base snapshot")
        .to_owned();

    let created = command_json(
        temp_dir.path(),
        &["lane", "create", "--name", "agent/feature", "--json"],
    );
    let feature_id = created["object"]["id"]
        .as_str()
        .expect("lane id")
        .to_owned();

    // Advance main after the branch point.
    std::fs::write(temp_dir.path().join("main-only.txt"), b"main\n").expect("write");
    let main_change = command_json(
        temp_dir.path(),
        &["change", "create", "-m", "main advance", "--json"],
    );
    let main_snapshot = main_change["object"]["resultingSnapshot"]["id"]
        .as_str()
        .expect("main snapshot")
        .to_owned();
    assert_ne!(main_snapshot, base_snapshot);

    // Switch to feature (still at base) and advance it separately.
    command_json(temp_dir.path(), &["lane", "switch", &feature_id, "--json"]);
    assert!(!temp_dir.path().join("main-only.txt").exists());
    std::fs::write(temp_dir.path().join("feature-only.txt"), b"feature\n").expect("write");
    let feature_change = command_json(
        temp_dir.path(),
        &["change", "create", "-m", "feature advance", "--json"],
    );
    let feature_snapshot = feature_change["object"]["resultingSnapshot"]["id"]
        .as_str()
        .expect("feature snapshot")
        .to_owned();
    assert_ne!(feature_snapshot, main_snapshot);
    assert_ne!(feature_snapshot, base_snapshot);

    let listed = command_json(temp_dir.path(), &["lane", "list", "--json"]);
    let objects = listed["objects"].as_array().expect("objects");
    let main = objects
        .iter()
        .find(|lane| lane["id"] == "lane_main")
        .expect("main");
    let feature = objects
        .iter()
        .find(|lane| lane["id"] == feature_id)
        .expect("feature");
    assert_eq!(main["headSnapshot"]["id"], main_snapshot);
    assert_eq!(feature["headSnapshot"]["id"], feature_snapshot);
    assert_eq!(feature["active"], true);
    assert_eq!(main["active"], false);

    // Returning to main restores main-only isolation.
    command_json(temp_dir.path(), &["lane", "switch", "lane_main", "--json"]);
    assert!(temp_dir.path().join("main-only.txt").is_file());
    assert!(!temp_dir.path().join("feature-only.txt").exists());
    assert!(temp_dir.path().join("shared.txt").is_file());
}

#[test]
fn merge_fast_forwards_when_active_lane_has_not_diverged() {
    let temp_dir = TempDir::new().expect("temp dir is available");
    command_json(temp_dir.path(), &["init", "--json"]);

    std::fs::write(temp_dir.path().join("base.txt"), b"base\n").expect("write");
    command_json(
        temp_dir.path(),
        &["change", "create", "-m", "add base", "--json"],
    );

    let created = command_json(
        temp_dir.path(),
        &["lane", "create", "--name", "agent/feature", "--json"],
    );
    let feature_id = created["object"]["id"]
        .as_str()
        .expect("lane id")
        .to_owned();

    command_json(temp_dir.path(), &["lane", "switch", &feature_id, "--json"]);
    std::fs::write(temp_dir.path().join("feature.txt"), b"feature\n").expect("write");
    let feature_change = command_json(
        temp_dir.path(),
        &["change", "create", "-m", "feature work", "--json"],
    );
    let feature_snapshot = feature_change["object"]["resultingSnapshot"]["id"]
        .as_str()
        .expect("feature snapshot")
        .to_owned();

    command_json(temp_dir.path(), &["lane", "switch", "lane_main", "--json"]);
    assert!(!temp_dir.path().join("feature.txt").exists());

    let merged = command_json(temp_dir.path(), &["merge", &feature_id, "--json"]);
    assert_eq!(merged["command"], "merge");
    assert_eq!(merged["status"], "merged");
    assert_eq!(merged["fastForward"], true);
    assert_eq!(merged["lane"]["id"], feature_id);
    assert_eq!(merged["headSnapshot"]["id"], feature_snapshot);
    assert_eq!(merged["change"], Value::Null);

    let head: Value = serde_json::from_slice(
        &std::fs::read(temp_dir.path().join(".sorrel/HEAD")).expect("read HEAD"),
    )
    .expect("HEAD json");
    assert_eq!(head["lane"], "lane_main");
    assert_eq!(head["snapshot"], feature_snapshot);
    assert!(temp_dir.path().join("feature.txt").is_file());

    let status = command_json(temp_dir.path(), &["status", "--json"]);
    assert_eq!(status["status"], "clean");
    assert_eq!(status["headSnapshot"]["id"], feature_snapshot);
}

#[test]
fn merge_clean_three_way_with_disjoint_edits() {
    let temp_dir = TempDir::new().expect("temp dir is available");
    command_json(temp_dir.path(), &["init", "--json"]);

    std::fs::write(temp_dir.path().join("shared.txt"), b"shared\n").expect("write");
    command_json(
        temp_dir.path(),
        &["change", "create", "-m", "add shared", "--json"],
    );

    let created = command_json(
        temp_dir.path(),
        &["lane", "create", "--name", "agent/feature", "--json"],
    );
    let feature_id = created["object"]["id"]
        .as_str()
        .expect("lane id")
        .to_owned();

    // Disjoint edit on main.
    std::fs::write(temp_dir.path().join("a.txt"), b"main-a\n").expect("write");
    let main_change = command_json(
        temp_dir.path(),
        &["change", "create", "-m", "main adds a", "--json"],
    );
    let main_before = main_change["object"]["resultingSnapshot"]["id"]
        .as_str()
        .expect("main snapshot")
        .to_owned();

    // Disjoint edit on feature.
    command_json(temp_dir.path(), &["lane", "switch", &feature_id, "--json"]);
    std::fs::write(temp_dir.path().join("b.txt"), b"feature-b\n").expect("write");
    command_json(
        temp_dir.path(),
        &["change", "create", "-m", "feature adds b", "--json"],
    );

    command_json(temp_dir.path(), &["lane", "switch", "lane_main", "--json"]);
    let merged = command_json(temp_dir.path(), &["merge", &feature_id, "--json"]);
    assert_eq!(merged["command"], "merge");
    assert_eq!(merged["status"], "merged");
    assert_eq!(merged["fastForward"], false);
    assert_eq!(merged["lane"]["id"], feature_id);
    assert_content_id(&merged["headSnapshot"]["id"]);
    assert_content_id(&merged["change"]["id"]);
    assert_eq!(merged["change"]["message"], format!("merge {feature_id}"));
    assert_ne!(merged["headSnapshot"]["id"], main_before);

    assert_eq!(
        std::fs::read_to_string(temp_dir.path().join("a.txt")).expect("read a"),
        "main-a\n"
    );
    assert_eq!(
        std::fs::read_to_string(temp_dir.path().join("b.txt")).expect("read b"),
        "feature-b\n"
    );
    assert_eq!(
        std::fs::read_to_string(temp_dir.path().join("shared.txt")).expect("read shared"),
        "shared\n"
    );

    let status = command_json(temp_dir.path(), &["status", "--json"]);
    assert_eq!(status["status"], "clean");
    assert_eq!(status["headSnapshot"]["id"], merged["headSnapshot"]["id"]);

    // changes.index gained a snapshot → change mapping for the merge.
    let index = std::fs::read_to_string(temp_dir.path().join(".sorrel/changes.index"))
        .expect("read changes.index");
    assert!(index.contains(merged["change"]["id"].as_str().expect("change id")));
    assert!(index.contains(merged["headSnapshot"]["id"].as_str().expect("head id")));
}

#[test]
fn merge_conflict_writes_markers_and_merge_state_abort_restores() {
    let temp_dir = TempDir::new().expect("temp dir is available");
    command_json(temp_dir.path(), &["init", "--json"]);

    std::fs::write(temp_dir.path().join("a.txt"), b"base\n").expect("write");
    command_json(
        temp_dir.path(),
        &["change", "create", "-m", "add a", "--json"],
    );

    let created = command_json(
        temp_dir.path(),
        &["lane", "create", "--name", "agent/feature", "--json"],
    );
    let feature_id = created["object"]["id"]
        .as_str()
        .expect("lane id")
        .to_owned();

    std::fs::write(temp_dir.path().join("a.txt"), b"main-edit\n").expect("write");
    let main_change = command_json(
        temp_dir.path(),
        &["change", "create", "-m", "main edits a", "--json"],
    );
    let main_snapshot = main_change["object"]["resultingSnapshot"]["id"]
        .as_str()
        .expect("main snapshot")
        .to_owned();

    command_json(temp_dir.path(), &["lane", "switch", &feature_id, "--json"]);
    std::fs::write(temp_dir.path().join("a.txt"), b"feature-edit\n").expect("write");
    command_json(
        temp_dir.path(),
        &["change", "create", "-m", "feature edits a", "--json"],
    );

    command_json(temp_dir.path(), &["lane", "switch", "lane_main", "--json"]);
    let head_before = std::fs::read(temp_dir.path().join(".sorrel/HEAD")).expect("read HEAD");

    let output = Command::cargo_bin("sorrel")
        .expect("sorrel binary is available")
        .current_dir(temp_dir.path())
        .args(["merge", &feature_id, "--json"])
        .output()
        .expect("run merge");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("merge conflicts") && stderr.contains("a.txt"),
        "stderr should list conflicted paths, got: {stderr}"
    );

    let markers = std::fs::read_to_string(temp_dir.path().join("a.txt")).expect("read a");
    assert!(
        markers.contains("<<<<<<< ours"),
        "missing ours marker: {markers}"
    );
    assert!(markers.contains("======="), "missing separator: {markers}");
    assert!(
        markers.contains(">>>>>>> theirs"),
        "missing theirs marker: {markers}"
    );
    assert!(markers.contains("main-edit"));
    assert!(markers.contains("feature-edit"));

    let merge_state_path = temp_dir.path().join(".sorrel/MERGE_STATE");
    assert!(merge_state_path.is_file(), "MERGE_STATE must be written");
    let merge_state: Value =
        serde_json::from_slice(&std::fs::read(&merge_state_path).expect("read MERGE_STATE"))
            .expect("MERGE_STATE json");
    assert_content_id(&merge_state["mergeResult"]);

    let head_after = std::fs::read(temp_dir.path().join(".sorrel/HEAD")).expect("read HEAD");
    assert_eq!(
        head_before, head_after,
        "conflicted merge must not advance HEAD"
    );

    let status = command_json(temp_dir.path(), &["status", "--json"]);
    assert_eq!(status["status"], "dirty");
    assert_eq!(status["headSnapshot"]["id"], main_snapshot);

    let aborted = command_json(temp_dir.path(), &["merge", "--abort", "--json"]);
    assert_eq!(aborted["command"], "merge");
    assert_eq!(aborted["status"], "aborted");
    assert_eq!(aborted["headSnapshot"]["id"], main_snapshot);
    assert!(!merge_state_path.exists(), "MERGE_STATE must be removed");

    assert_eq!(
        std::fs::read_to_string(temp_dir.path().join("a.txt")).expect("read a"),
        "main-edit\n"
    );
    let status = command_json(temp_dir.path(), &["status", "--json"]);
    assert_eq!(status["status"], "clean");
    assert_eq!(status["headSnapshot"]["id"], main_snapshot);
}

#[test]
fn merge_continue_after_manual_resolution() {
    let temp_dir = TempDir::new().expect("temp dir is available");
    command_json(temp_dir.path(), &["init", "--json"]);

    std::fs::write(temp_dir.path().join("a.txt"), b"base\n").expect("write");
    command_json(
        temp_dir.path(),
        &["change", "create", "-m", "add a", "--json"],
    );

    let created = command_json(
        temp_dir.path(),
        &["lane", "create", "--name", "agent/feature", "--json"],
    );
    let feature_id = created["object"]["id"]
        .as_str()
        .expect("lane id")
        .to_owned();

    std::fs::write(temp_dir.path().join("a.txt"), b"main-edit\n").expect("write");
    command_json(
        temp_dir.path(),
        &["change", "create", "-m", "main edits a", "--json"],
    );

    command_json(temp_dir.path(), &["lane", "switch", &feature_id, "--json"]);
    std::fs::write(temp_dir.path().join("a.txt"), b"feature-edit\n").expect("write");
    command_json(
        temp_dir.path(),
        &["change", "create", "-m", "feature edits a", "--json"],
    );

    command_json(temp_dir.path(), &["lane", "switch", "lane_main", "--json"]);

    let output = Command::cargo_bin("sorrel")
        .expect("sorrel binary is available")
        .current_dir(temp_dir.path())
        .args(["merge", &feature_id, "--json"])
        .output()
        .expect("run merge");
    assert!(!output.status.success());

    let merge_state_path = temp_dir.path().join(".sorrel/MERGE_STATE");
    let merge_state: Value =
        serde_json::from_slice(&std::fs::read(&merge_state_path).expect("read MERGE_STATE"))
            .expect("MERGE_STATE json");
    assert_eq!(merge_state["lane"], feature_id);
    assert_content_id(&merge_state["oursSnapshot"]);
    assert_content_id(&merge_state["theirsSnapshot"]);

    // Markers still present → continue must fail.
    let blocked = Command::cargo_bin("sorrel")
        .expect("sorrel binary")
        .current_dir(temp_dir.path())
        .args(["merge", "--continue", "--json"])
        .output()
        .expect("continue with markers");
    assert!(!blocked.status.success());
    assert!(String::from_utf8_lossy(&blocked.stderr).contains("unresolved conflict markers"));

    std::fs::write(temp_dir.path().join("a.txt"), b"resolved\n").expect("resolve");
    let continued = command_json(temp_dir.path(), &["merge", "--continue", "--json"]);
    assert_eq!(continued["command"], "merge");
    assert_eq!(continued["status"], "merged");
    assert_eq!(continued["continued"], true);
    assert_eq!(continued["fastForward"], false);
    assert!(!merge_state_path.exists());
    assert_eq!(
        std::fs::read_to_string(temp_dir.path().join("a.txt")).expect("read"),
        "resolved\n"
    );
    let status = command_json(temp_dir.path(), &["status", "--json"]);
    assert_eq!(status["status"], "clean");
}

#[test]
fn merge_with_self_errors() {
    let temp_dir = TempDir::new().expect("temp dir is available");
    command_json(temp_dir.path(), &["init", "--json"]);

    let output = Command::cargo_bin("sorrel")
        .expect("sorrel binary is available")
        .current_dir(temp_dir.path())
        .args(["merge", "lane_main", "--json"])
        .output()
        .expect("run merge");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("into itself"),
        "expected self-merge error, got: {stderr}"
    );
}

#[test]
fn merge_equal_heads_errors() {
    let temp_dir = TempDir::new().expect("temp dir is available");
    command_json(temp_dir.path(), &["init", "--json"]);
    let created = command_json(
        temp_dir.path(),
        &["lane", "create", "--name", "agent/feature", "--json"],
    );
    let feature_id = created["object"]["id"]
        .as_str()
        .expect("lane id")
        .to_owned();

    let output = Command::cargo_bin("sorrel")
        .expect("sorrel binary is available")
        .current_dir(temp_dir.path())
        .args(["merge", &feature_id, "--json"])
        .output()
        .expect("run merge");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("nothing to merge"),
        "expected equal-heads error, got: {stderr}"
    );
}

#[test]
fn merge_unrelated_histories_errors() {
    let temp_dir = TempDir::new().expect("temp dir is available");
    command_json(temp_dir.path(), &["init", "--json"]);

    // Build an orphan snapshot (no parents) that does not share ancestry with HEAD.
    let store = FileObjectStore::new(temp_dir.path().join(".sorrel")).expect("store");
    let blob = ObjectStore::write(&store, b"orphan\n").expect("write blob");
    let tree = write_tree(
        &store,
        vec![TreeEntry {
            name: "orphan.txt".to_owned(),
            path: PathBuf::from("orphan.txt"),
            entry_type: EntryType::File,
            object: ObjectRef::new(ObjectKind::Blob, blob),
            mode: EntryMode::Normal,
            size: Some(7),
            content_hash: None,
        }],
    )
    .expect("write tree");
    let mut options = SnapshotOptions::new("repo_orphan".to_owned());
    options.created_at = "2026-06-26T00:00:00Z".to_owned();
    options.message = Some("orphan root".to_owned());
    options.parents = Vec::new();
    let orphan = write_snapshot(&store, tree.id, options).expect("orphan snapshot");
    let orphan_hex = orphan.id.to_hex();

    let orphan_lane = "lane_orphan";
    let lane_entry = json!({
        "kind": "Lane",
        "id": orphan_lane,
        "name": "orphan",
        "baseSnapshot": { "kind": "Snapshot", "id": orphan_hex },
        "headSnapshot": { "kind": "Snapshot", "id": orphan_hex },
        "createdAt": "2026-06-26T00:00:00Z",
    });
    std::fs::create_dir_all(temp_dir.path().join(".sorrel/lanes")).expect("lanes dir");
    std::fs::write(
        temp_dir
            .path()
            .join(format!(".sorrel/lanes/{orphan_lane}.json")),
        serde_json::to_vec_pretty(&lane_entry).expect("lane json"),
    )
    .expect("write lane");
    std::fs::write(
        temp_dir.path().join(format!(".sorrel/heads/{orphan_lane}")),
        serde_json::to_vec_pretty(&json!({ "snapshot": orphan_hex })).expect("head json"),
    )
    .expect("write lane head");

    let output = Command::cargo_bin("sorrel")
        .expect("sorrel binary is available")
        .current_dir(temp_dir.path())
        .args(["merge", orphan_lane, "--json"])
        .output()
        .expect("run merge");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("unrelated histories") || stderr.contains("no merge base"),
        "expected unrelated-history error, got: {stderr}"
    );
}

#[test]
fn merge_missing_lane_errors() {
    let temp_dir = TempDir::new().expect("temp dir is available");
    command_json(temp_dir.path(), &["init", "--json"]);

    let output = Command::cargo_bin("sorrel")
        .expect("sorrel binary is available")
        .current_dir(temp_dir.path())
        .args(["merge", "lane_does_not_exist", "--json"])
        .output()
        .expect("run merge");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("does not exist"),
        "expected missing-lane error, got: {stderr}"
    );
}

#[test]
fn slice_create_persists_real_manifest() {
    let temp_dir = TempDir::new().expect("temp dir is available");
    command_json(temp_dir.path(), &["init", "--json"]);

    let value = command_json(
        temp_dir.path(),
        &[
            "slice",
            "create",
            "--name",
            "auth-lib",
            "--source-path",
            "src",
            "--entrypoint",
            "src/main.rs",
            "--json",
        ],
    );
    assert_eq!(value["command"], "slice create");
    assert_eq!(value["mocked"], false);
    assert_eq!(value["status"], "created");
    assert_eq!(value["persisted"], true);
    assert_eq!(value["object"]["kind"], "Slice");
    assert_eq!(value["object"]["name"], "auth-lib");
    // Non-JS sources are now a real "generic" slice, not a mock.
    assert_eq!(value["object"]["metadata"]["language"], "generic");
    assert_eq!(value["object"]["metadata"]["mocked"], false);
    assert!(value["object"]["id"]
        .as_str()
        .is_some_and(|id| id.starts_with("slice_")));
}

#[test]
fn policy_evaluate_supports_agent_path_write_decision() {
    assert_json(
        &[
            "policy",
            "evaluate",
            "--action",
            "path.write",
            "--principal",
            "agent:docs",
            "--resource",
            "path:docs/README.md",
            "--json",
        ],
        json!({
            "command": "policy evaluate",
            "mocked": false,
            "status": "allow",
            "decision": mock_policy_decision(ExpectedDecision {
                action: "path.write",
                subject: "agent:docs",
                resource_type: "path",
                resource_ref: "docs/README.md",
                environment: "dev",
                result: "allow",
                effect: "allow",
                reason: "Headless Core policy allows agents to write declared paths.",
                required_grant: Value::Null,
            })
        }),
    );
}

#[test]
fn policy_evaluate_supports_workflow_run_decision() {
    assert_json(
        &[
            "policy",
            "evaluate",
            "--action",
            "workflow.run",
            "--principal",
            "agent:docs",
            "--resource",
            "workflow:workflow_validate_protocol",
            "--json",
        ],
        json!({
            "command": "policy evaluate",
            "mocked": false,
            "status": "allow",
            "decision": mock_policy_decision(ExpectedDecision {
                action: "workflow.run",
                subject: "agent:docs",
                resource_type: "workflow",
                resource_ref: "workflow_validate_protocol",
                environment: "dev",
                result: "allow",
                effect: "allow",
                reason: "Headless Core policy allows mocked workflow runs.",
                required_grant: Value::Null,
            })
        }),
    );
}

#[test]
fn policy_evaluate_supports_secret_inject_needs_grant_decision() {
    assert_json(
        &[
            "policy",
            "evaluate",
            "--action",
            "secret.inject",
            "--principal",
            "agent:docs",
            "--resource",
            "secret:secret_database_url_dev",
            "--environment",
            "dev",
            "--json",
        ],
        json!({
            "command": "policy evaluate",
            "mocked": false,
            "status": "needs_grant",
            "decision": mock_policy_decision(ExpectedDecision {
                action: "secret.inject",
                subject: "agent:docs",
                resource_type: "secret",
                resource_ref: "secret_database_url_dev",
                environment: "dev",
                result: "needs_grant",
                effect: "require",
                reason: "Secret injection requires an explicit grant before values are materialized.",
                required_grant: json!({
                    "kind": "Grant",
                    "action": "secret.inject",
                    "resource": {
                        "type": "secret",
                        "ref": "secret_database_url_dev"
                    },
                    "environment": "dev"
                }),
            })
        }),
    );
}

#[test]
fn policy_change_apply_denies_self_grant() {
    assert_json(
        &[
            "policy",
            "change",
            "apply",
            "--actor",
            "agent:agent_17",
            "--target-principal",
            "agent:agent_17",
            "--capability",
            "secret.inject",
            "--capability",
            "policy.grant",
            "--signature",
            "sig_agent_17",
            "--json",
        ],
        json!({
            "command": "policy change apply",
            "mocked": false,
            "status": "deny",
            "trusted": true,
            "evaluation": {
                "schemaVersion": PROTOCOL_VERSION,
                "kind": "PolicyChangeEvaluation",
                "actor": {
                    "type": "agent",
                    "ref": "agent:agent_17"
                },
                "operation": "grant",
                "decision": "deny",
                "reason": "actor lacks policy.grant on repo_mock_local under the previous effective policy",
                "trusted": true,
                "evaluatedAt": MOCK_TIMESTAMP,
                "metadata": {
                    "mocked": false,
                    "backend": "sorrel-core"
                }
            },
            "change": {
                "schemaVersion": PROTOCOL_VERSION,
                "kind": "PolicyChange",
                "actor": {
                    "type": "agent",
                    "ref": "agent:agent_17"
                },
                "operation": "grant",
                "grant": {
                    "principal": {
                        "type": "agent",
                        "ref": "agent:agent_17"
                    },
                    "capabilities": [
                        "secret.inject",
                        "policy.grant"
                    ],
                    "resources": [
                        {
                            "scope": "repo",
                            "ref": "repo_mock_local"
                        }
                    ]
                },
                "signatures": [
                    "sig_agent_17"
                ]
            }
        }),
    );
}

#[test]
fn policy_change_apply_allows_delegated_grant() {
    assert_json(
        &[
            "policy",
            "change",
            "apply",
            "--actor",
            "user:alice",
            "--target-principal",
            "agent:agent_17",
            "--capability",
            "path.write",
            "--signature",
            "sig_alice",
            "--json",
        ],
        json!({
            "command": "policy change apply",
            "mocked": false,
            "status": "allow",
            "trusted": true,
            "evaluation": {
                "schemaVersion": PROTOCOL_VERSION,
                "kind": "PolicyChangeEvaluation",
                "actor": {
                    "type": "user",
                    "ref": "user:alice"
                },
                "operation": "grant",
                "decision": "allow",
                "reason": "Actor user:alice may grant [\"path.write\"] to agent:agent_17 under the previous effective policy.",
                "trusted": true,
                "evaluatedAt": MOCK_TIMESTAMP,
                "metadata": {
                    "mocked": false,
                    "backend": "sorrel-core"
                }
            },
            "change": {
                "schemaVersion": PROTOCOL_VERSION,
                "kind": "PolicyChange",
                "actor": {
                    "type": "user",
                    "ref": "user:alice"
                },
                "operation": "grant",
                "grant": {
                    "principal": {
                        "type": "agent",
                        "ref": "agent:agent_17"
                    },
                    "capabilities": [
                        "path.write"
                    ],
                    "resources": [
                        {
                            "scope": "repo",
                            "ref": "repo_mock_local"
                        }
                    ]
                },
                "signatures": [
                    "sig_alice"
                ]
            }
        }),
    );
}

#[test]
fn policy_change_apply_denies_unsigned_change() {
    assert_json(
        &[
            "policy",
            "change",
            "apply",
            "--actor",
            "agent:agent_17",
            "--target-principal",
            "agent:agent_17",
            "--capability",
            "path.write",
            "--json",
        ],
        json!({
            "command": "policy change apply",
            "mocked": false,
            "status": "deny",
            "trusted": false,
            "evaluation": {
                "schemaVersion": PROTOCOL_VERSION,
                "kind": "PolicyChangeEvaluation",
                "actor": {
                    "type": "agent",
                    "ref": "agent:agent_17"
                },
                "operation": "grant",
                "decision": "deny",
                "reason": "PolicyChange is unsigned or explicitly marked untrusted.",
                "trusted": false,
                "evaluatedAt": MOCK_TIMESTAMP,
                "metadata": {
                    "mocked": false,
                    "backend": "sorrel-core"
                }
            },
            "change": {
                "schemaVersion": PROTOCOL_VERSION,
                "kind": "PolicyChange",
                "actor": {
                    "type": "agent",
                    "ref": "agent:agent_17"
                },
                "operation": "grant",
                "grant": {
                    "principal": {
                        "type": "agent",
                        "ref": "agent:agent_17"
                    },
                    "capabilities": [
                        "path.write"
                    ],
                    "resources": [
                        {
                            "scope": "repo",
                            "ref": "repo_mock_local"
                        }
                    ]
                },
                "signatures": []
            }
        }),
    );
}

#[test]
fn grant_create_evaluates_and_persists_real_grant() {
    let temp_dir = TempDir::new().expect("temp dir is available");
    command_json(temp_dir.path(), &["init", "--json"]);

    let value = command_json(temp_dir.path(), &["grant", "create", "--json"]);
    assert_eq!(value["command"], "grant create");
    assert_eq!(value["mocked"], false);
    assert_eq!(value["persisted"], true);
    // The grant carries the real Core decision (allow/deny/needs_grant).
    let status = value["status"].as_str().expect("status is a string");
    assert!(
        matches!(status, "allow" | "deny" | "needs_grant"),
        "unexpected decision: {status}"
    );
    assert_eq!(value["object"]["kind"], "Grant");
    assert_eq!(value["object"]["metadata"]["mocked"], false);
    let grant_id = value["object"]["id"].as_str().expect("grant id");
    assert!(grant_id.starts_with("grant_"));
    assert!(temp_dir
        .path()
        .join(format!(".sorrel/grants/{grant_id}.json"))
        .is_file());
}

#[test]
fn grant_list_reads_persisted_grants() {
    let temp_dir = TempDir::new().expect("temp dir is available");
    command_json(temp_dir.path(), &["init", "--json"]);

    // Empty before any grant.
    let empty = command_json(temp_dir.path(), &["grant", "list", "--json"]);
    assert_eq!(empty["count"], 0);

    command_json(temp_dir.path(), &["grant", "create", "--json"]);
    let value = command_json(temp_dir.path(), &["grant", "list", "--json"]);
    assert_eq!(value["command"], "grant list");
    assert_eq!(value["mocked"], false);
    assert_eq!(value["count"], 1);
    assert_eq!(value["objects"][0]["kind"], "Grant");
}

#[test]
fn remote_list_reports_configured_remotes() {
    let temp_dir = TempDir::new().expect("temp dir is available");
    let init = command_json(temp_dir.path(), &["init", "--json"]);
    let repo_id = init["repoId"].as_str().expect("repoId");

    let empty = command_json(temp_dir.path(), &["remote", "list", "--json"]);
    assert_eq!(empty["command"], "remote list");
    assert_eq!(empty["mocked"], false);
    assert_eq!(empty["count"], 0);
    assert!(empty["remotes"]
        .as_object()
        .expect("remotes map")
        .is_empty());

    command_json(
        temp_dir.path(),
        &[
            "remote",
            "add",
            "origin",
            "http://127.0.0.1:8787",
            "--repo-id",
            repo_id,
            "--json",
        ],
    );

    let value = command_json(temp_dir.path(), &["remote", "list", "--json"]);
    assert_eq!(value["command"], "remote list");
    assert_eq!(value["mocked"], false);
    assert_eq!(value["count"], 1);
    assert_eq!(value["remotes"]["origin"]["url"], "http://127.0.0.1:8787");
    assert_eq!(value["remotes"]["origin"]["repoId"], repo_id);
    assert!(temp_dir.path().join(".sorrel/remotes.json").is_file());
}

#[test]
fn secret_refs_lists_declared_handles() {
    let temp_dir = TempDir::new().expect("temp dir is available");
    command_json(temp_dir.path(), &["init", "--json"]);

    let value = command_json(temp_dir.path(), &["secret", "refs", "--json"]);
    assert_eq!(value["command"], "secret refs");
    assert_eq!(value["mocked"], false);
    // No SecretRefs are declared by default; values never appear in the CLI.
    assert_eq!(value["count"], 0);
    assert!(value["objects"].as_array().expect("array").is_empty());
}

fn assert_json(args: &[&str], expected: Value) {
    let temp_dir = TempDir::new().expect("temp dir is available");
    assert_json_in_dir(temp_dir.path(), args, expected);
}

fn assert_nonempty_str(value: &Value) {
    assert!(
        value.as_str().is_some_and(|text| !text.is_empty()),
        "expected a non-empty string, got {value:?}"
    );
}

fn assert_content_id(value: &Value) {
    let text = value.as_str().expect("content id is a string");
    assert_eq!(text.len(), 64, "content id should be 64 hex chars: {text}");
    assert!(
        text.chars().all(|character| character.is_ascii_hexdigit()),
        "content id should be hex: {text}"
    );
}

fn assert_json_in_dir(current_dir: &Path, args: &[&str], expected: Value) {
    let actual = command_json(current_dir, args);
    assert_eq!(actual, expected);
}

fn command_json(current_dir: &Path, args: &[&str]) -> Value {
    let output = Command::cargo_bin("sorrel")
        .expect("sorrel binary is available")
        .current_dir(current_dir)
        .args(args)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    serde_json::from_slice::<Value>(&output).expect("stdout is valid JSON")
}

struct ExpectedDecision<'a> {
    action: &'a str,
    subject: &'a str,
    resource_type: &'a str,
    resource_ref: &'a str,
    environment: &'a str,
    result: &'a str,
    effect: &'a str,
    reason: &'a str,
    required_grant: Value,
}

fn mock_policy_decision(expected: ExpectedDecision<'_>) -> Value {
    json!({
        "schemaVersion": PROTOCOL_VERSION,
        "kind": "PolicyDecision",
        "id": format!("decision_{}", expected.action.replace('.', "_")),
        "action": expected.action,
        "subject": {
            "type": "agent",
            "ref": expected.subject
        },
        "resource": {
            "type": expected.resource_type,
            "ref": expected.resource_ref
        },
        "environment": expected.environment,
        "result": expected.result,
        "effect": expected.effect,
        "policy": {
            "kind": "Policy",
            "id": "policy_headless_core"
        },
        "matchedRule": {
            "effect": expected.effect,
            "action": expected.action,
            "subjects": [
                expected.subject
            ],
            "resources": [
                {
                    "type": expected.resource_type,
                    "ref": expected.resource_ref
                }
            ],
            "reason": expected.reason
        },
        "requiredGrant": expected.required_grant,
        "evaluatedAt": MOCK_TIMESTAMP,
        "metadata": {
            "mocked": false,
            "backend": "sorrel-core"
        }
    })
}
