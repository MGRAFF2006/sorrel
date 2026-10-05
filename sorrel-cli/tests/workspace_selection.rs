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
