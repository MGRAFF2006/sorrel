use sorrel_core::{
    explain_workspace_path, materialize_snapshot, materialize_workspace_snapshot,
    read_snapshot_files, FileObjectStore, InMemoryObjectStore, ObjectId, ObjectStore,
    ObjectStoreResult, SnapshotOptions,
};
use std::{fs, path::Path};
use tempfile::TempDir;

fn write(root: &Path, path: &str, content: &str) {
    let file = root.join(path);
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    fs::write(file, content).unwrap();
}

struct MetadataOnly<'a>(&'a InMemoryObjectStore);
impl ObjectStore for MetadataOnly<'_> {
    fn read(&self, id: &ObjectId) -> ObjectStoreResult<Vec<u8>> {
        let bytes = self.0.read(id)?;
        assert!(
            !bytes.starts_with(b"sorrel.blob.v0\n"),
            "explanation must not read file blobs"
        );
        Ok(bytes)
    }
    fn has(&self, id: &ObjectId) -> ObjectStoreResult<bool> {
        self.0.has(id)
    }
    fn write(&self, _: &[u8]) -> ObjectStoreResult<ObjectId> {
        panic!("explanation must not write objects")
    }
}

#[test]
fn explanation_matches_materialization_and_reads_only_baseline_metadata() {
    let root = TempDir::new().unwrap();
    let store = InMemoryObjectStore::new();
    write(root.path(), "tracked.txt", "tracked");
    write(
        root.path(),
        "cache/tracked.txt",
        "tracked inside ignored directory",
    );
    let baseline = materialize_workspace_snapshot(
        &store,
        root.path(),
        None,
        None,
        SnapshotOptions::new("repo"),
    )
    .unwrap();
    write(
        root.path(),
        ".gitignore",
        "tracked.txt\ncache/\nbuild/\n*.tmp\n",
    );
    write(root.path(), ".sorrelignore", "!keep.tmp\n!.env\n");
    write(root.path(), "src/.sorrelignore", "!keep.tmp\n");
    write(root.path(), "build/.sorrelignore", "!keep.txt\n");
    write(
        root.path(),
        "secretspec.toml",
        "provider = 'dotenv:credentials.data'\n",
    );
    for path in [
        "cache/untracked.txt",
        "build/keep.txt",
        "src/keep.tmp",
        "src/skip.tmp",
        "keep.tmp",
        ".env",
        ".env.example",
        "credentials.data",
        ".git/config",
        ".sorrel/local",
    ] {
        write(root.path(), path, "contents must not be read by explain");
    }
    let before = store.len();
    for (path, included, tracked, ignored, protected, metadata) in [
        ("tracked.txt", true, true, true, false, false),
        ("cache/tracked.txt", true, true, true, false, false),
        ("cache/untracked.txt", false, false, true, false, false),
        ("build/keep.txt", false, false, true, false, false),
        ("src/keep.tmp", true, false, false, false, false),
        ("src/skip.tmp", false, false, true, false, false),
        ("keep.tmp", true, false, false, false, false),
        (".env", false, false, false, true, false),
        (".env.example", true, false, false, false, false),
        ("credentials.data", false, false, false, true, false),
        (".git/config", false, false, false, false, true),
        (".sorrel/local", false, false, false, false, true),
    ] {
        let explanation =
            explain_workspace_path(&MetadataOnly(&store), root.path(), Some(&baseline.id), path)
                .unwrap();
        assert_eq!(
            (
                explanation.included,
                explanation.tracked,
                explanation.ignored,
                explanation.protected,
                explanation.metadata
            ),
            (included, tracked, ignored, protected, metadata),
            "{path}"
        );
        assert!(explanation.exists);
    }
    assert_eq!(store.len(), before);
    let snapshot = materialize_workspace_snapshot(
        &store,
        root.path(),
        Some(&baseline.id),
        None,
        SnapshotOptions::new("repo"),
    )
    .unwrap();
    let files = read_snapshot_files(&store, &snapshot.id).unwrap();
    for path in [
        "tracked.txt",
        "cache/tracked.txt",
        "cache/untracked.txt",
        "build/keep.txt",
        "src/keep.tmp",
        "src/skip.tmp",
        "keep.tmp",
        ".env",
        ".env.example",
        "credentials.data",
        ".git/config",
        ".sorrel/local",
    ] {
        assert_eq!(
            explain_workspace_path(&MetadataOnly(&store), root.path(), Some(&baseline.id), path)
                .unwrap()
                .included,
            files.contains_key(Path::new(path)),
            "{path}"
        );
    }
}

#[test]
fn legacy_tracked_protected_paths_are_explainable_without_weakening_snapshot_gate() {
    let root = TempDir::new().unwrap();
    let store = InMemoryObjectStore::new();
    write(root.path(), ".env", "synthetic protected fixture");
    let baseline = materialize_snapshot(&store, root.path(), SnapshotOptions::new("repo")).unwrap();
    let explanation = explain_workspace_path(
        &MetadataOnly(&store),
        root.path(),
        Some(&baseline.id),
        ".env",
    )
    .unwrap();
    assert!(explanation.tracked && explanation.protected && !explanation.included);
    let before = store.len();
    assert!(materialize_workspace_snapshot(
        &store,
        root.path(),
        Some(&baseline.id),
        None,
        SnapshotOptions::new("repo")
    )
    .is_err());
    assert_eq!(store.len(), before);
}

#[test]
fn missing_relative_paths_can_be_explained_but_escape_paths_are_rejected() {
    let root = TempDir::new().unwrap();
    let store = InMemoryObjectStore::new();
    let explanation =
        explain_workspace_path(&MetadataOnly(&store), root.path(), None, "./missing.txt").unwrap();
    assert!(explanation.included && !explanation.exists && explanation.supported_type);
    for path in [Path::new("../outside"), root.path()] {
        assert!(explain_workspace_path(&MetadataOnly(&store), root.path(), None, path).is_err());
    }
    assert_eq!(store.len(), 0);
}

#[cfg(unix)]
#[test]
fn symlink_ancestors_are_never_traversed_and_leaf_links_are_not_supported() {
    let root = TempDir::new().unwrap();
    let external = TempDir::new().unwrap();
    let store = InMemoryObjectStore::new();
    write(external.path(), ".sorrelignore", "[invalid pattern");
    std::os::unix::fs::symlink(external.path(), root.path().join("link")).unwrap();
    let explanation =
        explain_workspace_path(&MetadataOnly(&store), root.path(), None, "link").unwrap();
    assert!(!explanation.included && !explanation.supported_type && explanation.exists);
    assert!(explain_workspace_path(&MetadataOnly(&store), root.path(), None, "link/file").is_err());
}

#[test]
fn opening_existing_store_never_creates_missing_directories() {
    let root = TempDir::new().unwrap();
    assert!(FileObjectStore::open_existing(root.path()).is_err());
    assert_eq!(fs::read_dir(root.path()).unwrap().count(), 0);
    let store = FileObjectStore::new(root.path()).unwrap();
    let id = store.write(b"object").unwrap();
    fs::remove_dir(root.path().join("tmp")).unwrap();
    let reader = FileObjectStore::open_existing(root.path()).unwrap();
    assert_eq!(reader.read(&id).unwrap(), b"object");
    assert!(!root.path().join("tmp").exists());
}
