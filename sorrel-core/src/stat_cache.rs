//! Workspace stat cache for skipping re-hashing unchanged files during snapshots.
//!
//! The CLI (or another host) loads and saves the cache bytes; this module does
//! not hardcode `.sorrel/` paths. During tree materialization, each file's
//! size, mtime, and a platform-specific change fingerprint are compared to a
//! cached entry; on a match **and** a live object in the store, the cached blob
//! is reused without reading workspace file bytes.
//!
//! Unix fingerprints include device, inode, and ctime. Missing fingerprints,
//! unsupported platforms, and ctimes with only whole-second precision cause a
//! safe cache miss. Verification within the ctime second also misses, so writes
//! within a filesystem clock tick cannot seed a reusable entry. Filesystems
//! must update ctime on later writes; this is
//! an optimization for ordinary filesystem writes, not an integrity boundary
//! against forged filesystem metadata. Concurrent workspace mutation is not an
//! atomic snapshot and may require another snapshot after writers finish.
//!
//! # CLI integration example
//!
//! ```no_run
//! use sorrel_core::{
//!     materialize_snapshot_excluding_with_stat_cache, FileObjectStore, SnapshotOptions, StatCache,
//! };
//! use std::fs;
//!
//! # fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let store = FileObjectStore::new(".sorrel")?;
//! let cache_path = ".sorrel/stat-cache.json";
//! let mut stat_cache = if let Ok(bytes) = fs::read(cache_path) {
//!     StatCache::load(&bytes)?
//! } else {
//!     StatCache::new()
//! };
//!
//! let snapshot = materialize_snapshot_excluding_with_stat_cache(
//!     &store,
//!     ".",
//!     [".sorrel"],
//!     Some(&mut stat_cache),
//!     SnapshotOptions::new("my-repo"),
//! )?;
//!
//! let mut file = fs::File::create(cache_path)?;
//! stat_cache.save(&mut file)?;
//! # let _ = snapshot;
//! # Ok(())
//! # }
//! ```

use crate::{ObjectId, ObjectIdParseError};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{self, Read, Write},
};

const PROTOCOL_VERSION: &str = "sorrel.protocol.v0";

/// Result type used by stat-cache operations.
pub type StatCacheResult<T> = Result<T, StatCacheError>;

/// Errors returned while loading or saving a stat cache.
#[derive(Debug, thiserror::Error)]
pub enum StatCacheError {
    /// A cache file could not be read or written.
    #[error("stat cache I/O error: {0}")]
    Io(#[from] io::Error),

    /// The cache JSON could not be parsed or serialized.
    #[error(transparent)]
    Json(#[from] serde_json::Error),

    /// The cache had an unexpected protocol schema version.
    #[error("unsupported schema version {actual:?}; expected {expected:?}")]
    UnsupportedSchemaVersion {
        /// Expected protocol schema version.
        expected: &'static str,
        /// Actual protocol schema version.
        actual: String,
    },

    /// A cached object id was not valid hexadecimal.
    #[error("invalid object id {value:?}: {source}")]
    InvalidObjectId {
        /// Textual object ID value.
        value: String,
        /// Parse error.
        #[source]
        source: ObjectIdParseError,
    },
}

/// Cached filesystem metadata and blob object id for one workspace-relative path.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StatCacheEntry {
    /// File size in bytes at the time of the last hash.
    pub size: u64,
    /// Whole seconds since the UNIX epoch for `metadata().modified()`.
    pub mtime_secs: u64,
    /// Nanosecond fraction of `metadata().modified()`.
    pub mtime_nanos: u32,
    /// Content-addressed blob object id stored for this file.
    pub object_id: ObjectId,
}

/// Maps workspace-relative paths (UTF-8, `/` separators) to cached stat entries.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct StatCache {
    entries: BTreeMap<String, CachedEntry>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CachedEntry {
    data: StatCacheEntry,
    fingerprint: Option<ChangeFingerprint>,
}

impl StatCache {
    /// Creates an empty stat cache.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the cached entry for `path`, if any.
    ///
    /// `path` must use `/` separators and be relative to the workspace root
    /// (for example `src/lib.rs`).
    #[must_use]
    pub fn get(&self, path: &str) -> Option<&StatCacheEntry> {
        self.entries.get(path).map(|entry| &entry.data)
    }

    /// Inserts or replaces the cache entry for `path`.
    ///
    /// Manually inserted entries have no verified change fingerprint and cause
    /// a safe miss until a filesystem materialization refreshes them.
    pub fn insert(&mut self, path: impl Into<String>, entry: StatCacheEntry) {
        self.insert_verified(path.into(), entry, None);
    }

    /// Removes the cache entry for `path`, if present.
    pub fn remove(&mut self, path: &str) -> Option<StatCacheEntry> {
        self.entries.remove(path).map(|entry| entry.data)
    }

    /// Drops entries whose paths were not seen during the latest tree walk.
    pub fn retain(&mut self, paths_seen: &BTreeSet<String>) {
        self.entries.retain(|path, _| paths_seen.contains(path));
    }

    pub(crate) fn matches_fingerprint(&self, path: &str, metadata: &std::fs::Metadata) -> bool {
        ChangeFingerprint::from_metadata(metadata).is_some_and(|fingerprint| {
            self.entries
                .get(path)
                .and_then(|entry| entry.fingerprint.as_ref())
                == Some(&fingerprint)
        })
    }

    pub(crate) fn insert_verified(
        &mut self,
        path: String,
        entry: StatCacheEntry,
        fingerprint: Option<ChangeFingerprint>,
    ) {
        self.entries.insert(
            path,
            CachedEntry {
                data: entry,
                fingerprint,
            },
        );
    }

    /// Deserializes a stat cache from bytes.
    pub fn load(bytes: &[u8]) -> StatCacheResult<Self> {
        let stored: StoredStatCache = serde_json::from_slice(bytes)?;
        stored.into_cache()
    }

    /// Deserializes a stat cache from any reader.
    pub fn load_from_reader(mut reader: impl Read) -> StatCacheResult<Self> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes)?;
        Self::load(&bytes)
    }

    /// Serializes this cache to bytes.
    pub fn to_bytes(&self) -> StatCacheResult<Vec<u8>> {
        Ok(serde_json::to_vec(&StoredStatCache::from_cache(self))?)
    }

    /// Serializes this cache to any writer.
    pub fn save(&self, mut writer: impl Write) -> StatCacheResult<()> {
        writer.write_all(&self.to_bytes()?)?;
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ChangeFingerprint {
    device: u64,
    inode: u64,
    ctime_secs: i64,
    ctime_nanos: i64,
}

impl ChangeFingerprint {
    pub(crate) fn from_metadata(metadata: &std::fs::Metadata) -> Option<Self> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            let nanos = metadata.ctime_nsec();
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .ok()?;
            if !(1..1_000_000_000).contains(&nanos)
                || metadata.ino() == 0
                || i128::from(metadata.ctime()) >= i128::from(now.as_secs())
            {
                return None;
            }
            Some(Self {
                device: metadata.dev(),
                inode: metadata.ino(),
                ctime_secs: metadata.ctime(),
                ctime_nanos: nanos,
            })
        }
        #[cfg(not(unix))]
        {
            let _ = metadata;
            None
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredStatCache {
    schema_version: String,
    entries: BTreeMap<String, StoredStatCacheEntry>,
}

impl StoredStatCache {
    fn from_cache(cache: &StatCache) -> Self {
        Self {
            schema_version: PROTOCOL_VERSION.to_owned(),
            entries: cache
                .entries
                .iter()
                .map(|(path, entry)| {
                    (
                        path.clone(),
                        StoredStatCacheEntry::from_entry(&entry.data, entry.fingerprint.clone()),
                    )
                })
                .collect(),
        }
    }

    fn into_cache(self) -> StatCacheResult<StatCache> {
        if self.schema_version != PROTOCOL_VERSION {
            return Err(StatCacheError::UnsupportedSchemaVersion {
                expected: PROTOCOL_VERSION,
                actual: self.schema_version,
            });
        }

        let mut cache = StatCache::new();
        for (path, entry) in self.entries {
            let fingerprint = entry.change_fingerprint.clone();
            cache.insert_verified(path, entry.into_entry()?, fingerprint);
        }
        Ok(cache)
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredStatCacheEntry {
    size: u64,
    mtime_secs: u64,
    mtime_nanos: u32,
    object_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    change_fingerprint: Option<ChangeFingerprint>,
}

impl StoredStatCacheEntry {
    fn from_entry(entry: &StatCacheEntry, change_fingerprint: Option<ChangeFingerprint>) -> Self {
        Self {
            size: entry.size,
            mtime_secs: entry.mtime_secs,
            mtime_nanos: entry.mtime_nanos,
            object_id: entry.object_id.to_string(),
            change_fingerprint,
        }
    }

    fn into_entry(self) -> StatCacheResult<StatCacheEntry> {
        let object_id =
            self.object_id
                .parse()
                .map_err(|source| StatCacheError::InvalidObjectId {
                    value: self.object_id,
                    source,
                })?;

        Ok(StatCacheEntry {
            size: self.size,
            mtime_secs: self.mtime_secs,
            mtime_nanos: self.mtime_nanos,
            object_id,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        materialize_snapshot_excluding_with_stat_cache, InMemoryObjectStore, ObjectStore,
        SnapshotOptions,
    };
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[derive(Default)]
    struct CountingStore {
        inner: InMemoryObjectStore,
        blob_writes: AtomicUsize,
        after_blob_write: Option<Box<dyn Fn()>>,
    }

    impl ObjectStore for CountingStore {
        fn read(&self, id: &ObjectId) -> crate::ObjectStoreResult<Vec<u8>> {
            self.inner.read(id)
        }

        fn has(&self, id: &ObjectId) -> crate::ObjectStoreResult<bool> {
            self.inner.has(id)
        }

        fn write(&self, bytes: &[u8]) -> crate::ObjectStoreResult<ObjectId> {
            if bytes.starts_with(b"sorrel.blob.v0\n") {
                self.blob_writes.fetch_add(1, Ordering::Relaxed);
                if let Some(hook) = &self.after_blob_write {
                    hook();
                }
            }
            self.inner.write(bytes)
        }
    }

    #[test]
    fn round_trips_through_json() {
        let mut cache = StatCache::new();
        let object_id = ObjectId::for_bytes(b"blob");
        cache.insert(
            "src/lib.rs",
            StatCacheEntry {
                size: 42,
                mtime_secs: 1_700_000_000,
                mtime_nanos: 123_456_789,
                object_id,
            },
        );

        let bytes = cache.to_bytes().unwrap();
        let loaded = StatCache::load(&bytes).unwrap();
        assert_eq!(loaded, cache);

        let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(
            value.get("schemaVersion").and_then(|v| v.as_str()),
            Some(PROTOCOL_VERSION)
        );
    }

    #[test]
    fn retain_drops_unseen_paths() {
        let mut cache = StatCache::new();
        let id = ObjectId::for_bytes(b"x");
        let entry = StatCacheEntry {
            size: 1,
            mtime_secs: 1,
            mtime_nanos: 0,
            object_id: id,
        };
        cache.insert("keep.txt", entry.clone());
        cache.insert("drop.txt", entry);

        let mut seen = BTreeSet::new();
        seen.insert("keep.txt".to_owned());
        cache.retain(&seen);

        assert!(cache.get("keep.txt").is_some());
        assert!(cache.get("drop.txt").is_none());
    }

    #[test]
    fn cache_hit_skips_store_writes_on_unchanged_file() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("data.txt");
        std::fs::write(&file_path, b"unchanged content").unwrap();

        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            let metadata = std::fs::metadata(&file_path).unwrap();
            // A file verified in its ctime second intentionally cannot seed a
            // reusable entry: age it before testing actual read avoidance.
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
            while metadata.ctime_nsec() > 0
                && metadata.ino() > 0
                && i128::from(metadata.ctime())
                    >= i128::from(
                        std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap()
                            .as_secs(),
                    )
            {
                assert!(
                    std::time::Instant::now() < deadline,
                    "clock did not advance"
                );
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        }

        let store = CountingStore::default();
        let mut cache = StatCache::new();
        let options = SnapshotOptions::new("repo");

        materialize_snapshot_excluding_with_stat_cache(
            &store,
            temp_dir.path(),
            std::iter::empty::<&str>(),
            Some(&mut cache),
            options.clone(),
        )
        .unwrap();
        assert_eq!(store.blob_writes.load(Ordering::Relaxed), 1);
        // Reuse must survive the actual persistence boundary too.
        cache = StatCache::load(&cache.to_bytes().unwrap()).unwrap();
        let reusable =
            ChangeFingerprint::from_metadata(&std::fs::metadata(&file_path).unwrap()).is_some();

        materialize_snapshot_excluding_with_stat_cache(
            &store,
            temp_dir.path(),
            std::iter::empty::<&str>(),
            Some(&mut cache),
            options,
        )
        .unwrap();

        assert_eq!(
            store.blob_writes.load(Ordering::Relaxed),
            if reusable { 1 } else { 2 },
            "usable fingerprints skip blob writes; unsupported metadata rereads safely"
        );
        assert!(cache.get("data.txt").is_some());
    }

    #[test]
    fn legacy_cache_missing_fingerprint_rehashes_preserved_mtime_edit() {
        use std::fs::{File, FileTimes};
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("data.txt");
        std::fs::write(&path, b"old").unwrap();
        let mtime = std::fs::metadata(&path).unwrap().modified().unwrap();
        let store = CountingStore::default();
        let mut cache = StatCache::new();
        let materialize = |cache: &mut StatCache| {
            materialize_snapshot_excluding_with_stat_cache(
                &store,
                dir.path(),
                std::iter::empty::<&str>(),
                Some(cache),
                SnapshotOptions::new("repo"),
            )
            .unwrap()
        };
        materialize(&mut cache);
        let mut legacy: serde_json::Value =
            serde_json::from_slice(&cache.to_bytes().unwrap()).unwrap();
        legacy["entries"]["data.txt"]
            .as_object_mut()
            .unwrap()
            .remove("changeFingerprint");
        cache = StatCache::load(&serde_json::to_vec(&legacy).unwrap()).unwrap();
        assert!(
            cache.get("data.txt").is_some(),
            "old cache remains readable"
        );
        std::fs::write(&path, b"new").unwrap();
        File::open(&path)
            .unwrap()
            .set_times(FileTimes::new().set_modified(mtime))
            .unwrap();
        materialize(&mut cache);
        assert_eq!(store.blob_writes.load(Ordering::Relaxed), 2);
        let blob = crate::read_blob(&store, &cache.get("data.txt").unwrap().object_id).unwrap();
        assert_eq!(blob.content, b"new");
    }

    #[test]
    fn public_insert_invalidates_private_fingerprint() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("data.txt");
        std::fs::write(&path, b"content").unwrap();
        let metadata = std::fs::metadata(&path).unwrap();
        let mut cache = StatCache::new();
        materialize_snapshot_excluding_with_stat_cache(
            &InMemoryObjectStore::new(),
            dir.path(),
            std::iter::empty::<&str>(),
            Some(&mut cache),
            SnapshotOptions::new("repo"),
        )
        .unwrap();
        let entry = cache.get("data.txt").unwrap().clone();
        cache.insert("data.txt", entry);
        assert!(!cache.matches_fingerprint("data.txt", &metadata));
    }

    #[cfg(unix)]
    #[test]
    fn changed_during_read_does_not_seed_reusable_fingerprint() {
        use std::os::unix::fs::MetadataExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("data.txt");
        std::fs::write(&path, b"old").unwrap();
        let metadata = std::fs::metadata(&path).unwrap();
        if !(1..1_000_000_000).contains(&metadata.ctime_nsec()) || metadata.ino() == 0 {
            // This regression requires a reusable Unix fingerprint.
            return;
        }
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while ChangeFingerprint::from_metadata(&metadata).is_none() {
            assert!(
                std::time::Instant::now() < deadline,
                "clock did not advance"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let changed_path = path.clone();
        let mtime = std::fs::metadata(&path).unwrap().modified().unwrap();
        let did_change = std::sync::atomic::AtomicBool::new(false);
        let store = CountingStore {
            after_blob_write: Some(Box::new(move || {
                if !did_change.swap(true, Ordering::Relaxed) {
                    std::fs::write(&changed_path, b"new").unwrap();
                    std::fs::File::open(&changed_path)
                        .unwrap()
                        .set_times(std::fs::FileTimes::new().set_modified(mtime))
                        .unwrap();
                }
            })),
            ..CountingStore::default()
        };
        let mut cache = StatCache::new();
        let materialize = |cache: &mut StatCache| {
            materialize_snapshot_excluding_with_stat_cache(
                &store,
                dir.path(),
                std::iter::empty::<&str>(),
                Some(cache),
                SnapshotOptions::new("repo"),
            )
            .unwrap()
        };
        materialize(&mut cache);
        assert!(
            cache.entries["data.txt"].fingerprint.is_none(),
            "a file changed during verification must not persist a reusable fingerprint"
        );
        assert!(!cache.matches_fingerprint("data.txt", &std::fs::metadata(&path).unwrap()));
        materialize(&mut cache);
        let blob = crate::read_blob(&store, &cache.get("data.txt").unwrap().object_id).unwrap();
        assert_eq!(blob.content, b"new");
        assert_eq!(store.blob_writes.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn size_change_triggers_rehash() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("data.txt");
        std::fs::write(&file_path, b"short").unwrap();

        let store = InMemoryObjectStore::new();
        let mut cache = StatCache::new();
        let options = SnapshotOptions::new("repo");

        let first = materialize_snapshot_excluding_with_stat_cache(
            &store,
            temp_dir.path(),
            std::iter::empty::<&str>(),
            Some(&mut cache),
            options.clone(),
        )
        .unwrap();
        let first_blob = cache.get("data.txt").unwrap().object_id;

        std::fs::write(&file_path, b"much longer content now").unwrap();

        let second = materialize_snapshot_excluding_with_stat_cache(
            &store,
            temp_dir.path(),
            std::iter::empty::<&str>(),
            Some(&mut cache),
            options,
        )
        .unwrap();
        let second_blob = cache.get("data.txt").unwrap().object_id;

        assert_ne!(first_blob, second_blob);
        assert_ne!(first.id, second.id);
    }

    #[test]
    fn mtime_change_triggers_rehash() {
        use std::fs::{File, FileTimes};
        use std::time::{Duration, SystemTime};

        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("data.txt");
        std::fs::write(&file_path, b"same bytes").unwrap();

        let store = InMemoryObjectStore::new();
        let mut cache = StatCache::new();
        let options = SnapshotOptions::new("repo");

        materialize_snapshot_excluding_with_stat_cache(
            &store,
            temp_dir.path(),
            std::iter::empty::<&str>(),
            Some(&mut cache),
            options.clone(),
        )
        .unwrap();
        let cached_mtime = cache.get("data.txt").unwrap().mtime_secs;

        let past = SystemTime::now() - Duration::from_secs(3600);
        File::open(&file_path)
            .unwrap()
            .set_times(FileTimes::new().set_modified(past))
            .unwrap();

        materialize_snapshot_excluding_with_stat_cache(
            &store,
            temp_dir.path(),
            std::iter::empty::<&str>(),
            Some(&mut cache),
            options,
        )
        .unwrap();

        let refreshed_mtime = cache.get("data.txt").unwrap().mtime_secs;
        assert_ne!(cached_mtime, refreshed_mtime);
    }

    #[test]
    fn deleted_file_removes_cache_entry_and_rehash_on_readd() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("gone.txt");
        std::fs::write(&file_path, b"first").unwrap();

        let store = InMemoryObjectStore::new();
        let mut cache = StatCache::new();
        let options = SnapshotOptions::new("repo");

        materialize_snapshot_excluding_with_stat_cache(
            &store,
            temp_dir.path(),
            std::iter::empty::<&str>(),
            Some(&mut cache),
            options.clone(),
        )
        .unwrap();
        assert!(cache.get("gone.txt").is_some());

        std::fs::remove_file(&file_path).unwrap();
        materialize_snapshot_excluding_with_stat_cache(
            &store,
            temp_dir.path(),
            std::iter::empty::<&str>(),
            Some(&mut cache),
            options.clone(),
        )
        .unwrap();
        assert!(cache.get("gone.txt").is_none());

        std::fs::write(&file_path, b"second").unwrap();
        materialize_snapshot_excluding_with_stat_cache(
            &store,
            temp_dir.path(),
            std::iter::empty::<&str>(),
            Some(&mut cache),
            options,
        )
        .unwrap();
        let entry = cache.get("gone.txt").expect("re-added file is cached");
        let blob = crate::read_blob(&store, &entry.object_id).unwrap();
        assert_eq!(blob.content, b"second");
    }

    #[test]
    fn missing_cached_object_rehashes_and_refreshes_entry() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("data.txt");
        std::fs::write(&file_path, b"content").unwrap();

        let store = InMemoryObjectStore::new();
        let mut cache = StatCache::new();
        let options = SnapshotOptions::new("repo");

        materialize_snapshot_excluding_with_stat_cache(
            &store,
            temp_dir.path(),
            std::iter::empty::<&str>(),
            Some(&mut cache),
            options.clone(),
        )
        .unwrap();
        let stale_id = cache.get("data.txt").unwrap().object_id;

        cache.entries.get_mut("data.txt").unwrap().data.object_id =
            ObjectId::from_bytes([0xAA; 32]);
        assert!(!store.has(&ObjectId::from_bytes([0xAA; 32])).unwrap());

        materialize_snapshot_excluding_with_stat_cache(
            &store,
            temp_dir.path(),
            std::iter::empty::<&str>(),
            Some(&mut cache),
            options,
        )
        .unwrap();

        let refreshed = cache.get("data.txt").unwrap();
        assert_ne!(refreshed.object_id, ObjectId::from_bytes([0xAA; 32]));
        assert_eq!(refreshed.object_id, stale_id);
        assert!(store.has(&refreshed.object_id).unwrap());
    }
}
