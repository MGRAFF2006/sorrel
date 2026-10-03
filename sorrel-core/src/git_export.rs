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
    /// Safely update an existing destination checkout when exporting its active
    /// branch. Defaults to false for hosts whose source is the same worktree.
    pub checkout_existing_worktree: bool,
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
            checkout_existing_worktree: false,
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
    let ordered = topological_ancestors(store, options.tip_snapshot)?;
    for snapshot_id in &ordered {
        crate::validate_snapshot(store, snapshot_id)?;
    }
    let repo = open_or_init_repository(&options.git_path, options.init_if_missing)?;

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

    update_branch(
        &repo,
        &options.branch,
        &head_git_sha,
        options.checkout_existing_worktree,
    )?;

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

fn update_branch(
    repo: &git2::Repository,
    branch: &str,
    tip_sha: &str,
    checkout_existing_worktree: bool,
) -> GitExportResult<()> {
    let oid = git2::Oid::from_str(tip_sha)?;
    let commit = repo.find_commit(oid)?;
    let refname = format!("refs/heads/{branch}");
    // Capture HEAD state before creating the branch: when `init.defaultBranch`
    // matches the exported branch, HEAD resolves fine right after the ref
    // exists and the bootstrap checkout below would be skipped.
    let head_was_unborn = match repo.head() {
        Ok(_) => false,
        Err(error) if error.code() == git2::ErrorCode::UnbornBranch => true,
        Err(error) => return Err(error.into()),
    };
    if !git2::Reference::is_valid_name(&refname) {
        return Err(git2::Error::from_str("invalid export branch name").into());
    }
    let previous_tip = match repo.find_reference(&refname) {
        Ok(reference) => Some(
            reference
                .target()
                .ok_or_else(|| git2::Error::from_str("export branch is not a direct reference"))?,
        ),
        Err(error) if error.code() == git2::ErrorCode::NotFound => None,
        Err(error) => return Err(error.into()),
    };
    if checkout_existing_worktree
        && !head_was_unborn
        && !repo.is_bare()
        && repo.head()?.name()? == refname.as_str()
    {
        let mut status_options = git2::StatusOptions::new();
        status_options
            .include_untracked(false)
            .include_ignored(false)
            .update_index(false);
        if !repo.statuses(Some(&mut status_options))?.is_empty() {
            return Err(git2::Error::from_str(
                "Git destination has uncommitted tracked changes; commit or discard them before export",
            )
            .into());
        }
        let mut checkout = git2::build::CheckoutBuilder::new();
        checkout.safe().overwrite_ignored(false);
        repo.checkout_tree(commit.as_object(), Some(&mut checkout))?;
    }
    if head_was_unborn && !repo.is_bare() {
        let index = repo.index()?;
        if !index.is_empty() {
            return Err(git2::Error::from_str(
                "unborn Git destination has staged data; commit or clear it before export",
            )
            .into());
        }
        let tree = commit.tree()?;
        let missing_paths = preflight_bootstrap_checkout(repo, &tree)?;
        // Existing byte-identical files stay untouched (colocated bootstrap).
        // Safe checkout refuses new collisions before any branch is published.
        if !missing_paths.is_empty() {
            let mut checkout = git2::build::CheckoutBuilder::new();
            checkout
                .safe()
                .overwrite_ignored(false)
                .disable_pathspec_match(true);
            for path in missing_paths {
                checkout.path(path);
            }
            repo.checkout_tree(commit.as_object(), Some(&mut checkout))?;
        }
        // Fresh colocated repos must have an index matching the exported tree.
        let mut index = repo.index()?;
        index.read_tree(&tree)?;
        index.write()?;
    }
    match previous_tip {
        Some(previous) => {
            repo.reference_matching(&refname, oid, true, previous, "sorrel git export")?;
        }
        None => {
            repo.reference(&refname, oid, false, "sorrel git export")?;
        }
    }
    if head_was_unborn {
        repo.set_head(&refname)?;
    }
    Ok(())
}

/// Read-only collision preflight for a fresh checkout.
// git2 0.21's dry_run maps to libgit2 CHECKOUT_NONE, which does not inspect
// conflicts. Compare the target paths explicitly before the real safe checkout.
fn preflight_bootstrap_checkout(
    repo: &git2::Repository,
    tree: &git2::Tree<'_>,
) -> GitExportResult<Vec<PathBuf>> {
    let root = repo
        .workdir()
        .ok_or_else(|| git2::Error::from_str("Git checkout has no working directory"))?;
    let mut pending = vec![(tree.id(), PathBuf::new())];
    let mut missing_paths = Vec::new();
    while let Some((id, prefix)) = pending.pop() {
        let tree = repo.find_tree(id)?;
        for entry in &tree {
            let name = entry
                .name()
                .map_err(|_| git2::Error::from_str("Git tree path must be UTF-8"))?;
            let relative = prefix.join(name);
            let path = root.join(&relative);
            let metadata = match std::fs::symlink_metadata(&path) {
                Ok(metadata) => Some(metadata),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
                Err(error) => return Err(git2::Error::from_str(&error.to_string()).into()),
            };
            let directory = entry.kind() == Some(git2::ObjectType::Tree);
            if let Some(metadata) = &metadata {
                let safe = if directory {
                    metadata.is_dir() && !metadata.file_type().is_symlink()
                } else {
                    metadata.is_file()
                        && !metadata.file_type().is_symlink()
                        && std::fs::read(&path)
                            .map_err(|error| git2::Error::from_str(&error.to_string()))?
                            == repo.find_blob(entry.id())?.content()
                };
                if !safe {
                    return Err(git2::Error::from_str(&format!(
                        "Git export would overwrite existing destination path {}",
                        relative.display(),
                    ))
                    .into());
                }
            }
            if directory {
                pending.push((entry.id(), relative));
            } else if metadata.is_none() {
                missing_paths.push(relative);
            }
        }
    }
    Ok(missing_paths)
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

    fn checkout_history(store: &InMemoryObjectStore) -> (ObjectId, ObjectId) {
        let source = TempDir::new().unwrap();
        for (name, content) in [
            ("a.txt", "one\n"),
            ("removed.txt", "removed\n"),
            ("script", "echo hi\n"),
        ] {
            std::fs::write(source.path().join(name), content).unwrap();
        }
        let first =
            crate::materialize_snapshot(store, source.path(), SnapshotOptions::new("repo_export"))
                .unwrap();
        std::fs::write(source.path().join("a.txt"), "two\n").unwrap();
        std::fs::remove_file(source.path().join("removed.txt")).unwrap();
        std::fs::write(source.path().join("added.txt"), "added\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(
                source.path().join("script"),
                std::fs::Permissions::from_mode(0o755),
            )
            .unwrap();
        }
        let mut options = SnapshotOptions::new("repo_export");
        options.parents = vec![crate::ObjectRef::new(ObjectKind::Snapshot, first.id)];
        let second = crate::materialize_snapshot(store, source.path(), options).unwrap();
        (first.id, second.id)
    }

    #[test]
    fn existing_checkout_export_updates_files_index_deletions_and_modes() {
        let store = InMemoryObjectStore::new();
        let (first, second) = checkout_history(&store);
        let destination = TempDir::new().unwrap();
        git_export(&store, GitExportOptions::new(destination.path(), first)).unwrap();
        std::fs::write(destination.path().join("untracked.txt"), "keep\n").unwrap();
        let mut options = GitExportOptions::new(destination.path(), second);
        options.checkout_existing_worktree = true;
        let exported = git_export(&store, options).unwrap();
        assert_eq!(
            std::fs::read(destination.path().join("a.txt")).unwrap(),
            b"two\n"
        );
        assert_eq!(
            std::fs::read(destination.path().join("added.txt")).unwrap(),
            b"added\n"
        );
        assert!(!destination.path().join("removed.txt").exists());
        assert_eq!(
            std::fs::read(destination.path().join("untracked.txt")).unwrap(),
            b"keep\n"
        );
        let repo = git2::Repository::open(destination.path()).unwrap();
        assert_eq!(
            repo.head().unwrap().target().unwrap().to_string(),
            exported.head_git_sha
        );
        let mut options = git2::StatusOptions::new();
        options.include_untracked(false);
        assert!(repo.statuses(Some(&mut options)).unwrap().is_empty());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_ne!(
                std::fs::metadata(destination.path().join("script"))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o111,
                0
            );
        }
    }

    #[test]
    fn existing_checkout_export_refuses_dirty_staged_and_untracked_collisions() {
        let store = InMemoryObjectStore::new();
        let (first, second) = checkout_history(&store);
        for scenario in ["dirty", "staged", "untracked", "ignored"] {
            let destination = TempDir::new().unwrap();
            git_export(&store, GitExportOptions::new(destination.path(), first)).unwrap();
            let repo = git2::Repository::open(destination.path()).unwrap();
            let initial_tip = repo.head().unwrap().target();
            let changed = if matches!(scenario, "dirty" | "staged") {
                "a.txt"
            } else {
                "added.txt"
            };
            std::fs::write(destination.path().join(changed), "valuable local work\n").unwrap();
            if scenario == "staged" {
                let mut index = repo.index().unwrap();
                index.add_path(std::path::Path::new(changed)).unwrap();
                index.write().unwrap();
            }
            if scenario == "ignored" {
                std::fs::write(repo.path().join("info/exclude"), "added.txt\n").unwrap();
            }
            let original_index = std::fs::read(repo.path().join("index")).unwrap();
            let mut options = GitExportOptions::new(destination.path(), second);
            options.checkout_existing_worktree = true;
            assert!(git_export(&store, options).is_err(), "{scenario}");
            assert_eq!(repo.head().unwrap().target(), initial_tip, "{scenario}");
            assert_eq!(
                std::fs::read(repo.path().join("index")).unwrap(),
                original_index,
                "{scenario}"
            );
            assert_eq!(
                std::fs::read(destination.path().join(changed)).unwrap(),
                b"valuable local work\n",
                "{scenario}"
            );
            assert!(
                destination.path().join("removed.txt").exists(),
                "{scenario}"
            );
            if changed != "a.txt" {
                assert_eq!(
                    std::fs::read(destination.path().join("a.txt")).unwrap(),
                    b"one\n",
                    "{scenario}"
                );
            }
        }
    }

    #[test]
    fn existing_checkout_option_leaves_other_active_branches_alone() {
        let store = InMemoryObjectStore::new();
        let (first, second) = checkout_history(&store);
        let destination = TempDir::new().unwrap();
        git_export(&store, GitExportOptions::new(destination.path(), first)).unwrap();
        git(destination.path(), &["checkout", "-b", "other"]);
        std::fs::write(
            destination.path().join("a.txt"),
            "local other-branch work\n",
        )
        .unwrap();
        let repo = git2::Repository::open(destination.path()).unwrap();
        let original_index = std::fs::read(repo.path().join("index")).unwrap();
        let mut options = GitExportOptions::new(destination.path(), second);
        options.checkout_existing_worktree = true;
        git_export(&store, options).unwrap();
        assert_eq!(repo.head().unwrap().name().unwrap(), "refs/heads/other");
        assert_eq!(
            std::fs::read(destination.path().join("a.txt")).unwrap(),
            b"local other-branch work\n"
        );
        assert_eq!(
            std::fs::read(repo.path().join("index")).unwrap(),
            original_index
        );
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
    fn bootstrap_export_preserves_existing_destination_data_and_staged_index() {
        let store = InMemoryObjectStore::new();
        let tip = make_sorrel_history(&store);
        for already_git in [false, true] {
            let dest = TempDir::new().unwrap();
            if already_git {
                git2::Repository::init(dest.path()).unwrap();
            }
            std::fs::write(dest.path().join("a.txt"), b"unrecorded local bytes\n").unwrap();
            assert!(git_export(&store, GitExportOptions::new(dest.path(), tip)).is_err());
            assert_eq!(
                std::fs::read(dest.path().join("a.txt")).unwrap(),
                b"unrecorded local bytes\n"
            );
            let repo = git2::Repository::open(dest.path()).unwrap();
            assert!(repo.find_reference("refs/heads/main").is_err());
            assert!(repo.index().unwrap().is_empty());
        }
        let staged = TempDir::new().unwrap();
        let repo = git2::Repository::init(staged.path()).unwrap();
        std::fs::write(staged.path().join("keep.txt"), b"staged bytes\n").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(std::path::Path::new("keep.txt")).unwrap();
        index.write().unwrap();
        let original_index = std::fs::read(repo.path().join("index")).unwrap();
        assert!(git_export(&store, GitExportOptions::new(staged.path(), tip)).is_err());
        assert_eq!(
            std::fs::read(repo.path().join("index")).unwrap(),
            original_index
        );
        assert_eq!(
            std::fs::read(staged.path().join("keep.txt")).unwrap(),
            b"staged bytes\n"
        );
        assert!(!staged.path().join("a.txt").exists());
        assert!(repo.find_reference("refs/heads/main").is_err());
    }

    #[test]
    fn malformed_source_graph_is_rejected_before_destination_creation() {
        let store = InMemoryObjectStore::new();
        let blob = write_blob(&store, b"must not become repository metadata").unwrap();
        let tree = write_tree(
            &store,
            vec![TreeEntry {
                name: ".git".into(),
                path: ".git".into(),
                entry_type: EntryType::File,
                object: crate::ObjectRef::new(ObjectKind::Blob, blob.id),
                mode: EntryMode::Normal,
                size: Some(blob.size()),
                content_hash: Some(blob.content_hash),
            }],
        )
        .unwrap();
        let snapshot = write_snapshot(&store, tree.id, SnapshotOptions::new("repo")).unwrap();
        let parent = TempDir::new().unwrap();
        let destination = parent.path().join("must-not-exist");
        assert!(matches!(
            git_export(&store, GitExportOptions::new(&destination, snapshot.id)),
            Err(GitExportError::Snapshot(SnapshotError::InvalidPath { .. }))
        ));
        assert!(!destination.exists());
    }

    #[test]
    fn bootstrap_export_adopts_identical_worktree_files_without_overwriting_them() {
        let store = InMemoryObjectStore::new();
        let tip = make_sorrel_history(&store);
        let destination = TempDir::new().unwrap();
        std::fs::write(destination.path().join("a.txt"), b"two\n").unwrap();
        std::fs::write(destination.path().join("untracked.txt"), b"keep\n").unwrap();
        git_export(&store, GitExportOptions::new(destination.path(), tip)).unwrap();
        assert_eq!(
            std::fs::read(destination.path().join("a.txt")).unwrap(),
            b"two\n"
        );
        assert_eq!(
            std::fs::read(destination.path().join("untracked.txt")).unwrap(),
            b"keep\n"
        );
        let repo = git2::Repository::open(destination.path()).unwrap();
        assert!(repo.find_reference("refs/heads/main").is_ok());
        assert_eq!(repo.index().unwrap().len(), 1);
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
