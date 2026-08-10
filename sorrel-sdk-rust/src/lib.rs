//! Thin SDK over `sorrel-core` for embedding Sorrel in Rust applications.

pub use sorrel_core::{
    FileObjectStore, ObjectId, ObjectKind, ObjectRef, ObjectStore, Snapshot, SnapshotOptions,
};

use sorrel_core::{materialize_snapshot_excluding, write_snapshot, write_tree};
use std::path::{Path, PathBuf};
use thiserror::Error;

/// Errors raised by the SDK workspace helpers.
#[derive(Debug, Error)]
pub enum SdkError {
    #[error(transparent)]
    Store(#[from] sorrel_core::ObjectStoreError),
    #[error(transparent)]
    Snapshot(#[from] sorrel_core::SnapshotError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// On-disk workspace rooted at `root/.sorrel/objects`.
pub struct Workspace {
    root: PathBuf,
    store: FileObjectStore,
    repo_id: String,
}

impl Workspace {
    /// Creates object storage under `root/.sorrel/objects` and an empty initial snapshot.
    pub fn init(
        root: impl Into<PathBuf>,
        repo_id: impl Into<String>,
    ) -> Result<(Self, Snapshot), SdkError> {
        let root = root.into();
        let repo_id = repo_id.into();
        let objects = root.join(".sorrel").join("objects");
        std::fs::create_dir_all(&objects)?;
        let store = FileObjectStore::new(&objects)?;
        let mut options = SnapshotOptions::new(repo_id.clone());
        options.message = Some("initial snapshot".to_owned());
        let tree = write_tree(&store, Vec::new())?;
        let snapshot = write_snapshot(&store, tree.id, options)?;
        Ok((
            Self {
                root,
                store,
                repo_id,
            },
            snapshot,
        ))
    }

    /// Snapshot the working tree (excluding `.sorrel`) with `parent` as the parent snapshot.
    pub fn snapshot_working_tree(
        &self,
        parent: ObjectId,
        message: impl Into<String>,
    ) -> Result<Snapshot, SdkError> {
        let mut options = SnapshotOptions::new(self.repo_id.clone());
        options.message = Some(message.into());
        options.parents = vec![ObjectRef::new(ObjectKind::Snapshot, parent)];
        let snap = materialize_snapshot_excluding(
            &self.store,
            &self.root,
            [Path::new(".sorrel")],
            options,
        )?;
        Ok(snap)
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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn init_and_snapshot_round_trip() {
        let dir = TempDir::new().unwrap();
        let (ws, initial) = Workspace::init(dir.path(), "repo_sdk").unwrap();
        std::fs::write(dir.path().join("hello.txt"), b"sdk\n").unwrap();
        let next = ws.snapshot_working_tree(initial.id, "add hello").unwrap();
        assert_ne!(next.id, initial.id);
        assert!(ws.store().has(&next.id).unwrap());
    }
}
