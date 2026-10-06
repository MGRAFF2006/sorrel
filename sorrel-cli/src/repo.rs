//! On-disk repository layout helpers for the Sorrel prototype.
//!
//! A Sorrel workspace lives in a `.sorrel/` directory next to the working
//! tree:
//!
//! ```text
//! .sorrel/
//!   objects/        content-addressed object store (FileObjectStore root)
//!   slices/         persisted slice manifests (existing feature)
//!   lanes/          lane registry (one JSON object per lane id)
//!   heads/          per-lane head snapshot pointers (one file per lane id)
//!   manifest.json   repo identity + creation metadata + default lane
//!   HEAD            current lane + head snapshot pointer (atomically written)
//!   remotes.json    configured sync remotes (name -> url + repoId)
//!   changes.index   JSON-lines snapshot → change id map (append-only)
//!   git-map.json    Git SHA → snapshot/change map (from `sorrel git import`)
//!   MERGE_STATE     in-progress conflicted merge (MergeResult id)
//! ```
//!
//! `manifest.json` schema:
//!
//! ```json
//! {
//!   "schemaVersion": "sorrel.protocol.v0",
//!   "kind": "Workspace",
//!   "repoId": "repo_<hex>",
//!   "createdAt": "2026-06-26T12:00:00Z",
//!   "defaultLane": { "id": "lane_main", "name": "main" }
//! }
//! ```
//!
//! `HEAD` schema:
//!
//! ```json
//! { "lane": "lane_main", "snapshot": "<64-hex object id>" }
//! ```

use std::collections::BTreeMap;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};

/// Name of the workspace metadata directory.
pub const SORREL_DIR: &str = ".sorrel";
/// Slices subdirectory (existing slice feature).
pub const SLICES_DIR: &str = "slices";
/// Lanes subdirectory (persisted lane registry).
pub const LANES_DIR: &str = "lanes";
/// Stacks subdirectory (persisted stack registry).
pub const STACKS_DIR: &str = "stacks";
/// Per-lane head pointers subdirectory (one file per lane id).
pub const HEADS_DIR: &str = "heads";
/// Grants subdirectory (persisted grant PolicyChange documents).
pub const GRANTS_DIR: &str = "grants";
/// Secrets subdirectory (persisted SecretRef declarations).
pub const SECRETS_DIR: &str = "secrets";
/// Workspace manifest filename.
pub const MANIFEST_FILE: &str = "manifest.json";
/// HEAD pointer filename.
pub const HEAD_FILE: &str = "HEAD";
/// Remotes configuration filename.
pub const REMOTES_FILE: &str = "remotes.json";
/// Stat-cache filename (size+mtime -> blob id, to skip re-hashing unchanged files).
pub const STAT_CACHE_FILE: &str = "stat-cache.json";
/// Snapshot → change id index (JSON lines).
pub const CHANGES_INDEX_FILE: &str = "changes.index";
/// Git SHA → Sorrel snapshot id map written by `sorrel git import`.
pub const GIT_MAP_FILE: &str = "git-map.json";
/// In-progress conflicted merge state (stores MergeResult object id).
pub const MERGE_STATE_FILE: &str = "MERGE_STATE";
/// Default remote name when none is specified on push/pull.
pub const DEFAULT_REMOTE_NAME: &str = "origin";

/// Default lane identifier and display name for a freshly initialized repo.
pub const DEFAULT_LANE_ID: &str = "lane_main";
/// Default lane display name.
pub const DEFAULT_LANE_NAME: &str = "main";

/// Protocol schema version stamped into persisted objects.
pub const PROTOCOL_VERSION: &str = "sorrel.protocol.v0";

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);
const TRANSACTION_FILE: &str = "metadata-transaction.json";

/// Exclusive command-level guard for workspace reads and mutations.
///
/// The operating system releases this advisory lock when the process exits.
pub struct WorkspaceLock {
    _file: fs::File,
}

impl WorkspaceLock {
    /// Acquire the workspace guard and finish any interrupted metadata commit.
    pub fn acquire(root: &Path) -> io::Result<Self> {
        let _ = load_manifest_at(&root.join(MANIFEST_FILE))?;
        fs::create_dir_all(root)?;
        let path = root.join("write.lock");
        let file = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)?;
        fs2::FileExt::try_lock_exclusive(&file).map_err(|error| {
            if error.kind() == io::ErrorKind::WouldBlock
                || error.raw_os_error() == fs2::lock_contended_error().raw_os_error()
            {
                io::Error::new(
                    io::ErrorKind::WouldBlock,
                    format!(
                        "workspace is busy ({}); retry after the current command finishes",
                        path.display()
                    ),
                )
            } else {
                error
            }
        })?;
        let guard = Self { _file: file };
        // Recheck under the lock before interpreting any recovery journal.
        let _ = load_manifest_at(&root.join(MANIFEST_FILE))?;
        sorrel_core::durability::flush_root_ancestors(root)?;
        recover_metadata_transaction(root)?;
        Ok(guard)
    }
}

/// A metadata rename/unlink succeeded, but its directory barrier failed.
/// The visible state may already be committed; retain pending journal data.
#[derive(Debug)]
pub struct MetadataDurabilityUncertain {
    path: PathBuf,
    source: io::Error,
}

impl std::fmt::Display for MetadataDurabilityUncertain {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "metadata publication at {} may already have changed; durability is uncertain: {}",
            self.path.display(),
            self.source
        )
    }
}
impl std::error::Error for MetadataDurabilityUncertain {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.source)
    }
}
fn publication_uncertain(path: &Path, source: io::Error) -> io::Error {
    io::Error::new(
        source.kind(),
        MetadataDurabilityUncertain {
            path: path.to_owned(),
            source,
        },
    )
}
fn is_publication_uncertain(error: &io::Error) -> bool {
    error
        .get_ref()
        .is_some_and(|inner| inner.is::<MetadataDurabilityUncertain>())
}
fn metadata_root(path: &Path) -> &Path {
    path.ancestors()
        .skip(1)
        .find(|ancestor| ancestor.file_name().is_some_and(|name| name == SORREL_DIR))
        .unwrap_or_else(|| path.parent().unwrap_or_else(|| Path::new(".")))
}

#[cfg(test)]
fn stage_bytes(path: &Path, bytes: &[u8]) -> io::Result<PathBuf> {
    stage_bytes_with_flush(
        metadata_root(path),
        path,
        bytes,
        &sorrel_core::durability::flush_directory_chain,
    )
}

fn stage_bytes_with_flush(
    root: &Path,
    path: &Path,
    bytes: &[u8],
    flush: &dyn Fn(&Path, &Path) -> io::Result<()>,
) -> io::Result<PathBuf> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    loop {
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let temporary = parent.join(format!(".sorrel-{}-{sequence}.tmp", std::process::id()));
        let file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary);
        let mut file = match file {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        };
        if let Err(error) = file
            .write_all(bytes)
            .and_then(|()| file.sync_all())
            .and_then(|()| flush(parent, root))
        {
            drop(file);
            let _ = fs::remove_file(&temporary);
            return Err(error);
        }
        return Ok(temporary);
    }
}

fn write_bytes_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    write_bytes_atomic_with_flush(
        metadata_root(path),
        path,
        bytes,
        &sorrel_core::durability::flush_directory_chain,
    )
}

fn write_bytes_atomic_with_flush(
    root: &Path,
    path: &Path,
    bytes: &[u8],
    flush: &dyn Fn(&Path, &Path) -> io::Result<()>,
) -> io::Result<()> {
    let temporary = stage_bytes_with_flush(root, path, bytes, flush)?;
    if let Err(error) = fs::rename(&temporary, path) {
        let _ = fs::remove_file(&temporary);
        return Err(error);
    }
    flush(path.parent().unwrap_or_else(|| Path::new(".")), root)
        .map_err(|source| publication_uncertain(path, source))
}

fn metadata_transaction(root: &Path, updates: &[(PathBuf, Vec<u8>)]) -> io::Result<()> {
    metadata_transaction_with_flush(
        root,
        updates,
        &sorrel_core::durability::flush_directory_chain,
    )
}

fn metadata_transaction_with_flush(
    root: &Path,
    updates: &[(PathBuf, Vec<u8>)],
    flush: &dyn Fn(&Path, &Path) -> io::Result<()>,
) -> io::Result<()> {
    let journal = root.join(TRANSACTION_FILE);
    if journal.exists() {
        return Err(io::Error::other(
            "pending metadata transaction; acquire the workspace lock to recover it",
        ));
    }
    let mut staged = Vec::new();
    let preparation = (|| {
        for (target, bytes) in updates {
            // Refuse known invalid targets before any visible metadata changes.
            if target.exists() && !target.is_file() {
                return Err(io::Error::other(format!(
                    "metadata target is not a file: {}",
                    target.display()
                )));
            }
            let temporary = stage_bytes_with_flush(root, target, bytes, flush)?;
            staged.push((
                target.strip_prefix(root).unwrap().to_path_buf(),
                temporary.strip_prefix(root).unwrap().to_path_buf(),
                sorrel_core::ObjectId::for_bytes(bytes).to_hex(),
            ));
        }
        let mut bytes = serde_json::to_vec_pretty(&staged)?;
        bytes.push(b'\n');
        write_bytes_atomic_with_flush(root, &journal, &bytes, flush)
    })();
    if let Err(error) = preparation {
        if !is_publication_uncertain(&error) {
            for (_, temporary, _) in &staged {
                let _ = fs::remove_file(root.join(temporary));
            }
        }
        return Err(error);
    }
    recover_metadata_transaction_with_flush(root, flush)
}

fn recover_metadata_transaction(root: &Path) -> io::Result<()> {
    recover_metadata_transaction_with_flush(root, &sorrel_core::durability::flush_directory_chain)
}

fn recover_metadata_transaction_with_flush(
    root: &Path,
    flush: &dyn Fn(&Path, &Path) -> io::Result<()>,
) -> io::Result<()> {
    let journal = root.join(TRANSACTION_FILE);
    let bytes = match fs::read(&journal) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    let entries: Vec<(PathBuf, PathBuf, String)> = serde_json::from_slice(&bytes)?;
    for (target, temporary, _) in &entries {
        let valid_target = target == Path::new(HEAD_FILE)
            || target == Path::new(CHANGES_INDEX_FILE)
            || (target.parent() == Some(Path::new(HEADS_DIR))
                && target
                    .file_name()
                    .is_some_and(|name| name != "." && name != ".."));
        if !valid_target
            || temporary.parent() != target.parent()
            || !temporary.file_name().is_some_and(|name| {
                name.to_string_lossy().starts_with(".sorrel-")
                    && name.to_string_lossy().ends_with(".tmp")
            })
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid metadata transaction path",
            ));
        }
    }
    // A journal rename may have succeeded before its barrier failed.
    flush(root, root).map_err(|source| publication_uncertain(&journal, source))?;
    let mut snapshots = std::collections::BTreeSet::new();
    let mut changes = std::collections::BTreeSet::new();
    for (target, temporary, digest) in &entries {
        let target = root.join(target);
        let temporary = root.join(temporary);
        let data = fs::read(if temporary.is_file() {
            &temporary
        } else {
            &target
        })?;
        if sorrel_core::ObjectId::for_bytes(&data).to_hex() != *digest {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "metadata transaction is missing staged data",
            ));
        }
        if target.file_name().is_some_and(|name| name == HEAD_FILE)
            || target.parent() == Some(root.join(HEADS_DIR).as_path())
        {
            if let Ok(value) = serde_json::from_slice::<Value>(&data) {
                if let Some(snapshot) = value.get("snapshot").and_then(Value::as_str) {
                    snapshots.insert(snapshot.to_owned());
                }
            }
        }
        if target
            .file_name()
            .is_some_and(|name| name == CHANGES_INDEX_FILE)
        {
            for line in String::from_utf8_lossy(&data).lines() {
                if let Ok(value) = serde_json::from_str::<Value>(line) {
                    if let Some(snapshot) = value.get("snapshot").and_then(Value::as_str) {
                        snapshots.insert(snapshot.to_owned());
                    }
                    if let Some(change) = value.get("change").and_then(Value::as_str) {
                        changes.insert(change.to_owned());
                    }
                }
            }
        }
    }
    if !snapshots.is_empty() || !changes.is_empty() {
        let snapshots: Vec<_> = snapshots.iter().map(String::as_str).collect();
        let changes: Vec<_> = changes.iter().map(String::as_str).collect();
        flush_core_references(root, &snapshots, &changes)?;
    }
    for (target, temporary, _) in entries {
        let target = root.join(target);
        let temporary = root.join(temporary);
        let data_path = if temporary.is_file() {
            &temporary
        } else {
            &target
        };
        #[cfg(unix)]
        let file = fs::File::open(data_path)?;
        #[cfg(not(unix))]
        let file = fs::OpenOptions::new().write(true).open(data_path)?;
        file.sync_all()?;
        if temporary.is_file() {
            fs::rename(&temporary, &target)?;
        }
        flush(target.parent().unwrap(), root)
            .map_err(|source| publication_uncertain(&target, source))?;
    }
    fs::remove_file(&journal)?;
    flush(root, root).map_err(|source| publication_uncertain(&journal, source))
}

/// Absolute-ish path to the `.sorrel` directory rooted at the current dir.
#[must_use]
pub fn sorrel_dir() -> PathBuf {
    PathBuf::from(SORREL_DIR)
}

/// Root passed to `FileObjectStore`, which creates `objects/` and `tmp/`
/// subdirectories beneath it. We use `.sorrel` itself so the store lives at
/// `.sorrel/objects` and `.sorrel/tmp`.
#[must_use]
pub fn object_store_root() -> PathBuf {
    sorrel_dir()
}

/// Path to the manifest file.
#[must_use]
pub fn manifest_path() -> PathBuf {
    sorrel_dir().join(MANIFEST_FILE)
}

/// Path to the HEAD pointer file.
#[must_use]
pub fn head_path() -> PathBuf {
    sorrel_dir().join(HEAD_FILE)
}

/// Path to the per-lane heads directory (`.sorrel/heads/`).
#[must_use]
pub fn heads_dir() -> PathBuf {
    sorrel_dir().join(HEADS_DIR)
}

/// Path to the per-lane head file for `lane_id`.
#[must_use]
pub fn lane_head_path(lane_id: &str) -> PathBuf {
    heads_dir().join(sanitize_id(lane_id))
}

/// Path to the remotes configuration file.
#[must_use]
pub fn remotes_path() -> PathBuf {
    sorrel_dir().join(REMOTES_FILE)
}

/// Path to the workspace stat-cache file (`.sorrel/stat-cache.json`).
#[must_use]
pub fn stat_cache_path() -> PathBuf {
    sorrel_dir().join(STAT_CACHE_FILE)
}

/// Path to the snapshot → change index (`.sorrel/changes.index`).
#[must_use]
pub fn changes_index_path() -> PathBuf {
    sorrel_dir().join(CHANGES_INDEX_FILE)
}

/// Path to the Git import mapping file (`.sorrel/git-map.json`).
#[must_use]
pub fn git_map_path() -> PathBuf {
    sorrel_dir().join(GIT_MAP_FILE)
}

/// Path to the in-progress merge state file (`.sorrel/MERGE_STATE`).
#[must_use]
pub fn merge_state_path() -> PathBuf {
    sorrel_dir().join(MERGE_STATE_FILE)
}

/// Returns true when a conflicted merge is in progress.
#[must_use]
pub fn merge_in_progress() -> bool {
    merge_state_path().is_file()
}

/// On-disk record for an in-progress conflicted merge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MergeState {
    /// MergeResult object id (64-hex).
    pub merge_result: String,
    /// Lane id being merged into the active lane.
    pub lane: String,
    /// Merge-base snapshot id.
    pub base_snapshot: String,
    /// Active-lane (ours) snapshot id when the merge started.
    pub ours_snapshot: String,
    /// Incoming (theirs) snapshot id when the merge started.
    pub theirs_snapshot: String,
    /// Tentative merged snapshot used to preserve incoming tracked paths.
    pub working_snapshot: Option<String>,
    /// Commit message used when finalizing the merge.
    pub message: String,
}

/// Loads the full merge-state record from `.sorrel/MERGE_STATE`, if any.
pub fn load_merge_state_record() -> io::Result<Option<MergeState>> {
    let path = merge_state_path();
    if !path.is_file() {
        return Ok(None);
    }
    let bytes = fs::read(&path)?;
    let value: Value = serde_json::from_slice(&bytes)?;
    let merge_result = value
        .get("mergeResult")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    if merge_result.is_empty() {
        return Ok(None);
    }
    Ok(Some(MergeState {
        merge_result,
        lane: value
            .get("lane")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        base_snapshot: value
            .get("baseSnapshot")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        ours_snapshot: value
            .get("oursSnapshot")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        theirs_snapshot: value
            .get("theirsSnapshot")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        working_snapshot: value
            .get("workingSnapshot")
            .and_then(Value::as_str)
            .map(str::to_owned),
        message: value
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
    }))
}

/// Loads the MergeResult object id stored in `.sorrel/MERGE_STATE`, if any.
pub fn load_merge_state() -> io::Result<Option<String>> {
    Ok(load_merge_state_record()?.map(|state| state.merge_result))
}

/// Persists merge context for an in-progress conflicted merge.
pub fn write_merge_state_record(state: &MergeState) -> io::Result<()> {
    write_json_atomic(
        &merge_state_path(),
        &json!({
            "mergeResult": state.merge_result,
            "lane": state.lane,
            "baseSnapshot": state.base_snapshot,
            "oursSnapshot": state.ours_snapshot,
            "theirsSnapshot": state.theirs_snapshot,
            "workingSnapshot": state.working_snapshot,
            "message": state.message,
        }),
    )
}

/// Persists the MergeResult id for an in-progress conflicted merge.
///
/// Prefer [`write_merge_state_record`] so `--continue` has frozen parents.
pub fn write_merge_state(merge_result_id: &str) -> io::Result<()> {
    write_merge_state_record(&MergeState {
        merge_result: merge_result_id.to_owned(),
        lane: String::new(),
        base_snapshot: String::new(),
        ours_snapshot: String::new(),
        theirs_snapshot: String::new(),
        working_snapshot: None,
        message: String::new(),
    })
}

/// Removes `.sorrel/MERGE_STATE` if present.
pub fn clear_merge_state() -> io::Result<()> {
    match fs::remove_file(merge_state_path()) {
        Ok(()) => sorrel_core::durability::flush_directory_chain(&sorrel_dir(), &sorrel_dir())
            .map_err(|source| publication_uncertain(&merge_state_path(), source)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

/// One line of `.sorrel/changes.index`: resulting snapshot id → change id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangesIndexEntry {
    /// Resulting snapshot content id (64-hex).
    pub snapshot: String,
    /// Change object content id (64-hex).
    pub change: String,
}

/// Loads `.sorrel/changes.index` as a snapshot-id → change-id map.
///
/// Missing or unreadable index files yield an empty map so `log` never fails
/// on repos created before the index existed. Corrupt lines are skipped.
#[must_use]
pub fn load_changes_index() -> BTreeMap<String, String> {
    let path = changes_index_path();
    let Ok(bytes) = fs::read(&path) else {
        return BTreeMap::new();
    };
    let Ok(text) = String::from_utf8(bytes) else {
        return BTreeMap::new();
    };
    let mut map = BTreeMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let Some(snapshot) = value.get("snapshot").and_then(Value::as_str) else {
            continue;
        };
        let Some(change) = value.get("change").and_then(Value::as_str) else {
            continue;
        };
        if snapshot.len() == 64
            && change.len() == 64
            && snapshot.chars().all(|c| c.is_ascii_hexdigit())
            && change.chars().all(|c| c.is_ascii_hexdigit())
        {
            map.insert(snapshot.to_owned(), change.to_owned());
        }
    }
    map
}

/// Appends a snapshot → change mapping to `.sorrel/changes.index` atomically.
///
/// Reads the existing file (if any), appends one JSON line, and replaces the
/// file via temp + rename so concurrent readers never see a partial write.
/// The caller must hold [`WorkspaceLock`] to serialize read-modify-write.
pub fn append_changes_index(entry: &ChangesIndexEntry) -> io::Result<()> {
    flush_core_references(&sorrel_dir(), &[&entry.snapshot], &[&entry.change])?;
    write_bytes_atomic(&changes_index_path(), &changes_index_bytes(entry)?)
}

fn changes_index_bytes(entry: &ChangesIndexEntry) -> io::Result<Vec<u8>> {
    let path = changes_index_path();
    let mut body = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => Vec::new(),
        Err(error) => return Err(error),
    };
    if !body.is_empty() && !body.ends_with(b"\n") {
        body.push(b'\n');
    }
    let line = json!({
        "snapshot": entry.snapshot,
        "change": entry.change,
    });
    body.extend_from_slice(line.to_string().as_bytes());
    body.push(b'\n');

    Ok(body)
}

/// Loads the workspace stat cache, or an empty cache when none exists yet.
///
/// A corrupt or schema-incompatible cache file is treated as empty rather than
/// failing the command: the stat cache is a pure optimization, so a bad cache
/// simply forces a full re-hash on this run and is rewritten on save.
#[must_use]
pub fn load_stat_cache() -> sorrel_core::StatCache {
    match fs::read(stat_cache_path()) {
        Ok(bytes) => sorrel_core::StatCache::load(&bytes).unwrap_or_default(),
        Err(_) => sorrel_core::StatCache::new(),
    }
}

/// Saves the workspace stat cache atomically (temp file + rename).
pub fn save_stat_cache(cache: &sorrel_core::StatCache) -> io::Result<()> {
    let bytes = cache
        .to_bytes()
        .map_err(|error| io::Error::other(error.to_string()))?;
    write_bytes_atomic(&stat_cache_path(), &bytes)
}

/// Returns true when a workspace manifest already exists.
#[must_use]
pub fn is_initialized() -> bool {
    manifest_path().is_file()
}

/// Persisted HEAD pointer (current lane + head snapshot id).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Head {
    /// Active lane id.
    pub lane: String,
    /// Current head snapshot content id (hex), or empty for an unborn head.
    pub snapshot: String,
}

/// Generates a stable, dependency-light repository id of the form `repo_<hex>`.
///
/// Entropy is derived from the wall clock (nanoseconds) and the process id,
/// hashed via the engine's content-id primitive (BLAKE3) so the CLI needs no
/// direct hashing dependency.
#[must_use]
pub fn generate_repo_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let pid = std::process::id();
    let seed = format!("{nanos}:{pid}");
    let hex = sorrel_core::ObjectId::for_bytes(seed.as_bytes()).to_hex();
    format!("repo_{}", &hex[..16])
}

/// Returns the current time as an RFC3339 / ISO-8601 UTC string.
///
/// Dependency-light formatting of seconds since the Unix epoch into
/// `YYYY-MM-DDTHH:MM:SSZ` using a civil-time conversion (no chrono).
#[must_use]
pub fn now_rfc3339() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default();
    format_unix_seconds_utc(secs)
}

/// Converts seconds-since-epoch into an `YYYY-MM-DDTHH:MM:SSZ` UTC string.
#[must_use]
pub fn format_unix_seconds_utc(secs: u64) -> String {
    let days = secs / 86_400;
    let rem = secs % 86_400;
    let hour = rem / 3_600;
    let minute = (rem % 3_600) / 60;
    let second = rem % 60;

    let (year, month, day) = civil_from_days(days as i64);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

/// Howard Hinnant's days-from-civil inverse: convert day count to (y, m, d).
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = if month <= 2 { year + 1 } else { year };
    (year, month, day)
}

/// Writes `value` as pretty JSON to `path` atomically (temp file + rename).
pub fn write_json_atomic(path: &Path, value: &Value) -> io::Result<()> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    write_bytes_atomic(path, &bytes)
}

/// Loads the workspace manifest, if present, rejecting unsupported versions.
///
/// Optional fields in the supported namespace are preserved. No migration or
/// rewrite is performed when the version is missing, malformed, or unknown.
pub fn load_manifest() -> io::Result<Option<Value>> {
    load_manifest_at(&manifest_path())
}

fn load_manifest_at(path: &Path) -> io::Result<Option<Value>> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    let value: Value = serde_json::from_slice(&bytes)?;
    if value.get("schemaVersion").and_then(Value::as_str) != Some(PROTOCOL_VERSION) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("unsupported workspace schema version; expected {PROTOCOL_VERSION}"),
        ));
    }
    Ok(Some(value))
}

/// Builds the workspace manifest value for a new repo.
#[must_use]
pub fn build_manifest(repo_id: &str, created_at: &str) -> Value {
    json!({
        "schemaVersion": PROTOCOL_VERSION,
        "kind": "Workspace",
        "repoId": repo_id,
        "createdAt": created_at,
        "defaultLane": {
            "id": DEFAULT_LANE_ID,
            "name": DEFAULT_LANE_NAME
        }
    })
}

/// Writes the workspace manifest atomically.
pub fn write_manifest(value: &Value) -> io::Result<()> {
    write_json_atomic(&manifest_path(), value)
}

/// Loads the HEAD pointer, if present.
pub fn load_head() -> io::Result<Option<Head>> {
    load_head_raw()
}

/// Writes the HEAD pointer atomically and mirrors the snapshot into the active
/// lane's per-lane head file under `.sorrel/heads/`.
pub fn write_head(head: &Head) -> io::Result<()> {
    commit_head(head, None)
}

/// Publish a head and its change-index entry as one recoverable metadata commit.
/// The caller must hold [`WorkspaceLock`] throughout reading and updating state.
pub fn write_head_and_change(head: &Head, entry: &ChangesIndexEntry) -> io::Result<()> {
    commit_head(head, Some(entry))
}

fn flush_snapshot_closure(root: &Path, snapshot: &str) -> io::Result<()> {
    flush_core_references(root, &[snapshot], &[])
}

fn flush_core_references(root: &Path, snapshots: &[&str], changes: &[&str]) -> io::Result<()> {
    let store = sorrel_core::FileObjectStore::open_existing(root).map_err(io::Error::other)?;
    let mut roots = snapshots
        .iter()
        .map(|id| sorrel_core::parse_object_id_hex(id))
        .collect::<Result<Vec<_>, _>>()
        .map_err(io::Error::other)?;
    let mut change_queue = changes
        .iter()
        .map(|id| sorrel_core::parse_object_id_hex(id))
        .collect::<Result<Vec<_>, _>>()
        .map_err(io::Error::other)?;
    let mut ids = std::collections::BTreeSet::new();
    while let Some(id) = change_queue.pop() {
        if !ids.insert(id) {
            continue;
        }
        let change = sorrel_core::read_change(&store, &id).map_err(io::Error::other)?;
        roots.extend([change.base_snapshot.id, change.resulting_snapshot.id]);
        change_queue.extend(change.parent_changes.iter().map(|parent| parent.id));
    }
    ids.extend(sorrel_core::collect_closure(&store, &roots).map_err(io::Error::other)?);
    for id in ids {
        store.flush_existing(&id).map_err(io::Error::other)?;
    }
    Ok(())
}

fn commit_head(head: &Head, entry: Option<&ChangesIndexEntry>) -> io::Result<()> {
    let changes: Vec<_> = entry
        .map(|entry| entry.change.as_str())
        .into_iter()
        .collect();
    flush_core_references(&sorrel_dir(), &[&head.snapshot], &changes)?;
    let mut updates = Vec::new();
    if let Some(entry) = entry {
        updates.push((changes_index_path(), changes_index_bytes(entry)?));
    }
    updates.push((
        lane_head_path(&head.lane),
        serde_json::to_vec(&json!({ "snapshot": head.snapshot }))?,
    ));
    updates.push((
        head_path(),
        serde_json::to_vec(&json!({ "lane": head.lane, "snapshot": head.snapshot }))?,
    ));
    metadata_transaction(&sorrel_dir(), &updates)
}

/// Loads the per-lane head snapshot id for `lane_id`, if present.
///
/// Lazily migrates `.sorrel/heads/` from `HEAD` when the heads directory is
/// missing on an already-initialized workspace.
pub fn load_lane_head(lane_id: &str) -> io::Result<Option<String>> {
    ensure_heads_migrated()?;
    let path = lane_head_path(lane_id);
    if !path.is_file() {
        return Ok(None);
    }
    let bytes = fs::read(&path)?;
    let value: Value = serde_json::from_slice(&bytes)?;
    let snapshot = value
        .get("snapshot")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    if snapshot.is_empty() {
        Ok(None)
    } else {
        Ok(Some(snapshot))
    }
}

/// Writes the per-lane head snapshot pointer atomically.
pub fn write_lane_head(lane_id: &str, snapshot: &str) -> io::Result<()> {
    flush_snapshot_closure(&sorrel_dir(), snapshot)?;
    let value = json!({ "snapshot": snapshot });
    write_json_atomic(&lane_head_path(lane_id), &value)
}

/// Ensures `.sorrel/heads/` exists, creating it from `HEAD` when missing.
///
/// Older workspaces only persisted the global `HEAD` pointer. When the heads
/// directory is absent but `HEAD` exists, this creates the directory and seeds
/// the active lane's head file from `HEAD`. Idempotent when heads already exist.
pub fn ensure_heads_migrated() -> io::Result<()> {
    let dir = heads_dir();
    if dir.is_dir() {
        return Ok(());
    }
    let Some(head) = load_head_raw()? else {
        return Ok(());
    };
    fs::create_dir_all(&dir)?;
    write_lane_head(&head.lane, &head.snapshot)
}

/// Loads HEAD without triggering per-lane head migration (avoids recursion).
fn load_head_raw() -> io::Result<Option<Head>> {
    let path = head_path();
    if !path.is_file() {
        return Ok(None);
    }
    let bytes = fs::read(&path)?;
    let value: Value = serde_json::from_slice(&bytes)?;
    let lane = value
        .get("lane")
        .and_then(Value::as_str)
        .unwrap_or(DEFAULT_LANE_ID)
        .to_owned();
    let snapshot = value
        .get("snapshot")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    Ok(Some(Head { lane, snapshot }))
}

/// Returns true when a lane registry entry exists for `lane_id`.
#[must_use]
pub fn lane_exists(lane_id: &str) -> bool {
    registry_dir(LANES_DIR)
        .join(format!("{}.json", sanitize_id(lane_id)))
        .is_file()
}

/// Persisted remote endpoint configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Remote {
    /// Sync transport base URL (e.g. `http://host:port`).
    pub url: String,
    /// Repository id on the remote hub.
    pub repo_id: String,
}

/// In-memory remotes registry matching `.sorrel/remotes.json`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RemotesConfig {
    /// Named remotes keyed by remote name (e.g. `origin`).
    pub remotes: BTreeMap<String, Remote>,
}

impl RemotesConfig {
    /// Returns the named remote or the default `origin` remote.
    pub fn resolve(&self, name: Option<&str>) -> io::Result<(String, Remote)> {
        let resolved = name.unwrap_or(DEFAULT_REMOTE_NAME).to_owned();
        match self.remotes.get(&resolved) {
            Some(remote) => Ok((resolved, remote.clone())),
            None => Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("remote `{resolved}` is not configured; run `sorrel remote add`"),
            )),
        }
    }
}

/// Loads `.sorrel/remotes.json`, returning an empty config when missing.
pub fn load_remotes() -> io::Result<RemotesConfig> {
    let path = remotes_path();
    if !path.is_file() {
        return Ok(RemotesConfig::default());
    }
    let bytes = fs::read(&path)?;
    let value: Value = serde_json::from_slice(&bytes)?;
    let mut remotes = BTreeMap::new();
    if let Some(map) = value.get("remotes").and_then(Value::as_object) {
        for (name, entry) in map {
            let url = entry
                .get("url")
                .and_then(Value::as_str)
                .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "remote missing url"))?
                .to_owned();
            let repo_id = entry
                .get("repoId")
                .and_then(Value::as_str)
                .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "remote missing repoId"))?
                .to_owned();
            remotes.insert(name.clone(), Remote { url, repo_id });
        }
    }
    Ok(RemotesConfig { remotes })
}

/// Writes `.sorrel/remotes.json` atomically.
pub fn save_remotes(config: &RemotesConfig) -> io::Result<()> {
    let mut remotes = serde_json::Map::new();
    for (name, remote) in &config.remotes {
        remotes.insert(
            name.clone(),
            json!({
                "url": remote.url,
                "repoId": remote.repo_id,
            }),
        );
    }
    write_json_atomic(&remotes_path(), &json!({ "remotes": remotes }))
}

/// Adds or replaces a named remote and persists the registry.
pub fn add_remote(name: &str, url: &str, repo_id: &str) -> io::Result<()> {
    let mut config = load_remotes()?;
    config.remotes.insert(
        name.to_owned(),
        Remote {
            url: url.to_owned(),
            repo_id: repo_id.to_owned(),
        },
    );
    save_remotes(&config)
}

/// Path to a named registry subdirectory under `.sorrel/`.
#[must_use]
pub fn registry_dir(name: &str) -> PathBuf {
    sorrel_dir().join(name)
}

/// Writes a JSON registry entry atomically at `.sorrel/<dir>/<id>.json`.
pub fn write_registry_entry(dir: &str, id: &str, value: &Value) -> io::Result<()> {
    let path = registry_dir(dir).join(format!("{}.json", sanitize_id(id)));
    write_json_atomic(&path, value)
}

/// Lists all JSON registry entries under `.sorrel/<dir>/`, sorted by filename
/// for deterministic output. Missing directories yield an empty list.
pub fn list_registry_entries(dir: &str) -> io::Result<Vec<Value>> {
    let path = registry_dir(dir);
    if !path.is_dir() {
        return Ok(Vec::new());
    }
    let mut names: Vec<PathBuf> = fs::read_dir(&path)?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("json"))
        .collect();
    names.sort();
    let mut entries = Vec::with_capacity(names.len());
    for file in names {
        let bytes = fs::read(&file)?;
        entries.push(serde_json::from_slice(&bytes)?);
    }
    Ok(entries)
}

/// Sanitizes an id into a filesystem-safe registry filename stem.
#[must_use]
fn sanitize_id(id: &str) -> String {
    id.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_reader_rejects_unknown_versions_and_preserves_optional_fields() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join(MANIFEST_FILE);
        assert_eq!(load_manifest_at(&path).unwrap(), None);
        let mut manifest = build_manifest("repo_test", "timestamp");
        manifest["extension"] = json!({"enabled": true});
        write_json_atomic(&path, &manifest).unwrap();
        assert_eq!(load_manifest_at(&path).unwrap(), Some(manifest.clone()));
        for version in [json!("sorrel.protocol.v1"), json!(null), json!(42)] {
            manifest["schemaVersion"] = version;
            write_json_atomic(&path, &manifest).unwrap();
            assert!(
                matches!(load_manifest_at(&path), Err(error) if error.kind() == io::ErrorKind::InvalidData)
            );
        }
    }

    #[test]
    fn unsupported_manifest_blocks_lock_creation_and_transaction_recovery() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join(MANIFEST_FILE);
        let mut manifest = build_manifest("repo_test", "timestamp");
        manifest["schemaVersion"] = json!("sorrel.protocol.v1");
        write_json_atomic(&path, &manifest).unwrap();
        let manifest_bytes = fs::read(&path).unwrap();
        let head = root.path().join(HEAD_FILE);
        fs::write(&head, b"old head").unwrap();
        let staged = stage_bytes(&head, b"new head").unwrap();
        let journal = root.path().join(TRANSACTION_FILE);
        write_json_atomic(
            &journal,
            &json!([[
                HEAD_FILE,
                staged.strip_prefix(root.path()).unwrap(),
                sorrel_core::ObjectId::for_bytes(b"new head").to_hex(),
            ]]),
        )
        .unwrap();
        let journal_bytes = fs::read(&journal).unwrap();
        assert!(
            matches!(WorkspaceLock::acquire(root.path()), Err(error) if error.kind() == io::ErrorKind::InvalidData)
        );
        assert_eq!(fs::read(&head).unwrap(), b"old head");
        assert_eq!(fs::read(&path).unwrap(), manifest_bytes);
        assert_eq!(fs::read(&journal).unwrap(), journal_bytes);
        assert_eq!(fs::read(staged).unwrap(), b"new head");
        assert!(!root.path().join("write.lock").exists());
    }

    #[test]
    fn workspace_lock_rejects_another_writer_and_releases_on_drop() {
        let root = tempfile::tempdir().unwrap();
        let first = WorkspaceLock::acquire(root.path()).unwrap();
        assert!(
            matches!(WorkspaceLock::acquire(root.path()), Err(error) if error.kind() == io::ErrorKind::WouldBlock)
        );
        drop(first);
        assert!(WorkspaceLock::acquire(root.path()).is_ok());
    }

    #[test]
    fn transaction_preparation_failure_keeps_existing_metadata() {
        let root = tempfile::tempdir().unwrap();
        let head = root.path().join(HEAD_FILE);
        let index = root.path().join(CHANGES_INDEX_FILE);
        fs::write(&head, b"old head").unwrap();
        fs::create_dir(&index).unwrap();
        assert!(metadata_transaction(
            root.path(),
            &[
                (head.clone(), b"new head".to_vec()),
                (index, b"new index".to_vec()),
            ]
        )
        .is_err());
        assert_eq!(fs::read(head).unwrap(), b"old head");
        assert!(!root.path().join(TRANSACTION_FILE).exists());
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 2);
    }

    #[test]
    fn acquiring_lock_finishes_an_interrupted_index_and_head_commit() {
        let root = tempfile::tempdir().unwrap();
        let index = root.path().join(CHANGES_INDEX_FILE);
        let head = root.path().join(HEAD_FILE);
        fs::write(&head, b"old head").unwrap();
        let staged_index = stage_bytes(&index, b"new index").unwrap();
        let staged_head = stage_bytes(&head, b"new head").unwrap();
        let entries = vec![
            (
                PathBuf::from(CHANGES_INDEX_FILE),
                staged_index
                    .strip_prefix(root.path())
                    .unwrap()
                    .to_path_buf(),
                sorrel_core::ObjectId::for_bytes(b"new index").to_hex(),
            ),
            (
                PathBuf::from(HEAD_FILE),
                staged_head.strip_prefix(root.path()).unwrap().to_path_buf(),
                sorrel_core::ObjectId::for_bytes(b"new head").to_hex(),
            ),
        ];
        write_json_atomic(
            &root.path().join(TRANSACTION_FILE),
            &serde_json::to_value(entries).unwrap(),
        )
        .unwrap();
        fs::rename(staged_index, &index).unwrap();
        let _guard = WorkspaceLock::acquire(root.path()).unwrap();
        assert_eq!(fs::read(index).unwrap(), b"new index");
        assert_eq!(fs::read(head).unwrap(), b"new head");
        assert!(!root.path().join(TRANSACTION_FILE).exists());
    }

    #[test]
    fn recovery_rejects_journal_paths_outside_workspace_metadata() {
        let root = tempfile::tempdir().unwrap();
        write_json_atomic(
            &root.path().join(TRANSACTION_FILE),
            &json!([["../outside", ".sorrel-test.tmp", "digest"]]),
        )
        .unwrap();
        assert!(
            matches!(WorkspaceLock::acquire(root.path()), Err(error) if error.kind() == io::ErrorKind::InvalidData)
        );
    }

    #[test]
    fn recovery_rejects_lost_staging_when_target_has_not_been_committed() {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join(HEAD_FILE), b"old head").unwrap();
        write_json_atomic(
            &root.path().join(TRANSACTION_FILE),
            &json!([[
                HEAD_FILE,
                ".sorrel-missing.tmp",
                sorrel_core::ObjectId::for_bytes(b"new head").to_hex()
            ]]),
        )
        .unwrap();
        assert!(
            matches!(WorkspaceLock::acquire(root.path()), Err(error) if error.kind() == io::ErrorKind::InvalidData)
        );
        assert!(root.path().join(TRANSACTION_FILE).is_file());
        assert_eq!(fs::read(root.path().join(HEAD_FILE)).unwrap(), b"old head");
    }

    #[test]
    fn concurrent_atomic_writes_publish_complete_json_and_clean_temporaries() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("record.json");
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(8));
        let threads: Vec<_> = (0..8)
            .map(|index| {
                let path = path.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    write_json_atomic(
                        &path,
                        &json!({ "index": index, "content": "x".repeat(100_000) }),
                    )
                    .unwrap();
                })
            })
            .collect();
        for thread in threads {
            thread.join().unwrap();
        }
        let value: Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
        assert!(value["index"].as_u64().unwrap() < 8);
        assert_eq!(value["content"].as_str().unwrap().len(), 100_000);
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 1);
    }

    #[test]
    fn pending_head_recovery_validates_and_flushes_real_snapshot_closure_without_tmp() {
        use sorrel_core::{ObjectStore, SnapshotOptions};
        let outer = tempfile::tempdir().unwrap();
        let workspace = outer.path().join("workspace");
        fs::create_dir(&workspace).unwrap();
        fs::write(workspace.join("hello.txt"), b"hello").unwrap();
        let root = outer.path().join(SORREL_DIR);
        let store = sorrel_core::FileObjectStore::new(&root).unwrap();
        let snapshot = sorrel_core::materialize_snapshot(
            &store,
            &workspace,
            SnapshotOptions::new("repo_fixture"),
        )
        .unwrap();
        let tree = sorrel_core::read_tree(&store, &snapshot.root_tree.id).unwrap();
        let blob = tree.entries[0].object.id;
        let original = store.read(&blob).unwrap();
        let hex = blob.to_hex();
        let path = root.join("objects").join(&hex[..2]).join(&hex[2..]);
        fs::remove_dir(root.join("tmp")).unwrap();
        let head = root.join(HEAD_FILE);
        fs::write(&head, b"old").unwrap();
        let next =
            serde_json::to_vec(&json!({"lane": "lane_main", "snapshot": snapshot.id.to_hex()}))
                .unwrap();
        let error = metadata_transaction_with_flush(
            &root,
            &[(head.clone(), next.clone())],
            &fail_nth_barrier(3),
        )
        .unwrap_err();
        assert!(is_publication_uncertain(&error));
        fs::write(&path, b"corrupt terminal bytes").unwrap();
        assert!(recover_metadata_transaction(&root).is_err());
        assert_eq!(fs::read(&head).unwrap(), b"old");
        assert!(root.join(TRANSACTION_FILE).is_file());
        fs::write(path, original).unwrap();
        recover_metadata_transaction(&root).unwrap();
        assert_eq!(fs::read(head).unwrap(), next);
        assert!(!root.join(TRANSACTION_FILE).exists());
        assert!(!root.join("tmp").exists());
    }

    fn fail_nth_barrier(n: usize) -> impl Fn(&Path, &Path) -> io::Result<()> {
        let remaining = std::cell::Cell::new(n);
        move |path, root| {
            remaining.set(remaining.get().saturating_sub(1));
            if remaining.get() == 0 {
                return Err(io::Error::other("injected directory barrier failure"));
            }
            sorrel_core::durability::flush_directory_chain(path, root)
        }
    }

    #[test]
    fn atomic_metadata_barrier_failure_distinguishes_staging_from_publication() {
        let root = tempfile::tempdir().unwrap();
        let head = root.path().join(HEAD_FILE);
        fs::write(&head, b"old").unwrap();
        let before =
            write_bytes_atomic_with_flush(root.path(), &head, b"new", &fail_nth_barrier(1))
                .unwrap_err();
        assert!(!is_publication_uncertain(&before));
        assert_eq!(fs::read(&head).unwrap(), b"old");
        let after = write_bytes_atomic_with_flush(root.path(), &head, b"new", &fail_nth_barrier(2))
            .unwrap_err();
        assert!(is_publication_uncertain(&after));
        assert_eq!(fs::read(&head).unwrap(), b"new");
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 1);
    }

    #[test]
    fn journal_publication_failure_keeps_staging_and_retry_finishes() {
        let root = tempfile::tempdir().unwrap();
        let head = root.path().join(HEAD_FILE);
        fs::write(&head, b"old").unwrap();
        let error = metadata_transaction_with_flush(
            root.path(),
            &[(head.clone(), b"new".to_vec())],
            &fail_nth_barrier(3),
        )
        .unwrap_err();
        assert!(is_publication_uncertain(&error));
        assert_eq!(fs::read(&head).unwrap(), b"old");
        assert!(root.path().join(TRANSACTION_FILE).is_file());
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 3);
        recover_metadata_transaction(root.path()).unwrap();
        assert_eq!(fs::read(&head).unwrap(), b"new");
        assert!(!root.path().join(TRANSACTION_FILE).exists());
    }

    #[test]
    fn recovery_retries_barriers_for_already_published_targets_before_journal_cleanup() {
        let root = tempfile::tempdir().unwrap();
        let head = root.path().join(HEAD_FILE);
        fs::write(&head, b"old").unwrap();
        // Three preparation barriers, one journal retry barrier, then target rename.
        let error = metadata_transaction_with_flush(
            root.path(),
            &[(head.clone(), b"new".to_vec())],
            &fail_nth_barrier(5),
        )
        .unwrap_err();
        assert!(is_publication_uncertain(&error));
        assert_eq!(fs::read(&head).unwrap(), b"new");
        assert!(root.path().join(TRANSACTION_FILE).is_file());
        let calls = std::cell::Cell::new(0);
        recover_metadata_transaction_with_flush(root.path(), &|path, root| {
            calls.set(calls.get() + 1);
            sorrel_core::durability::flush_directory_chain(path, root)
        })
        .unwrap();
        assert_eq!(calls.get(), 3); // journal confirmation, target, journal deletion
        assert!(!root.path().join(TRANSACTION_FILE).exists());
        assert_eq!(fs::read(&head).unwrap(), b"new");
    }

    #[test]
    fn failed_journal_cleanup_reports_uncertainty_and_reappeared_journal_is_replayable() {
        let root = tempfile::tempdir().unwrap();
        let head = root.path().join(HEAD_FILE);
        fs::write(&head, b"old").unwrap();
        let calls = std::cell::Cell::new(0);
        let saved_journal = std::cell::RefCell::new(Vec::new());
        let error = metadata_transaction_with_flush(
            root.path(),
            &[(head.clone(), b"new".to_vec())],
            &|path, root| {
                calls.set(calls.get() + 1);
                if calls.get() == 5 {
                    *saved_journal.borrow_mut() = fs::read(root.join(TRANSACTION_FILE))?;
                }
                if calls.get() == 6 {
                    return Err(io::Error::other("injected journal cleanup barrier failure"));
                }
                sorrel_core::durability::flush_directory_chain(path, root)
            },
        )
        .unwrap_err();
        assert!(is_publication_uncertain(&error));
        assert_eq!(fs::read(&head).unwrap(), b"new");
        assert!(!root.path().join(TRANSACTION_FILE).exists());
        assert!(!saved_journal.borrow().is_empty());
        // Model an unlink that was visible but did not survive a restart.
        fs::write(root.path().join(TRANSACTION_FILE), &*saved_journal.borrow()).unwrap();
        recover_metadata_transaction(root.path()).unwrap();
        assert_eq!(fs::read(&head).unwrap(), b"new");
        assert!(!root.path().join(TRANSACTION_FILE).exists());
    }

    #[test]
    fn index_only_recovery_checks_parent_changes_and_their_linked_snapshot_blobs() {
        use sorrel_core::{ChangeOptions, ObjectStore, Principal, SnapshotOptions};
        let outer = tempfile::tempdir().unwrap();
        let workspace = outer.path().join("workspace");
        fs::create_dir(&workspace).unwrap();
        let root = outer.path().join(SORREL_DIR);
        let store = sorrel_core::FileObjectStore::new(&root).unwrap();
        let mut snapshots = Vec::new();
        for content in ["first", "second", "third"] {
            fs::write(workspace.join("hello.txt"), content).unwrap();
            snapshots.push(
                sorrel_core::materialize_snapshot(
                    &store,
                    &workspace,
                    SnapshotOptions::new("repo_fixture"),
                )
                .unwrap(),
            );
        }
        let first = sorrel_core::create_change(
            &store,
            snapshots[0].id,
            snapshots[1].id,
            ChangeOptions::new(Principal::system(), "first change"),
        )
        .unwrap();
        let mut options = ChangeOptions::new(Principal::system(), "second change");
        options.parent_changes.push(first.id);
        let second =
            sorrel_core::create_change(&store, snapshots[1].id, snapshots[2].id, options).unwrap();
        let index = root.join(CHANGES_INDEX_FILE);
        fs::write(&index, b"old index\n").unwrap();
        let next = format!(
            "{}\n",
            json!({"snapshot": snapshots[2].id.to_hex(), "change": second.id.to_hex()})
        );
        let error = metadata_transaction_with_flush(
            &root,
            &[(index.clone(), next.as_bytes().to_vec())],
            &fail_nth_barrier(3),
        )
        .unwrap_err();
        assert!(is_publication_uncertain(&error));
        let tree = sorrel_core::read_tree(&store, &snapshots[0].root_tree.id).unwrap();
        let blob = tree.entries[0].object.id;
        let original = store.read(&blob).unwrap();
        let hex = blob.to_hex();
        let blob_path = root.join("objects").join(&hex[..2]).join(&hex[2..]);
        fs::write(&blob_path, b"corrupt parent-change base blob").unwrap();
        assert!(recover_metadata_transaction(&root).is_err());
        assert_eq!(fs::read(&index).unwrap(), b"old index\n");
        assert!(root.join(TRANSACTION_FILE).is_file());
        fs::write(blob_path, original).unwrap();
        recover_metadata_transaction(&root).unwrap();
        assert_eq!(fs::read(index).unwrap(), next.as_bytes());
        assert!(!root.join(TRANSACTION_FILE).exists());
    }

    #[test]
    fn repo_id_has_prefix_and_is_nonempty() {
        let id = generate_repo_id();
        assert!(id.starts_with("repo_"));
        assert!(id.len() > "repo_".len());
    }

    #[test]
    fn unix_epoch_formats_to_known_date() {
        assert_eq!(format_unix_seconds_utc(0), "1970-01-01T00:00:00Z");
        // 2026-06-26T00:00:00Z == 1782432000 seconds.
        assert_eq!(
            format_unix_seconds_utc(1_782_432_000),
            "2026-06-26T00:00:00Z"
        );
        // A non-midnight check: 2000-01-01T12:34:56Z == 946730096 seconds.
        assert_eq!(format_unix_seconds_utc(946_730_096), "2000-01-01T12:34:56Z");
    }

    #[test]
    fn now_rfc3339_is_well_formed() {
        let stamp = now_rfc3339();
        assert_eq!(stamp.len(), 20);
        assert!(stamp.ends_with('Z'));
        assert_eq!(&stamp[4..5], "-");
    }
}
