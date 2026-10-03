use crate::ObjectId;
use std::{
    collections::HashMap,
    fs,
    io::{self, Write},
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        Mutex,
    },
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

    fn create_tmp(&self, id: &ObjectId) -> ObjectStoreResult<(PathBuf, fs::File)> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        loop {
            let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let path = self
                .tmp_dir()
                .join(format!("{id}.{}.{sequence}.tmp", std::process::id()));
            match fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
            {
                Ok(file) => return Ok((path, file)),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(ObjectStoreError::io(path, error)),
            }
        }
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
        match self.read(&id) {
            Ok(_) => return Ok(id),
            Err(ObjectStoreError::NotFound(_)) => {}
            Err(error) => return Err(error),
        }

        let shard_dir = self.shard_dir(&id);
        fs::create_dir_all(&shard_dir)
            .map_err(|source| ObjectStoreError::io(&shard_dir, source))?;
        #[cfg(unix)]
        fs::File::open(self.objects_dir())
            .and_then(|dir| dir.sync_all())
            .map_err(|source| ObjectStoreError::io(self.objects_dir(), source))?;

        let (tmp_path, mut tmp_file) = self.create_tmp(&id)?;
        let result = (|| {
            tmp_file
                .write_all(bytes)
                .map_err(|source| ObjectStoreError::io(&tmp_path, source))?;
            tmp_file
                .sync_all()
                .map_err(|source| ObjectStoreError::io(&tmp_path, source))?;
            // Publish without replacing an existing immutable object. Writers of
            // identical bytes may race; every winner is still verified below.
            match fs::hard_link(&tmp_path, &path) {
                Ok(()) => {
                    #[cfg(unix)]
                    fs::File::open(&shard_dir)
                        .and_then(|dir| dir.sync_all())
                        .map_err(|source| ObjectStoreError::io(&shard_dir, source))?;
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(ObjectStoreError::io(&path, error)),
            }
            self.read(&id)?;
            Ok(id)
        })();
        drop(tmp_file);
        let cleanup = fs::remove_file(&tmp_path);
        match result {
            Ok(id) => {
                cleanup.map_err(|source| ObjectStoreError::io(&tmp_path, source))?;
                Ok(id)
            }
            Err(error) => Err(error),
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

    #[test]
    fn concurrent_writers_publish_complete_objects_without_shared_temporaries() {
        let temp_dir = tempfile::tempdir().unwrap();
        let store = FileObjectStore::new(temp_dir.path()).unwrap();
        let content = vec![42; 128 * 1024];
        let barrier = std::sync::Barrier::new(12);
        std::thread::scope(|scope| {
            for _ in 0..12 {
                scope.spawn(|| {
                    barrier.wait();
                    for _ in 0..8 {
                        let id = store.write(&content).unwrap();
                        assert_eq!(store.read(&id).unwrap(), content);
                    }
                });
            }
        });
        assert_eq!(count_files(&store.objects_dir()), 1);
        assert_eq!(count_files(&store.tmp_dir()), 0);
    }

    #[test]
    fn object_store_process_writer() {
        let Some(root) = std::env::var_os("SORREL_TEST_OBJECT_STORE") else {
            return;
        };
        let store = FileObjectStore::new(PathBuf::from(root)).unwrap();
        for _ in 0..32 {
            let id = store.write(&vec![42; 128 * 1024]).unwrap();
            assert_eq!(store.read(&id).unwrap(), vec![42; 128 * 1024]);
        }
    }

    #[test]
    fn process_writers_publish_without_truncating_each_others_temporaries() {
        let dir = tempfile::tempdir().unwrap();
        let store = FileObjectStore::new(dir.path()).unwrap();
        let exe = std::env::current_exe().unwrap();
        let mut children = Vec::new();
        for _ in 0..6 {
            children.push(
                std::process::Command::new(&exe)
                    .args([
                        "--exact",
                        "store::tests::object_store_process_writer",
                        "--quiet",
                    ])
                    .env("SORREL_TEST_OBJECT_STORE", dir.path())
                    .stdout(std::process::Stdio::null())
                    .spawn()
                    .unwrap(),
            );
        }
        for mut child in children {
            assert!(child.wait().unwrap().success());
        }
        assert_eq!(count_files(&store.objects_dir()), 1);
        assert_eq!(count_files(&store.tmp_dir()), 0);
    }

    #[test]
    fn rewriting_corrupt_object_fails_without_overwriting_it() {
        let temp_dir = tempfile::tempdir().unwrap();
        let store = FileObjectStore::new(temp_dir.path()).unwrap();
        let id = store.write(b"original").unwrap();
        fs::write(store.object_path(&id), b"corrupt").unwrap();
        assert!(matches!(
            store.write(b"original"),
            Err(ObjectStoreError::ContentMismatch { .. })
        ));
        assert_eq!(fs::read(store.object_path(&id)).unwrap(), b"corrupt");
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
