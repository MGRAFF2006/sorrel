//! Thin SDK over `sorrel-core` for embedding Sorrel in Rust applications.

pub use sorrel_core::{
    FileObjectStore, ObjectId, ObjectKind, ObjectRef, ObjectStore, Snapshot, SnapshotOptions,
};

use serde_json::{json, Value};
use sorrel_core::{
    materialize_snapshot_filtered_with_stat_cache, read_snapshot, validate_snapshot,
    write_snapshot, write_tree,
};
use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
};
use thiserror::Error;

const SCHEMA_VERSION: &str = "sorrel.protocol.v0";
const DEFAULT_LANE: &str = "lane_main";

/// Errors raised by the SDK workspace helpers.
#[derive(Debug, Error)]
pub enum SdkError {
    #[error(transparent)]
    Store(#[from] sorrel_core::ObjectStoreError),
    #[error(transparent)]
    Snapshot(#[from] sorrel_core::SnapshotError),
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error("invalid Sorrel workspace: {0}")]
    InvalidWorkspace(String),
}

/// CLI-compatible on-disk workspace with object storage at `root/.sorrel/objects`.
/// Snapshot helpers create immutable objects without moving HEAD or lane refs.
pub struct Workspace {
    root: PathBuf,
    store: FileObjectStore,
    repo_id: String,
}

impl Workspace {
    /// Initializes CLI-compatible metadata and an empty initial snapshot.
    /// An existing workspace with the same repository id returns its current HEAD.
    /// Existing history is never reset, and earlier SDK-only storage needs migration.
    pub fn init(
        root: impl Into<PathBuf>,
        repo_id: impl Into<String>,
    ) -> Result<(Self, Snapshot), SdkError> {
        let root = root.into();
        let repo_id = repo_id.into();
        if repo_id.is_empty() {
            return Err(invalid("repository id is empty"));
        }
        fs::create_dir_all(&root)?;
        let root = fs::canonicalize(root)?;
        let metadata = root.join(".sorrel");
        fs::create_dir_all(&metadata)?;
        require_directory(&metadata)?;
        let _lock = lock_workspace(&metadata)?;
        reject_legacy_layout(&metadata)?;
        if metadata.join("manifest.json").exists() {
            let workspace = Self::open_locked(root)?;
            if workspace.repo_id != repo_id {
                return Err(invalid(
                    "existing repository id differs from the requested id",
                ));
            }
            let head = workspace.head_snapshot_locked()?;
            return Ok((workspace, head));
        }
        if metadata.join("HEAD").exists() || metadata.join("HEAD_TRANSACTION").exists() {
            return Err(invalid(
                "incomplete initialization; repair this workspace before initializing it",
            ));
        }
        let store = FileObjectStore::new(&metadata)?;
        let mut options = SnapshotOptions::new(repo_id.clone());
        options.message = Some("initial snapshot".to_owned());
        let created_at = options.created_at.clone();
        let tree = write_tree(&store, Vec::new())?;
        let snapshot = write_snapshot(&store, tree.id, options)?;
        for name in ["heads", "lanes", "stacks", "slices"] {
            fs::create_dir_all(metadata.join(name))?;
            require_directory(&metadata.join(name))?;
        }
        write_json_atomic(
            &metadata.join("heads").join(DEFAULT_LANE),
            &json!({ "snapshot": snapshot.id.to_string() }),
        )?;
        write_json_atomic(
            &metadata.join("HEAD"),
            &json!({ "lane": DEFAULT_LANE, "snapshot": snapshot.id.to_string() }),
        )?;
        write_json_atomic(
            &metadata.join("lanes").join(format!("{DEFAULT_LANE}.json")),
            &json!({
                "kind": "Lane", "id": DEFAULT_LANE, "name": "main",
                "baseSnapshot": { "kind": "Snapshot", "id": snapshot.id.to_string() },
                "headSnapshot": { "kind": "Snapshot", "id": snapshot.id.to_string() },
                "createdAt": created_at,
            }),
        )?;
        // Publish the manifest last: readers never mistake partial metadata for
        // a complete initialized workspace.
        write_json_atomic(
            &metadata.join("manifest.json"),
            &json!({
                "schemaVersion": SCHEMA_VERSION, "kind": "Workspace", "repoId": repo_id,
                "createdAt": created_at, "defaultLane": { "id": DEFAULT_LANE, "name": "main" }
            }),
        )?;
        Ok((
            Self {
                root,
                store,
                repo_id,
            },
            snapshot,
        ))
    }

    /// Opens and validates an existing CLI-compatible workspace and active HEAD.
    pub fn open(root: impl Into<PathBuf>) -> Result<Self, SdkError> {
        let root = fs::canonicalize(root.into())?;
        let metadata = root.join(".sorrel");
        require_directory(&metadata)?;
        let _lock = lock_workspace(&metadata)?;
        Self::open_locked(root)
    }

    fn open_locked(root: PathBuf) -> Result<Self, SdkError> {
        let metadata = root.join(".sorrel");
        reject_legacy_layout(&metadata)?;
        reject_pending_transaction(&metadata)?;
        let manifest = read_json(&metadata.join("manifest.json"))?;
        if manifest.get("schemaVersion").and_then(Value::as_str) != Some(SCHEMA_VERSION)
            || manifest.get("kind").and_then(Value::as_str) != Some("Workspace")
        {
            return Err(invalid("unsupported manifest schema or kind"));
        }
        let repo_id = string_field(&manifest, "repoId")?.to_owned();
        require_directory(&metadata.join("objects"))?;
        let workspace = Self {
            root,
            store: FileObjectStore::new(metadata)?,
            repo_id,
        };
        workspace.head_snapshot_locked()?;
        Ok(workspace)
    }

    /// Reads the active HEAD without assuming that the main lane is active.
    pub fn head_snapshot(&self) -> Result<Snapshot, SdkError> {
        let _lock = lock_workspace(&self.root.join(".sorrel"))?;
        self.head_snapshot_locked()
    }

    fn head_snapshot_locked(&self) -> Result<Snapshot, SdkError> {
        let metadata = self.root.join(".sorrel");
        reject_pending_transaction(&metadata)?;
        let head = read_json(&metadata.join("HEAD"))?;
        let lane = string_field(&head, "lane")?;
        if lane == "."
            || lane == ".."
            || !lane
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
        {
            return Err(invalid("unsafe active lane id"));
        }
        let snapshot_id = string_field(&head, "snapshot")?;
        let id = snapshot_id
            .parse()
            .map_err(|_| invalid("invalid HEAD snapshot id"))?;
        let lane_path = metadata.join("heads").join(lane);
        if lane_path.exists() {
            let lane_head = read_json(&lane_path)?;
            if string_field(&lane_head, "snapshot")? != snapshot_id {
                return Err(invalid(
                    "HEAD and active lane head disagree; run the CLI to repair refs",
                ));
            }
        }
        let snapshot = read_snapshot(&self.store, &id)?;
        if snapshot.repo != self.repo_id {
            return Err(invalid("HEAD belongs to another repository"));
        }
        validate_snapshot(&self.store, &id)?;
        Ok(snapshot)
    }

    /// Creates a detached snapshot, excluding repository metadata. HEAD stays put.
    /// Use the filtered variant to apply an embedding application's ignore rules.
    pub fn snapshot_working_tree(
        &self,
        parent: ObjectId,
        message: impl Into<String>,
    ) -> Result<Snapshot, SdkError> {
        self.snapshot_working_tree_filtered(parent, message, |_, _| Ok(true))
    }

    /// Creates a detached snapshot with a fallible relative-path inclusion filter.
    /// Returning false for a directory prunes its descendants. Repository metadata
    /// (`.sorrel` and `.git` at the root) is always excluded.
    pub fn snapshot_working_tree_filtered(
        &self,
        parent: ObjectId,
        message: impl Into<String>,
        mut include: impl FnMut(&Path, bool) -> io::Result<bool>,
    ) -> Result<Snapshot, SdkError> {
        let _lock = lock_workspace(&self.root.join(".sorrel"))?;
        reject_pending_transaction(&self.root.join(".sorrel"))?;
        let base = read_snapshot(&self.store, &parent)?;
        if base.repo != self.repo_id {
            return Err(invalid("parent belongs to another repository"));
        }
        validate_snapshot(&self.store, &parent)?;
        let mut options = SnapshotOptions::new(self.repo_id.clone());
        options.message = Some(message.into());
        options.parents = vec![ObjectRef::new(ObjectKind::Snapshot, parent)];
        Ok(materialize_snapshot_filtered_with_stat_cache(
            &self.store,
            &self.root,
            None,
            options,
            |path, is_dir| {
                let root_name = path
                    .to_str()
                    .unwrap_or_default()
                    .trim_end_matches(['.', ' ']);
                if path.components().count() == 1
                    && (root_name.eq_ignore_ascii_case(".sorrel")
                        || root_name.eq_ignore_ascii_case(".git"))
                {
                    return Ok(false);
                }
                include(path, is_dir)
            },
        )?)
    }

    pub fn store(&self) -> &FileObjectStore {
        &self.store
    }
    pub fn repo_id(&self) -> &str {
        &self.repo_id
    }
    pub fn root(&self) -> &Path {
        &self.root
    }
}

fn invalid(message: &str) -> SdkError {
    SdkError::InvalidWorkspace(message.to_owned())
}

fn string_field<'a>(value: &'a Value, field: &str) -> Result<&'a str, SdkError> {
    value
        .get(field)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| invalid(&format!("missing or empty {field}")))
}

fn read_json(path: &Path) -> Result<Value, SdkError> {
    validate_metadata_path(path)?;
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}

fn validate_metadata_path(path: &Path) -> Result<(), SdkError> {
    for parent in path.ancestors().skip(1) {
        require_directory(parent)?;
        if parent.file_name().is_some_and(|name| name == ".sorrel") {
            break;
        }
    }
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            Err(invalid("metadata file is a symlink"))
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn require_directory(path: &Path) -> Result<(), SdkError> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(invalid("workspace metadata must be a real directory"));
    }
    Ok(())
}

fn reject_legacy_layout(metadata: &Path) -> Result<(), SdkError> {
    if metadata.join("objects/objects").exists() {
        return Err(invalid(
            "legacy SDK nested object storage requires migration before opening",
        ));
    }
    Ok(())
}

fn reject_pending_transaction(metadata: &Path) -> Result<(), SdkError> {
    if metadata.join("HEAD_TRANSACTION").exists()
        || metadata.join("CHECKOUT_STATE").exists()
        || metadata.join("WORKSPACE_CREATE").exists()
    {
        return Err(invalid(
            "unfinished CLI transaction; run the Sorrel CLI to recover before using the SDK",
        ));
    }
    Ok(())
}

fn lock_workspace(metadata: &Path) -> Result<fs::File, SdkError> {
    require_directory(metadata)?;
    let path = metadata.join("LOCK");
    if fs::symlink_metadata(&path).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
        return Err(invalid("repository lock is a symlink"));
    }
    let file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)?;
    fs2::FileExt::lock_exclusive(&file)?;
    Ok(file)
}

fn write_json_atomic(path: &Path, value: &Value) -> Result<(), SdkError> {
    validate_metadata_path(path)?;
    let parent = path
        .parent()
        .ok_or_else(|| invalid("metadata file has no parent"))?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    temporary.write_all(&serde_json::to_vec_pretty(value)?)?;
    temporary.as_file().sync_all()?;
    temporary.persist(path).map_err(|error| error.error)?;
    #[cfg(unix)]
    fs::File::open(parent)?.sync_all()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[cfg(unix)]
    #[test]
    fn initialization_refuses_symlinked_ref_directories_without_external_writes() {
        let root = TempDir::new().unwrap();
        let outside = TempDir::new().unwrap();
        fs::create_dir(root.path().join(".sorrel")).unwrap();
        std::os::unix::fs::symlink(outside.path(), root.path().join(".sorrel/heads")).unwrap();
        assert!(Workspace::init(root.path(), "repo_sdk").is_err());
        assert_eq!(fs::read_dir(outside.path()).unwrap().count(), 0);
        assert!(!root.path().join(".sorrel/manifest.json").exists());
    }

    #[test]
    fn init_open_and_detached_snapshot_round_trip_without_resetting_head() {
        let dir = TempDir::new().unwrap();
        let (ws, initial) = Workspace::init(dir.path(), "repo_sdk").unwrap();
        assert!(dir.path().join(".sorrel/objects").is_dir());
        assert!(!dir.path().join(".sorrel/objects/objects").exists());
        std::fs::write(dir.path().join("hello.txt"), b"sdk\n").unwrap();
        fs::create_dir(dir.path().join(".git")).unwrap();
        fs::write(dir.path().join(".git/config"), b"metadata").unwrap();
        let next = ws.snapshot_working_tree(initial.id, "add hello").unwrap();
        assert_ne!(next.id, initial.id);
        let files = sorrel_core::read_snapshot_files(ws.store(), &next.id).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[Path::new("hello.txt")], b"sdk\n");
        assert_eq!(
            Workspace::open(dir.path())
                .unwrap()
                .head_snapshot()
                .unwrap()
                .id,
            initial.id
        );
        let (_, reopened) = Workspace::init(dir.path(), "repo_sdk").unwrap();
        assert_eq!(reopened.id, initial.id);
        assert!(Workspace::init(dir.path(), "another_repo").is_err());
        assert_eq!(ws.head_snapshot().unwrap().id, initial.id);
    }

    #[test]
    fn open_honors_non_main_lane_and_rejects_ref_disagreement() {
        let dir = TempDir::new().unwrap();
        let (_, initial) = Workspace::init(dir.path(), "repo_sdk").unwrap();
        let metadata = dir.path().join(".sorrel");
        write_json_atomic(
            &metadata.join("HEAD"),
            &json!({ "lane": "lane_agent", "snapshot": initial.id.to_string() }),
        )
        .unwrap();
        write_json_atomic(
            &metadata.join("heads/lane_agent"),
            &json!({ "snapshot": initial.id.to_string() }),
        )
        .unwrap();
        assert_eq!(
            Workspace::open(dir.path())
                .unwrap()
                .head_snapshot()
                .unwrap()
                .id,
            initial.id
        );
        write_json_atomic(
            &metadata.join("heads/lane_agent"),
            &json!({ "snapshot": ObjectId::for_bytes(b"missing").to_string() }),
        )
        .unwrap();
        assert!(Workspace::open(dir.path()).is_err());
    }

    #[test]
    fn filtered_snapshots_propagate_errors_and_never_include_metadata() {
        let dir = TempDir::new().unwrap();
        let (ws, initial) = Workspace::init(dir.path(), "repo_sdk").unwrap();
        fs::write(dir.path().join(".env"), b"private").unwrap();
        fs::write(dir.path().join("code.txt"), b"public").unwrap();
        let snap = ws
            .snapshot_working_tree_filtered(initial.id, "filtered", |path, _| {
                Ok(path != Path::new(".env"))
            })
            .unwrap();
        let files = sorrel_core::read_snapshot_files(ws.store(), &snap.id).unwrap();
        assert_eq!(files.len(), 1);
        assert!(files.contains_key(Path::new("code.txt")));
        assert!(ws
            .snapshot_working_tree_filtered(initial.id, "error", |_, _| Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "filter failed"
            )))
            .is_err());
    }

    #[test]
    fn unfinished_cli_checkout_requires_recovery_without_resetting_head() {
        let dir = TempDir::new().unwrap();
        let (_, initial) = Workspace::init(dir.path(), "repo_sdk").unwrap();
        let journal = dir.path().join(".sorrel/CHECKOUT_STATE");
        fs::write(&journal, b"pending checkout").unwrap();
        assert!(
            matches!(Workspace::open(dir.path()), Err(SdkError::InvalidWorkspace(message)) if message.contains("recover"))
        );
        assert!(Workspace::init(dir.path(), "repo_sdk").is_err());
        fs::remove_file(journal).unwrap();
        assert_eq!(
            Workspace::open(dir.path())
                .unwrap()
                .head_snapshot()
                .unwrap()
                .id,
            initial.id
        );
    }

    #[test]
    fn simultaneous_initialization_converges_on_one_persistent_head() {
        let dir = TempDir::new().unwrap();
        let barrier = std::sync::Barrier::new(6);
        let ids = std::thread::scope(|scope| {
            let handles = (0..6)
                .map(|_| {
                    scope.spawn(|| {
                        barrier.wait();
                        Workspace::init(dir.path(), "repo_sdk").unwrap().1.id
                    })
                })
                .collect::<Vec<_>>();
            handles
                .into_iter()
                .map(|handle| handle.join().unwrap())
                .collect::<Vec<_>>()
        });
        assert!(ids.iter().all(|id| id == &ids[0]));
        assert_eq!(
            Workspace::open(dir.path())
                .unwrap()
                .head_snapshot()
                .unwrap()
                .id,
            ids[0]
        );
    }

    #[test]
    fn old_sdk_layout_fails_with_migration_error_and_preserves_objects() {
        let dir = TempDir::new().unwrap();
        let old = dir.path().join(".sorrel/objects/objects");
        fs::create_dir_all(&old).unwrap();
        fs::write(old.join("valuable"), b"history").unwrap();
        assert!(
            matches!(Workspace::init(dir.path(), "repo_sdk"), Err(SdkError::InvalidWorkspace(message)) if message.contains("migration"))
        );
        assert_eq!(fs::read(old.join("valuable")).unwrap(), b"history");
    }
}
