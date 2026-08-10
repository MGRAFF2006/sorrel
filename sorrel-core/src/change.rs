use crate::{
    read_snapshot, read_tree, ObjectId, ObjectIdParseError, ObjectKind, ObjectRef, ObjectStore,
    ObjectStoreError, Principal, Snapshot, SnapshotError, TreeEntry,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Component, Path, PathBuf},
};

const PROTOCOL_VERSION: &str = "sorrel.protocol.v0";

/// Result type used by change object operations.
pub type ChangeResult<T> = Result<T, ChangeError>;

/// Errors returned while diffing, serializing, reading, or applying changes.
#[derive(Debug, thiserror::Error)]
pub enum ChangeError {
    /// The underlying object store failed.
    #[error(transparent)]
    ObjectStore(#[from] ObjectStoreError),

    /// Snapshot or tree access failed while building or applying a change.
    #[error(transparent)]
    Snapshot(#[from] SnapshotError),

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

    /// A protocol object reference had an invalid object ID.
    #[error("invalid object id {value:?}: {source}")]
    InvalidObjectId {
        /// Textual object ID value.
        value: String,
        /// Parse error.
        #[source]
        source: ObjectIdParseError,
    },

    /// A path could not be represented safely in a change.
    #[error("invalid change path {}", path.display())]
    InvalidPath {
        /// Invalid path.
        path: PathBuf,
    },

    /// A change reference pointed at the wrong object kind.
    #[error("invalid {field} object kind {actual:?}; expected {expected:?}")]
    InvalidObjectRefKind {
        /// Field containing the invalid reference.
        field: &'static str,
        /// Expected object kind.
        expected: &'static str,
        /// Actual object kind.
        actual: &'static str,
    },

    /// The caller tried to apply a change to a snapshot other than its base.
    #[error("change base snapshot mismatch: expected {expected}, found {actual}")]
    BaseSnapshotMismatch {
        /// Snapshot ID named by the change as its base.
        expected: ObjectId,
        /// Snapshot ID supplied by the caller.
        actual: ObjectId,
    },

    /// Placeholder for future conflict reporting.
    #[error("change conflict at {}", path.display())]
    Conflict {
        /// Path where the conflict was detected.
        path: PathBuf,
    },
}

/// Kind of path-level difference between two snapshots.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PathChangeKind {
    /// Path exists only in the resulting snapshot.
    Added,
    /// Path exists in both snapshots but points at different content or metadata.
    Modified,
    /// Path exists only in the base snapshot.
    Deleted,
}

impl PathChangeKind {
    fn as_protocol_kind(self) -> &'static str {
        match self {
            Self::Added => "added",
            Self::Modified => "modified",
            Self::Deleted => "deleted",
        }
    }

    fn from_protocol_kind(value: &str) -> ChangeResult<Self> {
        match value {
            "added" => Ok(Self::Added),
            "modified" => Ok(Self::Modified),
            "deleted" => Ok(Self::Deleted),
            other => Err(ChangeError::InvalidObjectKind {
                expected: "added, modified, or deleted",
                actual: other.to_owned(),
            }),
        }
    }
}

/// A single path-level difference between two snapshots.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PathChange {
    /// Slash-separated path relative to the snapshot root.
    pub path: PathBuf,
    /// Kind of difference for this path.
    pub kind: PathChangeKind,
}

/// Basic deterministic diff between two snapshots.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SnapshotDiff {
    /// Path-level changes sorted by relative path.
    pub changes: Vec<PathChange>,
}

impl SnapshotDiff {
    /// Returns sorted paths touched by this diff.
    #[must_use]
    pub fn touched_paths(&self) -> Vec<PathBuf> {
        self.changes
            .iter()
            .map(|change| change.path.clone())
            .collect()
    }

    /// Returns true when the two snapshots have no path-level differences.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.changes.is_empty()
    }
}

/// Options used when creating a change object.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChangeOptions {
    /// Change author.
    pub author: Principal,
    /// Parent changes that this change builds on.
    pub parent_changes: Vec<ObjectId>,
    /// Short change message.
    pub message: String,
    /// Longer optional change description.
    pub description: Option<String>,
}

impl ChangeOptions {
    /// Builds change options with no parent changes or long description.
    #[must_use]
    pub fn new(author: Principal, message: impl Into<String>) -> Self {
        Self {
            author,
            parent_changes: Vec::new(),
            message: message.into(),
            description: None,
        }
    }
}

/// Snapshot-to-snapshot change object.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Change {
    /// Content-addressed ID of the stored change object.
    pub id: ObjectId,
    /// Change author.
    pub author: Principal,
    /// Parent change references.
    pub parent_changes: Vec<ObjectRef>,
    /// Snapshot this change applies to.
    pub base_snapshot: ObjectRef,
    /// Snapshot produced by applying this change.
    pub resulting_snapshot: ObjectRef,
    /// Sorted paths touched by this change.
    pub touched_paths: Vec<PathBuf>,
    /// Basic snapshot-to-snapshot diff.
    pub diff: SnapshotDiff,
    /// Short change message.
    pub message: String,
    /// Longer optional change description.
    pub description: Option<String>,
}

/// Creates and stores a deterministic change object from two snapshots.
pub fn create_change(
    store: &impl ObjectStore,
    base_snapshot_id: ObjectId,
    resulting_snapshot_id: ObjectId,
    options: ChangeOptions,
) -> ChangeResult<Change> {
    let diff = snapshot_diff(store, &base_snapshot_id, &resulting_snapshot_id)?;
    let touched_paths = diff.touched_paths();
    let parent_changes = options
        .parent_changes
        .into_iter()
        .map(|id| ObjectRef::new(ObjectKind::Change, id))
        .collect::<Vec<_>>();
    let base_snapshot = ObjectRef::new(ObjectKind::Snapshot, base_snapshot_id);
    let resulting_snapshot = ObjectRef::new(ObjectKind::Snapshot, resulting_snapshot_id);

    let stored = StoredChange::from_parts(
        &options.author,
        &parent_changes,
        base_snapshot,
        resulting_snapshot,
        &touched_paths,
        &diff,
        &options.message,
        options.description.as_deref(),
    );
    let bytes = serde_json::to_vec(&stored)?;
    let id = store.write(&bytes)?;

    Ok(Change {
        id,
        author: options.author,
        parent_changes,
        base_snapshot,
        resulting_snapshot,
        touched_paths,
        diff,
        message: options.message,
        description: options.description,
    })
}

/// Reads a stored change object.
pub fn read_change(store: &impl ObjectStore, id: &ObjectId) -> ChangeResult<Change> {
    let bytes = store.read(id)?;
    let stored: StoredChange = serde_json::from_slice(&bytes)?;
    stored.ensure_kind("Change")?;
    stored.into_change(*id)
}

/// Computes a basic path-level diff between two snapshots.
pub fn snapshot_diff(
    store: &impl ObjectStore,
    base_snapshot_id: &ObjectId,
    resulting_snapshot_id: &ObjectId,
) -> ChangeResult<SnapshotDiff> {
    let base_entries = snapshot_entry_fingerprints(store, base_snapshot_id)?;
    let resulting_entries = snapshot_entry_fingerprints(store, resulting_snapshot_id)?;
    let paths = base_entries
        .keys()
        .chain(resulting_entries.keys())
        .cloned()
        .collect::<BTreeSet<_>>();

    let mut changes = Vec::new();
    for path in paths {
        let kind = match (base_entries.get(&path), resulting_entries.get(&path)) {
            (None, Some(_)) => Some(PathChangeKind::Added),
            (Some(_), None) => Some(PathChangeKind::Deleted),
            (Some(base), Some(resulting)) if base != resulting => Some(PathChangeKind::Modified),
            (Some(_), Some(_)) | (None, None) => None,
        };

        if let Some(kind) = kind {
            changes.push(PathChange { path, kind });
        }
    }

    Ok(SnapshotDiff { changes })
}

/// Applies a stored change to `current_snapshot_id`.
///
/// This first operation does not merge or patch content. It validates that the
/// caller is positioned at the change's base snapshot and then returns the
/// already-stored resulting snapshot.
pub fn apply_change(
    store: &impl ObjectStore,
    current_snapshot_id: &ObjectId,
    change_id: &ObjectId,
) -> ChangeResult<Snapshot> {
    let change = read_change(store, change_id)?;
    apply_loaded_change(store, current_snapshot_id, &change)
}

/// Applies an already-read change to `current_snapshot_id`.
pub fn apply_loaded_change(
    store: &impl ObjectStore,
    current_snapshot_id: &ObjectId,
    change: &Change,
) -> ChangeResult<Snapshot> {
    if *current_snapshot_id != change.base_snapshot.id {
        return Err(ChangeError::BaseSnapshotMismatch {
            expected: change.base_snapshot.id,
            actual: *current_snapshot_id,
        });
    }

    Ok(read_snapshot(store, &change.resulting_snapshot.id)?)
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct EntryFingerprint {
    entry_type: ObjectKind,
    mode: String,
    object: Option<ObjectId>,
    size: Option<u64>,
    content_hash: Option<ObjectId>,
}

fn snapshot_entry_fingerprints(
    store: &impl ObjectStore,
    snapshot_id: &ObjectId,
) -> ChangeResult<BTreeMap<PathBuf, EntryFingerprint>> {
    let snapshot = read_snapshot(store, snapshot_id)?;
    let mut entries = BTreeMap::new();
    collect_entry_fingerprints(store, &snapshot.root_tree.id, &mut entries)?;
    Ok(entries)
}

fn collect_entry_fingerprints(
    store: &impl ObjectStore,
    tree_id: &ObjectId,
    entries: &mut BTreeMap<PathBuf, EntryFingerprint>,
) -> ChangeResult<()> {
    let tree = read_tree(store, tree_id)?;
    for entry in tree.entries {
        let fingerprint = EntryFingerprint::from_entry(&entry);
        entries.insert(entry.path.clone(), fingerprint);

        if entry.object.kind == ObjectKind::Tree {
            collect_entry_fingerprints(store, &entry.object.id, entries)?;
        }
    }

    Ok(())
}

impl EntryFingerprint {
    fn from_entry(entry: &TreeEntry) -> Self {
        let object = if entry.object.kind == ObjectKind::Blob {
            Some(entry.object.id)
        } else {
            None
        };

        Self {
            entry_type: entry.object.kind,
            mode: match entry.mode {
                crate::EntryMode::Normal => "normal".to_owned(),
                crate::EntryMode::Executable => "executable".to_owned(),
                crate::EntryMode::Directory => "directory".to_owned(),
            },
            object,
            size: entry.size,
            content_hash: entry.content_hash,
        }
    }
}

fn kind_to_protocol(kind: ObjectKind) -> &'static str {
    match kind {
        ObjectKind::Blob => "Blob",
        ObjectKind::Tree => "Tree",
        ObjectKind::Snapshot => "Snapshot",
        ObjectKind::Change => "Change",
        ObjectKind::Lane => "Lane",
        ObjectKind::Stack => "Stack",
        ObjectKind::Conflict => "Conflict",
        ObjectKind::MergeResult => "MergeResult",
    }
}

fn kind_from_protocol(kind: &str) -> ChangeResult<ObjectKind> {
    match kind {
        "Blob" => Ok(ObjectKind::Blob),
        "Tree" => Ok(ObjectKind::Tree),
        "Snapshot" => Ok(ObjectKind::Snapshot),
        "Change" => Ok(ObjectKind::Change),
        "Lane" => Ok(ObjectKind::Lane),
        "Stack" => Ok(ObjectKind::Stack),
        "Conflict" => Ok(ObjectKind::Conflict),
        "MergeResult" => Ok(ObjectKind::MergeResult),
        other => Err(ChangeError::InvalidObjectKind {
            expected: "Blob, Tree, Snapshot, Change, Lane, Stack, Conflict, or MergeResult",
            actual: other.to_owned(),
        }),
    }
}

fn ensure_ref_kind(
    reference: &ObjectRef,
    expected: ObjectKind,
    field: &'static str,
) -> ChangeResult<()> {
    if reference.kind == expected {
        Ok(())
    } else {
        Err(ChangeError::InvalidObjectRefKind {
            field,
            expected: kind_to_protocol(expected),
            actual: kind_to_protocol(reference.kind),
        })
    }
}

fn validate_relative_path(path: &Path) -> ChangeResult<()> {
    if path.as_os_str().is_empty() {
        return Ok(());
    }

    let valid = path.components().all(|component| {
        matches!(component, Component::Normal(name) if !name.is_empty() && name != "." && name != "..")
    });

    if valid {
        Ok(())
    } else {
        Err(ChangeError::InvalidPath {
            path: path.to_path_buf(),
        })
    }
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

fn parse_protocol_path(path: &str) -> ChangeResult<PathBuf> {
    let path = PathBuf::from(path);
    validate_relative_path(&path)?;
    Ok(path)
}

fn parse_object_id(value: &str) -> ChangeResult<ObjectId> {
    value
        .parse()
        .map_err(|source| ChangeError::InvalidObjectId {
            value: value.to_owned(),
            source,
        })
}

#[derive(Serialize, Deserialize)]
struct StoredObjectRef {
    kind: String,
    id: String,
}

impl From<ObjectRef> for StoredObjectRef {
    fn from(value: ObjectRef) -> Self {
        Self {
            kind: kind_to_protocol(value.kind).to_owned(),
            id: value.id.to_string(),
        }
    }
}

impl StoredObjectRef {
    fn into_ref(self) -> ChangeResult<ObjectRef> {
        Ok(ObjectRef::new(
            kind_from_protocol(&self.kind)?,
            parse_object_id(&self.id)?,
        ))
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredChange {
    schema_version: String,
    kind: String,
    author: StoredPrincipal,
    parent_changes: Vec<StoredObjectRef>,
    base_snapshot: StoredObjectRef,
    resulting_snapshot: StoredObjectRef,
    touched_paths: Vec<String>,
    diff: Vec<StoredPathChange>,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
}

impl StoredChange {
    #[allow(clippy::too_many_arguments)]
    fn from_parts(
        author: &Principal,
        parent_changes: &[ObjectRef],
        base_snapshot: ObjectRef,
        resulting_snapshot: ObjectRef,
        touched_paths: &[PathBuf],
        diff: &SnapshotDiff,
        message: &str,
        description: Option<&str>,
    ) -> Self {
        Self {
            schema_version: PROTOCOL_VERSION.to_owned(),
            kind: "Change".to_owned(),
            author: StoredPrincipal::from_principal(author),
            parent_changes: parent_changes
                .iter()
                .copied()
                .map(StoredObjectRef::from)
                .collect(),
            base_snapshot: base_snapshot.into(),
            resulting_snapshot: resulting_snapshot.into(),
            touched_paths: touched_paths
                .iter()
                .map(|path| path_to_protocol_string(path))
                .collect(),
            diff: diff
                .changes
                .iter()
                .map(StoredPathChange::from_change)
                .collect(),
            message: message.to_owned(),
            description: description.map(str::to_owned),
        }
    }

    fn ensure_kind(&self, expected: &'static str) -> ChangeResult<()> {
        if self.schema_version != PROTOCOL_VERSION {
            return Err(ChangeError::UnsupportedSchemaVersion {
                expected: PROTOCOL_VERSION,
                actual: self.schema_version.clone(),
            });
        }

        if self.kind != expected {
            return Err(ChangeError::InvalidObjectKind {
                expected,
                actual: self.kind.clone(),
            });
        }

        Ok(())
    }

    fn into_change(self, id: ObjectId) -> ChangeResult<Change> {
        let parent_changes = self
            .parent_changes
            .into_iter()
            .map(StoredObjectRef::into_ref)
            .collect::<ChangeResult<Vec<_>>>()?;
        for parent in &parent_changes {
            ensure_ref_kind(parent, ObjectKind::Change, "parentChanges")?;
        }

        let base_snapshot = self.base_snapshot.into_ref()?;
        ensure_ref_kind(&base_snapshot, ObjectKind::Snapshot, "baseSnapshot")?;
        let resulting_snapshot = self.resulting_snapshot.into_ref()?;
        ensure_ref_kind(
            &resulting_snapshot,
            ObjectKind::Snapshot,
            "resultingSnapshot",
        )?;

        let touched_paths = self
            .touched_paths
            .into_iter()
            .map(|path| parse_protocol_path(&path))
            .collect::<ChangeResult<Vec<_>>>()?;
        let diff = SnapshotDiff {
            changes: self
                .diff
                .into_iter()
                .map(StoredPathChange::into_change)
                .collect::<ChangeResult<Vec<_>>>()?,
        };

        Ok(Change {
            id,
            author: self.author.into_principal(),
            parent_changes,
            base_snapshot,
            resulting_snapshot,
            touched_paths,
            diff,
            message: self.message,
            description: self.description,
        })
    }
}

#[derive(Serialize, Deserialize)]
struct StoredPathChange {
    path: String,
    kind: String,
}

impl StoredPathChange {
    fn from_change(change: &PathChange) -> Self {
        Self {
            path: path_to_protocol_string(&change.path),
            kind: change.kind.as_protocol_kind().to_owned(),
        }
    }

    fn into_change(self) -> ChangeResult<PathChange> {
        Ok(PathChange {
            path: parse_protocol_path(&self.path)?,
            kind: PathChangeKind::from_protocol_kind(&self.kind)?,
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
    use crate::{materialize_snapshot, InMemoryObjectStore, SnapshotOptions};
    use std::fs;

    #[test]
    fn creates_change_from_two_snapshots() {
        let store = InMemoryObjectStore::new();
        let base_dir = tempfile::tempdir().unwrap();
        let result_dir = tempfile::tempdir().unwrap();
        write_file(base_dir.path().join("README.md"), b"# Sorrel\n");
        write_file(result_dir.path().join("README.md"), b"# Sorrel Core\n");
        let base = snapshot_from_dir(&store, base_dir.path());
        let result = snapshot_from_dir(&store, result_dir.path());

        let change = create_change(
            &store,
            base.id,
            result.id,
            ChangeOptions::new(Principal::system(), "update readme"),
        )
        .unwrap();
        let read = read_change(&store, &change.id).unwrap();

        assert_eq!(read, change);
        assert_eq!(
            change.base_snapshot,
            ObjectRef::new(ObjectKind::Snapshot, base.id)
        );
        assert_eq!(
            change.resulting_snapshot,
            ObjectRef::new(ObjectKind::Snapshot, result.id)
        );
        assert_eq!(change.message, "update readme");
        assert_eq!(change.diff.changes.len(), 1);
    }

    #[test]
    fn detects_touched_paths() {
        let store = InMemoryObjectStore::new();
        let base_dir = tempfile::tempdir().unwrap();
        let result_dir = tempfile::tempdir().unwrap();
        write_file(base_dir.path().join("modified.txt"), b"old\n");
        write_file(base_dir.path().join("removed.txt"), b"gone\n");
        write_file(result_dir.path().join("added.txt"), b"new\n");
        write_file(result_dir.path().join("modified.txt"), b"new\n");
        let base = snapshot_from_dir(&store, base_dir.path());
        let result = snapshot_from_dir(&store, result_dir.path());

        let diff = snapshot_diff(&store, &base.id, &result.id).unwrap();

        assert_eq!(
            diff.touched_paths(),
            paths(&["added.txt", "modified.txt", "removed.txt"])
        );
        assert_eq!(
            diff.changes,
            vec![
                PathChange {
                    path: PathBuf::from("added.txt"),
                    kind: PathChangeKind::Added
                },
                PathChange {
                    path: PathBuf::from("modified.txt"),
                    kind: PathChangeKind::Modified
                },
                PathChange {
                    path: PathBuf::from("removed.txt"),
                    kind: PathChangeKind::Deleted
                }
            ]
        );
    }

    #[test]
    fn change_ids_are_deterministic() {
        let store = InMemoryObjectStore::new();
        let base_dir = tempfile::tempdir().unwrap();
        let result_dir = tempfile::tempdir().unwrap();
        write_file(base_dir.path().join("README.md"), b"# Sorrel\n");
        write_file(result_dir.path().join("README.md"), b"# Sorrel Core\n");
        let base = snapshot_from_dir(&store, base_dir.path());
        let result = snapshot_from_dir(&store, result_dir.path());

        let first = create_change(
            &store,
            base.id,
            result.id,
            ChangeOptions::new(Principal::system(), "update readme"),
        )
        .unwrap();
        let second = create_change(
            &store,
            base.id,
            result.id,
            ChangeOptions::new(Principal::system(), "update readme"),
        )
        .unwrap();

        assert_eq!(first.id, second.id);
    }

    #[test]
    fn applies_change() {
        let store = InMemoryObjectStore::new();
        let base_dir = tempfile::tempdir().unwrap();
        let result_dir = tempfile::tempdir().unwrap();
        write_file(base_dir.path().join("README.md"), b"# Sorrel\n");
        write_file(result_dir.path().join("README.md"), b"# Sorrel Core\n");
        let base = snapshot_from_dir(&store, base_dir.path());
        let result = snapshot_from_dir(&store, result_dir.path());
        let change = create_change(
            &store,
            base.id,
            result.id,
            ChangeOptions::new(Principal::system(), "update readme"),
        )
        .unwrap();

        let applied = apply_change(&store, &base.id, &change.id).unwrap();

        assert_eq!(applied, result);
    }

    #[test]
    fn rejects_change_when_base_snapshot_does_not_match() {
        let store = InMemoryObjectStore::new();
        let base_dir = tempfile::tempdir().unwrap();
        let result_dir = tempfile::tempdir().unwrap();
        let other_dir = tempfile::tempdir().unwrap();
        write_file(base_dir.path().join("README.md"), b"# Sorrel\n");
        write_file(result_dir.path().join("README.md"), b"# Sorrel Core\n");
        write_file(other_dir.path().join("README.md"), b"# Other\n");
        let base = snapshot_from_dir(&store, base_dir.path());
        let result = snapshot_from_dir(&store, result_dir.path());
        let other = snapshot_from_dir(&store, other_dir.path());
        let change = create_change(
            &store,
            base.id,
            result.id,
            ChangeOptions::new(Principal::system(), "update readme"),
        )
        .unwrap();

        let error = apply_change(&store, &other.id, &change.id).unwrap_err();

        assert!(matches!(
            error,
            ChangeError::BaseSnapshotMismatch { expected, actual }
                if expected == base.id && actual == other.id
        ));
    }

    fn snapshot_from_dir(store: &InMemoryObjectStore, path: &Path) -> Snapshot {
        materialize_snapshot(store, path, SnapshotOptions::new("repo_sorrel")).unwrap()
    }

    fn paths(paths: &[&str]) -> Vec<PathBuf> {
        paths.iter().map(PathBuf::from).collect()
    }

    fn write_file(path: impl AsRef<Path>, content: &[u8]) {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, content).unwrap();
    }
}
