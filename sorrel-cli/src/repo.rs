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
        message: String::new(),
    })
}

/// Removes `.sorrel/MERGE_STATE` if present.
pub fn clear_merge_state() -> io::Result<()> {
    match fs::remove_file(merge_state_path()) {
        Ok(()) => Ok(()),
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
pub fn append_changes_index(entry: &ChangesIndexEntry) -> io::Result<()> {
    let path = changes_index_path();
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;

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

    let tmp = parent.join(format!(".{CHANGES_INDEX_FILE}.tmp"));
    {
        let mut file = fs::File::create(&tmp)?;
        file.write_all(&body)?;
        file.flush()?;
    }
    fs::rename(&tmp, &path)
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
    let path = stat_cache_path();
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let tmp = parent.join(format!(".{STAT_CACHE_FILE}.tmp"));
    let bytes = cache
        .to_bytes()
        .map_err(|error| io::Error::other(error.to_string()))?;
    {
        let mut file = fs::File::create(&tmp)?;
        file.write_all(&bytes)?;
        file.flush()?;
    }
    fs::rename(&tmp, &path)
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
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let tmp = parent.join(format!(
        ".{}.tmp",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("sorrel")
    ));
    {
        let mut file = fs::File::create(&tmp)?;
        serde_json::to_writer_pretty(&mut file, value)?;
        writeln!(file)?;
        file.flush()?;
    }
    fs::rename(&tmp, path)
}

/// Loads the workspace manifest, if present.
pub fn load_manifest() -> io::Result<Option<Value>> {
    let path = manifest_path();
    if !path.is_file() {
        return Ok(None);
    }
    let bytes = fs::read(&path)?;
    let value = serde_json::from_slice(&bytes)?;
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
    let value = json!({
        "lane": head.lane,
        "snapshot": head.snapshot,
    });
    write_json_atomic(&head_path(), &value)?;
    write_lane_head(&head.lane, &head.snapshot)
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
