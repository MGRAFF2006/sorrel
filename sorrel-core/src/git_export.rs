//! One-way export from Sorrel snapshots into a Git repository.
//!
//! Walks the snapshot DAG reachable from a tip (parents before children),
//! materializes each snapshot tree as a Git tree, and writes a commit.
//! Snapshots already present in an optional reverse map are reused so repeated
//! exports stay idempotent.
//!
//! Together with incremental [`crate::git_import`] (seeded `known_commits`)
//! this powers the CLI's colocated Git mirror (`sorrel git sync`).

use crate::{
    collect_ancestors, read_blob, read_snapshot, read_tree, ObjectId, ObjectKind, ObjectStore,
    ObjectStoreError, Principal, Snapshot, SnapshotError, Tree, TreeEntry,
};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    path::PathBuf,
};

/// Result type for Git export operations.
pub type GitExportResult<T> = Result<T, GitExportError>;

/// Errors returned while exporting Sorrel history into Git.
#[derive(Debug, thiserror::Error)]
pub enum GitExportError {
    /// Underlying object store failure.
    #[error(transparent)]
    ObjectStore(#[from] ObjectStoreError),

    /// Snapshot/tree/blob read failure.
    #[error(transparent)]
    Snapshot(#[from] SnapshotError),

    /// History walk failure.
    #[error(transparent)]
    History(#[from] crate::HistoryError),

    /// libgit2 / git2 failure.
    #[error(transparent)]
    Git(#[from] git2::Error),

    /// Destination path exists but is not a Git repository and could not be initialized.
    #[error("git destination is not a repository: {path}")]
    NotARepository {
        /// Path that failed to open or init as Git.
        path: String,
    },

    /// Unsupported Sorrel tree entry while building a Git tree.
    #[error("unsupported tree entry at {path}: {detail}")]
    UnsupportedEntry {
        /// Path inside the snapshot tree.
        path: String,
        /// Human-readable reason.
        detail: String,
    },
}

/// Options controlling a one-way Sorrel → Git export.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GitExportOptions {
    /// Filesystem path for the destination Git repository (working tree or bare).
    pub git_path: PathBuf,
    /// Branch name to update (created if missing). Default `main`.
    pub branch: String,
    /// Tip snapshot to export (inclusive ancestors).
    pub tip_snapshot: ObjectId,
    /// Optional snapshot id → existing Git SHA map (skip re-export).
    pub snapshot_to_git: BTreeMap<ObjectId, String>,
    /// When true, create the destination repo with `git init` if missing.
    pub init_if_missing: bool,
}

impl GitExportOptions {
    /// Builds export options for `git_path` targeting `tip_snapshot`.
    #[must_use]
    pub fn new(git_path: impl Into<PathBuf>, tip_snapshot: ObjectId) -> Self {
        Self {
            git_path: git_path.into(),
            branch: "main".to_owned(),
            tip_snapshot,
            snapshot_to_git: BTreeMap::new(),
            init_if_missing: true,
        }
    }
}

/// One exported Sorrel snapshot mapped onto a Git commit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExportedCommit {
    /// Sorrel snapshot that was exported.
    pub snapshot_id: ObjectId,
    /// Full Git commit SHA (hex).
    pub git_sha: String,
    /// Commit subject line.
    pub message: String,
    /// True when the commit was newly written (false if reused from the map).
    pub created: bool,
}

/// Outcome of [`git_export`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExportResult {
    /// Commits in chronological order (oldest first).
    pub commits: Vec<ExportedCommit>,
    /// Git SHA of the tip commit after export.
    pub head_git_sha: String,
    /// Updated snapshot → Git SHA map (includes reused entries).
    pub snapshot_to_git: BTreeMap<ObjectId, String>,
    /// Branch that was updated.
    pub branch: String,
}

/// Exports Sorrel snapshot history into `options.git_path` as Git commits.
///
/// Snapshots are walked in topological order (parents before children). Merge
/// snapshots become Git merge commits when every parent was also exported.
pub fn git_export(
    store: &impl ObjectStore,
    options: GitExportOptions,
) -> GitExportResult<ExportResult> {
    let repo = open_or_init_repository(&options.git_path, options.init_if_missing)?;
    let ordered = topological_ancestors(store, options.tip_snapshot)?;

    let mut snapshot_to_git = options.snapshot_to_git.clone();
    let mut commits = Vec::with_capacity(ordered.len());
    let mut tree_cache: BTreeMap<ObjectId, git2::Oid> = BTreeMap::new();

    for snapshot_id in ordered {
        if let Some(existing) = snapshot_to_git.get(&snapshot_id).cloned() {
            if let Ok(oid) = git2::Oid::from_str(&existing) {
                if repo.find_commit(oid).is_ok() {
                    let snapshot = read_snapshot(store, &snapshot_id)?;
                    commits.push(ExportedCommit {
                        snapshot_id,
                        git_sha: existing,
                        message: snapshot_message(&snapshot),
                        created: false,
                    });
                    continue;
                }
            }
            // Mapped SHA is missing in this destination repo — re-export.
            snapshot_to_git.remove(&snapshot_id);
        }

        let snapshot = read_snapshot(store, &snapshot_id)?;
        let tree = read_tree(store, &snapshot.root_tree.id)?;
        let git_tree = export_tree(store, &repo, &tree, &mut tree_cache)?;

        let mut parent_commits = Vec::new();
        for parent in &snapshot.parents {
            if let Some(sha) = snapshot_to_git.get(&parent.id) {
                if let Ok(oid) = git2::Oid::from_str(sha) {
                    if let Ok(commit) = repo.find_commit(oid) {
                        parent_commits.push(commit);
                    }
                }
            }
        }
        let parent_refs: Vec<&git2::Commit<'_>> = parent_commits.iter().collect();

        let message = snapshot_message(&snapshot);
        let signature = signature_from_principal(&snapshot.author, &snapshot.created_at)?;
        let commit_oid = repo.commit(
            None,
            &signature,
            &signature,
            &message,
            &git_tree,
            &parent_refs,
        )?;
        let git_sha = commit_oid.to_string();
        snapshot_to_git.insert(snapshot_id, git_sha.clone());
        commits.push(ExportedCommit {
            snapshot_id,
            git_sha,
            message,
            created: true,
        });
    }

    let head_git_sha = snapshot_to_git
        .get(&options.tip_snapshot)
        .cloned()
        .ok_or_else(|| GitExportError::UnsupportedEntry {
            path: String::new(),
            detail: "tip snapshot missing from export map".to_owned(),
        })?;

    update_branch(&repo, &options.branch, &head_git_sha)?;

    Ok(ExportResult {
        commits,
        head_git_sha,
        snapshot_to_git,
        branch: options.branch,
    })
}

fn open_or_init_repository(
    path: &std::path::Path,
    init_if_missing: bool,
) -> GitExportResult<git2::Repository> {
    match git2::Repository::open(path) {
        Ok(repo) => Ok(repo),
        Err(_) if init_if_missing => {
            std::fs::create_dir_all(path).map_err(|err| git2::Error::from_str(&err.to_string()))?;
            // Prefer a non-bare working tree so `git checkout` is usable.
            Ok(git2::Repository::init(path)?)
        }
        Err(_) => Err(GitExportError::NotARepository {
            path: path.display().to_string(),
        }),
    }
}

fn update_branch(repo: &git2::Repository, branch: &str, tip_sha: &str) -> GitExportResult<()> {
    let oid = git2::Oid::from_str(tip_sha)?;
    let commit = repo.find_commit(oid)?;
    let refname = format!("refs/heads/{branch}");
    // Capture HEAD state before creating the branch: when `init.defaultBranch`
    // matches the exported branch, HEAD resolves fine right after the ref
    // exists and the bootstrap checkout below would be skipped.
    let head_was_unborn = repo.head().is_err();
    match repo.find_reference(&refname) {
        Ok(mut reference) => {
            reference.set_target(oid, "sorrel git export")?;
        }
        Err(_) => {
            repo.reference(&refname, oid, true, "sorrel git export")?;
        }
    }
    // Point HEAD at the branch when this is a fresh repo with no HEAD target yet.
    if head_was_unborn {
        repo.set_head(&refname)?;
        // Best-effort checkout for non-bare repos so the worktree matches.
        if !repo.is_bare() {
            let mut checkout = git2::build::CheckoutBuilder::new();
            checkout.force();
            repo.checkout_tree(commit.as_object(), Some(&mut checkout))?;
            // Align the index with the checked-out tree; otherwise a fresh
            // colocated repo is left with an empty index and the next `git
            // commit` would silently delete every exported file.
            let mut index = repo.index()?;
            index.read_tree(&commit.tree()?)?;
            index.write()?;
        }
    }
    Ok(())
}

/// Returns ancestors of `tip` (including `tip`) in topological order: parents first.
fn topological_ancestors(
    store: &impl ObjectStore,
    tip: ObjectId,
) -> GitExportResult<Vec<ObjectId>> {
    let ancestors = collect_ancestors(store, tip)?;
    let mut indegree: BTreeMap<ObjectId, usize> = BTreeMap::new();
    let mut children: BTreeMap<ObjectId, Vec<ObjectId>> = BTreeMap::new();

    for id in &ancestors {
        indegree.entry(*id).or_insert(0);
        let snapshot = read_snapshot(store, id)?;
        for parent in &snapshot.parents {
            if ancestors.contains(&parent.id) {
                *indegree.entry(*id).or_insert(0) += 1;
                children.entry(parent.id).or_default().push(*id);
            }
        }
    }

    let mut queue: VecDeque<ObjectId> = indegree
        .iter()
        .filter(|(_, deg)| **deg == 0)
        .map(|(id, _)| *id)
        .collect();
    // Stable order among roots.
    let mut roots: Vec<ObjectId> = queue.drain(..).collect();
    roots.sort();
    queue.extend(roots);

    let mut ordered = Vec::with_capacity(ancestors.len());
    let mut seen = BTreeSet::new();
    while let Some(id) = queue.pop_front() {
        if !seen.insert(id) {
            continue;
        }
        ordered.push(id);
        if let Some(kids) = children.get(&id) {
            let mut next = kids.clone();
            next.sort();
            for child in next {
                if let Some(deg) = indegree.get_mut(&child) {
                    *deg = deg.saturating_sub(1);
                    if *deg == 0 {
                        queue.push_back(child);
                    }
                }
            }
        }
    }

    if ordered.len() != ancestors.len() {
        // Cycle or incomplete graph — fall back to sorted ids (still deterministic).
        let mut fallback: Vec<_> = ancestors.into_iter().collect();
        fallback.sort();
        return Ok(fallback);
    }
    Ok(ordered)
}

fn snapshot_message(snapshot: &Snapshot) -> String {
    snapshot
        .message
        .as_deref()
        .filter(|m| !m.is_empty())
        .unwrap_or("(no message)")
        .to_owned()
}

fn signature_from_principal(
    author: &Principal,
    created_at: &str,
) -> GitExportResult<git2::Signature<'static>> {
    let (name, email) = parse_identity(author);
    let time = rfc3339_to_git_time(created_at);
    Ok(git2::Signature::new(&name, &email, &time)?)
}

fn parse_identity(author: &Principal) -> (String, String) {
    // Prefer "Name <email>" embedded in id (matches git_import).
    if let Some((name, email)) = split_angle_email(&author.id) {
        return (name, email);
    }
    if let Some(display) = author.display_name.as_deref() {
        if let Some((name, email)) = split_angle_email(display) {
            return (name, email);
        }
        return (display.to_owned(), format!("{display}@sorrel.local"));
    }
    (
        author.id.clone(),
        format!("{}@sorrel.local", author.id.replace(' ', "_")),
    )
}

fn split_angle_email(value: &str) -> Option<(String, String)> {
    let start = value.find('<')?;
    let end = value.find('>')?;
    if end <= start + 1 {
        return None;
    }
    let name = value[..start].trim();
    let email = value[start + 1..end].trim();
    if name.is_empty() || email.is_empty() {
        return None;
    }
    Some((name.to_owned(), email.to_owned()))
}

fn rfc3339_to_git_time(value: &str) -> git2::Time {
    // Expect `YYYY-MM-DDTHH:MM:SSZ` as produced by git_import / CLI.
    let secs = parse_rfc3339_secs(value).unwrap_or(0);
    git2::Time::new(secs, 0)
}

fn parse_rfc3339_secs(value: &str) -> Option<i64> {
    let value = value.trim().trim_end_matches('Z');
    let (date, time) = value.split_once('T')?;
    let mut d = date.split('-');
    let year: i64 = d.next()?.parse().ok()?;
    let month: u32 = d.next()?.parse().ok()?;
    let day: u32 = d.next()?.parse().ok()?;
    let mut t = time.split(':');
    let hour: u32 = t.next()?.parse().ok()?;
    let minute: u32 = t.next()?.parse().ok()?;
    let second: u32 = t.next()?.parse().ok()?;
    let days = days_from_civil(year, month, day)?;
    Some(days * 86_400 + i64::from(hour) * 3_600 + i64::from(minute) * 60 + i64::from(second))
}

fn days_from_civil(year: i64, month: u32, day: u32) -> Option<i64> {
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    let y = if month <= 2 { year - 1 } else { year };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = i64::from(if month > 2 { month - 3 } else { month + 9 });
    let doy = (153 * mp + 2) / 5 + i64::from(day) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Some(era * 146_097 + doe - 719_468)
}

fn export_tree<'repo>(
    store: &impl ObjectStore,
    repo: &'repo git2::Repository,
    tree: &Tree,
    cache: &mut BTreeMap<ObjectId, git2::Oid>,
) -> GitExportResult<git2::Tree<'repo>> {
    if let Some(oid) = cache.get(&tree.id) {
        return Ok(repo.find_tree(*oid)?);
    }

    let mut builder = repo.treebuilder(None)?;
    for entry in &tree.entries {
        insert_tree_entry(store, repo, &mut builder, entry, cache)?;
    }
    let oid = builder.write()?;
    cache.insert(tree.id, oid);
    Ok(repo.find_tree(oid)?)
}

fn insert_tree_entry(
    store: &impl ObjectStore,
    repo: &git2::Repository,
    builder: &mut git2::TreeBuilder<'_>,
    entry: &TreeEntry,
    cache: &mut BTreeMap<ObjectId, git2::Oid>,
) -> GitExportResult<()> {
    let path_display = entry.path.to_string_lossy().replace('\\', "/");
    match entry.object.kind {
        ObjectKind::Blob => {
            let blob = read_blob(store, &entry.object.id)?;
            let oid = repo.blob(&blob.content)?;
            let filemode = match entry.mode {
                crate::EntryMode::Executable => 0o100755,
                _ => 0o100644,
            };
            builder.insert(&entry.name, oid, filemode)?;
        }
        ObjectKind::Tree => {
            let child = read_tree(store, &entry.object.id)?;
            let child_tree = export_tree(store, repo, &child, cache)?;
            builder.insert(&entry.name, child_tree.id(), 0o040000)?;
        }
        other => {
            return Err(GitExportError::UnsupportedEntry {
                path: path_display,
                detail: format!("object kind {other:?}"),
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        git_import, write_blob, write_snapshot, write_tree, EntryMode, EntryType, GitImportOptions,
        InMemoryObjectStore, SnapshotOptions, TreeEntry,
    };
    use std::process::Command;
    use tempfile::TempDir;

    fn git(cwd: &std::path::Path, args: &[&str]) {
        let status = Command::new("git")
            .args(args)
            .current_dir(cwd)
            .env("GIT_AUTHOR_NAME", "Exporter")
            .env("GIT_AUTHOR_EMAIL", "exporter@example.com")
            .env("GIT_COMMITTER_NAME", "Exporter")
            .env("GIT_COMMITTER_EMAIL", "exporter@example.com")
            .status()
            .expect("spawn git");
        assert!(status.success(), "git {args:?} failed");
    }

    fn make_sorrel_history(store: &InMemoryObjectStore) -> ObjectId {
        let blob1 = write_blob(store, b"one\n").unwrap();
        let tree1 = write_tree(
            store,
            vec![TreeEntry {
                name: "a.txt".into(),
                path: "a.txt".into(),
                entry_type: EntryType::File,
                object: crate::ObjectRef::new(ObjectKind::Blob, blob1.id),
                mode: EntryMode::Normal,
                size: Some(blob1.size()),
                content_hash: Some(blob1.content_hash),
            }],
        )
        .unwrap();
        let mut opts = SnapshotOptions::new("repo_export");
        opts.message = Some("first".into());
        opts.created_at = "2024-01-01T00:00:00Z".into();
        let snap1 = write_snapshot(store, tree1.id, opts).unwrap();

        let blob2 = write_blob(store, b"two\n").unwrap();
        let tree2 = write_tree(
            store,
            vec![TreeEntry {
                name: "a.txt".into(),
                path: "a.txt".into(),
                entry_type: EntryType::File,
                object: crate::ObjectRef::new(ObjectKind::Blob, blob2.id),
                mode: EntryMode::Normal,
                size: Some(blob2.size()),
                content_hash: Some(blob2.content_hash),
            }],
        )
        .unwrap();
        let mut opts = SnapshotOptions::new("repo_export");
        opts.parents = vec![crate::ObjectRef::new(ObjectKind::Snapshot, snap1.id)];
        opts.message = Some("second".into());
        opts.created_at = "2024-01-02T00:00:00Z".into();
        write_snapshot(store, tree2.id, opts).unwrap().id
    }

    #[test]
    fn exports_linear_history_to_git() {
        let store = InMemoryObjectStore::new();
        let tip = make_sorrel_history(&store);
        let dest = TempDir::new().unwrap();

        let result = git_export(&store, GitExportOptions::new(dest.path(), tip)).expect("export");
        assert_eq!(result.commits.len(), 2);
        assert!(result.commits.iter().all(|c| c.created));
        assert_eq!(result.commits[0].message, "first");
        assert_eq!(result.commits[1].message, "second");
        assert_eq!(result.branch, "main");

        // The fresh checkout must populate the index too, or the next `git
        // commit` in a colocated repo would delete the exported files.
        let ls = Command::new("git")
            .args(["ls-files"])
            .current_dir(dest.path())
            .output()
            .unwrap();
        assert!(String::from_utf8_lossy(&ls.stdout).contains("a.txt"));

        git(dest.path(), &["checkout", "main"]);
        let content = std::fs::read_to_string(dest.path().join("a.txt")).unwrap();
        assert_eq!(content, "two\n");

        let log = Command::new("git")
            .args(["log", "--oneline", "--reverse"])
            .current_dir(dest.path())
            .output()
            .unwrap();
        let log = String::from_utf8_lossy(&log.stdout);
        assert!(log.contains("first"));
        assert!(log.contains("second"));
    }

    #[test]
    fn reexport_reuses_existing_map() {
        let store = InMemoryObjectStore::new();
        let tip = make_sorrel_history(&store);
        let dest = TempDir::new().unwrap();

        let first = git_export(&store, GitExportOptions::new(dest.path(), tip)).expect("export");
        let mut options = GitExportOptions::new(dest.path(), tip);
        options.snapshot_to_git = first.snapshot_to_git.clone();
        let second = git_export(&store, options).expect("re-export");
        assert!(second.commits.iter().all(|c| !c.created));
        assert_eq!(second.head_git_sha, first.head_git_sha);
    }

    #[test]
    fn round_trip_import_then_export() {
        let git_src = TempDir::new().unwrap();
        let root = git_src.path();
        git(root, &["init"]);
        git(root, &["config", "user.email", "rt@example.com"]);
        git(root, &["config", "user.name", "RoundTrip"]);
        std::fs::write(root.join("x.txt"), b"hello\n").unwrap();
        git(root, &["add", "x.txt"]);
        git(root, &["commit", "-m", "imported"]);

        let store = InMemoryObjectStore::new();
        let imported =
            git_import(&store, GitImportOptions::new(root, "repo_roundtrip")).expect("import");

        let dest = TempDir::new().unwrap();
        let exported = git_export(
            &store,
            GitExportOptions::new(dest.path(), imported.head_snapshot),
        )
        .expect("export");
        // empty base + imported commit
        assert!(!exported.commits.is_empty());
        git(dest.path(), &["checkout", "main"]);
        assert_eq!(
            std::fs::read_to_string(dest.path().join("x.txt")).unwrap(),
            "hello\n"
        );
    }

    #[test]
    fn export_to_fresh_repo_ignores_foreign_shas() {
        let git_src = TempDir::new().unwrap();
        let root = git_src.path();
        git(root, &["init"]);
        git(root, &["config", "user.email", "rt@example.com"]);
        git(root, &["config", "user.name", "RoundTrip"]);
        std::fs::write(root.join("x.txt"), b"hello\n").unwrap();
        git(root, &["add", "x.txt"]);
        git(root, &["commit", "-m", "imported"]);

        let store = InMemoryObjectStore::new();
        let imported =
            git_import(&store, GitImportOptions::new(root, "repo_foreign_map")).expect("import");

        let dest = TempDir::new().unwrap();
        let mut options = GitExportOptions::new(dest.path(), imported.head_snapshot);
        options.snapshot_to_git = imported
            .git_to_snapshot
            .iter()
            .map(|(sha, id)| (*id, sha.clone()))
            .collect();
        let exported = git_export(&store, options).expect("export");
        assert!(exported.commits.iter().any(|c| c.created));
        git(dest.path(), &["checkout", "main"]);
        assert_eq!(
            std::fs::read_to_string(dest.path().join("x.txt")).unwrap(),
            "hello\n"
        );
    }
}
