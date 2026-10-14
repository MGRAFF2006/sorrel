use assert_cmd::Command;
use serde_json::Value;
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

fn incoming_lane(root: &Path, directory: bool) -> String {
    sorrel(root, &["init"]);
    fs::write(root.join(".sorrelignore"), "data\n").unwrap();
    fs::write(root.join("keep.txt"), "keep me\n").unwrap();
    sorrel(root, &["change", "create", "-m", "base"]);
    let lane = sorrel(root, &["lane", "create", "--name", "incoming"])["object"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    sorrel(root, &["lane", "switch", &lane]);
    fs::write(root.join(".sorrelignore"), "").unwrap();
    fs::remove_file(root.join("keep.txt")).unwrap();
    if directory {
        fs::create_dir(root.join("data")).unwrap();
        fs::write(root.join("data/a.txt"), "incoming\n").unwrap();
    } else {
        fs::write(root.join("data"), "incoming\n").unwrap();
    }
    sorrel(root, &["change", "create", "-m", "incoming"]);
    sorrel(root, &["lane", "switch", "lane_main"]);
    lane
}

#[test]
fn restore_collision_preserves_ignored_files_and_head_before_deletions() {
    for directory in [false, true] {
        let workspace = tempfile::tempdir().unwrap();
        let root = workspace.path();
        let lane = incoming_lane(root, directory);
        let head = fs::read(root.join(".sorrel/HEAD")).unwrap();
        if directory {
            fs::create_dir(root.join("data")).unwrap();
            fs::write(root.join("data/a.txt"), "valuable\n").unwrap();
        } else {
            fs::write(root.join("data"), "valuable\n").unwrap();
        }
        Command::cargo_bin("sorrel")
            .unwrap()
            .current_dir(root)
            .args(["lane", "switch", &lane])
            .assert()
            .failure();
        assert_eq!(fs::read(root.join(".sorrel/HEAD")).unwrap(), head);
        assert_eq!(fs::read(root.join("keep.txt")).unwrap(), b"keep me\n");
        assert_eq!(
            fs::read(root.join(if directory { "data/a.txt" } else { "data" })).unwrap(),
            b"valuable\n"
        );
    }
}

#[cfg(unix)]
#[test]
fn ignored_symlink_cannot_redirect_restore_outside_workspace() {
    let workspace = tempfile::tempdir().unwrap();
    let root = workspace.path();
    let lane = incoming_lane(root, true);
    let external = tempfile::tempdir().unwrap();
    fs::write(external.path().join("a.txt"), "valuable\n").unwrap();
    std::os::unix::fs::symlink(external.path(), root.join("data")).unwrap();
    let head = fs::read(root.join(".sorrel/HEAD")).unwrap();
    Command::cargo_bin("sorrel")
        .unwrap()
        .current_dir(root)
        .args(["lane", "switch", &lane])
        .assert()
        .failure();
    assert_eq!(
        fs::read(external.path().join("a.txt")).unwrap(),
        b"valuable\n"
    );
    assert_eq!(fs::read(root.join("keep.txt")).unwrap(), b"keep me\n");
    assert_eq!(fs::read(root.join(".sorrel/HEAD")).unwrap(), head);
}

#[test]
fn empty_target_directory_cannot_replace_an_ignored_file() {
    let workspace = tempfile::tempdir().unwrap();
    let root = workspace.path();
    let lane = incoming_lane(root, true);
    sorrel(root, &["lane", "switch", &lane]);
    fs::remove_file(root.join("data/a.txt")).unwrap();
    sorrel(root, &["change", "create", "-m", "empty directory"]);
    sorrel(root, &["lane", "switch", "lane_main"]);
    fs::remove_dir(root.join("data")).unwrap();
    fs::write(root.join("data"), "valuable\n").unwrap();
    let head = fs::read(root.join(".sorrel/HEAD")).unwrap();
    Command::cargo_bin("sorrel")
        .unwrap()
        .current_dir(root)
        .args(["lane", "switch", &lane])
        .assert()
        .failure();
    assert_eq!(fs::read(root.join("data")).unwrap(), b"valuable\n");
    assert_eq!(fs::read(root.join("keep.txt")).unwrap(), b"keep me\n");
    assert_eq!(fs::read(root.join(".sorrel/HEAD")).unwrap(), head);
}

#[test]
fn tracked_file_directory_transitions_still_restore() {
    let workspace = tempfile::tempdir().unwrap();
    let root = workspace.path();
    sorrel(root, &["init"]);
    fs::write(root.join("data"), "file\n").unwrap();
    sorrel(root, &["change", "create", "-m", "file"]);
    let lane = sorrel(root, &["lane", "create", "--name", "directory"])["object"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    sorrel(root, &["lane", "switch", &lane]);
    fs::remove_file(root.join("data")).unwrap();
    fs::create_dir(root.join("data")).unwrap();
    fs::write(root.join("data/a.txt"), "directory\n").unwrap();
    sorrel(root, &["change", "create", "-m", "directory"]);
    for _ in 0..2 {
        sorrel(root, &["lane", "switch", "lane_main"]);
        assert_eq!(fs::read(root.join("data")).unwrap(), b"file\n");
        sorrel(root, &["lane", "switch", &lane]);
        assert_eq!(fs::read(root.join("data/a.txt")).unwrap(), b"directory\n");
    }
}

#[test]
fn invalid_target_hierarchy_is_rejected_before_deleting_current_files() {
    use sorrel_core::{
        write_blob, write_snapshot, write_tree, EntryMode, EntryType, FileObjectStore, ObjectKind,
        ObjectRef, SnapshotOptions, TreeEntry,
    };
    for child_path in ["a/x.txt", "outside.txt"] {
        let workspace = tempfile::tempdir().unwrap();
        let root = workspace.path();
        sorrel(root, &["init"]);
        fs::write(root.join("keep.txt"), "keep me\n").unwrap();
        sorrel(root, &["change", "create", "-m", "base"]);
        let lane = sorrel(root, &["lane", "create", "--name", "incoming"])["object"]["id"]
            .as_str()
            .unwrap()
            .to_owned();
        let head = fs::read(root.join(".sorrel/HEAD")).unwrap();
        let store = FileObjectStore::open_existing(root.join(".sorrel")).unwrap();
        let blob = write_blob(&store, b"incoming").unwrap();
        let file = |name: &str, path: &str| TreeEntry {
            name: name.to_owned(),
            path: path.into(),
            entry_type: EntryType::File,
            object: ObjectRef::new(ObjectKind::Blob, blob.id),
            mode: EntryMode::Normal,
            size: None,
            content_hash: None,
        };
        let child = write_tree(&store, vec![file("x.txt", child_path)]).unwrap();
        let mut entries = vec![TreeEntry {
            name: "a".to_owned(),
            path: "a".into(),
            entry_type: EntryType::Directory,
            object: ObjectRef::new(ObjectKind::Tree, child.id),
            mode: EntryMode::Directory,
            size: None,
            content_hash: None,
        }];
        if child_path == "a/x.txt" {
            entries.push(file("a", "a"));
        }
        let tree = write_tree(&store, entries).unwrap();
        let snapshot =
            write_snapshot(&store, tree.id, SnapshotOptions::new("repo_invalid")).unwrap();
        fs::write(
            root.join(".sorrel/heads").join(&lane),
            serde_json::to_vec(&serde_json::json!({ "snapshot": snapshot.id.to_hex() })).unwrap(),
        )
        .unwrap();
        Command::cargo_bin("sorrel")
            .unwrap()
            .current_dir(root)
            .args(["lane", "switch", &lane])
            .assert()
            .failure();
        assert_eq!(fs::read(root.join("keep.txt")).unwrap(), b"keep me\n");
        assert_eq!(fs::read(root.join(".sorrel/HEAD")).unwrap(), head);
        assert!(!root.join("a").exists());
        assert!(!root.join("outside.txt").exists());
    }
}

#[cfg(unix)]
#[test]
fn restore_refuses_to_modify_hard_linked_files() {
    let workspace = tempfile::tempdir().unwrap();
    let root = workspace.path();
    sorrel(root, &["init"]);
    fs::write(root.join("file.txt"), "base\n").unwrap();
    sorrel(root, &["change", "create", "-m", "base"]);
    let lane = sorrel(root, &["lane", "create", "--name", "incoming"])["object"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    sorrel(root, &["lane", "switch", &lane]);
    fs::write(root.join("file.txt"), "incoming\n").unwrap();
    sorrel(root, &["change", "create", "-m", "incoming"]);
    sorrel(root, &["lane", "switch", "lane_main"]);
    let external = tempfile::tempdir().unwrap();
    fs::hard_link(root.join("file.txt"), external.path().join("linked.txt")).unwrap();
    let head = fs::read(root.join(".sorrel/HEAD")).unwrap();
    Command::cargo_bin("sorrel")
        .unwrap()
        .current_dir(root)
        .args(["lane", "switch", &lane])
        .assert()
        .failure();
    assert_eq!(
        fs::read(external.path().join("linked.txt")).unwrap(),
        b"base\n"
    );
    assert_eq!(fs::read(root.join(".sorrel/HEAD")).unwrap(), head);
}
