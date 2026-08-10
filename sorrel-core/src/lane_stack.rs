use crate::{
    permissions::StoredPermissionMetadata, read_change, read_snapshot, ChangeError, ObjectId,
    ObjectIdParseError, ObjectKind, ObjectRef, ObjectStore, ObjectStoreError, PermissionError,
    PermissionMetadata, Principal, ResourceRef, SnapshotError, Visibility,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

const PROTOCOL_VERSION: &str = "sorrel.protocol.v0";

/// Result type used by Lane and Stack object operations.
pub type LaneStackResult<T> = Result<T, LaneStackError>;

/// Errors returned while serializing or reading Lane and Stack objects.
#[derive(Debug, thiserror::Error)]
pub enum LaneStackError {
    /// The underlying object store failed.
    #[error(transparent)]
    ObjectStore(#[from] ObjectStoreError),

    /// Snapshot access failed while validating lane or stack metadata.
    #[error(transparent)]
    Snapshot(#[from] SnapshotError),

    /// Change access failed while collecting touched resources.
    #[error(transparent)]
    Change(#[from] ChangeError),

    /// Permission metadata could not be read.
    #[error(transparent)]
    Permission(#[from] PermissionError),

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

    /// A lane or stack reference pointed at the wrong object kind.
    #[error("invalid {field} object kind {actual:?}; expected {expected:?}")]
    InvalidObjectRefKind {
        /// Field containing the invalid reference.
        field: &'static str,
        /// Expected object kind.
        expected: &'static str,
        /// Actual object kind.
        actual: &'static str,
    },
}

/// Options used when creating a Lane object.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LaneOptions {
    /// Lane name, for example `agent-17/fix-tests`.
    pub name: String,
    /// Snapshot where this lane started.
    pub base_snapshot: ObjectId,
    /// Current head snapshot for this lane.
    pub head_snapshot: ObjectId,
    /// Ordered changes currently active on this lane.
    pub changes: Vec<ObjectId>,
    /// Permission metadata embedded in the lane.
    pub permission_metadata: PermissionMetadata,
    /// Protocol timestamp string.
    pub created_at: String,
    /// Optional lane description or task summary.
    pub description: Option<String>,
}

impl LaneOptions {
    /// Builds lane options with deterministic defaults.
    #[must_use]
    pub fn new(
        name: impl Into<String>,
        base_snapshot: ObjectId,
        head_snapshot: ObjectId,
        owner: Principal,
        visibility: Visibility,
    ) -> Self {
        Self {
            name: name.into(),
            base_snapshot,
            head_snapshot,
            changes: Vec::new(),
            permission_metadata: PermissionMetadata::new(owner, visibility),
            created_at: "1970-01-01T00:00:00Z".to_owned(),
            description: None,
        }
    }
}

/// Isolated human or agent workstream metadata.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Lane {
    /// Content-addressed ID of the stored lane object.
    pub id: ObjectId,
    /// Lane name.
    pub name: String,
    /// Snapshot where this lane started.
    pub base_snapshot: ObjectRef,
    /// Current head snapshot for this lane.
    pub head_snapshot: ObjectRef,
    /// Ordered change references active on this lane.
    pub changes: Vec<ObjectRef>,
    /// Permission metadata for this lane.
    pub permission_metadata: PermissionMetadata,
    /// Protocol timestamp string.
    pub created_at: String,
    /// Optional lane description or task summary.
    pub description: Option<String>,
}

impl Lane {
    /// Returns true when one of this lane's referenced grants allows the capability.
    #[must_use]
    pub fn is_authorized_by_grant(
        &self,
        principal: &Principal,
        resource: &ResourceRef,
        capability: &str,
        grants: &[crate::Grant],
    ) -> bool {
        self.permission_metadata
            .is_authorized_by_grant(principal, resource, capability, grants)
    }
}

/// Options used when creating a Stack object.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StackOptions {
    /// Stack name.
    pub name: String,
    /// Snapshot where this stack started.
    pub base_snapshot: ObjectId,
    /// Head snapshot after applying this stack's changes.
    pub head_snapshot: ObjectId,
    /// Ordered changes contained by this stack.
    pub changes: Vec<ObjectId>,
    /// Other stack objects this stack depends on.
    pub dependency_stacks: Vec<ObjectId>,
    /// Permission metadata embedded in the stack.
    pub permission_metadata: PermissionMetadata,
    /// Protocol timestamp string.
    pub created_at: String,
    /// Optional stack description.
    pub description: Option<String>,
}

impl StackOptions {
    /// Builds stack options with deterministic defaults.
    #[must_use]
    pub fn new(
        name: impl Into<String>,
        base_snapshot: ObjectId,
        head_snapshot: ObjectId,
        owner: Principal,
        visibility: Visibility,
    ) -> Self {
        Self {
            name: name.into(),
            base_snapshot,
            head_snapshot,
            changes: Vec::new(),
            dependency_stacks: Vec::new(),
            permission_metadata: PermissionMetadata::new(owner, visibility),
            created_at: "1970-01-01T00:00:00Z".to_owned(),
            description: None,
        }
    }
}

/// Ordered set of changes prepared for review, submission, or dependency tracking.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Stack {
    /// Content-addressed ID of the stored stack object.
    pub id: ObjectId,
    /// Stack name.
    pub name: String,
    /// Snapshot where this stack started.
    pub base_snapshot: ObjectRef,
    /// Head snapshot after applying this stack's changes.
    pub head_snapshot: ObjectRef,
    /// Ordered changes contained by this stack.
    pub changes: Vec<ObjectRef>,
    /// Other stack objects this stack depends on.
    pub dependency_stacks: Vec<ObjectRef>,
    /// Permission metadata for this stack.
    pub permission_metadata: PermissionMetadata,
    /// Protocol timestamp string.
    pub created_at: String,
    /// Optional stack description.
    pub description: Option<String>,
}

/// Creates and stores a deterministic Lane object.
pub fn create_lane(store: &impl ObjectStore, options: LaneOptions) -> LaneStackResult<Lane> {
    read_snapshot(store, &options.base_snapshot)?;
    read_snapshot(store, &options.head_snapshot)?;

    let base_snapshot = ObjectRef::new(ObjectKind::Snapshot, options.base_snapshot);
    let head_snapshot = ObjectRef::new(ObjectKind::Snapshot, options.head_snapshot);
    let changes = options
        .changes
        .iter()
        .copied()
        .map(|id| ObjectRef::new(ObjectKind::Change, id))
        .collect::<Vec<_>>();
    let permission_metadata =
        permission_metadata_with_change_resources(store, options.permission_metadata, &changes)?;

    let stored = StoredLane::from_parts(
        &options.name,
        base_snapshot,
        head_snapshot,
        &changes,
        &permission_metadata,
        &options.created_at,
        options.description.as_deref(),
    );
    let bytes = serde_json::to_vec(&stored)?;
    let id = store.write(&bytes)?;

    Ok(Lane {
        id,
        name: options.name,
        base_snapshot,
        head_snapshot,
        changes,
        permission_metadata,
        created_at: options.created_at,
        description: options.description,
    })
}

/// Reads a stored Lane object.
pub fn read_lane(store: &impl ObjectStore, id: &ObjectId) -> LaneStackResult<Lane> {
    let bytes = store.read(id)?;
    let stored: StoredLane = serde_json::from_slice(&bytes)?;
    stored.ensure_kind("Lane")?;
    stored.into_lane(*id)
}

/// Creates and stores a deterministic Stack object.
pub fn create_stack(store: &impl ObjectStore, options: StackOptions) -> LaneStackResult<Stack> {
    read_snapshot(store, &options.base_snapshot)?;
    read_snapshot(store, &options.head_snapshot)?;

    let base_snapshot = ObjectRef::new(ObjectKind::Snapshot, options.base_snapshot);
    let head_snapshot = ObjectRef::new(ObjectKind::Snapshot, options.head_snapshot);
    let changes = options
        .changes
        .iter()
        .copied()
        .map(|id| ObjectRef::new(ObjectKind::Change, id))
        .collect::<Vec<_>>();
    let dependency_stacks = options
        .dependency_stacks
        .iter()
        .copied()
        .map(|id| ObjectRef::new(ObjectKind::Stack, id))
        .collect::<Vec<_>>();
    let permission_metadata =
        permission_metadata_with_change_resources(store, options.permission_metadata, &changes)?;

    let stored = StoredStack::from_parts(
        &options.name,
        base_snapshot,
        head_snapshot,
        &changes,
        &dependency_stacks,
        &permission_metadata,
        &options.created_at,
        options.description.as_deref(),
    );
    let bytes = serde_json::to_vec(&stored)?;
    let id = store.write(&bytes)?;

    Ok(Stack {
        id,
        name: options.name,
        base_snapshot,
        head_snapshot,
        changes,
        dependency_stacks,
        permission_metadata,
        created_at: options.created_at,
        description: options.description,
    })
}

/// Reads a stored Stack object.
pub fn read_stack(store: &impl ObjectStore, id: &ObjectId) -> LaneStackResult<Stack> {
    let bytes = store.read(id)?;
    let stored: StoredStack = serde_json::from_slice(&bytes)?;
    stored.ensure_kind("Stack")?;
    stored.into_stack(*id)
}

fn permission_metadata_with_change_resources(
    store: &impl ObjectStore,
    mut permission_metadata: PermissionMetadata,
    changes: &[ObjectRef],
) -> LaneStackResult<PermissionMetadata> {
    let mut touched_resources = permission_metadata
        .touched_resources
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();

    for change_ref in changes {
        ensure_ref_kind(change_ref, ObjectKind::Change, "changes")?;
        let change = read_change(store, &change_ref.id)?;
        for touched_path in change.touched_paths {
            touched_resources.insert(ResourceRef::path(touched_path));
        }
    }

    permission_metadata.set_canonical_touched_resources(touched_resources);
    Ok(permission_metadata)
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

fn kind_from_protocol(kind: &str) -> LaneStackResult<ObjectKind> {
    match kind {
        "Blob" => Ok(ObjectKind::Blob),
        "Tree" => Ok(ObjectKind::Tree),
        "Snapshot" => Ok(ObjectKind::Snapshot),
        "Change" => Ok(ObjectKind::Change),
        "Lane" => Ok(ObjectKind::Lane),
        "Stack" => Ok(ObjectKind::Stack),
        "Conflict" => Ok(ObjectKind::Conflict),
        "MergeResult" => Ok(ObjectKind::MergeResult),
        other => Err(LaneStackError::InvalidObjectKind {
            expected: "Blob, Tree, Snapshot, Change, Lane, Stack, Conflict, or MergeResult",
            actual: other.to_owned(),
        }),
    }
}

fn ensure_ref_kind(
    reference: &ObjectRef,
    expected: ObjectKind,
    field: &'static str,
) -> LaneStackResult<()> {
    if reference.kind == expected {
        Ok(())
    } else {
        Err(LaneStackError::InvalidObjectRefKind {
            field,
            expected: kind_to_protocol(expected),
            actual: kind_to_protocol(reference.kind),
        })
    }
}

fn parse_object_id(value: &str) -> LaneStackResult<ObjectId> {
    value
        .parse()
        .map_err(|source| LaneStackError::InvalidObjectId {
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
    fn into_ref(self) -> LaneStackResult<ObjectRef> {
        Ok(ObjectRef::new(
            kind_from_protocol(&self.kind)?,
            parse_object_id(&self.id)?,
        ))
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredLane {
    schema_version: String,
    kind: String,
    name: String,
    base_snapshot: StoredObjectRef,
    head_snapshot: StoredObjectRef,
    changes: Vec<StoredObjectRef>,
    permission_metadata: StoredPermissionMetadata,
    created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
}

impl StoredLane {
    fn from_parts(
        name: &str,
        base_snapshot: ObjectRef,
        head_snapshot: ObjectRef,
        changes: &[ObjectRef],
        permission_metadata: &PermissionMetadata,
        created_at: &str,
        description: Option<&str>,
    ) -> Self {
        Self {
            schema_version: PROTOCOL_VERSION.to_owned(),
            kind: "Lane".to_owned(),
            name: name.to_owned(),
            base_snapshot: base_snapshot.into(),
            head_snapshot: head_snapshot.into(),
            changes: changes.iter().copied().map(StoredObjectRef::from).collect(),
            permission_metadata: StoredPermissionMetadata::from_metadata(permission_metadata),
            created_at: created_at.to_owned(),
            description: description.map(str::to_owned),
        }
    }

    fn ensure_kind(&self, expected: &'static str) -> LaneStackResult<()> {
        if self.schema_version != PROTOCOL_VERSION {
            return Err(LaneStackError::UnsupportedSchemaVersion {
                expected: PROTOCOL_VERSION,
                actual: self.schema_version.clone(),
            });
        }

        if self.kind != expected {
            return Err(LaneStackError::InvalidObjectKind {
                expected,
                actual: self.kind.clone(),
            });
        }

        Ok(())
    }

    fn into_lane(self, id: ObjectId) -> LaneStackResult<Lane> {
        let base_snapshot = self.base_snapshot.into_ref()?;
        ensure_ref_kind(&base_snapshot, ObjectKind::Snapshot, "baseSnapshot")?;
        let head_snapshot = self.head_snapshot.into_ref()?;
        ensure_ref_kind(&head_snapshot, ObjectKind::Snapshot, "headSnapshot")?;
        let changes = self
            .changes
            .into_iter()
            .map(StoredObjectRef::into_ref)
            .collect::<LaneStackResult<Vec<_>>>()?;
        for change in &changes {
            ensure_ref_kind(change, ObjectKind::Change, "changes")?;
        }

        Ok(Lane {
            id,
            name: self.name,
            base_snapshot,
            head_snapshot,
            changes,
            permission_metadata: self.permission_metadata.into_metadata()?,
            created_at: self.created_at,
            description: self.description,
        })
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredStack {
    schema_version: String,
    kind: String,
    name: String,
    base_snapshot: StoredObjectRef,
    head_snapshot: StoredObjectRef,
    changes: Vec<StoredObjectRef>,
    dependency_stacks: Vec<StoredObjectRef>,
    permission_metadata: StoredPermissionMetadata,
    created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
}

impl StoredStack {
    #[allow(clippy::too_many_arguments)]
    fn from_parts(
        name: &str,
        base_snapshot: ObjectRef,
        head_snapshot: ObjectRef,
        changes: &[ObjectRef],
        dependency_stacks: &[ObjectRef],
        permission_metadata: &PermissionMetadata,
        created_at: &str,
        description: Option<&str>,
    ) -> Self {
        Self {
            schema_version: PROTOCOL_VERSION.to_owned(),
            kind: "Stack".to_owned(),
            name: name.to_owned(),
            base_snapshot: base_snapshot.into(),
            head_snapshot: head_snapshot.into(),
            changes: changes.iter().copied().map(StoredObjectRef::from).collect(),
            dependency_stacks: dependency_stacks
                .iter()
                .copied()
                .map(StoredObjectRef::from)
                .collect(),
            permission_metadata: StoredPermissionMetadata::from_metadata(permission_metadata),
            created_at: created_at.to_owned(),
            description: description.map(str::to_owned),
        }
    }

    fn ensure_kind(&self, expected: &'static str) -> LaneStackResult<()> {
        if self.schema_version != PROTOCOL_VERSION {
            return Err(LaneStackError::UnsupportedSchemaVersion {
                expected: PROTOCOL_VERSION,
                actual: self.schema_version.clone(),
            });
        }

        if self.kind != expected {
            return Err(LaneStackError::InvalidObjectKind {
                expected,
                actual: self.kind.clone(),
            });
        }

        Ok(())
    }

    fn into_stack(self, id: ObjectId) -> LaneStackResult<Stack> {
        let base_snapshot = self.base_snapshot.into_ref()?;
        ensure_ref_kind(&base_snapshot, ObjectKind::Snapshot, "baseSnapshot")?;
        let head_snapshot = self.head_snapshot.into_ref()?;
        ensure_ref_kind(&head_snapshot, ObjectKind::Snapshot, "headSnapshot")?;
        let changes = self
            .changes
            .into_iter()
            .map(StoredObjectRef::into_ref)
            .collect::<LaneStackResult<Vec<_>>>()?;
        for change in &changes {
            ensure_ref_kind(change, ObjectKind::Change, "changes")?;
        }
        let dependency_stacks = self
            .dependency_stacks
            .into_iter()
            .map(StoredObjectRef::into_ref)
            .collect::<LaneStackResult<Vec<_>>>()?;
        for stack in &dependency_stacks {
            ensure_ref_kind(stack, ObjectKind::Stack, "dependencyStacks")?;
        }

        Ok(Stack {
            id,
            name: self.name,
            base_snapshot,
            head_snapshot,
            changes,
            dependency_stacks,
            permission_metadata: self.permission_metadata.into_metadata()?,
            created_at: self.created_at,
            description: self.description,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        create_change, materialize_snapshot, AuditHook, ChangeOptions, Grant, GrantRef,
        InMemoryObjectStore, ObjectStore, PolicyRef, Snapshot, SnapshotOptions,
    };
    use serde_json::Value;
    use std::{fs, path::Path};

    #[test]
    fn lane_serialization_supports_private_and_redacted_visibility() {
        for visibility in [Visibility::Private, Visibility::Redacted] {
            let store = InMemoryObjectStore::new();
            let (base, result, change_id) = change_touching(&store, "src/lib.rs");
            let mut options = LaneOptions::new(
                format!("agent/lane-{}", visibility.as_str()),
                base.id,
                result.id,
                Principal::new("agent", "agent-17", Some("Agent 17".to_owned())),
                visibility,
            );
            options.changes.push(change_id);
            options
                .permission_metadata
                .policy_refs
                .push(PolicyRef::new("policy_lane_default"));
            options
                .permission_metadata
                .grant_refs
                .push(GrantRef::new("grant_lane_agent"));
            options
                .permission_metadata
                .audit_hooks
                .push(AuditHook::new("audit_lane", ["lane.write"]));

            let lane = create_lane(&store, options).unwrap();
            let read = read_lane(&store, &lane.id).unwrap();
            let json = stored_json(&store, lane.id);

            assert_eq!(read, lane);
            assert_eq!(json["kind"], "Lane");
            assert_eq!(
                json["permissionMetadata"]["visibility"],
                Value::String(visibility.as_str().to_owned())
            );
            assert_eq!(
                json["permissionMetadata"]["policyRefs"][0]["id"],
                "policy_lane_default"
            );
            assert_eq!(
                json["permissionMetadata"]["grantRefs"][0]["id"],
                "grant_lane_agent"
            );
            assert_eq!(
                json["permissionMetadata"]["auditHooks"][0]["id"],
                "audit_lane"
            );
        }
    }

    #[test]
    fn stack_serialization_supports_team_and_review_visibility() {
        for visibility in [Visibility::Team, Visibility::Review] {
            let store = InMemoryObjectStore::new();
            let (base, result, change_id) = change_touching(&store, "src/lib.rs");
            let mut options = StackOptions::new(
                format!("stack/{}", visibility.as_str()),
                base.id,
                result.id,
                Principal::new("user", "alice", Some("Alice".to_owned())),
                visibility,
            );
            options.changes.push(change_id);
            options
                .permission_metadata
                .policy_refs
                .push(PolicyRef::new("policy_review"));
            options
                .permission_metadata
                .grant_refs
                .push(GrantRef::new("grant_reviewers"));
            options
                .permission_metadata
                .audit_hooks
                .push(AuditHook::new("audit_stack", ["stack.submit"]));

            let stack = create_stack(&store, options).unwrap();
            let read = read_stack(&store, &stack.id).unwrap();
            let json = stored_json(&store, stack.id);

            assert_eq!(read, stack);
            assert_eq!(json["kind"], "Stack");
            assert_eq!(
                json["permissionMetadata"]["visibility"],
                Value::String(visibility.as_str().to_owned())
            );
            assert_eq!(
                json["permissionMetadata"]["policyRefs"][0]["id"],
                "policy_review"
            );
            assert_eq!(
                json["permissionMetadata"]["grantRefs"][0]["id"],
                "grant_reviewers"
            );
            assert_eq!(
                json["permissionMetadata"]["auditHooks"][0]["id"],
                "audit_stack"
            );
        }
    }

    #[test]
    fn lane_touched_paths_are_stored_as_resource_refs() {
        let store = InMemoryObjectStore::new();
        let (base, result, change_id) = change_touching(&store, "src/lib.rs");
        let mut options = LaneOptions::new(
            "agent/refactor-lib",
            base.id,
            result.id,
            Principal::new("agent", "agent-17", None),
            Visibility::Private,
        );
        options.changes.push(change_id);

        let lane = create_lane(&store, options).unwrap();
        let json = stored_json(&store, lane.id);

        assert_eq!(
            lane.permission_metadata.touched_resources,
            vec![ResourceRef::path("src/lib.rs")]
        );
        assert!(json.get("touchedPaths").is_none());
        assert_eq!(
            json["permissionMetadata"]["touchedResources"][0]["type"],
            "path"
        );
        assert_eq!(
            json["permissionMetadata"]["touchedResources"][0]["value"],
            "src/lib.rs"
        );
    }

    #[test]
    fn stack_touched_paths_are_stored_as_resource_refs() {
        let store = InMemoryObjectStore::new();
        let (base, result, change_id) = change_touching(&store, "README.md");
        let mut options = StackOptions::new(
            "stack/readme",
            base.id,
            result.id,
            Principal::new("user", "alice", None),
            Visibility::Review,
        );
        options.changes.push(change_id);

        let stack = create_stack(&store, options).unwrap();
        let json = stored_json(&store, stack.id);

        assert_eq!(
            stack.permission_metadata.touched_resources,
            vec![ResourceRef::path("README.md")]
        );
        assert_eq!(
            json["permissionMetadata"]["touchedResources"][0]["type"],
            "path"
        );
        assert_eq!(
            json["permissionMetadata"]["touchedResources"][0]["value"],
            "README.md"
        );
    }

    #[test]
    fn agent_lane_can_be_authorized_for_path_write_through_grants() {
        let store = InMemoryObjectStore::new();
        let (base, result, change_id) = change_touching(&store, "src/lib.rs");
        let owner = Principal::new("user", "alice", None);
        let agent = Principal::new("agent", "agent-17", None);
        let touched_resource = ResourceRef::path("src/lib.rs");
        let grant = Grant::new(
            "grant_agent_src_write",
            agent.clone(),
            touched_resource.clone(),
            ["path.write"],
        );
        let mut options = LaneOptions::new(
            "agent-17/fix-tests",
            base.id,
            result.id,
            owner,
            Visibility::Private,
        );
        options.changes.push(change_id);
        options
            .permission_metadata
            .grant_refs
            .push(GrantRef::new("grant_agent_src_write"));

        let lane = create_lane(&store, options).unwrap();

        assert!(lane.is_authorized_by_grant(&agent, &touched_resource, "path.write", &[grant]));
    }

    fn change_touching(
        store: &InMemoryObjectStore,
        changed_path: &str,
    ) -> (Snapshot, Snapshot, ObjectId) {
        let base_dir = tempfile::tempdir().unwrap();
        let result_dir = tempfile::tempdir().unwrap();
        write_file(base_dir.path().join(changed_path), b"old\n");
        write_file(result_dir.path().join(changed_path), b"new\n");
        let base =
            materialize_snapshot(store, base_dir.path(), SnapshotOptions::new("repo_sorrel"))
                .unwrap();
        let result = materialize_snapshot(
            store,
            result_dir.path(),
            SnapshotOptions::new("repo_sorrel"),
        )
        .unwrap();
        let change = create_change(
            store,
            base.id,
            result.id,
            ChangeOptions::new(Principal::system(), "update file"),
        )
        .unwrap();

        (base, result, change.id)
    }

    fn stored_json(store: &InMemoryObjectStore, id: ObjectId) -> Value {
        serde_json::from_slice(&store.read(&id).unwrap()).unwrap()
    }

    fn write_file(path: impl AsRef<Path>, content: &[u8]) {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, content).unwrap();
    }
}
