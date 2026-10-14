use std::{fs, path::Path};

use assert_cmd::Command;
use serde_json::Value;
use sorrel_core::{read_snapshot, read_snapshot_files, FileObjectStore, ObjectId, ObjectStore};
use sorrel_sdk::Workspace;
use tempfile::TempDir;

fn command(root: &Path, args: &[&str]) -> Value {
    let result = Command::cargo_bin("sorrel")
        .unwrap()
        .current_dir(root)
        .args(args)
        .assert()
        .success();
    serde_json::from_slice(&result.get_output().stdout).unwrap()
}

fn write_fixture(root: &Path) {
    fs::create_dir_all(root.join("private")).unwrap();
    fs::create_dir_all(root.join("build")).unwrap();
    fs::create_dir_all(root.join(".git")).unwrap();
    for (file, value) in [
        (".gitignore", "build/\n"),
        (".sorrelignore", "local.txt\n!.env\n"),
        (".env", "WORKSPACE_SECRET_DEFAULT"),
        ("private/credentials", "WORKSPACE_SECRET_CUSTOM"),
        (".env.example", "TOKEN=replace-me\n"),
        ("build/output", "BUILD_OUTPUT"),
        ("local.txt", "LOCAL_IGNORED"),
        (".git/config", "GIT_INTERNAL"),
        ("source.txt", "public source\n"),
        (
            "sorrel.secrets.yml",
            "secretRefs:\n  - id: token\n    provider: dotenv:private/credentials\n",
        ),
    ] {
        fs::write(root.join(file), value).unwrap();
    }
}

fn blob_id(contents: &str) -> ObjectId {
    ObjectId::for_bytes(format!("sorrel.blob.v0\n{contents}").as_bytes())
}

#[test]
fn status_diff_and_change_share_safe_selection_with_sdk() {
    let cli_root = TempDir::new().unwrap();
    let sdk_root = TempDir::new().unwrap();
    command(cli_root.path(), &["init", "--json"]);
    write_fixture(cli_root.path());
    write_fixture(sdk_root.path());

    let status = command(cli_root.path(), &["status", "--json"]);
    let added = status["worktree"]["changes"]["added"].as_array().unwrap();
    assert!(added.contains(&Value::from(".env.example")));
    assert!(!added.contains(&Value::from(".env")));
    assert!(!added.contains(&Value::from("private/credentials")));
    command(cli_root.path(), &["diff", "--json"]);
    let change = command(
        cli_root.path(),
        &["change", "create", "-m", "safe snapshot", "--json"],
    );
    let cli_id: ObjectId = change["object"]["resultingSnapshot"]["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    let cli_store = FileObjectStore::new(cli_root.path().join(".sorrel")).unwrap();
    for value in [
        "WORKSPACE_SECRET_DEFAULT",
        "WORKSPACE_SECRET_CUSTOM",
        "BUILD_OUTPUT",
        "LOCAL_IGNORED",
        "GIT_INTERNAL",
    ] {
        assert!(!cli_store.has(&blob_id(value)).unwrap());
    }
    let cache: Value =
        serde_json::from_slice(&fs::read(cli_root.path().join(".sorrel/stat-cache.json")).unwrap())
            .unwrap();
    assert!(!cache.to_string().contains("private/credentials"));
    assert!(!cache.to_string().contains("\".env\""));

    let (sdk, initial) = Workspace::init(sdk_root.path(), "repo_sdk").unwrap();
    let sdk_snapshot = sdk
        .snapshot_working_tree(initial.id, "safe snapshot")
        .unwrap();
    assert_eq!(
        read_snapshot(&cli_store, &cli_id).unwrap().root_tree.id,
        sdk_snapshot.root_tree.id
    );
    assert_eq!(
        read_snapshot_files(&cli_store, &cli_id).unwrap(),
        read_snapshot_files(sdk.store(), &sdk_snapshot.id).unwrap()
    );
    assert!(sdk_root.path().join(".sorrel/objects").is_dir());
    assert!(!sdk_root.path().join(".sorrel/objects/objects").exists());
}

#[test]
fn cli_keeps_tracked_files_when_a_later_ignore_rule_matches_them() {
    let root = TempDir::new().unwrap();
    command(root.path(), &["init", "--json"]);
    fs::write(root.path().join("source.txt"), "first\n").unwrap();
    command(root.path(), &["change", "create", "-m", "source", "--json"]);
    fs::write(root.path().join(".gitignore"), "source.txt\n").unwrap();
    fs::write(root.path().join("source.txt"), "second\n").unwrap();
    let status = command(root.path(), &["status", "--json"]);
    assert_eq!(
        status["worktree"]["changes"]["modified"],
        serde_json::json!(["source.txt"])
    );
    let change = command(
        root.path(),
        &["change", "create", "-m", "update tracked source", "--json"],
    );
    let id = change["object"]["resultingSnapshot"]["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    let store = FileObjectStore::new(root.path().join(".sorrel")).unwrap();
    assert_eq!(
        read_snapshot_files(&store, &id).unwrap()[Path::new("source.txt")],
        b"second\n"
    );
}

fn write(root: &Path, path: &str, bytes: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
}

fn assert_selective_failure(root: &Path, paths: &[&str]) {
    let head = fs::read(root.join(".sorrel/HEAD")).unwrap();
    let index = fs::read(root.join(".sorrel/changes.index")).ok();
    let cache = fs::read(root.join(".sorrel/stat-cache.json")).ok();
    let history = command(root, &["operation", "log", "--json"]);
    let lane = fs::read(root.join(".sorrel/heads/lane_main")).unwrap();
    let mut args = vec!["change", "create", "-m", "must fail", "--json"];
    for path in paths {
        args.extend(["--path", path]);
    }
    let output = Command::cargo_bin("sorrel")
        .unwrap()
        .current_dir(root)
        .args(args)
        .output()
        .unwrap();
    assert!(!output.status.success(), "{paths:?}");
    assert!(output.stdout.is_empty());
    assert_eq!(fs::read(root.join(".sorrel/HEAD")).unwrap(), head);
    assert_eq!(
        fs::read(root.join(".sorrel/heads/lane_main")).unwrap(),
        lane
    );
    assert_eq!(fs::read(root.join(".sorrel/changes.index")).ok(), index);
    assert_eq!(fs::read(root.join(".sorrel/stat-cache.json")).ok(), cache);
    assert_eq!(command(root, &["operation", "log", "--json"]), history);
}

#[test]
fn selective_recording_leaves_unselected_bytes_dirty_and_full_recording_remains_correct() {
    use serde_json::json;
    let workspace = TempDir::new().unwrap();
    let root = workspace.path();
    command(root, &["init", "--json"]);
    for path in [
        "chosen",
        "remove",
        "dir/edit",
        "dir/remove",
        "dir-sibling",
        "other-edit",
        "other-remove",
    ] {
        write(root, path, "before");
    }
    command(root, &["change", "create", "-m", "base", "--json"]);
    command(root, &["checkpoint", "create", "base", "--json"]);
    let lane = command(root, &["lane", "create", "--name", "untouched", "--json"])["object"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let lane_before = fs::read(root.join(".sorrel/heads").join(&lane)).unwrap();
    for path in [
        "chosen",
        "new",
        "dir/edit",
        "dir/new",
        "dir-sibling",
        "other-edit",
        "other-new",
    ] {
        write(root, path, "after");
    }
    for path in ["remove", "dir/remove", "other-remove"] {
        fs::remove_file(root.join(path)).unwrap();
    }
    command(root, &["status", "--json"]); // Prime cache before partial recording.
    let partial = command(
        root,
        &[
            "change", "create", "-m", "partial", "--path", "./chosen", "--path", "new", "--path",
            "remove", "--path", "dir/", "--path", "dir/edit", "--json",
        ],
    );
    assert_eq!(partial["object"]["changedPaths"], 6);
    assert_eq!(
        partial["object"]["diff"],
        json!({"added":["dir/new", "new"], "modified":["chosen", "dir/edit"], "deleted":["dir/remove", "remove"]})
    );
    let store = FileObjectStore::new(root.join(".sorrel")).unwrap();
    let id = partial["object"]["resultingSnapshot"]["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    let files = read_snapshot_files(&store, &id).unwrap();
    for path in ["dir-sibling", "other-edit", "other-remove"] {
        assert_eq!(files[Path::new(path)], b"before");
    }
    assert!(!files.contains_key(Path::new("other-new")));
    for path in [
        "chosen",
        "new",
        "dir/edit",
        "dir/new",
        "dir-sibling",
        "other-edit",
        "other-new",
    ] {
        assert_eq!(fs::read(root.join(path)).unwrap(), b"after");
    }
    for path in ["remove", "dir/remove", "other-remove"] {
        assert!(!root.join(path).exists());
    }
    let status = command(root, &["status", "--json"]);
    assert_eq!(status["worktree"]["dirty"], true);
    assert_eq!(
        status["worktree"]["changes"],
        json!({"added":["other-new"], "modified":["dir-sibling", "other-edit"], "deleted":["other-remove"]})
    );
    let diff = command(root, &["diff", "--json"]);
    assert!(diff.to_string().contains("other-edit"));
    assert!(!diff.to_string().contains("chosen"));
    let log = command(root, &["operation", "log", "--json"]);
    assert_eq!(log["operations"][0]["operation"]["action"], "change create");
    assert_eq!(
        log["operations"][0]["operation"]["after"]["head"]["snapshot"],
        partial["object"]["resultingSnapshot"]["id"]
    );
    for args in [
        &["undo", "--json"][..],
        &["checkpoint", "create", "dirty", "--json"],
        &["checkpoint", "restore", "base", "--json"],
    ] {
        Command::cargo_bin("sorrel")
            .unwrap()
            .current_dir(root)
            .args(args)
            .assert()
            .failure();
    }
    let later = command(root, &["change", "create", "-m", "remaining", "--json"]);
    assert_eq!(later["object"]["changedPaths"], 4);
    assert_eq!(
        command(root, &["status", "--json"])["worktree"]["dirty"],
        false
    );
    command(root, &["checkpoint", "create", "full", "--json"]);
    command(root, &["undo", "--json"]);
    assert_eq!(fs::read(root.join("other-edit")).unwrap(), b"before");
    assert_eq!(fs::read(root.join("chosen")).unwrap(), b"after");
    assert!(!root.join("other-new").exists());
    command(root, &["undo", "--json"]);
    assert_eq!(fs::read(root.join("other-edit")).unwrap(), b"after");
    command(root, &["checkpoint", "restore", "base", "--json"]);
    assert_eq!(fs::read(root.join("chosen")).unwrap(), b"before");
    command(root, &["checkpoint", "restore", "full", "--json"]);
    assert_eq!(fs::read(root.join("chosen")).unwrap(), b"after");
    assert_eq!(
        fs::read(root.join(".sorrel/heads").join(lane)).unwrap(),
        lane_before
    );
}

#[test]
fn invalid_ignored_private_and_pending_merge_selectors_leave_refs_index_cache_and_log_unchanged() {
    let workspace = TempDir::new().unwrap();
    let root = workspace.path();
    command(root, &["init", "--json"]);
    write_fixture(root);
    command(root, &["change", "create", "-m", "base", "--json"]);
    write(root, "source.txt", "updated");
    for path in [
        "",
        "../source.txt",
        "dir/../source.txt",
        "/tmp",
        ".sorrel/HEAD",
        ".git/config",
        "missing",
        "build",
        "build/output",
        "local.txt",
        ".env",
        "private/credentials",
    ] {
        assert_selective_failure(root, &["source.txt", path]);
    }
    // Valid selection with no selected difference also cannot publish an operation.
    assert_selective_failure(root, &[".env.example"]);
    write(root, ".sorrel/MERGE_STATE", "pending fixture");
    assert_selective_failure(root, &["source.txt"]);
    assert_eq!(fs::read(root.join("source.txt")).unwrap(), b"updated");
}

#[cfg(unix)]
#[test]
fn symlink_selectors_fail_without_publication_and_do_not_block_unrelated_selection() {
    let workspace = TempDir::new().unwrap();
    let external = TempDir::new().unwrap();
    let root = workspace.path();
    command(root, &["init", "--json"]);
    write(root, "source", "before");
    command(root, &["change", "create", "-m", "base", "--json"]);
    write(external.path(), "value", "OUTSIDE_PRIVATE_FIXTURE");
    std::os::unix::fs::symlink(external.path(), root.join("link")).unwrap();
    std::os::unix::fs::symlink(external.path().join("value"), root.join("leaf")).unwrap();
    write(root, "source", "after");
    for path in ["link", "link/value", "leaf"] {
        assert_selective_failure(root, &["source", path]);
    }
    command(
        root,
        &[
            "change", "create", "-m", "safe", "--path", "source", "--json",
        ],
    );
    let store = FileObjectStore::new(root.join(".sorrel")).unwrap();
    assert!(!store.has(&blob_id("OUTSIDE_PRIVATE_FIXTURE")).unwrap());
    assert_eq!(
        fs::read(external.path().join("value")).unwrap(),
        b"OUTSIDE_PRIVATE_FIXTURE"
    );
}

#[cfg(unix)]
#[test]
fn selective_modes_and_file_directory_replacements_use_native_changes() {
    use sorrel_core::{read_tree, EntryMode};
    use std::os::unix::fs::PermissionsExt;
    let workspace = TempDir::new().unwrap();
    let root = workspace.path();
    command(root, &["init", "--json"]);
    for path in ["mode", "later-mode", "file", "dir/old"] {
        write(root, path, "before");
        fs::set_permissions(root.join(path), fs::Permissions::from_mode(0o644)).unwrap();
    }
    command(root, &["change", "create", "-m", "base", "--json"]);
    for path in ["mode", "later-mode"] {
        fs::set_permissions(root.join(path), fs::Permissions::from_mode(0o755)).unwrap();
    }
    fs::remove_file(root.join("file")).unwrap();
    write(root, "file/new", "new");
    fs::remove_dir_all(root.join("dir")).unwrap();
    write(root, "dir", "replacement");
    assert_selective_failure(root, &["file/new"]);
    assert_selective_failure(root, &["dir/old"]);
    let change = command(
        root,
        &[
            "change",
            "create",
            "-m",
            "types and mode",
            "--path",
            "mode",
            "--path",
            "file",
            "--path",
            "dir",
            "--json",
        ],
    );
    assert_eq!(change["object"]["changedPaths"], 5);
    let store = FileObjectStore::new(root.join(".sorrel")).unwrap();
    let id = change["object"]["resultingSnapshot"]["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    let snapshot = read_snapshot(&store, &id).unwrap();
    let entries = read_tree(&store, &snapshot.root_tree.id).unwrap().entries;
    assert_eq!(
        entries.iter().find(|e| e.name == "mode").unwrap().mode,
        EntryMode::Executable
    );
    assert_eq!(
        entries
            .iter()
            .find(|e| e.name == "later-mode")
            .unwrap()
            .mode,
        EntryMode::Normal
    );
    assert_eq!(fs::read(root.join("file/new")).unwrap(), b"new");
    assert_eq!(fs::read(root.join("dir")).unwrap(), b"replacement");
    let later = command(
        root,
        &["change", "create", "-m", "remaining mode", "--json"],
    );
    assert_eq!(
        later["object"]["diff"]["modified"],
        serde_json::json!(["later-mode"])
    );
}
