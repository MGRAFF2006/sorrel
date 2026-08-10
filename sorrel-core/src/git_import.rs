//! One-way import from a Git repository into Sorrel snapshots and changes.
//!
//! Walks commits reachable from a selected ref (default `HEAD`) in topological
//! order, materializes each Git tree as Sorrel blobs/trees, writes a Snapshot,
//! and records a Change from the first-parent (or empty) base snapshot.
//!
//! Imports are incremental when [`GitImportOptions::known_commits`] is seeded
//! with a previous run's SHA → snapshot map: known commits (and their
//! ancestors) are excluded from the walk and used to resolve parent snapshots,
//! which is the building block for the colocated Git mirror.

use crate::{
    create_change, write_blob, write_snapshot, write_tree, ChangeError, ChangeOptions, EntryMode,
    EntryType, ObjectId, ObjectKind, ObjectRef, ObjectStore, ObjectStoreError, Principal,
    SnapshotError, SnapshotOptions, Tree, TreeEntry,
};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

/// Result type for Git import operations.
pub type GitImportResult<T> = Result<T, GitImportError>;

/// Errors returned while importing Git history into Sorrel.
#[derive(Debug, thiserror::Error)]
pub enum GitImportError {
    /// Underlying object store failure.
    #[error(transparent)]
    ObjectStore(#[from] ObjectStoreError),

    /// Snapshot/tree/blob materialization failure.
    #[error(transparent)]
    Snapshot(#[from] SnapshotError),

    /// Change object failure.
    #[error(transparent)]
    Change(#[from] ChangeError),

    /// libgit2 / git2 failure.
    #[error(transparent)]
    Git(#[from] git2::Error),

    /// Selected ref could not be resolved.
    #[error("git ref not found: {reference}")]
    RefNotFound {
        /// Ref name that failed to resolve.
        reference: String,
    },

    /// Unsupported Git object (symlink, submodule, …) at a path.
    #[error("unsupported git entry at {path}: {detail}")]
    UnsupportedEntry {
        /// Path inside the Git tree.
        path: String,
        /// Human-readable reason.
        detail: String,
    },
}

/// Options controlling a one-way Git → Sorrel import.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GitImportOptions {
    /// Filesystem path to the Git repository (working tree or bare `.git`).
    pub git_path: PathBuf,
    /// Git ref to import (default `HEAD`).
    pub git_ref: String,
    /// Optional maximum number of commits to import (oldest first after walk).
    pub limit: Option<usize>,
    /// Sorrel repository id stamped onto snapshots.
    pub repo_id: String,
    /// Git SHAs already imported by a previous run (full hex → snapshot id).
    ///
    /// Known commits and their ancestors are excluded from the walk; parent
    /// resolution consults this map, so repeated imports only materialize new
    /// commits. Root commits among the new ones still base on the empty
    /// snapshot.
    pub known_commits: BTreeMap<String, ObjectId>,
}

impl GitImportOptions {
    /// Builds import options for `git_path` with defaults (`HEAD`, no limit).
    #[must_use]
    pub fn new(git_path: impl Into<PathBuf>, repo_id: impl Into<String>) -> Self {
        Self {
            git_path: git_path.into(),
            git_ref: "HEAD".to_owned(),
            limit: None,
            repo_id: repo_id.into(),
            known_commits: BTreeMap::new(),
        }
    }
}

/// One imported Git commit mapped onto Sorrel objects.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImportedCommit {
    /// Full Git commit SHA (hex).
    pub git_sha: String,
    /// Sorrel snapshot produced for this commit.
    pub snapshot_id: ObjectId,
    /// Sorrel change linking the base snapshot to this snapshot.
    pub change_id: ObjectId,
    /// Commit subject (first line of the message).
    pub message: String,
}

/// Outcome of [`git_import`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImportResult {
    /// Imported commits in chronological order (oldest first).
    pub commits: Vec<ImportedCommit>,
    /// Snapshot id of the tip after import (last imported commit, or empty base).
    pub head_snapshot: ObjectId,
    /// Durable Git SHA → Sorrel snapshot id map (full hex keys). Includes
    /// entries seeded via [`GitImportOptions::known_commits`].
    pub git_to_snapshot: BTreeMap<String, ObjectId>,
    /// Empty base snapshot used as the parent for root commits.
    pub empty_base_snapshot: ObjectId,
}

/// Imports Git history into `store` as Sorrel snapshots and changes.
///
/// Commits are walked in topological order (parents before children). Linear
/// and merge commits are both imported; snapshot parents list all mapped Git
/// parents, while the Change base is the first parent (or the empty base for
/// root commits). Symlinks and submodules are skipped with an error if present.
pub fn git_import(
    store: &impl ObjectStore,
    options: GitImportOptions,
) -> GitImportResult<ImportResult> {
    let repo = open_repository(&options.git_path)?;
    let tip = resolve_commit(&repo, &options.git_ref)?;

    let empty_tree = write_tree(store, Vec::new())?;
    let mut empty_opts = SnapshotOptions::new(options.repo_id.clone());
    empty_opts.message = Some("empty base for git import".to_owned());
    empty_opts.created_at = "1970-01-01T00:00:00Z".to_owned();
    empty_opts.author = Principal::system();
    let empty_base = write_snapshot(store, empty_tree.id, empty_opts)?;

    let tip_sha = tip.id().to_string();
    let mut oids = collect_commits(&repo, tip, options.limit, &options.known_commits)?;
    // Topological walk from tip yields newest-first; reverse to oldest-first.
    oids.reverse();

    let mut git_to_snapshot: BTreeMap<String, ObjectId> = options.known_commits.clone();
    let mut git_to_change: BTreeMap<String, ObjectId> = BTreeMap::new();
    let mut commits = Vec::with_capacity(oids.len());
    // Cache Git tree oid → Sorrel tree id within this import.
    let mut tree_cache: BTreeMap<git2::Oid, ObjectId> = BTreeMap::new();

    for oid in oids {
        let git_sha = oid.to_string();
        if git_to_snapshot.contains_key(&git_sha) {
            continue;
        }
        let commit = repo.find_commit(oid)?;
        let message = commit_subject(&commit);
        let author = principal_from_signature(commit.author());
        let created_at = timestamp_to_rfc3339(commit.time().seconds());

        let git_tree = commit.tree()?;
        let sorrel_tree = import_tree(store, &repo, &git_tree, Path::new(""), &mut tree_cache)?;

        let mut parent_snapshots = Vec::new();
        let mut parent_changes = Vec::new();
        for parent in commit.parents() {
            let parent_sha = parent.id().to_string();
            if let Some(snapshot_id) = git_to_snapshot.get(&parent_sha) {
                parent_snapshots.push(ObjectRef::new(ObjectKind::Snapshot, *snapshot_id));
            }
            if let Some(change_id) = git_to_change.get(&parent_sha) {
                parent_changes.push(*change_id);
            }
        }

        let base_snapshot_id = parent_snapshots
            .first()
            .map(|r| r.id)
            .unwrap_or(empty_base.id);

        let mut snap_opts = SnapshotOptions::new(options.repo_id.clone());
        snap_opts.parents = parent_snapshots;
        snap_opts.message = Some(message.clone());
        snap_opts.created_at = created_at;
        snap_opts.author = author.clone();
        let snapshot = write_snapshot(store, sorrel_tree.id, snap_opts)?;

        let mut change_opts = ChangeOptions::new(author, message.clone());
        change_opts.parent_changes = parent_changes;
        change_opts.description = Some(format!("imported from git commit {git_sha}"));
        let change = create_change(store, base_snapshot_id, snapshot.id, change_opts)?;

        git_to_snapshot.insert(git_sha.clone(), snapshot.id);
        git_to_change.insert(git_sha.clone(), change.id);
        commits.push(ImportedCommit {
            git_sha,
            snapshot_id: snapshot.id,
            change_id: change.id,
            message,
        });
    }

    // The tip is either newly imported or seeded via `known_commits`; fall back
    // to the empty base only when the walk produced nothing and the tip is
    // unknown (e.g. an empty repository).
    let head_snapshot = git_to_snapshot
        .get(&tip_sha)
        .copied()
        .or_else(|| commits.last().map(|c| c.snapshot_id))
        .unwrap_or(empty_base.id);

    Ok(ImportResult {
        commits,
        head_snapshot,
        git_to_snapshot,
        empty_base_snapshot: empty_base.id,
    })
}

fn open_repository(path: &Path) -> GitImportResult<git2::Repository> {
    Ok(git2::Repository::discover(path)?)
}

fn resolve_commit<'a>(
    repo: &'a git2::Repository,
    reference: &str,
) -> GitImportResult<git2::Commit<'a>> {
    let obj = repo
        .revparse_single(reference)
        .map_err(|_| GitImportError::RefNotFound {
            reference: reference.to_owned(),
        })?;
    Ok(obj.peel_to_commit()?)
}

fn collect_commits(
    repo: &git2::Repository,
    tip: git2::Commit<'_>,
    limit: Option<usize>,
    known_commits: &BTreeMap<String, ObjectId>,
) -> GitImportResult<Vec<git2::Oid>> {
    let mut walk = repo.revwalk()?;
    walk.set_sorting(git2::Sort::TOPOLOGICAL | git2::Sort::TIME)?;
    walk.push(tip.id())?;
    for sha in known_commits.keys() {
        if let Ok(oid) = git2::Oid::from_str(sha) {
            // Hide known commits (and their ancestors) so the walk only yields
            // new history. SHAs absent from this repository are ignored.
            let _ = walk.hide(oid);
        }
    }

    let mut oids = Vec::new();
    for oid in walk {
        let oid = oid?;
        oids.push(oid);
        if let Some(max) = limit {
            if oids.len() >= max {
                break;
            }
        }
    }
    Ok(oids)
}

fn commit_subject(commit: &git2::Commit<'_>) -> String {
    if let Ok(Some(summary)) = commit.summary() {
        if !summary.is_empty() {
            return summary.to_owned();
        }
    }
    if let Ok(message) = commit.message() {
        if let Some(line) = message.lines().next() {
            if !line.is_empty() {
                return line.to_owned();
            }
        }
    }
    "(no message)".to_owned()
}

fn principal_from_signature(sig: git2::Signature<'_>) -> Principal {
    let name = sig.name().unwrap_or("unknown").to_owned();
    let email = sig.email().unwrap_or("unknown").to_owned();
    Principal::new("user", format!("{name} <{email}>"), Some(name))
}

fn timestamp_to_rfc3339(secs: i64) -> String {
    let secs = if secs < 0 { 0_u64 } else { secs as u64 };
    let days = secs / 86_400;
    let rem = secs % 86_400;
    let hour = rem / 3_600;
    let minute = (rem % 3_600) / 60;
    let second = rem % 60;
    let (year, month, day) = civil_from_days(days as i64);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    let y = y + i64::from(m <= 2);
    (y, m as u32, d as u32)
}

fn import_tree(
    store: &impl ObjectStore,
    repo: &git2::Repository,
    tree: &git2::Tree<'_>,
    prefix: &Path,
    cache: &mut BTreeMap<git2::Oid, ObjectId>,
) -> GitImportResult<Tree> {
    if let Some(id) = cache.get(&tree.id()) {
        // Re-read so callers get a full Tree value; trees are small JSON objects.
        return Ok(crate::read_tree(store, id)?);
    }

    let mut entries = Vec::new();
    for entry in tree.iter() {
        let name = match entry.name() {
            Ok(name) => name,
            Err(_) => {
                return Err(GitImportError::UnsupportedEntry {
                    path: prefix.join("(invalid-utf8)").display().to_string(),
                    detail: "non-UTF-8 path".to_owned(),
                });
            }
        };
        let relative = if prefix.as_os_str().is_empty() {
            PathBuf::from(name)
        } else {
            prefix.join(name)
        };
        let path_display = relative.to_string_lossy().replace('\\', "/");

        match entry.kind() {
            Some(git2::ObjectType::Blob) => {
                let filemode = entry.filemode();
                if filemode == 0o120000 {
                    return Err(GitImportError::UnsupportedEntry {
                        path: path_display,
                        detail: "symlink".to_owned(),
                    });
                }
                if filemode == 0o160000 {
                    return Err(GitImportError::UnsupportedEntry {
                        path: path_display,
                        detail: "submodule gitlink".to_owned(),
                    });
                }
                let blob = repo.find_blob(entry.id())?;
                let content = blob.content();
                let written = write_blob(store, content)?;
                let mode = if filemode == 0o100755 {
                    EntryMode::Executable
                } else {
                    EntryMode::Normal
                };
                entries.push(TreeEntry {
                    name: name.to_owned(),
                    path: relative,
                    entry_type: EntryType::File,
                    object: ObjectRef::new(ObjectKind::Blob, written.id),
                    mode,
                    size: Some(written.size()),
                    content_hash: Some(written.content_hash),
                });
            }
            Some(git2::ObjectType::Tree) => {
                let child = repo.find_tree(entry.id())?;
                let child_tree = import_tree(store, repo, &child, &relative, cache)?;
                entries.push(TreeEntry {
                    name: name.to_owned(),
                    path: relative,
                    entry_type: EntryType::Directory,
                    object: ObjectRef::new(ObjectKind::Tree, child_tree.id),
                    mode: EntryMode::Directory,
                    size: None,
                    content_hash: None,
                });
            }
            other => {
                return Err(GitImportError::UnsupportedEntry {
                    path: path_display,
                    detail: format!("object type {other:?}"),
                });
            }
        }
    }

    let written = write_tree(store, entries)?;
    cache.insert(tree.id(), written.id);
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{read_snapshot_files, InMemoryObjectStore};
    use std::process::Command;
    use tempfile::TempDir;

    fn git(cwd: &Path, args: &[&str]) {
        let status = Command::new("git")
            .args(args)
            .current_dir(cwd)
            .env("GIT_AUTHOR_NAME", "Importer")
            .env("GIT_AUTHOR_EMAIL", "importer@example.com")
            .env("GIT_COMMITTER_NAME", "Importer")
            .env("GIT_COMMITTER_EMAIL", "importer@example.com")
            .status()
            .expect("spawn git");
        assert!(status.success(), "git {args:?} failed");
    }

    fn make_linear_repo() -> TempDir {
        let dir = TempDir::new().expect("tempdir");
        let root = dir.path();
        git(root, &["init"]);
        git(root, &["config", "user.email", "importer@example.com"]);
        git(root, &["config", "user.name", "Importer"]);

        std::fs::write(root.join("a.txt"), b"one\n").unwrap();
        git(root, &["add", "a.txt"]);
        git(root, &["commit", "-m", "first"]);

        std::fs::write(root.join("a.txt"), b"two\n").unwrap();
        std::fs::write(root.join("b.txt"), b"bee\n").unwrap();
        git(root, &["add", "a.txt", "b.txt"]);
        git(root, &["commit", "-m", "second"]);

        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(root.join("src/main.rs"), b"fn main() {}\n").unwrap();
        git(root, &["add", "src/main.rs"]);
        git(root, &["commit", "-m", "third"]);

        dir
    }

    #[test]
    fn imports_linear_history_into_snapshots_and_changes() {
        let git_dir = make_linear_repo();
        let store = InMemoryObjectStore::new();
        let options = GitImportOptions::new(git_dir.path(), "repo_git_import_test");
        let result = git_import(&store, options).expect("import");

        assert_eq!(result.commits.len(), 3);
        assert_eq!(result.commits[0].message, "first");
        assert_eq!(result.commits[1].message, "second");
        assert_eq!(result.commits[2].message, "third");
        assert_eq!(result.head_snapshot, result.commits[2].snapshot_id);
        assert_eq!(result.git_to_snapshot.len(), 3);

        let files = read_snapshot_files(&store, &result.head_snapshot).expect("files");
        let paths: Vec<_> = files
            .keys()
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .collect();
        assert!(paths.iter().any(|p| p == "a.txt"));
        assert!(paths.iter().any(|p| p == "b.txt"));
        assert!(paths.iter().any(|p| p == "src/main.rs"));
        assert_eq!(
            files
                .get(Path::new("a.txt"))
                .map(|b| String::from_utf8_lossy(b).into_owned()),
            Some("two\n".to_owned())
        );
    }

    #[test]
    fn limit_imports_newest_n_commits() {
        let git_dir = make_linear_repo();
        let store = InMemoryObjectStore::new();
        let mut options = GitImportOptions::new(git_dir.path(), "repo_git_limit");
        options.limit = Some(2);
        let result = git_import(&store, options).expect("import");
        // Revwalk tip→root truncated to N, then reversed → the N newest commits.
        assert_eq!(result.commits.len(), 2);
        assert_eq!(result.commits[0].message, "second");
        assert_eq!(result.commits[1].message, "third");
    }

    #[test]
    fn incremental_import_skips_known_commits() {
        let git_dir = make_linear_repo();
        let store = InMemoryObjectStore::new();
        let first = git_import(
            &store,
            GitImportOptions::new(git_dir.path(), "repo_git_incremental"),
        )
        .expect("initial import");
        assert_eq!(first.commits.len(), 3);

        // Grow the Git history by one commit.
        std::fs::write(git_dir.path().join("d.txt"), b"dee\n").unwrap();
        git(git_dir.path(), &["add", "d.txt"]);
        git(git_dir.path(), &["commit", "-m", "fourth"]);

        let mut options = GitImportOptions::new(git_dir.path(), "repo_git_incremental");
        options.known_commits = first.git_to_snapshot.clone();
        let second = git_import(&store, options).expect("incremental import");

        assert_eq!(second.commits.len(), 1);
        assert_eq!(second.commits[0].message, "fourth");
        assert_eq!(second.head_snapshot, second.commits[0].snapshot_id);
        // The new snapshot's parent is the previously imported tip.
        let snapshot = crate::read_snapshot(&store, &second.head_snapshot).expect("snapshot");
        assert_eq!(snapshot.parents.len(), 1);
        assert_eq!(snapshot.parents[0].id, first.head_snapshot);
        // The returned map covers old and new commits.
        assert_eq!(second.git_to_snapshot.len(), 4);
    }

    #[test]
    fn incremental_import_with_known_tip_imports_nothing() {
        let git_dir = make_linear_repo();
        let store = InMemoryObjectStore::new();
        let first = git_import(
            &store,
            GitImportOptions::new(git_dir.path(), "repo_git_uptodate"),
        )
        .expect("initial import");

        let mut options = GitImportOptions::new(git_dir.path(), "repo_git_uptodate");
        options.known_commits = first.git_to_snapshot.clone();
        let second = git_import(&store, options).expect("no-op import");

        assert!(second.commits.is_empty());
        assert_eq!(second.head_snapshot, first.head_snapshot);
    }

    #[test]
    fn missing_ref_errors() {
        let git_dir = make_linear_repo();
        let store = InMemoryObjectStore::new();
        let mut options = GitImportOptions::new(git_dir.path(), "repo_git_missing");
        options.git_ref = "refs/heads/does-not-exist".to_owned();
        let err = git_import(&store, options).expect_err("should fail");
        assert!(matches!(err, GitImportError::RefNotFound { .. }));
    }
}
