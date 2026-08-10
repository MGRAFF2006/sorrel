//! Core storage primitives for Sorrel.

pub mod authority;
pub mod change;
pub mod conflict;
pub mod dag;
pub mod git_export;
pub mod git_import;
pub mod history;
pub mod lane_stack;
pub mod merge;
pub mod merge3;
pub mod object;
pub mod permissions;
pub mod policy;
pub mod snapshot;
pub mod stat_cache;
pub mod store;
pub mod transport;

pub use authority::{
    evaluate_policy_change, AuthorityRoot, AuthoritySignature, AuthoritySigningKey, PolicyChange,
    PolicyChangeAction, PolicyChangeContext, PolicyChangeEvaluation, PolicyChangeOutcome,
    PolicyChangeTrust, PolicyRoot, ProposedGrant, CAP_AUTHORITY_ADMIN, CAP_AUTHORITY_ROTATE,
    CAP_POLICY_DELEGATE, CAP_POLICY_GRANT,
};
pub use change::{
    apply_change, apply_loaded_change, create_change, read_change, snapshot_diff, Change,
    ChangeError, ChangeOptions, ChangeResult, PathChange, PathChangeKind, SnapshotDiff,
};
pub use conflict::{
    conflict_from_hunks, conflict_with_type, read_conflict, read_merge_result, write_conflict,
    write_merge_result, Conflict, ConflictError, ConflictResult, ConflictSides, ConflictType,
    MergeResult, MergeResultStatus,
};
pub use dag::{DagError, DagResult};
pub use git_export::{
    git_export, ExportResult, ExportedCommit, GitExportError, GitExportOptions, GitExportResult,
};
pub use git_import::{
    git_import, GitImportError, GitImportOptions, GitImportResult, ImportResult, ImportedCommit,
};
pub use history::{collect_ancestors, merge_base, merge_bases, HistoryError, HistoryResult};
pub use lane_stack::{
    create_lane, create_stack, read_lane, read_stack, Lane, LaneOptions, LaneStackError,
    LaneStackResult, Stack, StackOptions,
};
pub use merge::{merge_snapshots, MergeError, MergeOptions};
pub use object::{parse_object_id_hex, ObjectId, ObjectIdParseError};
pub use permissions::{
    AuditHook, Grant, GrantRef, PermissionError, PermissionMetadata, PermissionResult, PolicyRef,
    ResourceRef, Visibility,
};
pub use policy::{
    evaluate_policy, AuditEvent as PolicyAuditEvent, Capability, DecisionKind,
    Grant as PolicyGrant, GrantEffect, Policy, PolicyDecision, PolicyEvaluationRequest, PolicyRule,
    PrincipalDescriptor, PrincipalId, PrincipalKind, RedactionMarker, ResourceKind,
    ResourceRef as PolicyResourceRef, SecretRef,
};
pub use snapshot::{
    materialize_snapshot, materialize_snapshot_excluding,
    materialize_snapshot_excluding_with_stat_cache, materialize_snapshot_with_stat_cache,
    read_blob, read_snapshot, read_snapshot_files, read_tree, restore_snapshot_to_directory,
    write_blob, write_snapshot, write_tree, write_tree_from_directory,
    write_tree_from_directory_excluding, write_tree_from_directory_excluding_with_stat_cache,
    write_tree_from_directory_with_stat_cache, Blob, EntryMode, EntryType, ObjectKind, ObjectRef,
    Principal, Snapshot, SnapshotError, SnapshotOptions, SnapshotResult, Tree, TreeEntry,
};
pub use stat_cache::{StatCache, StatCacheEntry, StatCacheError, StatCacheResult};
pub use store::{
    FileObjectStore, InMemoryObjectStore, ObjectStore, ObjectStoreError, ObjectStoreResult,
};
pub use transport::{
    collect_closure, is_descendant, missing_in_target, missing_objects, transfer_objects,
    TransportError, TransportResult,
};
