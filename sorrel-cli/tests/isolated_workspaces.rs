//! Every repository/directory in this suite is disposable; no real workspace is opened.
use assert_cmd::Command;
use serde_json::{json, Value};
use sorrel_core::ObjectId;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Output,
};

fn run(root: &Path, args: &[&str]) -> Output {
    Command::cargo_bin("sorrel")
        .unwrap()
        .current_dir(root)
        .args(args)
        .arg("--json")
        .output()
        .unwrap()
}
fn ok(root: &Path, args: &[&str]) -> Value {
    let output = run(root, args);
    assert!(
        output.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}
fn fail(root: &Path, args: &[&str], message: &str) {
    let output = run(root, args);
    assert!(!output.status.success(), "{args:?} unexpectedly succeeded");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains(message),
        "{args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
fn read(path: impl AsRef<Path>) -> Value {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}
fn write(path: impl AsRef<Path>, value: &Value) {
    fs::write(path, serde_json::to_vec(value).unwrap()).unwrap();
}
fn head(root: &Path) -> Value {
    read(root.join(".sorrel/HEAD"))
}
fn record(root: &Path, text: &str) -> Value {
    fs::write(root.join("same.txt"), text).unwrap();
    ok(root, &["change", "create", "-m", text])
}
struct Fixture {
    _temp: tempfile::TempDir,
    main: PathBuf,
    other: PathBuf,
    lane: String,
}
impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let main = temp.path().join("main");
        let other = temp.path().join("other");
        fs::create_dir(&main).unwrap();
        ok(&main, &["init"]);
        record(&main, "base\n");
        let lane = ok(&main, &["lane", "create", "--name", "agent-b"])["object"]["id"]
            .as_str()
            .unwrap()
            .to_owned();
        ok(&main, &["workspace", "enable", "--apply"]);
        ok(&main, &["workspace", "add", &lane, other.to_str().unwrap()]);
        Self {
            _temp: temp,
            main,
            other,
            lane,
        }
    }
    fn common(&self) -> PathBuf {
        self.main.join(".sorrel")
    }
}

#[test]
fn independent_native_files_selected_full_recording_and_shared_heads_objects() {
    let f = Fixture::new();
    assert_eq!(fs::read(f.other.join("same.txt")).unwrap(), b"base\n");
    assert!(!f.other.join(".sorrel/objects").exists());
    fs::write(f.main.join("same.txt"), "agent a\n").unwrap();
    fs::write(f.other.join("same.txt"), "agent b\n").unwrap();
    fs::write(f.other.join("unselected.txt"), "later\n").unwrap();
    let main = ok(&f.main, &["change", "create", "-m", "a"]);
    let selected = ok(
        &f.other,
        &["change", "create", "--path", "same.txt", "-m", "b selected"],
    );
    assert_eq!(selected["object"]["changedPaths"], 1);
    assert_eq!(fs::read(f.main.join("same.txt")).unwrap(), b"agent a\n");
    let main_snapshot = main["object"]["resultingSnapshot"]["id"].as_str().unwrap();
    assert_eq!(head(&f.main)["snapshot"], main_snapshot);
    assert_eq!(
        head(&f.other)["snapshot"],
        selected["object"]["resultingSnapshot"]["id"]
    );
    let lanes = ok(&f.main, &["lane", "list"]);
    let remote = lanes["objects"]
        .as_array()
        .unwrap()
        .iter()
        .find(|l| l["id"] == f.lane)
        .unwrap();
    assert_eq!(remote["headSnapshot"]["id"], head(&f.other)["snapshot"]);
    let full = ok(&f.other, &["change", "create", "-m", "remaining"]);
    assert_eq!(full["object"]["changedPaths"], 1);
    assert_eq!(ok(&f.other, &["status"])["status"], "clean");
    let index = fs::read_to_string(f.common().join("changes.index")).unwrap();
    assert!(index.contains(main_snapshot));
    assert!(index.contains(full["object"]["id"].as_str().unwrap()));
    let store = sorrel_core::FileObjectStore::open_existing(f.common()).unwrap();
    sorrel_core::read_snapshot(
        &store,
        &sorrel_core::parse_object_id_hex(main_snapshot).unwrap(),
    )
    .unwrap();
    let list = ok(&f.other, &["workspace", "list"]);
    assert_eq!(list["workspaces"].as_object().unwrap().len(), 2);
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        assert_ne!(
            fs::metadata(f.main.join("same.txt")).unwrap().ino(),
            fs::metadata(f.other.join("same.txt")).unwrap().ino()
        );
    }
}

#[test]
fn merge_conflicts_cache_checkpoints_and_undo_are_directory_local() {
    let f = Fixture::new();
    record(&f.main, "a\n");
    record(&f.other, "b\n");
    ok(&f.main, &["status"]);
    ok(&f.main, &["checkpoint", "create", "main-tip"]);
    let untouched_head = fs::read(f.main.join(".sorrel/HEAD")).unwrap();
    let untouched_cache = fs::read(f.main.join(".sorrel/stat-cache.json")).unwrap();
    let untouched_log = ok(&f.main, &["operation", "log"]);
    let conflict = run(&f.other, &["merge", "lane_main"]);
    assert!(!conflict.status.success());
    assert!(f.other.join(".sorrel/MERGE_STATE").exists());
    assert!(!f.main.join(".sorrel/MERGE_STATE").exists());
    fail(
        &f.other,
        &["change", "create", "--path", "same.txt", "-m", "no"],
        "pending merge",
    );
    ok(&f.other, &["merge", "--abort"]);
    assert_eq!(fs::read(f.other.join("same.txt")).unwrap(), b"b\n");
    ok(&f.other, &["checkpoint", "create", "b-tip"]);
    record(&f.other, "b later\n");
    ok(&f.other, &["undo"]);
    assert_eq!(fs::read(f.other.join("same.txt")).unwrap(), b"b\n");
    ok(&f.other, &["checkpoint", "restore", "b-tip"]);
    assert_eq!(ok(&f.other, &["checkpoint", "list"])["count"], 1);
    assert_eq!(
        ok(&f.main, &["checkpoint", "list"])["checkpoints"][0]["name"],
        "main-tip"
    );
    assert_eq!(
        fs::read(f.main.join(".sorrel/HEAD")).unwrap(),
        untouched_head
    );
    assert_eq!(
        fs::read(f.main.join(".sorrel/stat-cache.json")).unwrap(),
        untouched_cache
    );
    assert_eq!(ok(&f.main, &["operation", "log"]), untouched_log);
    assert_eq!(fs::read(f.main.join("same.txt")).unwrap(), b"a\n");
}

#[test]
fn merging_recorded_work_leaves_other_dirty_checkout_untouched() {
    let f = Fixture::new();
    fs::write(f.main.join("a.txt"), "a\n").unwrap();
    ok(&f.main, &["change", "create", "-m", "a"]);
    fs::write(f.other.join("b.txt"), "b\n").unwrap();
    ok(&f.other, &["change", "create", "-m", "b"]);
    fs::write(f.other.join("same.txt"), "unrecorded agent b edit\n").unwrap();
    let other_head = head(&f.other);
    ok(&f.main, &["merge", &f.lane]);
    assert_eq!(fs::read(f.main.join("b.txt")).unwrap(), b"b\n");
    assert!(!f.other.join("a.txt").exists());
    assert_eq!(
        fs::read(f.other.join("same.txt")).unwrap(),
        b"unrecorded agent b edit\n"
    );
    assert_eq!(head(&f.other), other_head);
}

#[test]
fn ownership_fixed_lanes_git_rejection_and_common_lock_precede_side_effects() {
    let f = Fixture::new();
    let original = head(&f.main);
    for (root, lane) in [(&f.main, f.lane.as_str()), (&f.other, "lane_main")] {
        fail(root, &["lane", "switch", lane], "pinned");
    }
    let nested = f.main.join("nested");
    let already = f._temp.path().join("already");
    fs::create_dir(&already).unwrap();
    fail(
        &f.main,
        &["workspace", "add", &f.lane, nested.to_str().unwrap()],
        "owned",
    );
    fail(
        &f.main,
        &["workspace", "add", "lane_main", already.to_str().unwrap()],
        "owned",
    );
    for args in [
        &["git", "import"][..],
        &["git", "export"][..],
        &["git", "sync"][..],
    ] {
        fail(&f.other, args, "unavailable in linked workspaces");
    }
    assert!(!f.other.join(".git").exists());
    assert!(!f.other.join(".sorrel/git-map.json").exists());
    let guard = fs::File::open(f.common().join("write.lock")).unwrap();
    fs2::FileExt::try_lock_exclusive(&guard).unwrap();
    for root in [&f.main, &f.other] {
        fail(root, &["change", "create", "-m", "blocked"], "busy");
    }
    drop(guard);
    assert_eq!(head(&f.main), original);
    ok(&f.other, &["status"]);
}

#[test]
fn shared_provider_registry_protects_full_selected_and_explained_paths_without_copying_values() {
    let f = Fixture::new();
    fs::create_dir_all(f.common().join("secrets")).unwrap();
    write(
        f.common().join("secrets/credential.json"),
        &json!({"id":"credential", "provider":"dotenv:credentials.txt"}),
    );
    fs::write(
        f.main.join("credentials.txt"),
        "SYNTHETIC_MAIN_SECRET=never-copy",
    )
    .unwrap();
    fs::write(
        f.other.join("credentials.txt"),
        "SYNTHETIC_OTHER_SECRET=never-record",
    )
    .unwrap();
    for root in [&f.main, &f.other] {
        assert_eq!(
            ok(root, &["path", "explain", "credentials.txt"])["protected"],
            true
        );
        fail(
            root,
            &["change", "create", "--path", "credentials.txt", "-m", "no"],
            "selector",
        );
        fs::write(root.join("safe.txt"), "safe").unwrap();
        let capture = ok(root, &["change", "create", "-m", "safe only"]);
        assert_eq!(capture["object"]["changedPaths"], 1);
    }
    assert!(!f.other.join(".sorrel/secrets").exists());
    let store = sorrel_core::FileObjectStore::open_existing(f.common()).unwrap();
    for root in [&f.main, &f.other] {
        let id =
            sorrel_core::parse_object_id_hex(head(root)["snapshot"].as_str().unwrap()).unwrap();
        assert!(!sorrel_core::read_snapshot_files(&store, &id)
            .unwrap()
            .contains_key(Path::new("credentials.txt")));
    }
}

#[test]
fn explicit_enable_preview_durable_backup_and_failed_preflight_preserve_legacy() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    ok(root, &["init"]);
    record(root, "before");
    ok(root, &["checkpoint", "create", "before"]);
    let metadata = root.join(".sorrel");
    let manifest = fs::read(metadata.join("manifest.json")).unwrap();
    let head_before = fs::read(metadata.join("HEAD")).unwrap();
    let index = fs::read(metadata.join("changes.index")).unwrap();
    let preview = ok(root, &["workspace", "enable"]);
    assert_eq!(preview["status"], "preview");
    assert_eq!(fs::read(metadata.join("manifest.json")).unwrap(), manifest);
    assert!(!metadata.join("workspace-backups").exists());
    fs::write(
        metadata.join("workspace-backups"),
        "blocked backup directory",
    )
    .unwrap();
    fail(root, &["workspace", "enable", "--apply"], "File exists");
    assert_eq!(fs::read(metadata.join("manifest.json")).unwrap(), manifest);
    fs::remove_file(metadata.join("workspace-backups")).unwrap();
    fs::write(metadata.join("MERGE_STATE"), "pending").unwrap();
    fail(root, &["workspace", "enable", "--apply"], "pending merge");
    fs::remove_file(metadata.join("MERGE_STATE")).unwrap();
    assert_eq!(fs::read(metadata.join("manifest.json")).unwrap(), manifest);
    let result = ok(root, &["workspace", "enable", "--apply"]);
    let backup = Path::new(result["backup"].as_str().unwrap());
    assert_eq!(
        fs::read(backup.join("metadata/manifest.json")).unwrap(),
        manifest
    );
    assert_eq!(fs::read(backup.join("metadata/HEAD")).unwrap(), head_before);
    assert_eq!(
        fs::read(backup.join("metadata/changes.index")).unwrap(),
        index
    );
    assert!(!backup.join("metadata/objects").exists());
    for (relative, digest) in read(backup.join("backup.json"))["files"]
        .as_object()
        .unwrap()
    {
        assert_eq!(
            ObjectId::for_bytes(&fs::read(backup.join("metadata").join(relative)).unwrap())
                .to_hex(),
            digest.as_str().unwrap()
        );
    }
    assert_ne!(
        read(metadata.join("manifest.json"))["schemaVersion"],
        "sorrel.protocol.v0"
    );
    // The unchanged pre-enable metadata remains a working standalone format when restored in isolation.
    fs::write(metadata.join("manifest.json"), manifest).unwrap();
    ok(root, &["status"]);
    record(root, "standalone remains writable");
}

/// Capture a real CLI publication's bytes, then simulate a crash with staged local/shared destinations.
fn interrupted_record(f: &Fixture) -> Value {
    let old_index = fs::read(f.common().join("changes.index")).unwrap();
    let old_head_bytes = fs::read(f.other.join(".sorrel/HEAD")).unwrap();
    let old_lane_bytes = fs::read(f.common().join("heads").join(&f.lane)).unwrap();
    record(&f.other, "recover this\n");
    let operations = f.other.join(".sorrel/operations");
    let operation = fs::read_dir(&operations)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let after = head(&f.other);
    let link = fs::read(f.other.join(".sorrel/manifest.json")).unwrap();
    let link_value: Value = serde_json::from_slice(&link).unwrap();
    let manifest = read(f.common().join("manifest.json"));
    let mut entries = Vec::new();
    for (scope, base, relative) in [
        (
            "workspace",
            f.other.join(".sorrel"),
            PathBuf::from("operations").join(operation.file_name().unwrap()),
        ),
        ("common", f.common(), PathBuf::from("changes.index")),
        ("common", f.common(), PathBuf::from("heads").join(&f.lane)),
        ("workspace", f.other.join(".sorrel"), PathBuf::from("HEAD")),
    ] {
        let bytes = fs::read(base.join(&relative)).unwrap();
        let temporary = relative
            .parent()
            .unwrap()
            .join(format!(".sorrel-{}.tmp", entries.len()));
        fs::write(base.join(&temporary), &bytes).unwrap();
        entries.push(json!({"scope":scope,"path":relative,"temporary":temporary,"digest":ObjectId::for_bytes(&bytes).to_hex(), "beforeDigest": match relative.to_str().unwrap() {
                "HEAD" => Some(ObjectId::for_bytes(&old_head_bytes).to_hex()),
                "changes.index" => Some(ObjectId::for_bytes(&old_index).to_hex()),
                name if name.starts_with("heads/") => Some(ObjectId::for_bytes(&old_lane_bytes).to_hex()),
                _=>None,
            }}));
    }
    fs::remove_file(operation).unwrap();
    fs::write(f.other.join(".sorrel/HEAD"), old_head_bytes).unwrap();
    fs::write(f.common().join("heads").join(&f.lane), old_lane_bytes).unwrap();
    fs::write(f.common().join("changes.index"), old_index).unwrap();
    let journal = json!({"version":1,"repoId":manifest["repoId"],"workspaceId":link_value["workspaceId"],
        "binding":manifest["workspaces"][link_value["workspaceId"].as_str().unwrap()],
        "linkDigest":ObjectId::for_bytes(&link).to_hex(),"entries":entries});
    write(f.common().join("metadata-transaction.json"), &journal);
    after
}

#[test]
fn common_lock_replays_interrupted_cross_root_publication_before_another_workspace_can_publish() {
    let f = Fixture::new();
    let expected = interrupted_record(&f);
    let main_before = head(&f.main);
    let journal = read(f.common().join("metadata-transaction.json"));
    // First publication already renamed: replay must accept its exact digest/history and finish the rest.
    let entry = &journal["entries"][0];
    fs::rename(
        f.other
            .join(".sorrel")
            .join(entry["temporary"].as_str().unwrap()),
        f.other
            .join(".sorrel")
            .join(entry["path"].as_str().unwrap()),
    )
    .unwrap();
    fs::write(f.main.join("a.txt"), "another command\n").unwrap();
    ok(&f.main, &["change", "create", "-m", "after recovery"]);
    assert_eq!(head(&f.other), expected);
    assert_ne!(head(&f.main), main_before);
    assert_eq!(
        read(f.common().join("heads").join(&f.lane))["snapshot"],
        expected["snapshot"]
    );
    assert!(!f.common().join("metadata-transaction.json").exists());
    assert_eq!(ok(&f.other, &["operation", "log"])["count"], 1);
    ok(&f.other, &["status"]);
}

#[test]
fn invalid_cross_root_recovery_fails_closed_before_any_ref_or_outside_write() {
    for case in [
        "stale-link",
        "missing-link",
        "missing-root",
        "foreign-head",
        "ref-mismatch",
        "missing-object",
        "scope",
        "traversal",
        "index",
        "stale-index",
        "link-digest",
        "missing-blob",
        "payload-ref",
    ] {
        let f = Fixture::new();
        let expected = interrupted_record(&f);
        let main_before = fs::read(f.main.join(".sorrel/HEAD")).unwrap();
        let lane_before = fs::read(f.common().join("heads").join(&f.lane)).unwrap();
        let local_before = fs::read(f.other.join(".sorrel/HEAD")).unwrap();
        let sentinel = f._temp.path().join("outside");
        fs::write(&sentinel, "unchanged").unwrap();
        let path = f.common().join("metadata-transaction.json");
        let mut journal = read(&path);
        match case {
            "link-digest" => {
                let link = f.other.join(".sorrel/manifest.json");
                let mut bytes = fs::read(&link).unwrap();
                bytes.push(b'\n');
                fs::write(link, bytes).unwrap();
            }
            "missing-blob" => {
                let store = sorrel_core::FileObjectStore::open_existing(f.common()).unwrap();
                let snapshot = sorrel_core::read_snapshot(
                    &store,
                    &sorrel_core::parse_object_id_hex(expected["snapshot"].as_str().unwrap())
                        .unwrap(),
                )
                .unwrap();
                let tree = sorrel_core::read_tree(&store, &snapshot.root_tree.id).unwrap();
                let id = tree
                    .entries
                    .iter()
                    .find(|e| e.path == Path::new("same.txt"))
                    .unwrap()
                    .object
                    .id
                    .to_hex();
                fs::remove_file(f.common().join("objects").join(&id[..2]).join(&id[2..])).unwrap();
            }
            "payload-ref" => {
                let entry = &mut journal["entries"][3];
                let bytes = serde_json::to_vec(
                    &json!({"lane":f.lane,"snapshot":head(&f.main)["snapshot"]}),
                )
                .unwrap();
                fs::write(
                    f.other
                        .join(".sorrel")
                        .join(entry["temporary"].as_str().unwrap()),
                    &bytes,
                )
                .unwrap();
                entry["digest"] = json!(ObjectId::for_bytes(&bytes).to_hex());
            }
            "stale-link" => {
                let link = f.other.join(".sorrel/manifest.json");
                let mut value = read(&link);
                value["repoId"] = json!("different");
                write(link, &value);
            }
            "missing-link" => {
                fs::remove_file(f.other.join(".sorrel/manifest.json")).unwrap();
            }
            "missing-root" => {
                fs::rename(&f.other, f._temp.path().join("moved")).unwrap();
            }
            "foreign-head" => {
                journal["entries"][2]["path"] = json!("heads/lane_main");
            }
            "ref-mismatch" => {
                write(
                    f.common().join("heads").join(&f.lane),
                    &json!({"snapshot":head(&f.main)["snapshot"]}),
                );
                // main and lane began at same base; use a valid unrelated recorded snapshot instead.
                let store = sorrel_core::FileObjectStore::open_existing(f.common()).unwrap();
                let tree = sorrel_core::write_tree(&store, vec![]).unwrap();
                let snap = sorrel_core::write_snapshot(
                    &store,
                    tree.id,
                    sorrel_core::SnapshotOptions::new("unrelated"),
                )
                .unwrap();
                write(
                    f.common().join("heads").join(&f.lane),
                    &json!({"snapshot":snap.id.to_hex()}),
                );
            }
            "missing-object" => {
                let id = expected["snapshot"].as_str().unwrap();
                // Object-store shard layout, deleting only this disposable test's new snapshot.
                let object = f.common().join("objects").join(&id[..2]).join(&id[2..]);
                fs::remove_file(object).unwrap();
            }
            "scope" => journal["entries"][3]["scope"] = json!("common"),
            "traversal" => journal["entries"][3]["temporary"] = json!("../../outside"),
            "stale-index" => {
                fs::write(
                    f.common().join("changes.index"),
                    "independently changed shared index\n",
                )
                .unwrap();
            }
            "index" => {
                let entry = &mut journal["entries"][1];
                let bytes = b"{\"snapshot\":\"bad\",\"change\":\"bad\"}\n";
                fs::write(f.common().join(entry["temporary"].as_str().unwrap()), bytes).unwrap();
                entry["digest"] = json!(ObjectId::for_bytes(bytes).to_hex());
            }
            _ => unreachable!(),
        }
        write(&path, &journal);
        let lane_before = if case == "ref-mismatch" {
            fs::read(f.common().join("heads").join(&f.lane)).unwrap()
        } else {
            lane_before
        };
        let result = run(&f.main, &["status"]);
        assert!(
            !result.status.success(),
            "case {case} unexpectedly recovered"
        );
        assert_eq!(
            fs::read(f.main.join(".sorrel/HEAD")).unwrap(),
            main_before,
            "{case}"
        );
        assert_eq!(
            fs::read(f.common().join("heads").join(&f.lane)).unwrap(),
            lane_before,
            "{case}"
        );
        if case != "missing-root" {
            assert_eq!(
                fs::read(f.other.join(".sorrel/HEAD")).unwrap(),
                local_before,
                "{case}"
            );
        }
        assert_eq!(fs::read(&sentinel).unwrap(), b"unchanged", "{case}");
        assert!(path.exists(), "{case} consumed invalid journal");
    }
}

#[cfg(unix)]
#[test]
fn linked_metadata_symlinks_hardlinks_and_unregistered_links_are_rejected_without_outside_writes() {
    use std::os::unix::fs::symlink;
    for case in [
        "root-symlink",
        "head-symlink",
        "head-hardlink",
        "unregistered",
    ] {
        let f = Fixture::new();
        let outside = f._temp.path().join("outside");
        fs::create_dir(&outside).unwrap();
        let original = head(&f.other);
        write(outside.join("HEAD"), &original);
        match case {
            "root-symlink" => {
                fs::rename(f.other.join(".sorrel"), outside.join("saved")).unwrap();
                symlink(outside.join("saved"), f.other.join(".sorrel")).unwrap();
            }
            "head-symlink" => {
                fs::remove_file(f.other.join(".sorrel/HEAD")).unwrap();
                symlink(outside.join("HEAD"), f.other.join(".sorrel/HEAD")).unwrap();
            }
            "head-hardlink" => {
                fs::remove_file(f.other.join(".sorrel/HEAD")).unwrap();
                fs::hard_link(outside.join("HEAD"), f.other.join(".sorrel/HEAD")).unwrap();
            }
            "unregistered" => {
                let path = f.common().join("manifest.json");
                let mut manifest = read(&path);
                manifest["workspaces"]
                    .as_object_mut()
                    .unwrap()
                    .retain(|id, _| id == "default");
                write(path, &manifest);
            }
            _ => unreachable!(),
        }
        assert!(!run(&f.other, &["status"]).status.success(), "{case}");
        assert_eq!(read(outside.join("HEAD")), original);
    }
}

#[test]
fn failed_creation_does_not_register_or_take_lane_ownership() {
    let f = Fixture::new();
    let lane = ok(&f.main, &["lane", "create", "--name", "free"])["object"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let before = fs::read(f.common().join("manifest.json")).unwrap();
    let nested = f.main.join("nested");
    fail(
        &f.main,
        &["workspace", "add", &lane, nested.to_str().unwrap()],
        "overlaps",
    );
    assert!(!nested.exists());
    let existing = f._temp.path().join("existing");
    fs::create_dir(&existing).unwrap();
    fs::write(existing.join("private.txt"), "untouched").unwrap();
    fail(
        &f.main,
        &["workspace", "add", &lane, existing.to_str().unwrap()],
        "exists",
    );
    assert_eq!(
        fs::read(existing.join("private.txt")).unwrap(),
        b"untouched"
    );
    assert_eq!(fs::read(f.common().join("manifest.json")).unwrap(), before);
    let new = f._temp.path().join("valid");
    ok(&f.main, &["workspace", "add", &lane, new.to_str().unwrap()]);
}

#[test]
fn add_refuses_recorded_provider_paths_before_copying_a_registered_secret() {
    let f = Fixture::new();
    fs::write(
        f.main.join("credentials.txt"),
        "synthetic once-recorded value",
    )
    .unwrap();
    ok(&f.main, &["change", "create", "-m", "legacy content"]);
    let lane = ok(&f.main, &["lane", "create", "--name", "legacy-secret"])["object"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    fs::create_dir(f.common().join("secrets")).unwrap();
    write(
        f.common().join("secrets/provider.json"),
        &json!({"provider":"dotenv:credentials.txt"}),
    );
    let destination = f._temp.path().join("must-not-copy");
    fail(
        &f.main,
        &["workspace", "add", &lane, destination.to_str().unwrap()],
        "protected",
    );
    assert!(!destination.exists());
}

#[test]
fn default_git_bridge_refuses_to_advance_a_divergence_lane_owned_by_another_directory() {
    fn git(root: &Path, args: &[&str]) {
        let result = std::process::Command::new("git")
            .current_dir(root)
            .args(args)
            .env("GIT_AUTHOR_NAME", "Test")
            .env("GIT_AUTHOR_EMAIL", "test@example.test")
            .env("GIT_COMMITTER_NAME", "Test")
            .env("GIT_COMMITTER_EMAIL", "test@example.test")
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
    let f = Fixture::new();
    let mirror = f._temp.path().join("mirror");
    fs::create_dir(&mirror).unwrap();
    ok(
        &f.main,
        &[
            "git",
            "export",
            mirror.to_str().unwrap(),
            "--branch",
            "main",
        ],
    );
    git(&mirror, &["checkout", "main"]);
    record(&f.main, "sorrel side\n");
    fs::write(mirror.join("git.txt"), "first git change\n").unwrap();
    git(&mirror, &["add", "git.txt"]);
    git(&mirror, &["commit", "-m", "git first"]);
    let parked = ok(
        &f.main,
        &["git", "sync", mirror.to_str().unwrap(), "--branch", "main"],
    );
    assert_eq!(parked["status"], "diverged");
    let lane = parked["lane"]["id"].as_str().unwrap();
    let checkout = f._temp.path().join("git-lane");
    ok(
        &f.main,
        &["workspace", "add", lane, checkout.to_str().unwrap()],
    );
    let before = fs::read(f.common().join("heads").join(lane)).unwrap();
    let registry = fs::read(f.common().join("lanes").join(format!("{lane}.json"))).unwrap();
    let owned_head = head(&checkout);
    fs::write(mirror.join("git.txt"), "second git change\n").unwrap();
    git(&mirror, &["add", "git.txt"]);
    git(&mirror, &["commit", "-m", "git second"]);
    fail(
        &f.main,
        &["git", "sync", mirror.to_str().unwrap(), "--branch", "main"],
        "owned by another workspace",
    );
    assert_eq!(
        fs::read(f.common().join("heads").join(lane)).unwrap(),
        before
    );
    assert_eq!(
        fs::read(f.common().join("lanes").join(format!("{lane}.json"))).unwrap(),
        registry
    );
    assert_eq!(head(&checkout), owned_head);
    assert_eq!(
        fs::read(checkout.join("git.txt")).unwrap(),
        b"first git change\n"
    );
}

#[test]
fn missing_link_cannot_be_reinitialized_as_a_second_repository() {
    let f = Fixture::new();
    fs::remove_file(f.other.join(".sorrel/manifest.json")).unwrap();
    let before = fs::read(f.other.join(".sorrel/HEAD")).unwrap();
    for args in [
        &["status"][..],
        &["init"][..],
        &["change", "create", "-m", "no"][..],
    ] {
        fail(&f.other, args, "missing workspace manifest");
    }
    assert_eq!(fs::read(f.other.join(".sorrel/HEAD")).unwrap(), before);
    assert!(!f.other.join(".sorrel/objects").exists());
    assert!(!f.other.join(".sorrel/write.lock").exists());
}

#[test]
fn invalid_identity_heads_and_pending_journal_fail_enable_before_original_metadata_changes() {
    for case in ["identity", "head", "journal"] {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        ok(root, &["init"]);
        let metadata = root.join(".sorrel");
        match case {
            "identity" => {
                let path = metadata.join("manifest.json");
                let mut value = read(&path);
                value["repoId"] = json!(null);
                write(path, &value);
            }
            "head" => {
                let path = metadata.join("HEAD");
                let mut value = read(&path);
                value["lane"] = json!("../lane_main");
                write(path, &value);
            }
            "journal" => {
                fs::write(metadata.join("metadata-transaction.json"), "[]").unwrap();
            }
            _ => unreachable!(),
        }
        let manifest = fs::read(metadata.join("manifest.json")).unwrap();
        let head = fs::read(metadata.join("HEAD")).unwrap();
        assert!(
            !run(root, &["workspace", "enable", "--apply"])
                .status
                .success(),
            "{case}"
        );
        assert_eq!(fs::read(metadata.join("manifest.json")).unwrap(), manifest);
        assert_eq!(fs::read(metadata.join("HEAD")).unwrap(), head);
        assert!(!metadata.join("workspace-backups").exists());
        if case == "journal" {
            assert_eq!(
                fs::read(metadata.join("metadata-transaction.json")).unwrap(),
                b"[]"
            );
        }
    }
}

#[cfg(target_os = "linux")]
#[test]
fn enabling_between_configuration_and_lock_acquisition_cannot_bypass_lane_ownership() {
    use std::{
        process::{Child, Command as Process, Stdio},
        thread,
        time::{Duration, Instant},
    };
    struct ChildGuard(Child);
    impl Drop for ChildGuard {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    let temp = tempfile::tempdir().unwrap();
    let main = temp.path().join("main");
    fs::create_dir(&main).unwrap();
    ok(&main, &["init"]);
    record(&main, "base\n");
    let lane = ok(&main, &["lane", "create", "--name", "owned"])["object"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    // Pause the actual CLI at libc's flock boundary; no product test hooks or timing race.
    let source = temp.path().join("pause.c");
    let library = temp.path().join("pause.so");
    fs::write(
        &source,
        r#"
#define _GNU_SOURCE
#include <dlfcn.h>
#include <sys/file.h>
#include <signal.h>
int flock(int fd, int op) {
    static int paused = 0;
    int (*real_flock)(int,int) = dlsym(RTLD_NEXT, "flock");
    if (!paused && (op & LOCK_EX)) { paused=1; raise(SIGSTOP); }
    return real_flock(fd,op);
}
"#,
    )
    .unwrap();
    let compile = Process::new("cc")
        .args(["-shared", "-fPIC", "-o"])
        .arg(&library)
        .arg(&source)
        .arg("-ldl")
        .output()
        .unwrap();
    assert!(
        compile.status.success(),
        "{}",
        String::from_utf8_lossy(&compile.stderr)
    );
    let mut child = ChildGuard(
        Process::new(assert_cmd::cargo::cargo_bin("sorrel"))
            .current_dir(&main)
            .args(["lane", "switch", &lane, "--json"])
            .env("LD_PRELOAD", &library)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let state = fs::read_to_string(format!("/proc/{}/status", child.0.id())).unwrap();
        if state
            .lines()
            .any(|line| line.starts_with("State:") && line.contains("T (stopped)"))
        {
            break;
        }
        assert!(
            child.0.try_wait().unwrap().is_none(),
            "CLI exited before flock pause"
        );
        assert!(Instant::now() < deadline, "CLI did not reach flock pause");
        thread::sleep(Duration::from_millis(10));
    }
    ok(&main, &["workspace", "enable", "--apply"]);
    let other = temp.path().join("other");
    ok(&main, &["workspace", "add", &lane, other.to_str().unwrap()]);
    let main_head = head(&main);
    let other_head = head(&other);
    assert!(Process::new("kill")
        .args(["-CONT", &child.0.id().to_string()])
        .status()
        .unwrap()
        .success());
    let deadline = Instant::now() + Duration::from_secs(10);
    while child.0.try_wait().unwrap().is_none() {
        assert!(Instant::now() < deadline, "resumed CLI did not finish");
        thread::sleep(Duration::from_millis(10));
    }
    let result = child.0.wait().unwrap();
    assert!(
        !result.success(),
        "stale configured CLI bypassed workspace ownership"
    );
    assert_eq!(head(&main), main_head);
    assert_eq!(head(&other), other_head);
    ok(&main, &["status"]);
    ok(&other, &["status"]);
}

#[test]
fn recovery_rejects_split_head_and_bound_lane_states_even_if_the_ref_is_omitted() {
    for omit_lane in [false, true] {
        let f = Fixture::new();
        interrupted_record(&f);
        let main_before = head(&f.main);
        let local_before = head(&f.other);
        let lane_before = fs::read(f.common().join("heads").join(&f.lane)).unwrap();
        let path = f.common().join("metadata-transaction.json");
        let mut journal = read(&path);
        journal["entries"]
            .as_array_mut()
            .unwrap()
            .retain(|entry| entry["path"] != format!("heads/{}", f.lane));
        let entry = &mut journal["entries"][0];
        let temporary = f
            .other
            .join(".sorrel")
            .join(entry["temporary"].as_str().unwrap());
        let mut operation = read(&temporary);
        if omit_lane {
            operation["before"]["laneHeads"]
                .as_object_mut()
                .unwrap()
                .remove(&f.lane);
            operation["after"]["laneHeads"]
                .as_object_mut()
                .unwrap()
                .remove(&f.lane);
        } else {
            operation["after"]["laneHeads"][&f.lane] = local_before["snapshot"].clone();
        }
        let bytes = serde_json::to_vec(&operation).unwrap();
        let digest = ObjectId::for_bytes(&bytes).to_hex();
        fs::write(temporary, bytes).unwrap();
        entry["digest"] = json!(digest);
        entry["path"] = json!(format!(
            "operations/{:020}-{digest}.json",
            operation["sequence"].as_u64().unwrap()
        ));
        write(&path, &journal);
        fail(
            &f.main,
            &["status"],
            "HEAD disagrees with authoritative lane state",
        );
        assert_eq!(head(&f.main), main_before);
        assert_eq!(head(&f.other), local_before);
        assert_eq!(
            fs::read(f.common().join("heads").join(&f.lane)).unwrap(),
            lane_before
        );
        assert!(path.exists());
    }
}

#[test]
fn creation_checks_shared_and_recorded_provider_paths_in_destination_coordinates() {
    for registry in [true, false] {
        let f = Fixture::new();
        let destination = f._temp.path().join("credential-destination");
        let provider = format!("dotenv:{}", destination.join("credentials.txt").display());
        fs::write(
            f.main.join("credentials.txt"),
            "synthetic recorded bytes must not be copied",
        )
        .unwrap();
        if !registry {
            fs::write(
                f.main.join("sorrel.secrets.yml"),
                format!("provider: '{provider}'\n"),
            )
            .unwrap();
        }
        ok(
            &f.main,
            &["change", "create", "-m", "recorded legacy paths"],
        );
        let lane = ok(&f.main, &["lane", "create", "--name", "destination"])["object"]["id"]
            .as_str()
            .unwrap()
            .to_owned();
        if registry {
            fs::create_dir(f.common().join("secrets")).unwrap();
            write(
                f.common().join("secrets/provider.json"),
                &json!({"provider":provider}),
            );
        } else {
            // The destination must be checked against recorded config even if
            // the invoking directory has since removed its working config.
            fs::remove_file(f.main.join("sorrel.secrets.yml")).unwrap();
        }
        let before = fs::read(f.common().join("manifest.json")).unwrap();
        fail(
            &f.main,
            &["workspace", "add", &lane, destination.to_str().unwrap()],
            "protected",
        );
        assert!(!destination.exists());
        assert_eq!(fs::read(f.common().join("manifest.json")).unwrap(), before);
    }
}

#[test]
fn enable_refuses_noncanonical_lane_refs_before_backup_or_format_change() {
    for inactive in [true, false] {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        ok(root, &["init"]);
        let lane = if inactive {
            ok(root, &["lane", "create", "--name", "bad-ref"])["object"]["id"]
                .as_str()
                .unwrap()
                .to_owned()
        } else {
            "lane_main".to_owned()
        };
        let metadata = root.join(".sorrel");
        let path = metadata.join("heads").join(lane);
        let mut value = read(&path);
        value["unexpected"] = json!(true);
        write(&path, &value);
        let manifest = fs::read(metadata.join("manifest.json")).unwrap();
        let pointer = fs::read(&path).unwrap();
        fail(root, &["workspace", "enable", "--apply"], "unknown field");
        assert_eq!(fs::read(metadata.join("manifest.json")).unwrap(), manifest);
        assert_eq!(fs::read(path).unwrap(), pointer);
        assert!(!metadata.join("workspace-backups").exists());
    }
}

#[test]
fn incoming_merge_cannot_restore_provider_paths_protected_in_the_receiving_directory() {
    for merge_kind in ["fast-forward", "clean-three-way", "conflicted"] {
        let f = Fixture::new();
        fs::create_dir(f.common().join("secrets")).unwrap();
        write(
            f.common().join("secrets/provider.json"),
            &json!({"provider":format!("dotenv:{}",f.main.join("credentials.txt").display())}),
        );
        if merge_kind != "fast-forward" {
            record(&f.main, "receiver diverges\n");
            if merge_kind == "conflicted" {
                record(&f.other, "incoming diverges\n");
            }
        }
        fs::write(
            f.other.join("credentials.txt"),
            "synthetic ordinary incoming file",
        )
        .unwrap();
        ok(
            &f.other,
            &["change", "create", "-m", "incoming credentials name"],
        );
        let before = head(&f.main);
        let other_before = head(&f.other);
        let history = ok(&f.main, &["operation", "log"]);
        let same = fs::read(f.main.join("same.txt")).unwrap();
        fail(&f.main, &["merge", &f.lane], "protected");
        assert_eq!(head(&f.main), before);
        assert_eq!(head(&f.other), other_before);
        assert_eq!(fs::read(f.main.join("same.txt")).unwrap(), same);
        assert_eq!(ok(&f.main, &["operation", "log"]), history);
        assert!(!f.main.join("credentials.txt").exists());
        assert!(!f.main.join(".sorrel/MERGE_STATE").exists());
        assert!(!f.common().join("metadata-transaction.json").exists());
        ok(&f.main, &["status"]);
    }
}

#[test]
fn malformed_shared_inactive_refs_fail_attachment_before_materialization_or_binding() {
    for case in ["unknown-field", "symlink", "hardlink"] {
        let f = Fixture::new();
        let lane = ok(&f.main, &["lane", "create", "--name", "invalid-inactive"])["object"]["id"]
            .as_str()
            .unwrap()
            .to_owned();
        let path = f.common().join("heads").join(&lane);
        match case {
            "unknown-field" => {
                let mut value = read(&path);
                value["unexpected"] = json!(true);
                write(&path, &value);
            }
            #[cfg(unix)]
            "symlink" => {
                let outside = f._temp.path().join("outside-ref");
                fs::rename(&path, &outside).unwrap();
                std::os::unix::fs::symlink(outside, &path).unwrap();
            }
            #[cfg(unix)]
            "hardlink" => {
                fs::hard_link(&path, f._temp.path().join("outside-ref")).unwrap();
            }
            _ => continue,
        }
        let destination = f._temp.path().join("must-not-create");
        let before = fs::read(f.common().join("manifest.json")).unwrap();
        assert!(
            !run(
                &f.main,
                &["workspace", "add", &lane, destination.to_str().unwrap()]
            )
            .status
            .success(),
            "{case}"
        );
        assert!(!destination.exists());
        assert_eq!(fs::read(f.common().join("manifest.json")).unwrap(), before);
    }
}

#[test]
fn linked_slice_creation_writes_the_single_common_registry() {
    let f = Fixture::new();
    let value = ok(&f.other, &["slice", "create", "--name", "from-linked"]);
    assert_eq!(value["persisted"], true);
    assert!(f.common().join("slices/from-linked.json").is_file());
    assert!(!f.other.join(".sorrel/slices").exists());
    let stored = read(f.common().join("slices/from-linked.json"));
    assert_eq!(stored["name"], "from-linked");
}

#[test]
fn shared_undo_and_checkpoint_restore_refuse_newly_protected_historical_paths() {
    let f = Fixture::new();
    fs::write(
        f.main.join("credentials.txt"),
        "synthetic previously ordinary content",
    )
    .unwrap();
    ok(
        &f.main,
        &["change", "create", "-m", "before provider registration"],
    );
    ok(&f.main, &["checkpoint", "create", "before-provider"]);
    fs::remove_file(f.main.join("credentials.txt")).unwrap();
    ok(
        &f.main,
        &["change", "create", "-m", "remove credential-shaped file"],
    );
    fs::create_dir(f.common().join("secrets")).unwrap();
    write(
        f.common().join("secrets/provider.json"),
        &json!({"provider":format!("dotenv:{}",f.main.join("credentials.txt").display())}),
    );
    let before = head(&f.main);
    let history = ok(&f.main, &["operation", "log"]);
    for args in [
        &["undo"][..],
        &["checkpoint", "restore", "before-provider"][..],
    ] {
        fail(&f.main, args, "protected");
        assert_eq!(head(&f.main), before);
        assert_eq!(ok(&f.main, &["operation", "log"]), history);
        assert!(!f.main.join("credentials.txt").exists());
        assert!(!f.common().join("metadata-transaction.json").exists());
    }
}
