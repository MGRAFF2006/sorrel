use assert_cmd::Command;
use serde_json::Value;
use sorrel_core::{FileObjectStore, ObjectId, ObjectStore};
use std::{collections::BTreeMap, fs, path::Path, time::Duration};
use tempfile::TempDir;

fn command(root: &Path, args: &[&str]) -> Value {
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

fn objects(root: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut objects = BTreeMap::new();
    for shard in fs::read_dir(root.join(".sorrel/objects")).unwrap() {
        for object in fs::read_dir(shard.unwrap().path()).unwrap() {
            let path = object.unwrap().path();
            objects.insert(
                path.file_name().unwrap().to_string_lossy().into_owned(),
                fs::read(path).unwrap(),
            );
        }
    }
    objects
}

fn assert_no_preview(root: &Path) {
    assert_eq!(fs::read_dir(root.join(".sorrel/tmp")).unwrap().count(), 0);
}

fn assert_valid_cache(root: &Path) {
    let cache: Value =
        serde_json::from_slice(&fs::read(root.join(".sorrel/stat-cache.json")).unwrap()).unwrap();
    let store = FileObjectStore::new(root.join(".sorrel")).unwrap();
    for entry in cache["entries"].as_object().unwrap().values() {
        let id: ObjectId = entry["objectId"].as_str().unwrap().parse().unwrap();
        assert!(store.has(&id).unwrap());
    }
}

#[test]
fn repeated_clean_and_dirty_status_preserve_durable_objects() {
    let root = TempDir::new().unwrap();
    fs::write(root.path().join("tracked.txt"), "committed\n").unwrap();
    command(root.path(), &["init"]);
    command(root.path(), &["change", "create", "-m", "baseline"]);
    let before = objects(root.path());
    let head = fs::read(root.path().join(".sorrel/HEAD")).unwrap();
    for iteration in 0..3 {
        if iteration > 0 {
            std::thread::sleep(Duration::from_millis(1050));
        }
        let status = command(root.path(), &["status"]);
        assert_eq!(status["worktree"]["dirty"], false);
        assert_eq!(objects(root.path()), before);
        assert_no_preview(root.path());
        assert_valid_cache(root.path());
    }
    fs::write(root.path().join("tracked.txt"), "modified tracked bytes\n").unwrap();
    fs::write(root.path().join("added.txt"), "uncommitted added bytes\n").unwrap();
    for _ in 0..3 {
        let status = command(root.path(), &["status"]);
        assert_eq!(status["worktree"]["dirty"], true);
        assert_eq!(
            status["worktree"]["changes"]["modified"],
            serde_json::json!(["tracked.txt"])
        );
        assert_eq!(
            status["worktree"]["changes"]["added"],
            serde_json::json!(["added.txt"])
        );
        assert_eq!(objects(root.path()), before);
        assert_no_preview(root.path());
        assert_valid_cache(root.path());
    }
    assert_eq!(fs::read(root.path().join(".sorrel/HEAD")).unwrap(), head);
    command(
        root.path(),
        &["change", "create", "-m", "record previewed changes"],
    );
    assert!(objects(root.path()).len() > before.len());
    let committed = objects(root.path());
    assert_eq!(command(root.path(), &["status"])["status"], "clean");
    assert_eq!(objects(root.path()), committed);
    assert_valid_cache(root.path());
    assert_no_preview(root.path());
}

#[cfg(unix)]
#[test]
fn failed_status_cleans_preview_and_preserves_metadata() {
    let root = TempDir::new().unwrap();
    command(root.path(), &["init"]);
    command(root.path(), &["status"]);
    let before = objects(root.path());
    let head = fs::read(root.path().join(".sorrel/HEAD")).unwrap();
    let cache = fs::read(root.path().join(".sorrel/stat-cache.json")).unwrap();
    fs::write(root.path().join("a-new.txt"), "previewed before failure\n").unwrap();
    std::os::unix::fs::symlink("a-new.txt", root.path().join("z-unsupported-link")).unwrap();
    let output = Command::cargo_bin("sorrel")
        .unwrap()
        .current_dir(root.path())
        .args(["status", "--json"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert_eq!(objects(root.path()), before);
    assert_eq!(fs::read(root.path().join(".sorrel/HEAD")).unwrap(), head);
    assert_eq!(
        fs::read(root.path().join(".sorrel/stat-cache.json")).unwrap(),
        cache
    );
    assert_no_preview(root.path());
    fs::remove_file(root.path().join("z-unsupported-link")).unwrap();
    assert_eq!(command(root.path(), &["status"])["status"], "dirty");
    assert_no_preview(root.path());
}

#[cfg(unix)]
#[test]
fn status_refuses_a_symlinked_temporary_directory() {
    let root = TempDir::new().unwrap();
    let outside = TempDir::new().unwrap();
    command(root.path(), &["init"]);
    fs::remove_dir(root.path().join(".sorrel/tmp")).unwrap();
    std::os::unix::fs::symlink(outside.path(), root.path().join(".sorrel/tmp")).unwrap();
    let output = Command::cargo_bin("sorrel")
        .unwrap()
        .current_dir(root.path())
        .args(["status", "--json"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("not a symlink"));
    assert_eq!(fs::read_dir(outside.path()).unwrap().count(), 0);
}

#[test]
fn status_rejects_a_corrupt_durable_blob_even_on_a_cache_miss() {
    let root = TempDir::new().unwrap();
    command(root.path(), &["init"]);
    fs::write(root.path().join("tracked.txt"), "unchanged bytes\n").unwrap();
    command(root.path(), &["change", "create", "-m", "baseline"]);
    let cache_path = root.path().join(".sorrel/stat-cache.json");
    let cache: Value = serde_json::from_slice(&fs::read(&cache_path).unwrap()).unwrap();
    let id = cache["entries"]["tracked.txt"]["objectId"]
        .as_str()
        .unwrap();
    let blob = root
        .path()
        .join(".sorrel/objects")
        .join(&id[..2])
        .join(&id[2..]);
    fs::write(&blob, b"corrupt durable blob").unwrap();
    fs::remove_file(&cache_path).unwrap();
    let before = objects(root.path());
    let head = fs::read(root.path().join(".sorrel/HEAD")).unwrap();
    let output = Command::cargo_bin("sorrel")
        .unwrap()
        .current_dir(root.path())
        .args(["status", "--json"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("content digest mismatch"));
    assert_eq!(objects(root.path()), before);
    assert_eq!(fs::read(root.path().join(".sorrel/HEAD")).unwrap(), head);
    assert!(!cache_path.exists());
    assert_no_preview(root.path());
}
