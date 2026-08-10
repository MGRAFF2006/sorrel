use crate::ObjectId;
use std::{
    collections::HashMap,
    fs,
    io::{self, Write},
    path::PathBuf,
    sync::Mutex,
};

/// Result type used by Sorrel object stores.
pub type ObjectStoreResult<T> = Result<T, ObjectStoreError>;

/// Content-addressed object storage.
pub trait ObjectStore {
    /// Reads the bytes for `id`.
    fn read(&self, id: &ObjectId) -> ObjectStoreResult<Vec<u8>>;

    /// Writes `bytes` and returns their content-derived object ID.
    ///
    /// Rewriting the same bytes is idempotent and should not create duplicate
    /// stored objects.
    fn write(&self, bytes: &[u8]) -> ObjectStoreResult<ObjectId>;

    /// Returns whether `id` exists in this store.
    fn has(&self, id: &ObjectId) -> ObjectStoreResult<bool>;
}

/// Errors returned by object stores.
#[derive(Debug, thiserror::Error)]
pub enum ObjectStoreError {
    /// The requested object does not exist.
    #[error("object {0} not found")]
    NotFound(ObjectId),

    /// A filesystem operation failed.
    #[error("object store I/O error at {}: {source}", path.display())]
    Io {
        /// Path involved in the failing operation.
        path: PathBuf,
        /// Original I/O error.
        #[source]
        source: io::Error,
    },

    /// Stored bytes did not match their requested content address.
    #[error("object {expected} content digest mismatch: found {actual}")]
    ContentMismatch {
        /// Object ID requested by the caller.
        expected: ObjectId,
        /// Object ID computed from the bytes that were read.
        actual: ObjectId,
    },
}

impl ObjectStoreError {
    fn io(path: impl Into<PathBuf>, source: io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }
}

/// Volatile in-memory object store for tests, agents, and ephemeral workspaces.
#[derive(Debug, Default)]
pub struct InMemoryObjectStore {
    objects: Mutex<HashMap<ObjectId, Vec<u8>>>,
}

impl InMemoryObjectStore {
    /// Creates an empty in-memory object store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the number of unique objects currently stored.
    #[must_use]
    pub fn len(&self) -> usize {
        self.objects
            .lock()
            .expect("object store mutex poisoned")
            .len()
    }

    /// Returns true when the store contains no objects.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl ObjectStore for InMemoryObjectStore {
    fn read(&self, id: &ObjectId) -> ObjectStoreResult<Vec<u8>> {
        self.objects
            .lock()
            .expect("object store mutex poisoned")
            .get(id)
            .cloned()
            .ok_or(ObjectStoreError::NotFound(*id))
    }

    fn write(&self, bytes: &[u8]) -> ObjectStoreResult<ObjectId> {
        let id = ObjectId::for_bytes(bytes);
        self.objects
            .lock()
            .expect("object store mutex poisoned")
            .entry(id)
            .or_insert_with(|| bytes.to_vec());
        Ok(id)
    }

    fn has(&self, id: &ObjectId) -> ObjectStoreResult<bool> {
        Ok(self
            .objects
            .lock()
            .expect("object store mutex poisoned")
            .contains_key(id))
    }
}

/// Filesystem-backed object store.
///
/// Objects are stored below `<root>/objects` using a two-character fanout
/// directory derived from each object's hexadecimal ID.
#[derive(Debug, Clone)]
pub struct FileObjectStore {
    root: PathBuf,
}

impl FileObjectStore {
    /// Opens or creates a filesystem object store rooted at `root`.
    pub fn new(root: impl Into<PathBuf>) -> ObjectStoreResult<Self> {
        let root = root.into();
        let objects_dir = root.join("objects");
        let tmp_dir = root.join("tmp");

        fs::create_dir_all(&objects_dir)
            .map_err(|source| ObjectStoreError::io(&objects_dir, source))?;
        fs::create_dir_all(&tmp_dir).map_err(|source| ObjectStoreError::io(&tmp_dir, source))?;

        Ok(Self { root })
    }

    fn objects_dir(&self) -> PathBuf {
        self.root.join("objects")
    }

    fn tmp_dir(&self) -> PathBuf {
        self.root.join("tmp")
    }

    fn object_path(&self, id: &ObjectId) -> PathBuf {
        let hex = id.to_string();
        self.objects_dir().join(&hex[..2]).join(&hex[2..])
    }

    fn shard_dir(&self, id: &ObjectId) -> PathBuf {
        let hex = id.to_string();
        self.objects_dir().join(&hex[..2])
    }

    fn tmp_path(&self, id: &ObjectId) -> PathBuf {
        self.tmp_dir().join(format!("{id}.tmp"))
    }
}

impl ObjectStore for FileObjectStore {
    fn read(&self, id: &ObjectId) -> ObjectStoreResult<Vec<u8>> {
        let path = self.object_path(id);
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Err(ObjectStoreError::NotFound(*id));
            }
            Err(error) => return Err(ObjectStoreError::io(path, error)),
        };

        let actual = ObjectId::for_bytes(&bytes);
        if actual != *id {
            return Err(ObjectStoreError::ContentMismatch {
                expected: *id,
                actual,
            });
        }

        Ok(bytes)
    }

    fn write(&self, bytes: &[u8]) -> ObjectStoreResult<ObjectId> {
        let id = ObjectId::for_bytes(bytes);
        let path = self.object_path(&id);
        if path.exists() {
            return Ok(id);
        }

        let shard_dir = self.shard_dir(&id);
        fs::create_dir_all(&shard_dir)
            .map_err(|source| ObjectStoreError::io(&shard_dir, source))?;

        let tmp_path = self.tmp_path(&id);
        {
            let mut tmp_file = fs::File::create(&tmp_path)
                .map_err(|source| ObjectStoreError::io(&tmp_path, source))?;
            tmp_file
                .write_all(bytes)
                .map_err(|source| ObjectStoreError::io(&tmp_path, source))?;
            tmp_file
                .sync_all()
                .map_err(|source| ObjectStoreError::io(&tmp_path, source))?;
        }

        match fs::rename(&tmp_path, &path) {
            Ok(()) => Ok(id),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                let _ = fs::remove_file(&tmp_path);
                Ok(id)
            }
            Err(error) => {
                let _ = fs::remove_file(&tmp_path);
                Err(ObjectStoreError::io(path, error))
            }
        }
    }

    fn has(&self, id: &ObjectId) -> ObjectStoreResult<bool> {
        let path = self.object_path(id);
        match fs::metadata(&path) {
            Ok(metadata) => Ok(metadata.is_file()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(ObjectStoreError::io(path, error)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn assert_content_addressed_store(store: &impl ObjectStore) {
        let bytes = b"hello from sorrel";
        let expected = ObjectId::for_bytes(bytes);

        let id = store.write(bytes).unwrap();

        assert_eq!(id, expected);
        assert!(store.has(&id).unwrap());
        assert_eq!(store.read(&id).unwrap(), bytes);
    }

    fn assert_missing_read(store: &impl ObjectStore) {
        let missing = ObjectId::for_bytes(b"not stored");

        assert!(!store.has(&missing).unwrap());
        assert!(matches!(
            store.read(&missing).unwrap_err(),
            ObjectStoreError::NotFound(id) if id == missing
        ));
    }

    #[test]
    fn in_memory_store_reads_and_writes_by_content_id() {
        let store = InMemoryObjectStore::new();

        assert_content_addressed_store(&store);
    }

    #[test]
    fn in_memory_store_deduplicates_equal_content() {
        let store = InMemoryObjectStore::new();

        let first = store.write(b"same bytes").unwrap();
        let second = store.write(b"same bytes").unwrap();

        assert_eq!(first, second);
        assert_eq!(store.len(), 1);
    }

    #[test]
    fn in_memory_store_reports_missing_objects() {
        assert_missing_read(&InMemoryObjectStore::new());
    }

    #[test]
    fn filesystem_store_reads_and_writes_by_content_id() {
        let temp_dir = tempfile::tempdir().unwrap();
        let store = FileObjectStore::new(temp_dir.path()).unwrap();

        assert_content_addressed_store(&store);
    }

    #[test]
    fn filesystem_store_deduplicates_equal_content() {
        let temp_dir = tempfile::tempdir().unwrap();
        let store = FileObjectStore::new(temp_dir.path()).unwrap();

        let first = store.write(b"same bytes").unwrap();
        let second = store.write(b"same bytes").unwrap();
        let path = store.object_path(&first);

        assert_eq!(first, second);
        assert!(path.is_file());
        assert_eq!(count_files(temp_dir.path().join("objects").as_path()), 1);
    }

    #[test]
    fn filesystem_store_reports_missing_objects() {
        let temp_dir = tempfile::tempdir().unwrap();
        let store = FileObjectStore::new(temp_dir.path()).unwrap();

        assert_missing_read(&store);
    }

    #[test]
    fn filesystem_store_rejects_corrupt_object_bytes() {
        let temp_dir = tempfile::tempdir().unwrap();
        let store = FileObjectStore::new(temp_dir.path()).unwrap();
        let id = store.write(b"original").unwrap();
        let path = store.object_path(&id);

        fs::write(&path, b"corrupt").unwrap();

        assert!(matches!(
            store.read(&id).unwrap_err(),
            ObjectStoreError::ContentMismatch { expected, actual }
                if expected == id && actual == ObjectId::for_bytes(b"corrupt")
        ));
    }

    fn count_files(path: &Path) -> usize {
        fs::read_dir(path)
            .unwrap()
            .map(|entry| {
                let entry = entry.unwrap();
                if entry.file_type().unwrap().is_dir() {
                    count_files(&entry.path())
                } else {
                    1
                }
            })
            .sum()
    }
}
