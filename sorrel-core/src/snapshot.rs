use crate::{
    stat_cache::{ChangeFingerprint, StatCache, StatCacheEntry},
    ObjectId, ObjectIdParseError, ObjectStore, ObjectStoreError,
};
use cap_fs_ext::DirExt;
use cap_std::fs::{Dir, OpenOptions};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsStr,
    fs,
    io::{self, Write},
    path::{Component, Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::UNIX_EPOCH,
};

const PROTOCOL_VERSION: &str = "sorrel.protocol.v0";
const BLOB_PREFIX: &[u8] = b"sorrel.blob.v0\n";
static RESTORE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Result type used by snapshot object operations.
pub type SnapshotResult<T> = Result<T, SnapshotError>;

/// Errors returned while serializing, materializing, or reading snapshot objects.
#[derive(Debug, thiserror::Error)]
pub enum SnapshotError {
    /// The underlying object store failed.
    #[error(transparent)]
    ObjectStore(#[from] ObjectStoreError),

    /// A filesystem operation failed.
    #[error("snapshot I/O error at {}: {source}", path.display())]
    Io {
        /// Path involved in the failing operation.
        path: PathBuf,
        /// Original I/O error.
        #[source]
        source: io::Error,
    },

    /// A stored JSON object could not be serialized or deserialized.
    #[error(transparent)]
    Json(#[from] serde_json::Error),

    /// A stored object had an unexpected protocol schema version.
    #[error("unsupported schema version {actual:?}; expected {expected:?}")]
    UnsupportedSchemaVersion {
        /// Expected protocol schema version.
        expected: &'static str,
        /// Actual protocol schema version.
        actual: String,
    },

    /// A stored object had an unexpected object kind.
    #[error("invalid object kind {actual:?}; expected {expected:?}")]
    InvalidObjectKind {
        /// Expected object kind.
        expected: &'static str,
        /// Actual object kind.
        actual: String,
    },

    /// Blob bytes did not contain the Sorrel blob envelope.
    #[error("object {0} is not a Sorrel blob")]
    InvalidBlob(ObjectId),

    /// A protocol object reference had an invalid object ID.
    #[error("invalid object id {value:?}: {source}")]
    InvalidObjectId {
        /// Textual object ID value.
        value: String,
        /// Parse error.
        #[source]
        source: ObjectIdParseError,
    },

    /// A path could not be represented safely in a snapshot.
    #[error("invalid snapshot path {}", path.display())]
    InvalidPath {
        /// Invalid path.
        path: PathBuf,
    },

    /// A secret-provider file is already present in the tracked snapshot.
    #[error("protected secret file {} is already tracked; back up the workspace, rotate exposed credentials, and create a clean workspace before snapshotting", path.display())]
    TrackedSecret { path: PathBuf },

    /// Ignore rules or secret-provider configuration could not be loaded safely.
    #[error("invalid workspace configuration at {}: {message}", path.display())]
    WorkspaceConfiguration { path: PathBuf, message: String },

    /// Materialization encountered a filesystem object this first model does not support.
    #[error("unsupported filesystem object at {}", path.display())]
    UnsupportedFileType {
        /// Unsupported path.
        path: PathBuf,
    },
}

impl SnapshotError {
    fn io(path: impl Into<PathBuf>, source: io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }
}

/// Object kinds used by this first snapshot model.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ObjectKind {
    /// Raw file content wrapped as a Sorrel blob object.
    Blob,
    /// Directory tree object.
    Tree,
    /// Workspace snapshot object.
    Snapshot,
    /// Snapshot-to-snapshot change object.
    Change,
    /// Isolated human or agent workstream object.
    Lane,
    /// Ordered stack of changes.
    Stack,
    /// First-class path-level merge conflict object.
    Conflict,
    /// First-class merge outcome object.
    MergeResult,
}

impl ObjectKind {
    fn as_protocol_kind(self) -> &'static str {
        match self {
            Self::Blob => "Blob",
            Self::Tree => "Tree",
            Self::Snapshot => "Snapshot",
            Self::Change => "Change",
            Self::Lane => "Lane",
            Self::Stack => "Stack",
            Self::Conflict => "Conflict",
            Self::MergeResult => "MergeResult",
        }
    }
}

/// Reference to an object in the Sorrel object store.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ObjectRef {
    /// Expected object kind.
    pub kind: ObjectKind,
    /// Content-addressed object identifier.
    pub id: ObjectId,
}

impl ObjectRef {
    /// Builds an object reference.
    #[must_use]
    pub const fn new(kind: ObjectKind, id: ObjectId) -> Self {
        Self { kind, id }
    }
}

/// Materialized blob content.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Blob {
    /// Content-addressed ID of the stored blob object.
    pub id: ObjectId,
    /// Raw file bytes contained by this blob.
    pub content: Vec<u8>,
    /// BLAKE3 hash of the raw file bytes.
    pub content_hash: ObjectId,
}

impl Blob {
    /// Returns the raw content size in bytes.
    #[must_use]
    pub fn size(&self) -> u64 {
        self.content.len() as u64
    }
}

/// Tree entry type.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EntryType {
    /// File entry pointing at a [`Blob`].
    File,
    /// Directory entry pointing at another [`Tree`].
    Directory,
}

impl EntryType {
    fn as_protocol_type(self) -> &'static str {
        match self {
            Self::File => "file",
            Self::Directory => "directory",
        }
    }

    fn from_protocol_type(value: &str) -> SnapshotResult<Self> {
        match value {
            "file" => Ok(Self::File),
            "directory" => Ok(Self::Directory),
            other => Err(SnapshotError::InvalidObjectKind {
                expected: "file or directory",
                actual: other.to_owned(),
            }),
        }
    }
}

/// Tree entry mode.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EntryMode {
    /// Normal file mode.
    Normal,
    /// Executable file mode.
    Executable,
    /// Directory mode.
    Directory,
}

impl EntryMode {
    fn as_protocol_mode(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Executable => "executable",
            Self::Directory => "directory",
        }
    }

    fn from_protocol_mode(value: &str) -> SnapshotResult<Self> {
        match value {
            "normal" => Ok(Self::Normal),
            "executable" => Ok(Self::Executable),
            "directory" => Ok(Self::Directory),
            other => Err(SnapshotError::InvalidObjectKind {
                expected: "normal, executable, or directory",
                actual: other.to_owned(),
            }),
        }
    }
}

/// Entry in a Sorrel tree object.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TreeEntry {
    /// Basename for this entry.
    pub name: String,
    /// Slash-separated path relative to the snapshot root.
    pub path: PathBuf,
    /// Entry type.
    pub entry_type: EntryType,
    /// Referenced object.
    pub object: ObjectRef,
    /// Entry mode.
    pub mode: EntryMode,
    /// File size in bytes for file entries.
    pub size: Option<u64>,
    /// BLAKE3 hash of raw file bytes for file entries.
    pub content_hash: Option<ObjectId>,
}

/// Directory tree object.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Tree {
    /// Content-addressed ID of the stored tree object.
    pub id: ObjectId,
    /// Deterministically sorted tree entries.
    pub entries: Vec<TreeEntry>,
}

/// Principal that created a snapshot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Principal {
    /// Protocol principal type, for example `user`, `agent`, or `system`.
    pub principal_type: String,
    /// Principal identifier.
    pub id: String,
    /// Optional display name.
    pub display_name: Option<String>,
}

impl Principal {
    /// Builds a principal.
    #[must_use]
    pub fn new(
        principal_type: impl Into<String>,
        id: impl Into<String>,
        display_name: Option<String>,
    ) -> Self {
        Self {
            principal_type: principal_type.into(),
            id: id.into(),
            display_name,
        }
    }

    /// Builds a system principal for deterministic local materialization.
    #[must_use]
    pub fn system() -> Self {
        Self::new("system", "system", Some("Sorrel".to_owned()))
    }
}

/// Options used when creating a snapshot object.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SnapshotOptions {
    /// Repository identifier.
    pub repo: String,
    /// Parent snapshots.
    pub parents: Vec<ObjectRef>,
    /// Optional snapshot message.
    pub message: Option<String>,
    /// Protocol timestamp string.
    pub created_at: String,
    /// Snapshot author.
    pub author: Principal,
}

impl SnapshotOptions {
    /// Builds deterministic default options for a repository.
    ///
    /// Callers that need a real wall-clock timestamp should set
    /// [`SnapshotOptions::created_at`] explicitly before writing the snapshot.
    #[must_use]
    pub fn new(repo: impl Into<String>) -> Self {
        Self {
            repo: repo.into(),
            parents: Vec::new(),
            message: None,
            created_at: "1970-01-01T00:00:00Z".to_owned(),
            author: Principal::system(),
        }
    }
}

/// Snapshot object.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Snapshot {
    /// Content-addressed ID of the stored snapshot object.
    pub id: ObjectId,
    /// Repository identifier.
    pub repo: String,
    /// Root tree reference.
    pub root_tree: ObjectRef,
    /// Parent snapshot references.
    pub parents: Vec<ObjectRef>,
    /// Optional snapshot message.
    pub message: Option<String>,
    /// Protocol timestamp string.
    pub created_at: String,
    /// Snapshot author.
    pub author: Principal,
}

/// Stores raw bytes as a Sorrel blob object.
pub fn write_blob(store: &impl ObjectStore, content: &[u8]) -> SnapshotResult<Blob> {
    let bytes = blob_bytes(content);
    let id = store.write(&bytes)?;

    Ok(Blob {
        id,
        content: content.to_vec(),
        content_hash: ObjectId::for_bytes(content),
    })
}

/// Reads a Sorrel blob object.
pub fn read_blob(store: &impl ObjectStore, id: &ObjectId) -> SnapshotResult<Blob> {
    let bytes = store.read(id)?;
    let content = bytes
        .strip_prefix(BLOB_PREFIX)
        .ok_or(SnapshotError::InvalidBlob(*id))?
        .to_vec();

    Ok(Blob {
        id: *id,
        content_hash: ObjectId::for_bytes(&content),
        content,
    })
}

/// Stores a tree object with deterministic entry ordering.
pub fn write_tree(store: &impl ObjectStore, entries: Vec<TreeEntry>) -> SnapshotResult<Tree> {
    let mut entries = entries;
    sort_entries(&mut entries);

    let stored = StoredTree::from_entries(&entries);
    let bytes = serde_json::to_vec(&stored)?;
    let id = store.write(&bytes)?;

    Ok(Tree { id, entries })
}

/// Reads a tree object.
pub fn read_tree(store: &impl ObjectStore, id: &ObjectId) -> SnapshotResult<Tree> {
    let bytes = store.read(id)?;
    let stored: StoredTree = serde_json::from_slice(&bytes)?;
    stored.ensure_kind("Tree")?;

    Ok(Tree {
        id: *id,
        entries: stored.into_entries()?,
    })
}

/// Materializes a filesystem directory as a tree object.
pub fn write_tree_from_directory(
    store: &impl ObjectStore,
    root: impl AsRef<Path>,
) -> SnapshotResult<Tree> {
    write_tree_from_directory_with_stat_cache(store, root, None)
}

/// Materializes a filesystem directory as a tree object, optionally using a
/// [`StatCache`] to skip re-hashing unchanged files.
pub fn write_tree_from_directory_with_stat_cache(
    store: &impl ObjectStore,
    root: impl AsRef<Path>,
    stat_cache: Option<&mut StatCache>,
) -> SnapshotResult<Tree> {
    write_tree_from_directory_excluding_with_stat_cache(
        store,
        root,
        std::iter::empty::<&str>(),
        stat_cache,
    )
}

/// Materializes a filesystem directory as a tree object, skipping the given
/// top-level entry names (e.g. `.sorrel`) at the working-tree root.
///
/// Exclusions apply only at the root directory; nested files/directories with
/// the same name are still included. This lets a Sorrel workspace snapshot its
/// own working tree without recursing into the on-disk object store.
pub fn write_tree_from_directory_excluding<I, S>(
    store: &impl ObjectStore,
    root: impl AsRef<Path>,
    excluded_root_names: I,
) -> SnapshotResult<Tree>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    write_tree_from_directory_excluding_with_stat_cache(store, root, excluded_root_names, None)
}

/// Materializes a filesystem directory as a tree object, skipping the given
/// top-level entry names and optionally using a [`StatCache`] to skip
/// re-hashing unchanged files.
///
/// Exclusions apply only at the root directory; nested files/directories with
/// the same name are still included. See
/// [`write_tree_from_directory_excluding`].
pub fn write_tree_from_directory_excluding_with_stat_cache<I, S>(
    store: &impl ObjectStore,
    root: impl AsRef<Path>,
    excluded_root_names: I,
    stat_cache: Option<&mut StatCache>,
) -> SnapshotResult<Tree>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let excluded = excluded_root_names
        .into_iter()
        .map(|name| name.as_ref().to_os_string())
        .collect::<BTreeSet<_>>();
    match stat_cache {
        Some(cache) => {
            let mut paths_seen = BTreeSet::new();
            let tree = write_tree_from_dir(
                store,
                root.as_ref(),
                Path::new(""),
                &excluded,
                Some(cache),
                Some(&mut paths_seen),
                None,
            )?;
            cache.retain(&paths_seen);
            Ok(tree)
        }
        None => write_tree_from_dir(
            store,
            root.as_ref(),
            Path::new(""),
            &excluded,
            None,
            None,
            None,
        ),
    }
}

/// Stores a snapshot object for an existing root tree.
pub fn write_snapshot(
    store: &impl ObjectStore,
    root_tree_id: ObjectId,
    options: SnapshotOptions,
) -> SnapshotResult<Snapshot> {
    let root_tree = ObjectRef::new(ObjectKind::Tree, root_tree_id);
    let stored = StoredSnapshot::from_options(&options, root_tree);
    let bytes = serde_json::to_vec(&stored)?;
    let id = store.write(&bytes)?;

    Ok(Snapshot {
        id,
        repo: options.repo,
        root_tree,
        parents: options.parents,
        message: options.message,
        created_at: options.created_at,
        author: options.author,
    })
}

/// Reads a snapshot object.
pub fn read_snapshot(store: &impl ObjectStore, id: &ObjectId) -> SnapshotResult<Snapshot> {
    let bytes = store.read(id)?;
    let stored: StoredSnapshot = serde_json::from_slice(&bytes)?;
    stored.ensure_kind("Snapshot")?;
    stored.into_snapshot(*id)
}

/// Materializes a filesystem directory as a snapshot.
pub fn materialize_snapshot(
    store: &impl ObjectStore,
    root: impl AsRef<Path>,
    options: SnapshotOptions,
) -> SnapshotResult<Snapshot> {
    materialize_snapshot_with_stat_cache(store, root, None, options)
}

/// Materializes a filesystem directory as a snapshot, optionally using a
/// [`StatCache`] to skip re-hashing unchanged files.
pub fn materialize_snapshot_with_stat_cache(
    store: &impl ObjectStore,
    root: impl AsRef<Path>,
    stat_cache: Option<&mut StatCache>,
    options: SnapshotOptions,
) -> SnapshotResult<Snapshot> {
    let root_tree = write_tree_from_directory_with_stat_cache(store, root, stat_cache)?;
    write_snapshot(store, root_tree.id, options)
}

/// Materializes a filesystem directory as a snapshot, skipping the given
/// top-level entry names (e.g. `.sorrel`) at the working-tree root.
///
/// This is the persistence-friendly entry point used by the CLI: it avoids
/// copying the working tree to a scratch directory just to exclude the object
/// store. Exclusions apply only at the root (see
/// [`write_tree_from_directory_excluding`]).
pub fn materialize_snapshot_excluding<I, S>(
    store: &impl ObjectStore,
    root: impl AsRef<Path>,
    excluded_root_names: I,
    options: SnapshotOptions,
) -> SnapshotResult<Snapshot>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    materialize_snapshot_excluding_with_stat_cache(store, root, excluded_root_names, None, options)
}

/// Materializes a filesystem directory as a snapshot, skipping the given
/// top-level entry names and optionally using a [`StatCache`] to skip
/// re-hashing unchanged files.
///
/// This is the persistence-friendly entry point used by the CLI: it avoids
/// copying the working tree to a scratch directory just to exclude the object
/// store. Exclusions apply only at the root (see
/// [`write_tree_from_directory_excluding`]).
pub fn materialize_snapshot_excluding_with_stat_cache<I, S>(
    store: &impl ObjectStore,
    root: impl AsRef<Path>,
    excluded_root_names: I,
    stat_cache: Option<&mut StatCache>,
    options: SnapshotOptions,
) -> SnapshotResult<Snapshot>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let root_tree = write_tree_from_directory_excluding_with_stat_cache(
        store,
        root,
        excluded_root_names,
        stat_cache,
    )?;
    write_snapshot(store, root_tree.id, options)
}

/// Reads all files in a snapshot into memory, keyed by relative path.
pub fn read_snapshot_files(
    store: &impl ObjectStore,
    snapshot_id: &ObjectId,
) -> SnapshotResult<BTreeMap<PathBuf, Vec<u8>>> {
    let snapshot = read_snapshot(store, snapshot_id)?;
    let mut files = BTreeMap::new();
    collect_tree_files(store, &snapshot.root_tree.id, &mut files)?;
    Ok(files)
}

/// Restores a snapshot into a target directory.
///
/// Existing files are overwritten when their paths are present in the snapshot.
/// Files that are not present in the snapshot are left untouched.
/// Top-level `.sorrel` and `.git` paths are rejected before any files are written.
pub fn restore_snapshot_to_directory(
    store: &impl ObjectStore,
    snapshot_id: &ObjectId,
    target: impl AsRef<Path>,
) -> SnapshotResult<()> {
    let target = target.as_ref();
    let snapshot = read_snapshot(store, snapshot_id)?;
    validate_restore_tree(store, &snapshot.root_tree.id, target)?;
    fs::create_dir_all(target).map_err(|source| SnapshotError::io(target, source))?;
    let directory = Dir::open_ambient_dir(target, cap_std::ambient_authority())
        .map_err(|source| SnapshotError::io(target, source))?;
    restore_tree(store, &snapshot.root_tree.id, target, &directory)
}

/// Writes a relative working-tree file without following links, preserving existing permissions.
pub fn write_snapshot_file_to_directory(
    target: impl AsRef<Path>,
    relative_path: impl AsRef<Path>,
    bytes: &[u8],
) -> SnapshotResult<()> {
    let target = target.as_ref();
    let relative_path = relative_path.as_ref();
    let path = safe_target_path(target, relative_path)?;
    let name = relative_path
        .file_name()
        .ok_or_else(|| SnapshotError::InvalidPath {
            path: relative_path.to_path_buf(),
        })?;
    let directory = Dir::open_ambient_dir(target, cap_std::ambient_authority())
        .map_err(|source| SnapshotError::io(target, source))?;
    let parent = restore_parent(&directory, relative_path.parent().unwrap_or(Path::new("")))
        .map_err(|source| SnapshotError::io(&path, source))?;
    write_restored_file(&parent, name, &path, bytes, None)
}

/// Removes a relative tracked file and its empty parents without following links.
/// Missing paths are ignored. The target directory itself is never removed.
pub fn remove_snapshot_file_from_directory(
    target: impl AsRef<Path>,
    relative_path: impl AsRef<Path>,
) -> SnapshotResult<()> {
    let target = target.as_ref();
    let relative_path = relative_path.as_ref();
    let path = safe_target_path(target, relative_path)?;
    let name = relative_path
        .file_name()
        .ok_or_else(|| SnapshotError::InvalidPath {
            path: relative_path.to_path_buf(),
        })?;
    let mut parent = Dir::open_ambient_dir(target, cap_std::ambient_authority())
        .map_err(|source| SnapshotError::io(target, source))?;
    let mut ancestors = Vec::new();
    for component in relative_path.parent().unwrap_or(Path::new("")).components() {
        let Component::Normal(name) = component else {
            return Err(SnapshotError::InvalidPath {
                path: relative_path.to_path_buf(),
            });
        };
        let child = match parent.open_dir_nofollow(name) {
            Ok(child) => child,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(source) => return Err(SnapshotError::io(&path, source)),
        };
        ancestors.push((parent, name.to_owned()));
        parent = child;
    }
    match parent.remove_file(name) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(source) => return Err(SnapshotError::io(&path, source)),
    }
    drop(parent);
    for (parent, name) in ancestors.into_iter().rev() {
        match parent.remove_dir(name) {
            Ok(()) => {}
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::NotFound
                        | io::ErrorKind::DirectoryNotEmpty
                        | io::ErrorKind::PermissionDenied
                ) =>
            {
                break
            }
            Err(source) => return Err(SnapshotError::io(&path, source)),
        }
    }
    Ok(())
}

fn write_file_blob(
    store: &impl ObjectStore,
    cache: &mut StatCache,
    protocol_path: &str,
    child_path: &Path,
    metadata: &fs::Metadata,
) -> SnapshotResult<Blob> {
    let fingerprint = ChangeFingerprint::from_metadata(metadata);
    let content = fs::read(child_path).map_err(|source| SnapshotError::io(child_path, source))?;
    let blob = write_blob(store, &content)?;
    let after = fs::metadata(child_path).map_err(|source| SnapshotError::io(child_path, source))?;
    let (mtime_secs, mtime_nanos) = file_mtime(metadata, child_path)?;
    // A writer or replacement during the read must not seed a reusable entry.
    let verified = fingerprint.filter(|fingerprint| {
        Some(fingerprint) == ChangeFingerprint::from_metadata(&after).as_ref()
            && metadata.len() == after.len()
            && file_mtime(&after, child_path).ok() == Some((mtime_secs, mtime_nanos))
            && content.len() as u64 == metadata.len()
    });
    cache.insert_verified(
        protocol_path.to_owned(),
        StatCacheEntry {
            size: metadata.len(),
            mtime_secs,
            mtime_nanos,
            object_id: blob.id,
        },
        verified,
    );
    Ok(blob)
}

fn file_mtime(metadata: &fs::Metadata, path: &Path) -> SnapshotResult<(u64, u32)> {
    let modified = metadata
        .modified()
        .map_err(|source| SnapshotError::io(path, source))?;
    let duration = modified.duration_since(UNIX_EPOCH).map_err(|_| {
        SnapshotError::io(
            path,
            io::Error::new(io::ErrorKind::InvalidData, "mtime before UNIX epoch"),
        )
    })?;
    Ok((duration.as_secs(), duration.subsec_nanos()))
}

fn blob_bytes(content: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(BLOB_PREFIX.len() + content.len());
    bytes.extend_from_slice(BLOB_PREFIX);
    bytes.extend_from_slice(content);
    bytes
}

pub(crate) fn write_tree_from_dir(
    store: &impl ObjectStore,
    root: &Path,
    relative_dir: &Path,
    excluded_root_names: &BTreeSet<std::ffi::OsString>,
    mut stat_cache: Option<&mut StatCache>,
    mut paths_seen: Option<&mut BTreeSet<String>>,
    mut selection: Option<&mut crate::workspace::WorkspaceSelection>,
) -> SnapshotResult<Tree> {
    let directory = root.join(relative_dir);
    let is_root = relative_dir.as_os_str().is_empty();
    let mut children = fs::read_dir(&directory)
        .map_err(|source| SnapshotError::io(&directory, source))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|source| SnapshotError::io(&directory, source))?;

    children.sort_by_key(|entry| entry.file_name());

    let mut entries = Vec::with_capacity(children.len());
    for child in children {
        let child_path = child.path();
        // Skip excluded names only at the working-tree root.
        if is_root && excluded_root_names.contains(&child.file_name()) {
            continue;
        }
        let name = file_name_to_string(&child.file_name(), &child_path)?;
        let relative_path = relative_dir.join(&name);
        validate_relative_path(&relative_path)?;

        let file_type = child
            .file_type()
            .map_err(|source| SnapshotError::io(&child_path, source))?;

        if let Some(selection) = selection.as_deref_mut() {
            if !selection.allows(&relative_path, file_type.is_dir())? {
                continue;
            }
        }

        if file_type.is_dir() {
            let child_tree = write_tree_from_dir(
                store,
                root,
                &relative_path,
                excluded_root_names,
                stat_cache.as_deref_mut(),
                paths_seen.as_deref_mut(),
                selection.as_deref_mut(),
            )?;
            entries.push(TreeEntry {
                name,
                path: relative_path,
                entry_type: EntryType::Directory,
                object: ObjectRef::new(ObjectKind::Tree, child_tree.id),
                mode: EntryMode::Directory,
                size: None,
                content_hash: None,
            });
        } else if file_type.is_file() {
            let protocol_path = path_to_protocol_string(&relative_path);
            if let Some(seen) = paths_seen.as_deref_mut() {
                seen.insert(protocol_path.clone());
            }

            let metadata = fs::metadata(&child_path)
                .map_err(|source| SnapshotError::io(&child_path, source))?;
            let (mtime_secs, mtime_nanos) = file_mtime(&metadata, &child_path)?;
            let file_size = metadata.len();

            let blob = if let Some(cache) = stat_cache.as_deref_mut() {
                if let Some(entry) = cache.get(&protocol_path) {
                    if entry.size == file_size
                        && entry.mtime_secs == mtime_secs
                        && entry.mtime_nanos == mtime_nanos
                        && cache.matches_fingerprint(&protocol_path, &metadata)
                        && store.has(&entry.object_id)?
                    {
                        read_blob(store, &entry.object_id)?
                    } else {
                        write_file_blob(store, cache, &protocol_path, &child_path, &metadata)?
                    }
                } else {
                    write_file_blob(store, cache, &protocol_path, &child_path, &metadata)?
                }
            } else {
                let content = fs::read(&child_path)
                    .map_err(|source| SnapshotError::io(&child_path, source))?;
                write_blob(store, &content)?
            };

            entries.push(TreeEntry {
                name,
                path: relative_path,
                entry_type: EntryType::File,
                object: ObjectRef::new(ObjectKind::Blob, blob.id),
                mode: file_mode(&child_path)?,
                size: Some(blob.size()),
                content_hash: Some(blob.content_hash),
            });
        } else {
            return Err(SnapshotError::UnsupportedFileType { path: child_path });
        }
    }

    write_tree(store, entries)
}

fn collect_tree_files(
    store: &impl ObjectStore,
    tree_id: &ObjectId,
    files: &mut BTreeMap<PathBuf, Vec<u8>>,
) -> SnapshotResult<()> {
    let tree = read_tree(store, tree_id)?;
    for entry in tree.entries {
        match entry.entry_type {
            EntryType::File => {
                let blob = read_blob(store, &entry.object.id)?;
                files.insert(entry.path, blob.content);
            }
            EntryType::Directory => collect_tree_files(store, &entry.object.id, files)?,
        }
    }

    Ok(())
}

fn validate_restore_tree(
    store: &impl ObjectStore,
    tree_id: &ObjectId,
    target: &Path,
) -> SnapshotResult<()> {
    for entry in read_tree(store, tree_id)?.entries {
        safe_target_path(target, &entry.path)?;
        if entry.entry_type == EntryType::Directory {
            validate_restore_tree(store, &entry.object.id, target)?;
        }
    }
    Ok(())
}

fn restore_tree(
    store: &impl ObjectStore,
    tree_id: &ObjectId,
    target: &Path,
    directory: &Dir,
) -> SnapshotResult<()> {
    let tree = read_tree(store, tree_id)?;
    for entry in tree.entries {
        let output_path = safe_target_path(target, &entry.path)?;
        let name = entry
            .path
            .file_name()
            .ok_or_else(|| SnapshotError::InvalidPath {
                path: entry.path.clone(),
            })?;
        let parent = restore_parent(directory, entry.path.parent().unwrap_or(Path::new("")))
            .map_err(|source| SnapshotError::io(&output_path, source))?;
        match entry.entry_type {
            EntryType::File => {
                let blob = read_blob(store, &entry.object.id)?;
                write_restored_file(&parent, name, &output_path, &blob.content, Some(entry.mode))?;
            }
            EntryType::Directory => {
                match parent.create_dir(name) {
                    Ok(()) => {}
                    Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                    Err(source) => return Err(SnapshotError::io(&output_path, source)),
                }
                parent
                    .open_dir_nofollow(name)
                    .map_err(|source| SnapshotError::io(&output_path, source))?;
                restore_tree(store, &entry.object.id, target, directory)?;
            }
        }
    }
    Ok(())
}

fn restore_parent(directory: &Dir, path: &Path) -> io::Result<Dir> {
    let mut parent = directory.try_clone()?;
    for component in path.components() {
        let Component::Normal(name) = component else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid restore path",
            ));
        };
        match parent.create_dir(name) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
        // Hold each directory handle and reject links atomically while traversing.
        parent = parent.open_dir_nofollow(name)?;
    }
    Ok(parent)
}

fn open_restore_temporary_file(
    parent: &Dir,
    name: &str,
    permissions: &mut Option<cap_std::fs::Permissions>,
) -> io::Result<cap_std::fs::File> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use cap_std::fs::OpenOptionsExt;
        if permissions.is_none() {
            // Sample umask on a separate empty inode. A reader holding this probe's
            // descriptor must never see the contents of the private replacement.
            options.mode(0o666);
            let probe = parent.open_with(name, &options)?;
            let metadata = probe.metadata();
            drop(probe);
            parent.remove_file(name)?;
            *permissions = Some(metadata?.permissions());
        }
        options.mode(0o600);
    }
    let file = parent.open_with(name, &options)?;
    let result = (|| {
        if permissions.is_none() {
            *permissions = Some(file.metadata()?.permissions());
        }
        #[cfg(unix)]
        {
            use cap_std::fs::PermissionsExt;
            file.set_permissions(cap_std::fs::Permissions::from_mode(0o600))?;
        }
        Ok::<_, io::Error>(())
    })();
    if let Err(error) = result {
        drop(file);
        parent.remove_file(name).map_err(|cleanup| {
            io::Error::other(format!(
                "{error}; temporary restore file cleanup failed: {cleanup}"
            ))
        })?;
        return Err(error);
    }
    Ok(file)
}

fn write_restored_file(
    parent: &Dir,
    name: &OsStr,
    path: &Path,
    bytes: &[u8],
    mode: Option<EntryMode>,
) -> SnapshotResult<()> {
    let mut permissions = match parent.symlink_metadata(name) {
        Ok(metadata) if !metadata.is_file() => {
            return Err(SnapshotError::UnsupportedFileType {
                path: path.to_path_buf(),
            })
        }
        Ok(metadata) if metadata.permissions().readonly() => {
            return Err(SnapshotError::io(
                path,
                io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "restore target is read-only",
                ),
            ))
        }
        Ok(metadata) => Some(metadata.permissions()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => None,
        Err(source) => return Err(SnapshotError::io(path, source)),
    };
    let (temporary, mut file) = loop {
        let sequence = RESTORE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let name = format!(".sorrel-restore-{}.{}.tmp", std::process::id(), sequence);
        match open_restore_temporary_file(parent, &name, &mut permissions) {
            Ok(file) => break (name, file),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(source) => return Err(SnapshotError::io(path, source)),
        }
    };
    let result = (|| {
        file.write_all(bytes)
            .map_err(|source| SnapshotError::io(path, source))?;
        if let Some(mode) = mode {
            set_file_mode(&file, path, mode, permissions)?;
        } else if let Some(permissions) = permissions {
            #[cfg(unix)]
            let permissions = {
                use cap_std::fs::PermissionsExt;
                cap_std::fs::Permissions::from_mode(permissions.mode() & 0o777)
            };
            file.set_permissions(permissions)
                .map_err(|source| SnapshotError::io(path, source))?;
        }
        drop(file);
        // Replacing a directory entry never writes through a symlink or hard link.
        parent
            .rename(&temporary, parent, name)
            .map_err(|source| SnapshotError::io(path, source))
    })();
    match result {
        Ok(()) => Ok(()),
        Err(error) => match parent.remove_file(&temporary) {
            Ok(()) => Err(error),
            Err(cleanup) if cleanup.kind() == io::ErrorKind::NotFound => Err(error),
            Err(cleanup) => Err(SnapshotError::io(
                path,
                io::Error::other(format!(
                    "{error}; temporary restore file cleanup failed: {cleanup}"
                )),
            )),
        },
    }
}

fn sort_entries(entries: &mut [TreeEntry]) {
    entries.sort_by(|left, right| {
        path_to_protocol_string(&left.path).cmp(&path_to_protocol_string(&right.path))
    });
}

fn file_name_to_string(name: &OsStr, path: &Path) -> SnapshotResult<String> {
    let name = name
        .to_str()
        .ok_or_else(|| SnapshotError::InvalidPath {
            path: path.to_path_buf(),
        })?
        .to_owned();

    if name.is_empty() || name == "." || name == ".." {
        return Err(SnapshotError::InvalidPath {
            path: path.to_path_buf(),
        });
    }

    Ok(name)
}

fn validate_relative_path(path: &Path) -> SnapshotResult<()> {
    if path.as_os_str().is_empty() {
        return Ok(());
    }

    let valid = path.components().all(|component| {
        matches!(component, Component::Normal(name) if !name.is_empty() && name != "." && name != "..")
    });

    if valid {
        Ok(())
    } else {
        Err(SnapshotError::InvalidPath {
            path: path.to_path_buf(),
        })
    }
}

fn safe_target_path(target: &Path, relative_path: &Path) -> SnapshotResult<PathBuf> {
    validate_relative_path(relative_path)?;
    let reserved = relative_path.components().next().is_some_and(|component| {
        let Component::Normal(name) = component else {
            return true;
        };
        // Windows ignores trailing dots/spaces and uses case-insensitive names.
        let name = name.to_string_lossy();
        let name = name.trim_end_matches(['.', ' ']);
        name.eq_ignore_ascii_case(".sorrel") || name.eq_ignore_ascii_case(".git")
    });
    if relative_path.as_os_str().is_empty() || reserved {
        return Err(SnapshotError::InvalidPath {
            path: relative_path.to_path_buf(),
        });
    }
    Ok(target.join(relative_path))
}

fn path_to_protocol_string(path: &Path) -> String {
    path.components()
        .filter_map(|component| match component {
            Component::Normal(name) => name.to_str(),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

fn parse_protocol_path(path: &str) -> SnapshotResult<PathBuf> {
    let path = PathBuf::from(path);
    validate_relative_path(&path)?;
    Ok(path)
}

fn parse_object_id(value: &str) -> SnapshotResult<ObjectId> {
    value
        .parse()
        .map_err(|source| SnapshotError::InvalidObjectId {
            value: value.to_owned(),
            source,
        })
}

#[cfg(unix)]
fn file_mode(path: &Path) -> SnapshotResult<EntryMode> {
    use std::os::unix::fs::PermissionsExt;

    let metadata = fs::metadata(path).map_err(|source| SnapshotError::io(path, source))?;
    if metadata.permissions().mode() & 0o111 != 0 {
        Ok(EntryMode::Executable)
    } else {
        Ok(EntryMode::Normal)
    }
}

#[cfg(not(unix))]
fn file_mode(_path: &Path) -> SnapshotResult<EntryMode> {
    Ok(EntryMode::Normal)
}

#[cfg(unix)]
fn set_file_mode(
    file: &cap_std::fs::File,
    path: &Path,
    mode: EntryMode,
    existing: Option<cap_std::fs::Permissions>,
) -> SnapshotResult<()> {
    use cap_std::fs::PermissionsExt as _;
    use std::os::unix::fs::PermissionsExt;

    // Snapshots track executability, not read/write access. New files retain umask.
    let permissions = match existing {
        Some(permissions) => permissions,
        None => file
            .metadata()
            .map_err(|source| SnapshotError::io(path, source))?
            .permissions(),
    };
    let read_write = permissions.mode() & 0o666;
    let executable = match mode {
        EntryMode::Executable => (read_write & 0o444) >> 2,
        EntryMode::Normal | EntryMode::Directory => 0,
    };
    // Do not carry set-id bits onto replacement content.
    let permissions = fs::Permissions::from_mode(read_write | executable);
    file.set_permissions(cap_std::fs::Permissions::from_std(permissions))
        .map_err(|source| SnapshotError::io(path, source))
}

#[cfg(not(unix))]
fn set_file_mode(
    _file: &cap_std::fs::File,
    _path: &Path,
    _mode: EntryMode,
    existing: Option<cap_std::fs::Permissions>,
) -> SnapshotResult<()> {
    if let Some(permissions) = existing {
        _file
            .set_permissions(permissions)
            .map_err(|source| SnapshotError::io(_path, source))?;
    }
    Ok(())
}

#[derive(Serialize, Deserialize)]
struct StoredObjectRef {
    kind: String,
    id: String,
}

impl From<ObjectRef> for StoredObjectRef {
    fn from(value: ObjectRef) -> Self {
        Self {
            kind: value.kind.as_protocol_kind().to_owned(),
            id: value.id.to_string(),
        }
    }
}

impl StoredObjectRef {
    fn into_ref(self) -> SnapshotResult<ObjectRef> {
        let kind = match self.kind.as_str() {
            "Blob" => ObjectKind::Blob,
            "Tree" => ObjectKind::Tree,
            "Snapshot" => ObjectKind::Snapshot,
            "Change" => ObjectKind::Change,
            "Lane" => ObjectKind::Lane,
            "Stack" => ObjectKind::Stack,
            "Conflict" => ObjectKind::Conflict,
            "MergeResult" => ObjectKind::MergeResult,
            other => {
                return Err(SnapshotError::InvalidObjectKind {
                    expected: "Blob, Tree, Snapshot, Change, Lane, Stack, Conflict, or MergeResult",
                    actual: other.to_owned(),
                });
            }
        };

        Ok(ObjectRef::new(kind, parse_object_id(&self.id)?))
    }
}

#[derive(Serialize, Deserialize)]
struct StoredHash {
    algorithm: String,
    value: String,
}

impl From<ObjectId> for StoredHash {
    fn from(value: ObjectId) -> Self {
        Self {
            algorithm: "blake3".to_owned(),
            value: value.to_string(),
        }
    }
}

impl StoredHash {
    fn into_object_id(self) -> SnapshotResult<ObjectId> {
        if self.algorithm != "blake3" {
            return Err(SnapshotError::InvalidObjectKind {
                expected: "blake3",
                actual: self.algorithm,
            });
        }

        parse_object_id(&self.value)
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredTree {
    schema_version: String,
    kind: String,
    entries: Vec<StoredTreeEntry>,
}

impl StoredTree {
    fn from_entries(entries: &[TreeEntry]) -> Self {
        Self {
            schema_version: PROTOCOL_VERSION.to_owned(),
            kind: "Tree".to_owned(),
            entries: entries.iter().map(StoredTreeEntry::from_entry).collect(),
        }
    }

    fn ensure_kind(&self, expected: &'static str) -> SnapshotResult<()> {
        if self.schema_version != PROTOCOL_VERSION {
            return Err(SnapshotError::UnsupportedSchemaVersion {
                expected: PROTOCOL_VERSION,
                actual: self.schema_version.clone(),
            });
        }

        if self.kind != expected {
            return Err(SnapshotError::InvalidObjectKind {
                expected,
                actual: self.kind.clone(),
            });
        }

        Ok(())
    }

    fn into_entries(self) -> SnapshotResult<Vec<TreeEntry>> {
        self.entries
            .into_iter()
            .map(StoredTreeEntry::into_entry)
            .collect()
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredTreeEntry {
    name: String,
    path: String,
    #[serde(rename = "type")]
    entry_type: String,
    object: StoredObjectRef,
    mode: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    size: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    content_hash: Option<StoredHash>,
}

impl StoredTreeEntry {
    fn from_entry(entry: &TreeEntry) -> Self {
        Self {
            name: entry.name.clone(),
            path: path_to_protocol_string(&entry.path),
            entry_type: entry.entry_type.as_protocol_type().to_owned(),
            object: entry.object.into(),
            mode: entry.mode.as_protocol_mode().to_owned(),
            size: entry.size,
            content_hash: entry.content_hash.map(StoredHash::from),
        }
    }

    fn into_entry(self) -> SnapshotResult<TreeEntry> {
        let path = parse_protocol_path(&self.path)?;
        Ok(TreeEntry {
            name: self.name,
            path,
            entry_type: EntryType::from_protocol_type(&self.entry_type)?,
            object: self.object.into_ref()?,
            mode: EntryMode::from_protocol_mode(&self.mode)?,
            size: self.size,
            content_hash: self
                .content_hash
                .map(StoredHash::into_object_id)
                .transpose()?,
        })
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredSnapshot {
    schema_version: String,
    kind: String,
    repo: String,
    root_tree: StoredObjectRef,
    parents: Vec<StoredObjectRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
    created_at: String,
    author: StoredPrincipal,
}

impl StoredSnapshot {
    fn from_options(options: &SnapshotOptions, root_tree: ObjectRef) -> Self {
        Self {
            schema_version: PROTOCOL_VERSION.to_owned(),
            kind: "Snapshot".to_owned(),
            repo: options.repo.clone(),
            root_tree: root_tree.into(),
            parents: options
                .parents
                .iter()
                .copied()
                .map(StoredObjectRef::from)
                .collect(),
            message: options.message.clone(),
            created_at: options.created_at.clone(),
            author: StoredPrincipal::from_principal(&options.author),
        }
    }

    fn ensure_kind(&self, expected: &'static str) -> SnapshotResult<()> {
        if self.schema_version != PROTOCOL_VERSION {
            return Err(SnapshotError::UnsupportedSchemaVersion {
                expected: PROTOCOL_VERSION,
                actual: self.schema_version.clone(),
            });
        }

        if self.kind != expected {
            return Err(SnapshotError::InvalidObjectKind {
                expected,
                actual: self.kind.clone(),
            });
        }

        Ok(())
    }

    fn into_snapshot(self, id: ObjectId) -> SnapshotResult<Snapshot> {
        Ok(Snapshot {
            id,
            repo: self.repo,
            root_tree: self.root_tree.into_ref()?,
            parents: self
                .parents
                .into_iter()
                .map(StoredObjectRef::into_ref)
                .collect::<SnapshotResult<Vec<_>>>()?,
            message: self.message,
            created_at: self.created_at,
            author: self.author.into_principal(),
        })
    }
}

#[derive(Serialize, Deserialize)]
struct StoredPrincipal {
    #[serde(rename = "type")]
    principal_type: String,
    id: String,
    #[serde(rename = "displayName", skip_serializing_if = "Option::is_none")]
    display_name: Option<String>,
}

impl StoredPrincipal {
    fn from_principal(principal: &Principal) -> Self {
        Self {
            principal_type: principal.principal_type.clone(),
            id: principal.id.clone(),
            display_name: principal.display_name.clone(),
        }
    }

    fn into_principal(self) -> Principal {
        Principal {
            principal_type: self.principal_type,
            id: self.id,
            display_name: self.display_name,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::InMemoryObjectStore;

    #[test]
    fn stores_and_reads_blob() {
        let store = InMemoryObjectStore::new();

        let blob = write_blob(&store, b"hello snapshot").unwrap();
        let read = read_blob(&store, &blob.id).unwrap();

        assert_eq!(read, blob);
        assert_eq!(read.content, b"hello snapshot");
        assert_eq!(read.content_hash, ObjectId::for_bytes(b"hello snapshot"));
    }

    #[test]
    fn creates_tree_from_files() {
        let temp_dir = tempfile::tempdir().unwrap();
        write_file(temp_dir.path().join("README.md"), b"# Sorrel\n");
        fs::create_dir(temp_dir.path().join("src")).unwrap();
        write_file(temp_dir.path().join("src/lib.rs"), b"pub fn core() {}\n");
        let store = InMemoryObjectStore::new();

        let tree = write_tree_from_directory(&store, temp_dir.path()).unwrap();
        let root_tree = read_tree(&store, &tree.id).unwrap();

        assert_eq!(root_tree.entries.len(), 2);
        assert_eq!(root_tree.entries[0].path, PathBuf::from("README.md"));
        assert_eq!(root_tree.entries[0].entry_type, EntryType::File);
        assert_eq!(root_tree.entries[1].path, PathBuf::from("src"));
        assert_eq!(root_tree.entries[1].entry_type, EntryType::Directory);

        let src_tree = read_tree(&store, &root_tree.entries[1].object.id).unwrap();
        assert_eq!(src_tree.entries.len(), 1);
        assert_eq!(src_tree.entries[0].path, PathBuf::from("src/lib.rs"));
    }

    #[test]
    fn excludes_root_names_but_keeps_nested_same_names() {
        let temp_dir = tempfile::tempdir().unwrap();
        write_file(temp_dir.path().join("README.md"), b"# Sorrel\n");
        // Root-level `.sorrel` (the object store) must be excluded.
        fs::create_dir(temp_dir.path().join(".sorrel")).unwrap();
        write_file(
            temp_dir.path().join(".sorrel/objects"),
            b"do-not-snapshot\n",
        );
        // A nested directory that happens to share the excluded name must be kept.
        fs::create_dir_all(temp_dir.path().join("src/.sorrel")).unwrap();
        write_file(temp_dir.path().join("src/.sorrel/keep.txt"), b"keep\n");
        let store = InMemoryObjectStore::new();

        let snapshot = materialize_snapshot_excluding(
            &store,
            temp_dir.path(),
            [".sorrel"],
            SnapshotOptions::new("repo_sorrel"),
        )
        .unwrap();

        let files = read_snapshot_files(&store, &snapshot.id).unwrap();
        assert!(files.contains_key(&PathBuf::from("README.md")));
        assert!(files.contains_key(&PathBuf::from("src/.sorrel/keep.txt")));
        assert!(
            !files.keys().any(|path| path.starts_with(".sorrel")),
            "root .sorrel must not be snapshotted: {files:?}"
        );
    }

    #[test]
    fn creates_snapshot() {
        let temp_dir = tempfile::tempdir().unwrap();
        write_file(temp_dir.path().join("README.md"), b"# Sorrel\n");
        let store = InMemoryObjectStore::new();

        let snapshot =
            materialize_snapshot(&store, temp_dir.path(), SnapshotOptions::new("repo_sorrel"))
                .unwrap();
        let read = read_snapshot(&store, &snapshot.id).unwrap();

        assert_eq!(read.repo, "repo_sorrel");
        assert_eq!(read.root_tree.kind, ObjectKind::Tree);
        assert_eq!(read.root_tree, snapshot.root_tree);
    }

    #[test]
    fn reads_snapshot_back_into_memory_and_directory() {
        let temp_dir = tempfile::tempdir().unwrap();
        write_file(temp_dir.path().join("README.md"), b"# Sorrel\n");
        fs::create_dir(temp_dir.path().join("src")).unwrap();
        write_file(temp_dir.path().join("src/lib.rs"), b"pub fn core() {}\n");
        let store = InMemoryObjectStore::new();
        let snapshot =
            materialize_snapshot(&store, temp_dir.path(), SnapshotOptions::new("repo_sorrel"))
                .unwrap();

        let files = read_snapshot_files(&store, &snapshot.id).unwrap();
        assert_eq!(files[&PathBuf::from("README.md")], b"# Sorrel\n");
        assert_eq!(files[&PathBuf::from("src/lib.rs")], b"pub fn core() {}\n");

        let restore_dir = tempfile::tempdir().unwrap();
        restore_snapshot_to_directory(&store, &snapshot.id, restore_dir.path()).unwrap();
        assert_eq!(
            fs::read(restore_dir.path().join("README.md")).unwrap(),
            b"# Sorrel\n"
        );
        assert_eq!(
            fs::read(restore_dir.path().join("src/lib.rs")).unwrap(),
            b"pub fn core() {}\n"
        );
    }

    #[cfg(unix)]
    #[test]
    fn restoration_rejects_symlink_components_and_preserves_external_files() {
        use std::os::unix::fs::symlink;
        for final_link in [false, true] {
            let source = tempfile::tempdir().unwrap();
            fs::create_dir_all(source.path().join("linked")).unwrap();
            write_file(source.path().join("linked/file"), b"replacement");
            let store = InMemoryObjectStore::new();
            let snapshot =
                materialize_snapshot(&store, source.path(), SnapshotOptions::new("repo_test"))
                    .unwrap();
            let target = tempfile::tempdir().unwrap();
            let outside = tempfile::tempdir().unwrap();
            write_file(outside.path().join("file"), b"private-original");
            if final_link {
                fs::create_dir(target.path().join("linked")).unwrap();
                symlink(
                    outside.path().join("file"),
                    target.path().join("linked/file"),
                )
                .unwrap();
            } else {
                symlink(outside.path(), target.path().join("linked")).unwrap();
            }
            assert!(restore_snapshot_to_directory(&store, &snapshot.id, target.path()).is_err());
            assert_eq!(
                fs::read(outside.path().join("file")).unwrap(),
                b"private-original"
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn restoration_rejects_internal_metadata_symlink_aliases() {
        use std::os::unix::fs::symlink;
        let source = tempfile::tempdir().unwrap();
        fs::create_dir_all(source.path().join("alias")).unwrap();
        write_file(source.path().join("alias/HEAD"), b"replacement");
        let store = InMemoryObjectStore::new();
        let snapshot =
            materialize_snapshot(&store, source.path(), SnapshotOptions::new("repo_test")).unwrap();
        let target = tempfile::tempdir().unwrap();
        fs::create_dir(target.path().join(".sorrel")).unwrap();
        write_file(target.path().join(".sorrel/HEAD"), b"original");
        symlink(".sorrel", target.path().join("alias")).unwrap();
        assert!(restore_snapshot_to_directory(&store, &snapshot.id, target.path()).is_err());
        assert_eq!(
            fs::read(target.path().join(".sorrel/HEAD")).unwrap(),
            b"original"
        );
    }

    #[test]
    fn restoration_replaces_hard_links_without_modifying_other_names() {
        let source = tempfile::tempdir().unwrap();
        write_file(source.path().join("file"), b"replacement");
        let store = InMemoryObjectStore::new();
        let snapshot =
            materialize_snapshot(&store, source.path(), SnapshotOptions::new("repo_test")).unwrap();
        let target = tempfile::tempdir().unwrap();
        write_file(target.path().join("outside"), b"original");
        fs::hard_link(target.path().join("outside"), target.path().join("file")).unwrap();
        restore_snapshot_to_directory(&store, &snapshot.id, target.path()).unwrap();
        assert_eq!(
            fs::read(target.path().join("outside")).unwrap(),
            b"original"
        );
        assert_eq!(
            fs::read(target.path().join("file")).unwrap(),
            b"replacement"
        );
        assert_eq!(fs::read_dir(target.path()).unwrap().count(), 2);
    }

    #[cfg(unix)]
    #[test]
    fn tracked_file_removal_rejects_parent_links_and_unlinks_final_links() {
        use std::os::unix::fs::symlink;
        let target = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        write_file(outside.path().join("file"), b"private-original");
        symlink(outside.path(), target.path().join("alias")).unwrap();
        assert!(remove_snapshot_file_from_directory(target.path(), "alias/file").is_err());
        assert_eq!(
            fs::read(outside.path().join("file")).unwrap(),
            b"private-original"
        );
        symlink(outside.path().join("file"), target.path().join("link")).unwrap();
        remove_snapshot_file_from_directory(target.path(), "link").unwrap();
        assert!(!target.path().join("link").exists());
        assert_eq!(
            fs::read(outside.path().join("file")).unwrap(),
            b"private-original"
        );
        fs::create_dir_all(target.path().join("a/b")).unwrap();
        write_file(target.path().join("a/b/file"), b"tracked");
        remove_snapshot_file_from_directory(target.path(), "a/b/file").unwrap();
        assert!(!target.path().join("a").exists());
        assert!(target.path().exists());
        remove_snapshot_file_from_directory(target.path(), "a/b/missing").unwrap();
        assert!(remove_snapshot_file_from_directory(target.path(), "../file").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn restoration_preserves_read_only_files_and_leaves_no_temporary_files() {
        use std::os::unix::fs::PermissionsExt;
        let source = tempfile::tempdir().unwrap();
        write_file(source.path().join("file"), b"replacement");
        let store = InMemoryObjectStore::new();
        let snapshot =
            materialize_snapshot(&store, source.path(), SnapshotOptions::new("repo_test")).unwrap();
        let target = tempfile::tempdir().unwrap();
        write_file(target.path().join("file"), b"original");
        fs::set_permissions(
            target.path().join("file"),
            fs::Permissions::from_mode(0o400),
        )
        .unwrap();
        assert!(restore_snapshot_to_directory(&store, &snapshot.id, target.path()).is_err());
        assert_eq!(fs::read(target.path().join("file")).unwrap(), b"original");
        assert_eq!(fs::read_dir(target.path()).unwrap().count(), 1);
    }

    #[test]
    fn tracked_removal_preserves_reserved_metadata() {
        for name in [".sorrel", ".git", ".GIT", ".sorrel. "] {
            let target = tempfile::tempdir().unwrap();
            fs::create_dir(target.path().join(name)).unwrap();
            write_file(target.path().join(name).join("keep"), b"metadata");
            assert!(remove_snapshot_file_from_directory(
                target.path(),
                Path::new(name).join("keep")
            )
            .is_err());
            assert_eq!(
                fs::read(target.path().join(name).join("keep")).unwrap(),
                b"metadata"
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn restoration_preserves_read_write_permissions_and_adjusts_only_execute_bits() {
        use std::os::unix::fs::PermissionsExt;
        for (snapshot_mode, existing_mode, expected) in [
            (0o644, 0o600, 0o600),
            (0o755, 0o600, 0o700),
            (0o755, 0o640, 0o750),
            (0o644, 0o750, 0o640),
            (0o755, 0o620, 0o720),
            (0o644, 0o6750, 0o640),
        ] {
            let source = tempfile::tempdir().unwrap();
            write_file(source.path().join("file"), b"new");
            fs::set_permissions(
                source.path().join("file"),
                fs::Permissions::from_mode(snapshot_mode),
            )
            .unwrap();
            let store = InMemoryObjectStore::new();
            let snapshot =
                materialize_snapshot(&store, source.path(), SnapshotOptions::new("repo_test"))
                    .unwrap();
            let target = tempfile::tempdir().unwrap();
            write_file(target.path().join("file"), b"old");
            fs::set_permissions(
                target.path().join("file"),
                fs::Permissions::from_mode(existing_mode),
            )
            .unwrap();
            restore_snapshot_to_directory(&store, &snapshot.id, target.path()).unwrap();
            assert_eq!(fs::read(target.path().join("file")).unwrap(), b"new");
            assert_eq!(
                fs::metadata(target.path().join("file"))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o7777,
                expected
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn restoration_of_new_files_respects_umask() {
        use std::os::unix::fs::PermissionsExt;
        const CHILD: &str = "SORREL_RESTORE_UMASK_CHILD";
        if std::env::var_os(CHILD).is_none() {
            for mask in ["022", "077"] {
                let output = std::process::Command::new("sh")
                    .args(["-c", "umask \"$2\"; exec \"$1\" --exact snapshot::tests::restoration_of_new_files_respects_umask --test-threads=1", "sorrel-umask-test"])
                    .arg(std::env::current_exe().unwrap())
                    .arg(mask)
                    .env(CHILD, mask)
                    .output().unwrap();
                assert!(
                    output.status.success(),
                    "{mask}: {}{}",
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                );
            }
            return;
        }
        let source = tempfile::tempdir().unwrap();
        for (name, mode) in [("normal", 0o644), ("executable", 0o755)] {
            write_file(source.path().join(name), b"new");
            fs::set_permissions(source.path().join(name), fs::Permissions::from_mode(mode))
                .unwrap();
        }
        let store = InMemoryObjectStore::new();
        let snapshot =
            materialize_snapshot(&store, source.path(), SnapshotOptions::new("repo_test")).unwrap();
        let target = tempfile::tempdir().unwrap();
        restore_snapshot_to_directory(&store, &snapshot.id, target.path()).unwrap();
        let expected = if std::env::var(CHILD).unwrap() == "022" {
            [("normal", 0o644), ("executable", 0o755)]
        } else {
            [("normal", 0o600), ("executable", 0o700)]
        };
        for (name, mode) in expected {
            assert_eq!(
                fs::metadata(target.path().join(name))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                mode
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn restoration_temporary_file_is_private_while_writing() {
        use cap_std::fs::PermissionsExt;
        const CHILD: &str = "SORREL_RESTORE_PRIVATE_TEMP_CHILD";
        if std::env::var_os(CHILD).is_none() {
            let output = std::process::Command::new("sh")
                .args(["-c", "umask 022; exec \"$1\" --exact snapshot::tests::restoration_temporary_file_is_private_while_writing --test-threads=1", "sorrel-private-temp-test"])
                .arg(std::env::current_exe().unwrap())
                .env(CHILD, "1")
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            return;
        }
        let target = tempfile::tempdir().unwrap();
        let parent = Dir::open_ambient_dir(target.path(), cap_std::ambient_authority()).unwrap();
        for existing in [true, false] {
            let name = if existing {
                ".sorrel-restore-private.tmp"
            } else {
                ".sorrel-restore-new.tmp"
            };
            let mut permissions = existing.then(|| cap_std::fs::Permissions::from_mode(0o600));
            let mut file = open_restore_temporary_file(&parent, name, &mut permissions).unwrap();
            assert_eq!(
                permissions.unwrap().mode() & 0o777,
                if existing { 0o600 } else { 0o644 }
            );
            assert_eq!(file.metadata().unwrap().permissions().mode() & 0o777, 0o600);
            for bytes in [
                b"private first chunk".as_slice(),
                b"private second chunk".as_slice(),
            ] {
                file.write_all(bytes).unwrap();
                assert_eq!(
                    parent.symlink_metadata(name).unwrap().permissions().mode() & 0o777,
                    0o600
                );
            }
            assert_eq!(
                parent.read(name).unwrap(),
                b"private first chunkprivate second chunk"
            );
        }
    }

    #[test]
    fn restoration_temp_name_cannot_remove_destination() {
        const CHILD: &str = "SORREL_RESTORE_COLLISION_CHILD";
        if std::env::var_os(CHILD).is_none() {
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "snapshot::tests::restoration_temp_name_cannot_remove_destination",
                    "--test-threads=1",
                ])
                .env(CHILD, "1")
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            return;
        }
        let source = tempfile::tempdir().unwrap();
        let name = format!(".sorrel-restore-{}.0.tmp", std::process::id());
        write_file(source.path().join(&name), b"restored");
        let store = InMemoryObjectStore::new();
        let snapshot =
            materialize_snapshot(&store, source.path(), SnapshotOptions::new("repo_test")).unwrap();
        let target = tempfile::tempdir().unwrap();
        restore_snapshot_to_directory(&store, &snapshot.id, target.path()).unwrap();
        assert_eq!(fs::read(target.path().join(name)).unwrap(), b"restored");
        assert_eq!(fs::read_dir(target.path()).unwrap().count(), 1);
    }

    #[cfg(unix)]
    #[test]
    fn working_tree_overlay_is_confined_and_preserves_permissions() {
        use std::os::unix::fs::{symlink, PermissionsExt};
        let target = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        write_file(outside.path().join("file"), b"outside");
        symlink(outside.path(), target.path().join("alias")).unwrap();
        assert!(write_snapshot_file_to_directory(target.path(), "alias/file", b"marker").is_err());
        symlink(outside.path().join("file"), target.path().join("link")).unwrap();
        assert!(write_snapshot_file_to_directory(target.path(), "link", b"marker").is_err());
        fs::hard_link(outside.path().join("file"), target.path().join("hard")).unwrap();
        fs::set_permissions(
            target.path().join("hard"),
            fs::Permissions::from_mode(0o4700),
        )
        .unwrap();
        write_snapshot_file_to_directory(target.path(), "hard", b"marker").unwrap();
        assert_eq!(fs::read(outside.path().join("file")).unwrap(), b"outside");
        assert_eq!(fs::read(target.path().join("hard")).unwrap(), b"marker");
        assert_eq!(
            fs::metadata(target.path().join("hard"))
                .unwrap()
                .permissions()
                .mode()
                & 0o7777,
            0o700
        );
        assert!(
            write_snapshot_file_to_directory(target.path(), ".sorrel/HEAD", b"marker").is_err()
        );
    }

    #[cfg(unix)]
    #[test]
    fn held_restore_parent_ignores_a_later_directory_symlink_swap() {
        use std::os::unix::fs::symlink;
        let target = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let root = Dir::open_ambient_dir(target.path(), cap_std::ambient_authority()).unwrap();
        let parent = restore_parent(&root, Path::new("nested")).unwrap();
        fs::rename(target.path().join("nested"), target.path().join("original")).unwrap();
        symlink(outside.path(), target.path().join("nested")).unwrap();
        write_restored_file(
            &parent,
            OsStr::new("file"),
            &target.path().join("nested/file"),
            b"restored",
            Some(EntryMode::Normal),
        )
        .unwrap();
        assert_eq!(
            fs::read(target.path().join("original/file")).unwrap(),
            b"restored"
        );
        assert!(!outside.path().join("file").exists());
    }

    #[test]
    fn restore_rejects_metadata_paths_before_writing_any_files() {
        for path in [
            ".sorrel/HEAD",
            ".git/config",
            ".git/hooks/pre-commit",
            ".GiT/config",
            ".SORREL/HEAD",
            ".git./config",
            ".sorrel /HEAD",
            "",
        ] {
            let store = InMemoryObjectStore::new();
            let blob = write_blob(&store, b"replacement").unwrap();
            let entry = |path: &str| TreeEntry {
                name: path.to_owned(),
                path: PathBuf::from(path),
                entry_type: EntryType::File,
                object: ObjectRef::new(ObjectKind::Blob, blob.id),
                mode: EntryMode::Normal,
                size: None,
                content_hash: None,
            };
            let tree = write_tree(&store, vec![entry("0-safe.txt"), entry(path)]).unwrap();
            let snapshot =
                write_snapshot(&store, tree.id, SnapshotOptions::new("repo_test")).unwrap();
            let target = tempfile::tempdir().unwrap();
            fs::create_dir_all(target.path().join(".sorrel")).unwrap();
            fs::create_dir_all(target.path().join(".git")).unwrap();
            write_file(target.path().join(".sorrel/HEAD"), b"original-head");
            write_file(target.path().join(".git/config"), b"original-config");
            assert!(
                restore_snapshot_to_directory(&store, &snapshot.id, target.path()).is_err(),
                "{path}"
            );
            assert!(!target.path().join("0-safe.txt").exists(), "{path}");
            assert_eq!(
                fs::read(target.path().join(".sorrel/HEAD")).unwrap(),
                b"original-head"
            );
            assert_eq!(
                fs::read(target.path().join(".git/config")).unwrap(),
                b"original-config"
            );
        }
    }

    #[test]
    fn restore_keeps_nested_metadata_named_directories() {
        let source = tempfile::tempdir().unwrap();
        fs::create_dir_all(source.path().join("src/.sorrel")).unwrap();
        write_file(
            source.path().join("src/.sorrel/example"),
            b"ordinary-content",
        );
        let store = InMemoryObjectStore::new();
        let snapshot =
            materialize_snapshot(&store, source.path(), SnapshotOptions::new("repo_test")).unwrap();
        let target = tempfile::tempdir().unwrap();
        restore_snapshot_to_directory(&store, &snapshot.id, target.path()).unwrap();
        assert_eq!(
            fs::read(target.path().join("src/.sorrel/example")).unwrap(),
            b"ordinary-content"
        );
    }

    #[test]
    fn snapshot_ids_are_deterministic_for_identical_content() {
        let first_dir = tempfile::tempdir().unwrap();
        let second_dir = tempfile::tempdir().unwrap();
        fs::create_dir(first_dir.path().join("src")).unwrap();
        write_file(first_dir.path().join("src/lib.rs"), b"pub fn core() {}\n");
        write_file(first_dir.path().join("README.md"), b"# Sorrel\n");

        write_file(second_dir.path().join("README.md"), b"# Sorrel\n");
        fs::create_dir(second_dir.path().join("src")).unwrap();
        write_file(second_dir.path().join("src/lib.rs"), b"pub fn core() {}\n");

        let store = InMemoryObjectStore::new();
        let first = materialize_snapshot(
            &store,
            first_dir.path(),
            SnapshotOptions::new("repo_sorrel"),
        )
        .unwrap();
        let second = materialize_snapshot(
            &store,
            second_dir.path(),
            SnapshotOptions::new("repo_sorrel"),
        )
        .unwrap();

        assert_eq!(first.id, second.id);
        assert_eq!(first.root_tree.id, second.root_tree.id);
    }

    fn write_file(path: impl AsRef<Path>, content: &[u8]) {
        fs::write(path, content).unwrap();
    }
}
