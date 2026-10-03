use assert_cmd::Command;
use serde_json::{json, Value};
use sorrel_core::{FileObjectStore, ObjectId, ObjectStore};
use std::{fs, path::Path};

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

fn fail(root: &Path, args: &[&str], code: &str) {
    let output = Command::cargo_bin("sorrel")
        .unwrap()
        .current_dir(root)
        .args(args)
        .arg("--json")
        .output()
        .unwrap();
    assert!(!output.status.success());
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["error"]["code"], code, "{value}");
}

fn assert_worker_author(root: &Path, expected: &str) {
    let head: Value =
        serde_json::from_slice(&fs::read(root.join(".sorrel/HEAD")).unwrap()).unwrap();
    let snapshot_id: ObjectId = head["snapshot"].as_str().unwrap().parse().unwrap();
    let store = FileObjectStore::new(root.join(".sorrel")).unwrap();
    let snapshot = sorrel_core::read_snapshot(&store, &snapshot_id).unwrap();
    assert_eq!(snapshot.author.principal_type, "agent");
    assert_eq!(snapshot.author.id, expected);
    let index = fs::read_to_string(root.join(".sorrel/changes.index")).unwrap();
    let mapping: Value = index
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .find(|entry| entry["snapshot"].as_str() == Some(snapshot_id.to_hex().as_str()))
        .unwrap();
    let change_id: ObjectId = mapping["change"].as_str().unwrap().parse().unwrap();
    let change = sorrel_core::read_change(&store, &change_id).unwrap();
    assert_eq!(change.author.principal_type, "agent");
    assert_eq!(change.author.id, expected);
}

fn prepare() -> (
    tempfile::TempDir,
    std::path::PathBuf,
    std::path::PathBuf,
    std::path::PathBuf,
) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("owner");
    let first = dir.path().join("first");
    let second = dir.path().join("second");
    fs::create_dir(&root).unwrap();
    run(&root, &["init"]);
    fs::write(root.join("a.txt"), "base a\n").unwrap();
    fs::write(root.join("b.txt"), "base b\n").unwrap();
    run(&root, &["change", "create", "-m", "Shared base"]);
    run(
        &root,
        &[
            "workspace",
            "create",
            first.to_str().unwrap(),
            "--agent",
            "first",
        ],
    );
    run(
        &root,
        &[
            "workspace",
            "create",
            second.to_str().unwrap(),
            "--agent",
            "second",
            "--task",
            "Second task",
        ],
    );
    (dir, root, first, second)
}

#[test]
fn two_workers_keep_isolated_edits_and_integrate_both_with_history() {
    let (_dir, root, first, second) = prepare();
    assert!(run(&root, &["agent", "active"])["agents"][0]
        .get("task")
        .is_none());
    assert_eq!(
        run(&first, &["log"])["entries"][0]["message"],
        "Shared base"
    );
    fs::write(first.join("a.txt"), "first a\n").unwrap();
    run(&first, &["change", "create", "-m", "First implementation"]);
    fs::write(second.join("b.txt"), "second b\n").unwrap();
    run(
        &second,
        &["change", "create", "-m", "Second implementation"],
    );
    assert_worker_author(&first, "first");
    assert_worker_author(&second, "second");
    assert_eq!(fs::read_to_string(root.join("a.txt")).unwrap(), "base a\n");
    assert_eq!(
        fs::read_to_string(second.join("a.txt")).unwrap(),
        "base a\n"
    );
    run(&root, &["workspace", "integrate", "first"]);
    run(&root, &["workspace", "integrate", "second"]);
    assert_eq!(fs::read_to_string(root.join("a.txt")).unwrap(), "first a\n");
    assert_eq!(
        fs::read_to_string(root.join("b.txt")).unwrap(),
        "second b\n"
    );
    assert_eq!(run(&root, &["status"])["worktree"]["dirty"], false);
    let index = fs::read_to_string(root.join(".sorrel/changes.index")).unwrap();
    let store = FileObjectStore::new(root.join(".sorrel")).unwrap();
    let messages: Vec<_> = index
        .lines()
        .map(|line| {
            let record: Value = serde_json::from_str(line).unwrap();
            let id: ObjectId = record["change"].as_str().unwrap().parse().unwrap();
            sorrel_core::read_change(&store, &id).unwrap().message
        })
        .collect();
    assert!(messages
        .iter()
        .any(|message| message == "First implementation"));
    assert!(messages
        .iter()
        .any(|message| message == "Second implementation"));
    assert!(!root.join(".sorrel/WORKSPACE_CREATE").exists());
}

#[test]
fn dirty_worker_additions_are_rejected_and_ignored_local_files_are_allowed() {
    let (_dir, root, first, _) = prepare();
    fs::write(first.join("new.txt"), "unrecorded\n").unwrap();
    fail(
        &root,
        &["workspace", "integrate", "first"],
        "dirty_worktree",
    );
    assert!(!root.join("new.txt").exists());
    run(&first, &["change", "create", "-m", "Add new"]);
    fs::write(first.join(".env"), "TEST_ONLY=local\n").unwrap();
    fs::create_dir(first.join("node_modules")).unwrap();
    fs::write(first.join("node_modules/untracked"), "ignored\n").unwrap();
    run(&root, &["workspace", "integrate", "first"]);
    assert_eq!(
        fs::read_to_string(root.join("new.txt")).unwrap(),
        "unrecorded\n"
    );
    assert!(!root.join(".env").exists());
}

#[test]
fn two_workers_conflict_without_losing_either_working_directory() {
    let (_dir, root, first, second) = prepare();
    fs::write(first.join("a.txt"), "first\n").unwrap();
    run(&first, &["change", "create", "-m", "First"]);
    fs::write(second.join("a.txt"), "second\n").unwrap();
    run(&second, &["change", "create", "-m", "Second"]);
    run(&root, &["workspace", "integrate", "first"]);
    fail(
        &root,
        &["workspace", "integrate", "second"],
        "merge_conflict",
    );
    assert_eq!(fs::read_to_string(first.join("a.txt")).unwrap(), "first\n");
    assert_eq!(
        fs::read_to_string(second.join("a.txt")).unwrap(),
        "second\n"
    );
    run(&root, &["merge", "--abort"]);
    assert_eq!(fs::read_to_string(root.join("a.txt")).unwrap(), "first\n");
}

#[test]
fn worker_cannot_replace_owner_history_mapping() {
    let (_dir, root, first, _) = prepare();
    let store = FileObjectStore::new(first.join(".sorrel")).unwrap();
    let index = fs::read_to_string(first.join(".sorrel/changes.index")).unwrap();
    let entry: Value = serde_json::from_str(index.lines().next().unwrap()).unwrap();
    let current: ObjectId = entry["change"].as_str().unwrap().parse().unwrap();
    let mut object: Value = serde_json::from_slice(&store.read(&current).unwrap()).unwrap();
    object["message"] = json!("Spoofed owner message");
    let changed = store.write(&serde_json::to_vec(&object).unwrap()).unwrap();
    fs::write(
        first.join(".sorrel/changes.index"),
        format!(
            "{}\n",
            json!({"snapshot":entry["snapshot"],"change":changed.to_hex()})
        ),
    )
    .unwrap();
    let before = fs::read(root.join(".sorrel/changes.index")).unwrap();
    fail(&root, &["workspace", "integrate", "first"], "invalid_data");
    assert_eq!(
        fs::read(root.join(".sorrel/changes.index")).unwrap(),
        before
    );
}

#[test]
fn worker_tip_must_descend_from_recorded_base() {
    let (_dir, root, first, _) = prepare();
    let head_path = first.join(".sorrel/HEAD");
    let mut head: Value = serde_json::from_slice(&fs::read(&head_path).unwrap()).unwrap();
    let snapshot: ObjectId = head["snapshot"].as_str().unwrap().parse().unwrap();
    let store = FileObjectStore::new(first.join(".sorrel")).unwrap();
    let initial = sorrel_core::read_snapshot(&store, &snapshot)
        .unwrap()
        .parents[0]
        .id;
    head["snapshot"] = json!(initial.to_hex());
    fs::write(&head_path, serde_json::to_vec(&head).unwrap()).unwrap();
    fail(
        &root,
        &["workspace", "integrate", "first"],
        "invalid_workspace",
    );
    assert_eq!(fs::read_to_string(root.join("a.txt")).unwrap(), "base a\n");
}

#[test]
fn interrupted_staged_publication_recovers_all_owner_records() {
    let (dir, root, first, _) = prepare();
    let workspace: Value =
        serde_json::from_slice(&fs::read(first.join(".sorrel/workspace.json")).unwrap()).unwrap();
    let lane = workspace["lane"].as_str().unwrap();
    let lane_path = root.join(format!(".sorrel/lanes/{lane}.json"));
    let lane_record: Value = serde_json::from_slice(&fs::read(&lane_path).unwrap()).unwrap();
    let agent_path = root.join(".sorrel/agents/agents/first.json");
    let agent: Value = serde_json::from_slice(&fs::read(&agent_path).unwrap()).unwrap();
    for path in [
        lane_path,
        root.join(format!(".sorrel/heads/{lane}")),
        agent_path,
        root.join(".sorrel/workspaces/first.json"),
    ] {
        fs::remove_file(path).unwrap();
    }
    let staging = dir.path().join(".sorrel-workspace-interrupted");
    fs::rename(&first, &staging).unwrap();
    let journal = json!({"workspace":workspace,"lane":lane_record,"agent":agent,"staging":staging});
    fs::write(
        root.join(".sorrel/WORKSPACE_CREATE"),
        serde_json::to_vec(&journal).unwrap(),
    )
    .unwrap();
    let listed = run(&root, &["workspace", "list"]);
    assert_eq!(listed["workspaces"].as_array().unwrap().len(), 2);
    assert!(first.is_dir());
    assert!(!staging.exists());
    assert!(root.join(".sorrel/workspaces/first.json").is_file());
    assert!(root.join(".sorrel/agents/agents/first.json").is_file());
    assert!(!root.join(".sorrel/WORKSPACE_CREATE").exists());
    assert_eq!(run(&first, &["status"])["worktree"]["dirty"], false);
}

#[test]
fn moved_worker_is_rejected_without_recreating_a_directory() {
    let (dir, root, first, _) = prepare();
    let renamed = dir.path().join("first-renamed");
    fs::rename(&first, &renamed).unwrap();
    fail(
        &root,
        &["workspace", "integrate", "first"],
        "invalid_workspace",
    );
    assert!(!first.exists());
    assert!(renamed.join(".sorrel/workspace.json").is_file());
}

#[test]
fn worker_identity_survives_lane_switch_and_malformed_links_fail_closed() {
    let (_dir, _root, first, _) = prepare();
    let lane = run(&first, &["lane", "create", "--name", "scratch"])["object"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    run(&first, &["lane", "switch", &lane]);
    fs::write(first.join("a.txt"), "agent on another lane\n").unwrap();
    run(&first, &["change", "create", "-m", "Scratch work"]);
    assert_worker_author(&first, "first");
    let path = first.join(".sorrel/workspace.json");
    let mut link: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    link["id"] = json!("other");
    fs::write(path, serde_json::to_vec(&link).unwrap()).unwrap();
    fs::write(first.join("a.txt"), "must not record\n").unwrap();
    fail(
        &first,
        &["change", "create", "-m", "Invalid identity"],
        "invalid_workspace",
    );
}

fn git(root: &Path, args: &[&str]) -> String {
    let output = std::process::Command::new("git")
        .current_dir(root)
        .args([
            "-c",
            "user.name=Sorrel test",
            "-c",
            "user.email=test@example.invalid",
        ])
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

#[test]
fn limited_git_import_workspace_integration_and_export_preserve_root_change_history() {
    let dir = tempfile::tempdir().unwrap();
    let fixture = dir.path().join("git-source");
    let root = dir.path().join("owner");
    let worker = dir.path().join("worker");
    let exported = dir.path().join("git-exported");
    fs::create_dir(&fixture).unwrap();
    fs::create_dir(&root).unwrap();
    git(&fixture, &["init", "--initial-branch=main"]);
    for version in 1..=3 {
        fs::write(fixture.join("file.txt"), format!("version {version}\n")).unwrap();
        git(&fixture, &["add", "file.txt"]);
        git(&fixture, &["commit", "-m", &format!("Version {version}")]);
    }
    run(&root, &["init"]);
    run(
        &root,
        &["git", "import", fixture.to_str().unwrap(), "--limit", "2"],
    );
    run(
        &root,
        &[
            "workspace",
            "create",
            worker.to_str().unwrap(),
            "--agent",
            "worker",
        ],
    );
    let log = run(&worker, &["log"]);
    assert_eq!(log["entries"].as_array().unwrap().len(), 2);
    assert_eq!(log["entries"][0]["message"], "Version 3\n");
    assert_eq!(log["entries"][1]["message"], "Version 2\n");
    assert!(!log["entries"][1]["change"].is_null());
    fs::write(worker.join("file.txt"), "version 4\n").unwrap();
    run(&worker, &["change", "create", "-m", "Agent version 4"]);
    run(&root, &["workspace", "integrate", "worker"]);
    run(&root, &["git", "export", exported.to_str().unwrap()]);
    assert_eq!(git(&exported, &["show", "main:file.txt"]), "version 4\n");
    assert!(git(&exported, &["log", "main", "--format=%s"]).contains("Agent version 4"));
}

#[test]
fn nonroot_change_cannot_use_unrelated_empty_baseline() {
    let (_dir, root, first, _) = prepare();
    fs::write(first.join("a.txt"), "updated\n").unwrap();
    run(&first, &["change", "create", "-m", "Update"]);
    let store = FileObjectStore::new(first.join(".sorrel")).unwrap();
    let head: Value =
        serde_json::from_slice(&fs::read(first.join(".sorrel/HEAD")).unwrap()).unwrap();
    let tip: ObjectId = head["snapshot"].as_str().unwrap().parse().unwrap();
    let repo = sorrel_core::read_snapshot(&store, &tip).unwrap().repo;
    let empty = tempfile::tempdir().unwrap();
    let mut options = sorrel_core::SnapshotOptions::new(repo);
    options.message = Some("Unrelated empty baseline".to_owned());
    let baseline = sorrel_core::materialize_snapshot(&store, empty.path(), options).unwrap();
    let forged = sorrel_core::create_change(
        &store,
        baseline.id,
        tip,
        sorrel_core::ChangeOptions::new(sorrel_core::Principal::system(), "Unrelated base"),
    )
    .unwrap();
    let index_path = first.join(".sorrel/changes.index");
    let index = fs::read_to_string(&index_path).unwrap();
    let lines: Vec<_> = index
        .lines()
        .map(|line| {
            let mut entry: Value = serde_json::from_str(line).unwrap();
            if entry["snapshot"].as_str() == Some(tip.to_hex().as_str()) {
                entry["change"] = json!(forged.id.to_hex());
            }
            entry.to_string()
        })
        .collect();
    fs::write(index_path, format!("{}\n", lines.join("\n"))).unwrap();
    fail(&root, &["workspace", "integrate", "first"], "invalid_data");
}
