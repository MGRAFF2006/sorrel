//! Core storage primitives for Sorrel.

pub mod object;
pub mod store;

pub use object::{ObjectId, ObjectIdParseError};
pub use store::{
    FileObjectStore, InMemoryObjectStore, ObjectStore, ObjectStoreError, ObjectStoreResult,
};
