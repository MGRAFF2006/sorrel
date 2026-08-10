//! Workspace stat cache for skipping re-hashing unchanged files during snapshots.
//!
//! The CLI (or another host) loads and saves the cache bytes; this module does
//! not hardcode `.sorrel/` paths. During tree materialization, each file's
//! `(size, mtime)` is compared to a cached entry; on a match **and** a live
//! object in the store, the cached blob id is reused without reading file
//! bytes from disk.
//!
//! # mtime granularity
//!
//! Entries use [`std::fs::Metadata::modified`]. On filesystems with
//! one-second resolution, two edits within the same second that keep the same
//! size may not be detected (acceptable for v0).
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
    entries: BTreeMap<String, StatCacheEntry>,
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
        self.entries.get(path)
    }

    /// Inserts or replaces the cache entry for `path`.
    pub fn insert(&mut self, path: impl Into<String>, entry: StatCacheEntry) {
        self.entries.insert(path.into(), entry);
    }

    /// Removes the cache entry for `path`, if present.
    pub fn remove(&mut self, path: &str) -> Option<StatCacheEntry> {
        self.entries.remove(path)
    }

    /// Drops entries whose paths were not seen during the latest tree walk.
    pub fn retain(&mut self, paths_seen: &BTreeSet<String>) {
        self.entries.retain(|path, _| paths_seen.contains(path));
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
                .map(|(path, entry)| (path.clone(), StoredStatCacheEntry::from_entry(entry)))
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

        let entries = self
            .entries
            .into_iter()
            .map(|(path, entry)| entry.into_entry().map(|e| (path, e)))
            .collect::<StatCacheResult<BTreeMap<_, _>>>()?;

        Ok(StatCache { entries })
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredStatCacheEntry {
    size: u64,
    mtime_secs: u64,
    mtime_nanos: u32,
    object_id: String,
}

impl StoredStatCacheEntry {
    fn from_entry(entry: &StatCacheEntry) -> Self {
        Self {
            size: entry.size,
            mtime_secs: entry.mtime_secs,
            mtime_nanos: entry.mtime_nanos,
            object_id: entry.object_id.to_string(),
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
        let writes_after_first = store.len();

        materialize_snapshot_excluding_with_stat_cache(
            &store,
            temp_dir.path(),
            std::iter::empty::<&str>(),
            Some(&mut cache),
            options,
        )
        .unwrap();

        assert_eq!(
            store.len(),
            writes_after_first,
            "unchanged file should reuse cached blob without new store objects"
        );
        assert!(cache.get("data.txt").is_some());
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

        cache.insert(
            "data.txt",
            StatCacheEntry {
                size: std::fs::metadata(&file_path).unwrap().len(),
                mtime_secs: file_mtime(&file_path).0,
                mtime_nanos: file_mtime(&file_path).1,
                object_id: ObjectId::from_bytes([0xAA; 32]),
            },
        );
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

    fn file_mtime(path: &std::path::Path) -> (u64, u32) {
        use std::time::UNIX_EPOCH;

        let modified = std::fs::metadata(path)
            .unwrap()
            .modified()
            .unwrap()
            .duration_since(UNIX_EPOCH)
            .unwrap();
        (modified.as_secs(), modified.subsec_nanos())
    }
}
