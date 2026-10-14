use sorrel_core::{
    materialize_workspace_snapshot, materialize_workspace_snapshot_paths, read_snapshot_files,
    read_tree, snapshot_diff, EntryMode, InMemoryObjectStore, ObjectId, ObjectStore, Snapshot,
    SnapshotOptions, StatCache,
};
use std::{
    fs,
    path::{Path, PathBuf},
};
use tempfile::TempDir;

fn write(root: &Path, path: &str, bytes: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
}

fn full(
    store: &InMemoryObjectStore,
    root: &Path,
    base: Option<&ObjectId>,
    cache: &mut StatCache,
) -> Snapshot {
    materialize_workspace_snapshot(store, root, base, Some(cache), SnapshotOptions::new("repo"))
        .unwrap()
}

fn selected(
    store: &InMemoryObjectStore,
    root: &Path,
    base: &ObjectId,
    paths: &[&str],
    cache: &mut StatCache,
) -> Snapshot {
    materialize_workspace_snapshot_paths(
        store,
        root,
        base,
        &paths.iter().map(PathBuf::from).collect::<Vec<_>>(),
        Some(cache),
        SnapshotOptions::new("repo"),
    )
    .unwrap()
}

#[test]
fn overlays_selected_changes_preserves_unselected_entries_and_later_full_recording() {
    let root = TempDir::new().unwrap();
    let store = InMemoryObjectStore::new();
    let mut cache = StatCache::new();
    for path in [
        "chosen",
        "deleted",
        "tree/old",
        "tree/edit",
        "tree-sibling",
        "other/edit",
        "other/deleted",
    ] {
        write(root.path(), path, "before");
    }
    let base = full(&store, root.path(), None, &mut cache);
    for path in [
        "chosen",
        "new",
        "tree/edit",
        "tree/new",
        "tree-sibling",
        "other/edit",
        "other/new",
    ] {
        write(root.path(), path, "after");
    }
    for path in ["deleted", "tree/old", "other/deleted"] {
        fs::remove_file(root.path().join(path)).unwrap();
    }
    let partial = selected(
        &store,
        root.path(),
        &base.id,
        &["./chosen", "new", "deleted", "tree/", "tree/edit"],
        &mut cache,
    );
    let files = read_snapshot_files(&store, &partial.id).unwrap();
    for path in ["chosen", "new", "tree/edit", "tree/new"] {
        assert_eq!(files[Path::new(path)], b"after");
    }
    for path in ["tree-sibling", "other/edit", "other/deleted"] {
        assert_eq!(files[Path::new(path)], b"before");
    }
    for path in ["deleted", "tree/old", "other/new"] {
        assert!(!files.contains_key(Path::new(path)));
    }
    assert_eq!(fs::read(root.path().join("other/edit")).unwrap(), b"after");
    assert_eq!(fs::read(root.path().join("other/new")).unwrap(), b"after");
    assert!(!root.path().join("other/deleted").exists());
    assert!(cache.get("other/edit").is_none());
    let later = full(&store, root.path(), Some(&partial.id), &mut cache);
    let diff = snapshot_diff(&store, &partial.id, &later.id).unwrap();
    assert_eq!(
        diff.changes
            .iter()
            .map(|c| c.path.to_str().unwrap())
            .collect::<Vec<_>>(),
        ["other/deleted", "other/edit", "other/new", "tree-sibling"]
    );
    let files = read_snapshot_files(&store, &later.id).unwrap();
    assert_eq!(files[Path::new("other/edit")], b"after");
    assert_eq!(files[Path::new("other/new")], b"after");
    assert!(!files.contains_key(Path::new("other/deleted")));
}

#[test]
fn selecting_a_deleted_child_preserves_deleted_unselected_siblings() {
    let root = TempDir::new().unwrap();
    let store = InMemoryObjectStore::new();
    let mut cache = StatCache::new();
    write(root.path(), "dir/a", "a");
    write(root.path(), "dir/b", "b");
    let base = full(&store, root.path(), None, &mut cache);
    fs::remove_dir_all(root.path().join("dir")).unwrap();
    let partial = selected(&store, root.path(), &base.id, &["dir/a"], &mut cache);
    let files = read_snapshot_files(&store, &partial.id).unwrap();
    assert_eq!(files.len(), 1);
    assert_eq!(files[Path::new("dir/b")], b"b");
    let deleted = selected(&store, root.path(), &partial.id, &["dir"], &mut cache);
    assert!(read_snapshot_files(&store, &deleted.id).unwrap().is_empty());
}

#[test]
fn selectors_cannot_bypass_filters_or_publish_any_objects_on_validation_failure() {
    let root = TempDir::new().unwrap();
    let store = InMemoryObjectStore::new();
    let mut cache = StatCache::new();
    write(root.path(), "tracked", "before");
    let base = full(&store, root.path(), None, &mut cache);
    write(root.path(), ".sorrelignore", "ignored/\ntracked\n");
    write(root.path(), "ignored/value", "IGNORED_FIXTURE");
    write(root.path(), ".env", "PRIVATE_FIXTURE");
    write(root.path(), ".env.example", "EXAMPLE_PRIVATE_FIXTURE");
    write(root.path(), "private/provider", "PROVIDER_PRIVATE_FIXTURE");
    write(root.path(), "sorrel.secrets.yml", "localDev:\n  import:\n    envFiles:\n      - path: private/provider\n      - path: .env.example\n");
    write(root.path(), "new", "PUBLIC_NOT_YET_SELECTED");
    for path in [
        "",
        "../outside",
        "dir/../tracked",
        "/tmp",
        ".sorrel",
        ".sorrel/HEAD",
        ".git/config",
        "unknown",
        "ignored",
        "ignored/value",
        "tracked",
        ".env",
        ".ENV",
        ".env.example",
        "private/provider",
    ] {
        let before = store.len();
        let cache_before = cache.to_bytes().unwrap();
        assert!(
            materialize_workspace_snapshot_paths(
                &store,
                root.path(),
                &base.id,
                &[PathBuf::from("new"), PathBuf::from(path)],
                Some(&mut cache),
                SnapshotOptions::new("repo")
            )
            .is_err(),
            "{path}"
        );
        assert_eq!(store.len(), before, "{path}");
        assert_eq!(cache.to_bytes().unwrap(), cache_before, "{path}");
    }
    let partial = selected(&store, root.path(), &base.id, &["private"], &mut cache);
    assert!(!read_snapshot_files(&store, &partial.id)
        .unwrap()
        .contains_key(Path::new("private/provider")));
    for value in [
        "IGNORED_FIXTURE",
        "PRIVATE_FIXTURE",
        "EXAMPLE_PRIVATE_FIXTURE",
        "PROVIDER_PRIVATE_FIXTURE",
        "PUBLIC_NOT_YET_SELECTED",
    ] {
        let id = ObjectId::for_bytes(format!("sorrel.blob.v0\n{value}").as_bytes());
        assert!(!store.has(&id).unwrap());
    }
}

#[cfg(unix)]
#[test]
fn preserves_modes_and_records_file_directory_transitions_without_touching_files() {
    use std::os::unix::fs::PermissionsExt;
    let root = TempDir::new().unwrap();
    let store = InMemoryObjectStore::new();
    let mut cache = StatCache::new();
    for path in [
        "selected-mode",
        "unselected-mode",
        "file-to-dir",
        "dir-to-file/old",
    ] {
        write(root.path(), path, "bytes");
        fs::set_permissions(root.path().join(path), fs::Permissions::from_mode(0o644)).unwrap();
    }
    let base = full(&store, root.path(), None, &mut cache);
    for path in ["selected-mode", "unselected-mode"] {
        fs::set_permissions(root.path().join(path), fs::Permissions::from_mode(0o755)).unwrap();
    }
    fs::remove_file(root.path().join("file-to-dir")).unwrap();
    write(root.path(), "file-to-dir/new", "new");
    fs::remove_dir_all(root.path().join("dir-to-file")).unwrap();
    write(root.path(), "dir-to-file", "replacement");
    let before = store.len();
    assert!(materialize_workspace_snapshot_paths(
        &store,
        root.path(),
        &base.id,
        &["file-to-dir/new".into()],
        None,
        SnapshotOptions::new("repo")
    )
    .is_err());
    assert_eq!(store.len(), before);
    let partial = selected(
        &store,
        root.path(),
        &base.id,
        &["selected-mode", "file-to-dir", "dir-to-file"],
        &mut cache,
    );
    let files = read_snapshot_files(&store, &partial.id).unwrap();
    assert_eq!(files[Path::new("file-to-dir/new")], b"new");
    assert_eq!(files[Path::new("dir-to-file")], b"replacement");
    let entries = read_tree(&store, &partial.root_tree.id).unwrap().entries;
    assert_eq!(
        entries
            .iter()
            .find(|e| e.name == "selected-mode")
            .unwrap()
            .mode,
        EntryMode::Executable
    );
    assert_eq!(
        entries
            .iter()
            .find(|e| e.name == "unselected-mode")
            .unwrap()
            .mode,
        EntryMode::Normal
    );
    assert_eq!(
        fs::metadata(root.path().join("unselected-mode"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o755
    );
    let later = full(&store, root.path(), Some(&partial.id), &mut cache);
    assert_eq!(
        snapshot_diff(&store, &partial.id, &later.id)
            .unwrap()
            .changes
            .len(),
        1
    );
}

#[cfg(unix)]
#[test]
fn rejects_symlink_selectors_and_ancestors_without_following_outside_workspace() {
    use std::os::unix::fs::symlink;
    let root = TempDir::new().unwrap();
    let external = TempDir::new().unwrap();
    let store = InMemoryObjectStore::new();
    let mut cache = StatCache::new();
    write(root.path(), "safe", "before");
    let base = full(&store, root.path(), None, &mut cache);
    write(external.path(), "outside", "OUTSIDE_FIXTURE");
    symlink(external.path(), root.path().join("link")).unwrap();
    symlink(external.path().join("outside"), root.path().join("leaf")).unwrap();
    for path in ["link", "link/outside", "leaf"] {
        let before = store.len();
        assert!(materialize_workspace_snapshot_paths(
            &store,
            root.path(),
            &base.id,
            &[path.into()],
            None,
            SnapshotOptions::new("repo")
        )
        .is_err());
        assert_eq!(store.len(), before);
    }
    write(root.path(), "safe", "after");
    selected(&store, root.path(), &base.id, &["safe"], &mut cache);
    let id = ObjectId::for_bytes(b"sorrel.blob.v0\nOUTSIDE_FIXTURE");
    assert!(!store.has(&id).unwrap());
}

#[test]
fn handles_empty_directory_deletions_literal_metacharacters_and_root_selection() {
    let root = TempDir::new().unwrap();
    let store = InMemoryObjectStore::new();
    let mut cache = StatCache::new();
    fs::create_dir(root.path().join("empty")).unwrap();
    write(root.path(), "file*", "literal");
    write(root.path(), "file-other", "before");
    let base = full(&store, root.path(), None, &mut cache);
    fs::remove_dir(root.path().join("empty")).unwrap();
    write(root.path(), "file*", "selected literal");
    write(root.path(), "file-other", "after");
    let partial = selected(
        &store,
        root.path(),
        &base.id,
        &["empty", "file*"],
        &mut cache,
    );
    assert!(!read_tree(&store, &partial.root_tree.id)
        .unwrap()
        .entries
        .iter()
        .any(|e| e.name == "empty"));
    let files = read_snapshot_files(&store, &partial.id).unwrap();
    assert_eq!(files[Path::new("file*")], b"selected literal");
    assert_eq!(files[Path::new("file-other")], b"before");
    let all = selected(&store, root.path(), &partial.id, &["."], &mut cache);
    assert_eq!(
        read_snapshot_files(&store, &all.id).unwrap()[Path::new("file-other")],
        b"after"
    );
    let ordinary = selected(&store, root.path(), &partial.id, &[], &mut cache);
    assert_eq!(ordinary.root_tree.id, all.root_tree.id);
}

#[test]
fn selective_recording_still_rejects_previously_tracked_secrets_globally() {
    let root = TempDir::new().unwrap();
    let store = InMemoryObjectStore::new();
    write(root.path(), ".env", "LEGACY_PRIVATE_FIXTURE");
    let base = sorrel_core::materialize_snapshot(&store, root.path(), SnapshotOptions::new("repo"))
        .unwrap();
    write(root.path(), "public", "public");
    let before = store.len();
    assert!(matches!(
        materialize_workspace_snapshot_paths(
            &store,
            root.path(),
            &base.id,
            &["public".into()],
            None,
            SnapshotOptions::new("repo")
        ),
        Err(sorrel_core::SnapshotError::TrackedSecret { .. })
    ));
    assert_eq!(store.len(), before);
}
