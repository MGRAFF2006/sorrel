//! Core storage primitives for Sorrel.

pub mod object;
pub mod permission;
pub mod store;

pub use object::{ObjectId, ObjectIdParseError};
pub use permission::{
    evaluate, evaluate_with_audit, AuditEvent, Capability, EvaluationContext, Grant, GrantEffect,
    PolicyDecision, Principal, PrincipalKind, Redaction, ResourceRef, SecretRef,
};
pub use store::{
    FileObjectStore, InMemoryObjectStore, ObjectStore, ObjectStoreError, ObjectStoreResult,
};
