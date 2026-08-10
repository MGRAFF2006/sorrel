//! First-class conflict and merge-result objects.
//!
//! [`Conflict`] stores path-level merge conflict hunks (from [`crate::merge3`])
//! as content-addressed JSON. [`MergeResult`] records whether a merge completed
//! cleanly ([`MergeResultStatus::Clean`] with a [`merged_snapshot`](MergeResult::merged_snapshot))
//! or produced unresolved conflicts ([`MergeResultStatus::Conflicted`] listing
//! conflict object ids).
//!
//! The stored JSON forms follow the `Conflict` / `MergeResult` definitions in
//! the `sorrel-protocol` object schema: conflicts carry `repoId`, content refs
//! (`{ "object": "<64-hex>" }`) for `base` / `ours` / `theirs`, and an optional
//! `resolution` blob id; merge results carry `repoId`, the three input snapshot
//! ids, and bare 64-hex conflict / merged-snapshot ids.

use crate::{merge3::ConflictHunk, ObjectId, ObjectIdParseError, ObjectStore, ObjectStoreError};
use serde::{Deserialize, Serialize};
use std::path::{Component, Path, PathBuf};

const PROTOCOL_VERSION: &str = "sorrel.protocol.v0";

/// Result type used by conflict object operations.
pub type ConflictResult<T> = Result<T, ConflictError>;

/// Errors returned while serializing, reading, or validating conflict objects.
#[derive(Debug, thiserror::Error)]
pub enum ConflictError {
    /// The underlying object store failed.
    #[error(transparent)]
    ObjectStore(#[from] ObjectStoreError),

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

    /// A path could not be represented safely in a conflict.
    #[error("invalid conflict path {}", path.display())]
    InvalidPath {
        /// Invalid path.
        path: PathBuf,
    },

    /// A conflict was missing both `ours` and `theirs` content refs.
    #[error("conflict for {} must carry at least one of ours/theirs", path.display())]
    MissingConflictSides {
        /// Conflicted path.
        path: PathBuf,
    },

    /// A clean merge result listed unresolved conflicts.
    #[error("clean merge result must not list conflicts (found {count})")]
    CleanMergeResultWithConflicts {
        /// Number of conflicts listed.
        count: usize,
    },

    /// A merge-result status string was not recognized.
    #[error("invalid merge result status {actual:?}; expected \"clean\" or \"conflicted\"")]
    InvalidMergeStatus {
        /// Actual status value.
        actual: String,
    },

    /// A conflict type string was not recognized.
    #[error(
        "invalid conflict type {actual:?}; expected \"content\", \"binary\", \"add_add\", or \"modify_delete\""
    )]
    InvalidConflictType {
        /// Actual conflict type value.
        actual: String,
    },
}

/// Merge completion status carried by a [`MergeResult`] object.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MergeResultStatus {
    /// Merge produced a snapshot with no unresolved conflicts.
    Clean,
    /// Merge left one or more unresolved [`Conflict`] objects.
    Conflicted,
}

impl MergeResultStatus {
    fn as_protocol(self) -> &'static str {
        match self {
            Self::Clean => "clean",
            Self::Conflicted => "conflicted",
        }
    }

    fn from_protocol(value: &str) -> ConflictResult<Self> {
        match value {
            "clean" => Ok(Self::Clean),
            "conflicted" => Ok(Self::Conflicted),
            other => Err(ConflictError::InvalidMergeStatus {
                actual: other.to_owned(),
            }),
        }
    }
}

/// Why a path-level [`Conflict`] could not be auto-merged.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConflictType {
    /// Overlapping textual edits on both sides.
    Content,
    /// At least one side was not valid UTF-8.
    Binary,
    /// Both sides added the path with different content.
    AddAdd,
    /// One side modified the path while the other deleted it.
    ModifyDelete,
}

impl ConflictType {
    fn as_protocol(self) -> &'static str {
        match self {
            Self::Content => "content",
            Self::Binary => "binary",
            Self::AddAdd => "add_add",
            Self::ModifyDelete => "modify_delete",
        }
    }

    fn from_protocol(value: &str) -> ConflictResult<Self> {
        match value {
            "content" => Ok(Self::Content),
            "binary" => Ok(Self::Binary),
            "add_add" => Ok(Self::AddAdd),
            "modify_delete" => Ok(Self::ModifyDelete),
            other => Err(ConflictError::InvalidConflictType {
                actual: other.to_owned(),
            }),
        }
    }
}

/// Blob content ids for the three sides of a path-level conflict.
///
/// The deleted side of a `modify_delete` conflict is `None`; `base` is `None`
/// for `add_add` conflicts where no common ancestor blob exists. At least one
/// of `ours` / `theirs` must be present to store the conflict.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ConflictSides {
    /// Base (common ancestor) blob content id, when the path existed in base.
    pub base: Option<ObjectId>,
    /// Our side's blob content id, when the path survives on our side.
    pub ours: Option<ObjectId>,
    /// Their side's blob content id, when the path survives on their side.
    pub theirs: Option<ObjectId>,
}

/// Path-level conflict object with structured hunks.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Conflict {
    /// Content-addressed ID of the stored conflict object.
    pub id: ObjectId,
    /// Repository the conflict belongs to (protocol `repoId`).
    pub repo_id: String,
    /// Slash-separated path relative to the snapshot root.
    pub path: PathBuf,
    /// Conflict classification (protocol `conflictType`).
    pub conflict_type: ConflictType,
    /// Blob content ids for base / ours / theirs.
    pub sides: ConflictSides,
    /// Structured conflict regions (base line indices are 0-based).
    pub hunks: Vec<ConflictHunk>,
    /// Resolved blob content id; `None` while the conflict is unresolved.
    pub resolution: Option<ObjectId>,
}

/// Outcome of a merge, stored as a first-class object.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MergeResult {
    /// Content-addressed ID of the stored merge-result object.
    pub id: ObjectId,
    /// Repository the merge belongs to (protocol `repoId`).
    pub repo_id: String,
    /// Common ancestor snapshot the merge was computed against.
    pub base_snapshot: ObjectId,
    /// Our side's input snapshot.
    pub ours_snapshot: ObjectId,
    /// Their side's input snapshot.
    pub theirs_snapshot: ObjectId,
    /// Whether the merge completed cleanly or left conflicts.
    pub status: MergeResultStatus,
    /// Merged snapshot when [`status`](Self::status) is
    /// [`MergeResultStatus::Clean`].
    pub merged_snapshot: Option<ObjectId>,
    /// Conflict object content ids when status is
    /// [`MergeResultStatus::Conflicted`].
    pub conflicts: Vec<ObjectId>,
}

/// Builds an in-memory [`Conflict`] from a path, side refs, and
/// [`ConflictHunk`] values.
///
/// Defaults [`ConflictType`] to [`ConflictType::Content`]. The returned `id`
/// is a zero placeholder; [`write_conflict`] assigns the content-addressed id
/// when the object is stored.
#[must_use]
pub fn conflict_from_hunks(
    repo_id: impl Into<String>,
    path: impl Into<PathBuf>,
    sides: ConflictSides,
    hunks: impl IntoIterator<Item = ConflictHunk>,
) -> Conflict {
    conflict_with_type(repo_id, path, ConflictType::Content, sides, hunks)
}

/// Builds an in-memory [`Conflict`] with an explicit [`ConflictType`].
///
/// The returned `id` is a zero placeholder; [`write_conflict`] assigns the
/// content-addressed id when the object is stored.
#[must_use]
pub fn conflict_with_type(
    repo_id: impl Into<String>,
    path: impl Into<PathBuf>,
    conflict_type: ConflictType,
    sides: ConflictSides,
    hunks: impl IntoIterator<Item = ConflictHunk>,
) -> Conflict {
    Conflict {
        id: ObjectId::from_bytes([0; 32]),
        repo_id: repo_id.into(),
        path: path.into(),
        conflict_type,
        sides,
        hunks: hunks.into_iter().collect(),
        resolution: None,
    }
}

/// Serializes and stores a [`Conflict`], returning it with its content id.
///
/// At least one of `sides.ours` / `sides.theirs` must be present, matching the
/// protocol schema's `anyOf` requirement.
pub fn write_conflict(store: &impl ObjectStore, conflict: &Conflict) -> ConflictResult<Conflict> {
    validate_relative_path(&conflict.path)?;
    if conflict.sides.ours.is_none() && conflict.sides.theirs.is_none() {
        return Err(ConflictError::MissingConflictSides {
            path: conflict.path.clone(),
        });
    }

    let stored = StoredConflict::from_conflict(conflict);
    let bytes = serde_json::to_vec(&stored)?;
    let id = store.write(&bytes)?;
    Ok(Conflict {
        id,
        ..conflict.clone()
    })
}

/// Reads a stored conflict object.
pub fn read_conflict(store: &impl ObjectStore, id: &ObjectId) -> ConflictResult<Conflict> {
    let bytes = store.read(id)?;
    let stored: StoredConflict = serde_json::from_slice(&bytes)?;
    stored.ensure_kind("Conflict")?;
    stored.into_conflict(*id)
}

/// Serializes and stores a [`MergeResult`], returning it with its content id.
///
/// Clean merge results must not list conflicts, matching the protocol schema's
/// conditional constraint.
pub fn write_merge_result(
    store: &impl ObjectStore,
    result: &MergeResult,
) -> ConflictResult<MergeResult> {
    if result.status == MergeResultStatus::Clean && !result.conflicts.is_empty() {
        return Err(ConflictError::CleanMergeResultWithConflicts {
            count: result.conflicts.len(),
        });
    }

    let stored = StoredMergeResult::from_result(result);
    let bytes = serde_json::to_vec(&stored)?;
    let id = store.write(&bytes)?;
    Ok(MergeResult {
        id,
        ..result.clone()
    })
}

/// Reads a stored merge-result object.
pub fn read_merge_result(store: &impl ObjectStore, id: &ObjectId) -> ConflictResult<MergeResult> {
    let bytes = store.read(id)?;
    let stored: StoredMergeResult = serde_json::from_slice(&bytes)?;
    stored.ensure_kind("MergeResult")?;
    stored.into_merge_result(*id)
}

fn validate_relative_path(path: &Path) -> ConflictResult<()> {
    if path.as_os_str().is_empty() {
        return Ok(());
    }

    let valid = path.components().all(|component| {
        matches!(component, Component::Normal(name) if !name.is_empty() && name != "." && name != "..")
    });

    if valid {
        Ok(())
    } else {
        Err(ConflictError::InvalidPath {
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

fn parse_protocol_path(path: &str) -> ConflictResult<PathBuf> {
    let path = PathBuf::from(path);
    validate_relative_path(&path)?;
    Ok(path)
}

fn parse_object_id(value: &str) -> ConflictResult<ObjectId> {
    value
        .parse()
        .map_err(|source| ConflictError::InvalidObjectId {
            value: value.to_owned(),
            source,
        })
}

fn ensure_protocol_header(
    schema_version: &str,
    kind: &str,
    expected: &'static str,
) -> ConflictResult<()> {
    if schema_version != PROTOCOL_VERSION {
        return Err(ConflictError::UnsupportedSchemaVersion {
            expected: PROTOCOL_VERSION,
            actual: schema_version.to_owned(),
        });
    }

    if kind != expected {
        return Err(ConflictError::InvalidObjectKind {
            expected,
            actual: kind.to_owned(),
        });
    }

    Ok(())
}

/// Protocol `ContentObjectRef`: `{ "object": "<64-hex>" }`.
#[derive(Serialize, Deserialize)]
struct StoredContentRef {
    object: String,
}

impl StoredContentRef {
    fn from_id(id: ObjectId) -> Self {
        Self {
            object: id.to_hex(),
        }
    }

    fn into_id(self) -> ConflictResult<ObjectId> {
        parse_object_id(&self.object)
    }
}

fn option_ref_from_id(id: Option<ObjectId>) -> Option<StoredContentRef> {
    id.map(StoredContentRef::from_id)
}

fn option_ref_into_id(reference: Option<StoredContentRef>) -> ConflictResult<Option<ObjectId>> {
    reference.map(StoredContentRef::into_id).transpose()
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredConflictHunk {
    base_start: usize,
    base_lines: Vec<String>,
    ours_lines: Vec<String>,
    theirs_lines: Vec<String>,
}

impl StoredConflictHunk {
    fn from_hunk(hunk: &ConflictHunk) -> Self {
        Self {
            base_start: hunk.base_start,
            base_lines: hunk.base_lines.clone(),
            ours_lines: hunk.ours_lines.clone(),
            theirs_lines: hunk.theirs_lines.clone(),
        }
    }

    fn into_hunk(self) -> ConflictHunk {
        ConflictHunk {
            base_start: self.base_start,
            base_lines: self.base_lines,
            ours_lines: self.ours_lines,
            theirs_lines: self.theirs_lines,
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredConflict {
    schema_version: String,
    kind: String,
    repo_id: String,
    path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    base: Option<StoredContentRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    ours: Option<StoredContentRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    theirs: Option<StoredContentRef>,
    conflict_type: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    hunks: Vec<StoredConflictHunk>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    resolution: Option<String>,
}

impl StoredConflict {
    fn from_conflict(conflict: &Conflict) -> Self {
        Self {
            schema_version: PROTOCOL_VERSION.to_owned(),
            kind: "Conflict".to_owned(),
            repo_id: conflict.repo_id.clone(),
            path: path_to_protocol_string(&conflict.path),
            base: option_ref_from_id(conflict.sides.base),
            ours: option_ref_from_id(conflict.sides.ours),
            theirs: option_ref_from_id(conflict.sides.theirs),
            conflict_type: conflict.conflict_type.as_protocol().to_owned(),
            hunks: conflict
                .hunks
                .iter()
                .map(StoredConflictHunk::from_hunk)
                .collect(),
            resolution: conflict.resolution.map(|id| id.to_hex()),
        }
    }

    fn ensure_kind(&self, expected: &'static str) -> ConflictResult<()> {
        ensure_protocol_header(&self.schema_version, &self.kind, expected)
    }

    fn into_conflict(self, id: ObjectId) -> ConflictResult<Conflict> {
        Ok(Conflict {
            id,
            repo_id: self.repo_id,
            path: parse_protocol_path(&self.path)?,
            conflict_type: ConflictType::from_protocol(&self.conflict_type)?,
            sides: ConflictSides {
                base: option_ref_into_id(self.base)?,
                ours: option_ref_into_id(self.ours)?,
                theirs: option_ref_into_id(self.theirs)?,
            },
            hunks: self
                .hunks
                .into_iter()
                .map(StoredConflictHunk::into_hunk)
                .collect(),
            resolution: self
                .resolution
                .as_deref()
                .map(parse_object_id)
                .transpose()?,
        })
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredMergeResult {
    schema_version: String,
    kind: String,
    repo_id: String,
    base_snapshot: String,
    ours_snapshot: String,
    theirs_snapshot: String,
    status: String,
    conflicts: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    merged_snapshot: Option<String>,
}

impl StoredMergeResult {
    fn from_result(result: &MergeResult) -> Self {
        Self {
            schema_version: PROTOCOL_VERSION.to_owned(),
            kind: "MergeResult".to_owned(),
            repo_id: result.repo_id.clone(),
            base_snapshot: result.base_snapshot.to_hex(),
            ours_snapshot: result.ours_snapshot.to_hex(),
            theirs_snapshot: result.theirs_snapshot.to_hex(),
            status: result.status.as_protocol().to_owned(),
            conflicts: result.conflicts.iter().map(|id| id.to_hex()).collect(),
            merged_snapshot: result.merged_snapshot.map(|id| id.to_hex()),
        }
    }

    fn ensure_kind(&self, expected: &'static str) -> ConflictResult<()> {
        ensure_protocol_header(&self.schema_version, &self.kind, expected)
    }

    fn into_merge_result(self, id: ObjectId) -> ConflictResult<MergeResult> {
        let status = MergeResultStatus::from_protocol(&self.status)?;
        let conflicts = self
            .conflicts
            .iter()
            .map(|value| parse_object_id(value))
            .collect::<ConflictResult<Vec<_>>>()?;
        if status == MergeResultStatus::Clean && !conflicts.is_empty() {
            return Err(ConflictError::CleanMergeResultWithConflicts {
                count: conflicts.len(),
            });
        }

        Ok(MergeResult {
            id,
            repo_id: self.repo_id,
            base_snapshot: parse_object_id(&self.base_snapshot)?,
            ours_snapshot: parse_object_id(&self.ours_snapshot)?,
            theirs_snapshot: parse_object_id(&self.theirs_snapshot)?,
            status,
            merged_snapshot: self
                .merged_snapshot
                .as_deref()
                .map(parse_object_id)
                .transpose()?,
            conflicts,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::InMemoryObjectStore;

    fn sides(store: &InMemoryObjectStore) -> ConflictSides {
        ConflictSides {
            base: Some(store.write(b"base").unwrap()),
            ours: Some(store.write(b"ours").unwrap()),
            theirs: Some(store.write(b"theirs").unwrap()),
        }
    }

    #[test]
    fn conflict_round_trips_through_store() {
        let store = InMemoryObjectStore::new();
        let conflict = conflict_from_hunks(
            "repo_test",
            "src/main.rs",
            sides(&store),
            [ConflictHunk {
                base_start: 2,
                base_lines: vec!["base".into()],
                ours_lines: vec!["ours".into()],
                theirs_lines: vec!["theirs".into()],
            }],
        );

        let written = write_conflict(&store, &conflict).unwrap();
        let read = read_conflict(&store, &written.id).unwrap();

        assert_eq!(read, written);
        assert_eq!(read.repo_id, "repo_test");
        assert_eq!(read.path, PathBuf::from("src/main.rs"));
        assert_eq!(read.conflict_type, ConflictType::Content);
        assert_eq!(read.sides, conflict.sides);
        assert!(read.resolution.is_none());
        assert_eq!(read.hunks.len(), 1);
        assert_eq!(read.hunks[0].base_start, 2);

        let bytes = store.read(&written.id).unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["conflictType"], "content");
        assert_eq!(json["repoId"], "repo_test");
    }

    #[test]
    fn stored_conflict_matches_protocol_shape() {
        let store = InMemoryObjectStore::new();
        let sides = sides(&store);
        let conflict = conflict_from_hunks(
            "repo_test",
            "README.md",
            sides,
            [ConflictHunk {
                base_start: 0,
                base_lines: vec!["a".into()],
                ours_lines: vec!["b".into()],
                theirs_lines: vec!["c".into()],
            }],
        );

        let written = write_conflict(&store, &conflict).unwrap();
        let bytes = store.read(&written.id).unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();

        // Protocol Conflict: repoId + `{ "object": hex }` content refs, string
        // arrays in hunks, and no engine-internal `{kind, id}` refs.
        assert_eq!(json["schemaVersion"], "sorrel.protocol.v0");
        assert_eq!(json["kind"], "Conflict");
        assert_eq!(json["repoId"], "repo_test");
        assert_eq!(json["path"], "README.md");
        assert_eq!(json["base"]["object"], sides.base.unwrap().to_hex());
        assert_eq!(json["ours"]["object"], sides.ours.unwrap().to_hex());
        assert_eq!(json["theirs"]["object"], sides.theirs.unwrap().to_hex());
        assert!(json["hunks"][0]["baseLines"].is_array());
        assert!(json.get("resolution").is_none());
    }

    #[test]
    fn conflict_without_either_side_is_rejected() {
        let store = InMemoryObjectStore::new();
        let conflict = conflict_with_type(
            "repo_test",
            "a.txt",
            ConflictType::ModifyDelete,
            ConflictSides::default(),
            [],
        );

        let error = write_conflict(&store, &conflict).unwrap_err();
        assert!(matches!(error, ConflictError::MissingConflictSides { .. }));
    }

    #[test]
    fn modify_delete_conflict_stores_single_side() {
        let store = InMemoryObjectStore::new();
        let ours = store.write(b"ours").unwrap();
        let conflict = conflict_with_type(
            "repo_test",
            "a.txt",
            ConflictType::ModifyDelete,
            ConflictSides {
                base: Some(store.write(b"base").unwrap()),
                ours: Some(ours),
                theirs: None,
            },
            [],
        );

        let written = write_conflict(&store, &conflict).unwrap();
        let bytes = store.read(&written.id).unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();

        assert_eq!(json["conflictType"], "modify_delete");
        assert_eq!(json["ours"]["object"], ours.to_hex());
        assert!(json.get("theirs").is_none());
        assert!(json.get("hunks").is_none());

        let read = read_conflict(&store, &written.id).unwrap();
        assert_eq!(read.sides.ours, Some(ours));
        assert_eq!(read.sides.theirs, None);
    }

    #[test]
    fn conflict_resolution_round_trips() {
        let store = InMemoryObjectStore::new();
        let resolved = store.write(b"resolved").unwrap();
        let mut conflict = conflict_from_hunks("repo_test", "a.txt", sides(&store), []);
        conflict.resolution = Some(resolved);

        let written = write_conflict(&store, &conflict).unwrap();
        let bytes = store.read(&written.id).unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["resolution"], resolved.to_hex());

        let read = read_conflict(&store, &written.id).unwrap();
        assert_eq!(read.resolution, Some(resolved));
    }

    #[test]
    fn conflict_ids_are_deterministic() {
        let store = InMemoryObjectStore::new();
        let conflict = conflict_from_hunks(
            "repo_test",
            "README.md",
            sides(&store),
            [ConflictHunk {
                base_start: 0,
                base_lines: vec!["a".into()],
                ours_lines: vec!["b".into()],
                theirs_lines: vec!["c".into()],
            }],
        );

        let first = write_conflict(&store, &conflict).unwrap();
        let second = write_conflict(&store, &conflict).unwrap();

        assert_eq!(first.id, second.id);
    }

    fn merge_inputs(store: &InMemoryObjectStore) -> (ObjectId, ObjectId, ObjectId) {
        (
            store.write(b"base-snapshot").unwrap(),
            store.write(b"ours-snapshot").unwrap(),
            store.write(b"theirs-snapshot").unwrap(),
        )
    }

    #[test]
    fn merge_result_round_trips_through_store() {
        let store = InMemoryObjectStore::new();
        let (base, ours, theirs) = merge_inputs(&store);
        let snapshot_id = store.write(b"snapshot-bytes").unwrap();
        let result = MergeResult {
            id: ObjectId::from_bytes([0; 32]),
            repo_id: "repo_test".to_owned(),
            base_snapshot: base,
            ours_snapshot: ours,
            theirs_snapshot: theirs,
            status: MergeResultStatus::Clean,
            merged_snapshot: Some(snapshot_id),
            conflicts: Vec::new(),
        };

        let written = write_merge_result(&store, &result).unwrap();
        let read = read_merge_result(&store, &written.id).unwrap();

        assert_eq!(read, written);
        assert_eq!(read.repo_id, "repo_test");
        assert_eq!(read.base_snapshot, base);
        assert_eq!(read.ours_snapshot, ours);
        assert_eq!(read.theirs_snapshot, theirs);
        assert_eq!(read.status, MergeResultStatus::Clean);
        assert!(read.conflicts.is_empty());
        assert_eq!(read.merged_snapshot, Some(snapshot_id));
    }

    #[test]
    fn merge_result_ids_are_deterministic() {
        let store = InMemoryObjectStore::new();
        let (base, ours, theirs) = merge_inputs(&store);
        let snapshot_id = store.write(b"snapshot-bytes").unwrap();
        let result = MergeResult {
            id: ObjectId::from_bytes([0; 32]),
            repo_id: "repo_test".to_owned(),
            base_snapshot: base,
            ours_snapshot: ours,
            theirs_snapshot: theirs,
            status: MergeResultStatus::Clean,
            merged_snapshot: Some(snapshot_id),
            conflicts: Vec::new(),
        };

        let first = write_merge_result(&store, &result).unwrap();
        let second = write_merge_result(&store, &result).unwrap();

        assert_eq!(first.id, second.id);
    }

    #[test]
    fn stored_merge_result_matches_protocol_shape() {
        let store = InMemoryObjectStore::new();
        let (base, ours, theirs) = merge_inputs(&store);
        let snapshot_id = store.write(b"merged").unwrap();
        let result = MergeResult {
            id: ObjectId::from_bytes([0; 32]),
            repo_id: "repo_test".to_owned(),
            base_snapshot: base,
            ours_snapshot: ours,
            theirs_snapshot: theirs,
            status: MergeResultStatus::Clean,
            merged_snapshot: Some(snapshot_id),
            conflicts: Vec::new(),
        };

        let written = write_merge_result(&store, &result).unwrap();
        let bytes = store.read(&written.id).unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();

        // Protocol MergeResult: repoId, three input snapshot ids, and bare
        // 64-hex ids (no `{kind, id}` object refs).
        assert_eq!(json["status"], "clean");
        assert_eq!(json["kind"], "MergeResult");
        assert_eq!(json["repoId"], "repo_test");
        assert_eq!(json["baseSnapshot"], base.to_hex());
        assert_eq!(json["oursSnapshot"], ours.to_hex());
        assert_eq!(json["theirsSnapshot"], theirs.to_hex());
        assert!(json["conflicts"].as_array().unwrap().is_empty());
        assert_eq!(json["mergedSnapshot"], snapshot_id.to_hex());
        assert!(written.conflicts.is_empty());
        assert!(written.merged_snapshot.is_some());
    }

    #[test]
    fn clean_merge_result_with_conflicts_is_rejected() {
        let store = InMemoryObjectStore::new();
        let (base, ours, theirs) = merge_inputs(&store);
        let result = MergeResult {
            id: ObjectId::from_bytes([0; 32]),
            repo_id: "repo_test".to_owned(),
            base_snapshot: base,
            ours_snapshot: ours,
            theirs_snapshot: theirs,
            status: MergeResultStatus::Clean,
            merged_snapshot: Some(store.write(b"merged").unwrap()),
            conflicts: vec![store.write(b"conflict").unwrap()],
        };

        let error = write_merge_result(&store, &result).unwrap_err();
        assert!(matches!(
            error,
            ConflictError::CleanMergeResultWithConflicts { count: 1 }
        ));
    }

    #[test]
    fn conflicted_merge_result_lists_conflict_ids() {
        let store = InMemoryObjectStore::new();
        let (base, ours, theirs) = merge_inputs(&store);
        let conflict = write_conflict(
            &store,
            &conflict_from_hunks(
                "repo_test",
                "a.txt",
                sides(&store),
                [ConflictHunk {
                    base_start: 0,
                    base_lines: vec![],
                    ours_lines: vec!["ours".into()],
                    theirs_lines: vec!["theirs".into()],
                }],
            ),
        )
        .unwrap();
        let result = MergeResult {
            id: ObjectId::from_bytes([0; 32]),
            repo_id: "repo_test".to_owned(),
            base_snapshot: base,
            ours_snapshot: ours,
            theirs_snapshot: theirs,
            status: MergeResultStatus::Conflicted,
            merged_snapshot: None,
            conflicts: vec![conflict.id],
        };

        let written = write_merge_result(&store, &result).unwrap();
        let read = read_merge_result(&store, &written.id).unwrap();
        let bytes = store.read(&written.id).unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();

        assert_eq!(read.status, MergeResultStatus::Conflicted);
        assert!(read.merged_snapshot.is_none());
        assert_eq!(read.conflicts, vec![conflict.id]);
        assert_eq!(json["status"], "conflicted");
        assert_eq!(json["conflicts"][0], conflict.id.to_hex());
        assert!(json.get("mergedSnapshot").is_none());
    }
}
