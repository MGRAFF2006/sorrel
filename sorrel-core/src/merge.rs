//! Snapshot-level three-way merge.
//!
//! [`merge_snapshots`] merges `ours` and `theirs` against an explicit common
//! ancestor `base` using path-level [`snapshot_diff`](crate::snapshot_diff)
//! plus content-level [`merge3`](crate::merge3::merge3) for diverging file
//! edits. Clean merges write a merge snapshot (parents `[ours, theirs]`) and a
//! [`MergeResult`](crate::MergeResult) with status `clean`. Conflicts write
//! first-class [`Conflict`](crate::Conflict) objects and a conflicted
//! [`MergeResult`](crate::MergeResult) without a merged snapshot.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use crate::merge3::{self, MergeOutcome as TextMergeOutcome};
use crate::{
    conflict_with_type, read_blob, read_snapshot, read_tree, snapshot_diff, write_blob,
    write_conflict, write_merge_result, write_snapshot, write_tree, ChangeError, Conflict,
    ConflictError, ConflictSides, ConflictType, EntryMode, EntryType, MergeResult,
    MergeResultStatus, ObjectId, ObjectKind, ObjectRef, ObjectStore, ObjectStoreError,
    PathChangeKind, Principal, SnapshotError, SnapshotOptions, Tree, TreeEntry,
};

/// Errors returned while merging snapshots.
#[derive(Debug, thiserror::Error)]
pub enum MergeError {
    /// The underlying object store failed.
    #[error(transparent)]
    Store(#[from] ObjectStoreError),
    /// Reading or writing snapshot/tree/blob objects failed.
    #[error(transparent)]
    Snapshot(#[from] SnapshotError),
    /// Computing a path-level snapshot diff failed.
    #[error(transparent)]
    Change(#[from] ChangeError),
    /// Reading or writing conflict / merge-result objects failed.
    #[error(transparent)]
    Conflict(#[from] ConflictError),
}

/// Options used when writing a merge snapshot / merge-result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MergeOptions {
    /// Merge author.
    pub author: Principal,
    /// Repository identifier for the merged snapshot.
    pub repo: String,
    /// Merge snapshot / merge-result message.
    pub message: String,
    /// Protocol timestamp string. Deterministic by default, like
    /// [`SnapshotOptions::new`].
    pub created_at: String,
}

impl MergeOptions {
    /// Builds merge options with a deterministic timestamp.
    #[must_use]
    pub fn new(author: Principal, repo: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            author,
            repo: repo.into(),
            message: message.into(),
            created_at: "1970-01-01T00:00:00Z".to_owned(),
        }
    }
}

/// File-level view of one tree entry, sufficient to rebuild it.
#[derive(Clone, Debug, Eq, PartialEq)]
struct FileEntry {
    blob: ObjectId,
    mode: EntryMode,
    size: Option<u64>,
    content_hash: Option<ObjectId>,
}

/// Merges `ours` and `theirs` against the given `base` snapshot.
///
/// Path-level changes come from [`snapshot_diff`](crate::snapshot_diff)
/// (`base → ours` and `base → theirs`). Diverging content edits are resolved
/// with [`merge3`](crate::merge3::merge3). Returns a stored
/// [`MergeResult`](crate::MergeResult): clean merges include
/// `mergedSnapshot`; conflicted merges list stored conflict object ids and do
/// not write a merged snapshot.
pub fn merge_snapshots(
    store: &impl ObjectStore,
    base: &ObjectId,
    ours: &ObjectId,
    theirs: &ObjectId,
    options: &MergeOptions,
) -> Result<MergeResult, MergeError> {
    let base_files = collect_file_entries(store, base)?;
    let our_files = collect_file_entries(store, ours)?;
    let their_files = collect_file_entries(store, theirs)?;

    let our_diff = snapshot_diff(store, base, ours)?;
    let their_diff = snapshot_diff(store, base, theirs)?;

    let our_changes = file_change_map(&our_diff.changes, &base_files, &our_files, &their_files);
    let their_changes = file_change_map(&their_diff.changes, &base_files, &our_files, &their_files);

    let mut merged = base_files.clone();
    let mut conflicts: BTreeMap<PathBuf, Conflict> = BTreeMap::new();

    let paths: BTreeSet<PathBuf> = our_changes
        .keys()
        .chain(their_changes.keys())
        .cloned()
        .collect();

    for path in paths {
        let our_change = our_changes.get(&path).copied();
        let their_change = their_changes.get(&path).copied();

        match (our_change, their_change) {
            (Some(kind), None) => apply_one_sided(&mut merged, &path, kind, &our_files),
            (None, Some(kind)) => apply_one_sided(&mut merged, &path, kind, &their_files),
            (None, None) => {}
            (Some(our_kind), Some(their_kind)) => {
                resolve_both_changed(
                    store,
                    &options.repo,
                    &path,
                    our_kind,
                    their_kind,
                    &base_files,
                    &our_files,
                    &their_files,
                    &mut merged,
                    &mut conflicts,
                )?;
            }
        }
    }

    if conflicts.is_empty() {
        let root_tree = write_merged_tree(store, &merged)?;
        let mut snapshot_options = SnapshotOptions::new(options.repo.clone());
        snapshot_options.parents = vec![
            ObjectRef::new(ObjectKind::Snapshot, *ours),
            ObjectRef::new(ObjectKind::Snapshot, *theirs),
        ];
        snapshot_options.message = Some(options.message.clone());
        snapshot_options.created_at = options.created_at.clone();
        snapshot_options.author = options.author.clone();
        let snapshot = write_snapshot(store, root_tree.id, snapshot_options)?;

        let result = MergeResult {
            id: ObjectId::from_bytes([0; 32]),
            repo_id: options.repo.clone(),
            base_snapshot: *base,
            ours_snapshot: *ours,
            theirs_snapshot: *theirs,
            status: MergeResultStatus::Clean,
            merged_snapshot: Some(snapshot.id),
            conflicts: Vec::new(),
        };
        return Ok(write_merge_result(store, &result)?);
    }

    let conflict_ids = conflicts
        .values()
        .map(|conflict| conflict.id)
        .collect::<Vec<_>>();
    let result = MergeResult {
        id: ObjectId::from_bytes([0; 32]),
        repo_id: options.repo.clone(),
        base_snapshot: *base,
        ours_snapshot: *ours,
        theirs_snapshot: *theirs,
        status: MergeResultStatus::Conflicted,
        merged_snapshot: None,
        conflicts: conflict_ids,
    };
    Ok(write_merge_result(store, &result)?)
}

fn file_change_map(
    changes: &[crate::PathChange],
    base_files: &BTreeMap<PathBuf, FileEntry>,
    our_files: &BTreeMap<PathBuf, FileEntry>,
    their_files: &BTreeMap<PathBuf, FileEntry>,
) -> BTreeMap<PathBuf, PathChangeKind> {
    changes
        .iter()
        .filter(|change| {
            base_files.contains_key(&change.path)
                || our_files.contains_key(&change.path)
                || their_files.contains_key(&change.path)
        })
        .map(|change| (change.path.clone(), change.kind))
        .collect()
}

fn apply_one_sided(
    merged: &mut BTreeMap<PathBuf, FileEntry>,
    path: &Path,
    kind: PathChangeKind,
    side_files: &BTreeMap<PathBuf, FileEntry>,
) {
    match kind {
        PathChangeKind::Added | PathChangeKind::Modified => {
            if let Some(entry) = side_files.get(path) {
                merged.insert(path.to_path_buf(), entry.clone());
            }
        }
        PathChangeKind::Deleted => {
            merged.remove(path);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn resolve_both_changed(
    store: &impl ObjectStore,
    repo: &str,
    path: &Path,
    our_kind: PathChangeKind,
    their_kind: PathChangeKind,
    base_files: &BTreeMap<PathBuf, FileEntry>,
    our_files: &BTreeMap<PathBuf, FileEntry>,
    their_files: &BTreeMap<PathBuf, FileEntry>,
    merged: &mut BTreeMap<PathBuf, FileEntry>,
    conflicts: &mut BTreeMap<PathBuf, Conflict>,
) -> Result<(), MergeError> {
    let our_entry = our_files.get(path);
    let their_entry = their_files.get(path);
    let sides = ConflictSides {
        base: base_files.get(path).map(|entry| entry.blob),
        ours: our_entry.map(|entry| entry.blob),
        theirs: their_entry.map(|entry| entry.blob),
    };

    // Identical changes on both sides collapse.
    if our_entry == their_entry {
        match our_entry {
            Some(entry) => {
                merged.insert(path.to_path_buf(), entry.clone());
            }
            None => {
                merged.remove(path);
            }
        }
        return Ok(());
    }

    match (our_kind, their_kind) {
        (PathChangeKind::Deleted, PathChangeKind::Deleted) => {
            merged.remove(path);
            Ok(())
        }
        (PathChangeKind::Modified, PathChangeKind::Deleted)
        | (PathChangeKind::Deleted, PathChangeKind::Modified)
        | (PathChangeKind::Added, PathChangeKind::Deleted)
        | (PathChangeKind::Deleted, PathChangeKind::Added) => {
            // Keep the modified/added version; record modify_delete.
            let kept = our_entry.or(their_entry);
            if let Some(entry) = kept {
                merged.insert(path.to_path_buf(), entry.clone());
            } else {
                merged.remove(path);
            }
            let conflict = write_conflict(
                store,
                &conflict_with_type(repo, path, ConflictType::ModifyDelete, sides, []),
            )?;
            conflicts.insert(path.to_path_buf(), conflict);
            Ok(())
        }
        (PathChangeKind::Added, PathChangeKind::Added) => merge_file_contents(
            store,
            repo,
            path,
            &[],
            our_entry,
            their_entry,
            sides,
            ConflictType::AddAdd,
            merged,
            conflicts,
        ),
        (PathChangeKind::Modified, PathChangeKind::Modified)
        | (PathChangeKind::Added, PathChangeKind::Modified)
        | (PathChangeKind::Modified, PathChangeKind::Added) => {
            let base_bytes = match base_files.get(path) {
                Some(entry) => read_blob(store, &entry.blob)?.content,
                None => Vec::new(),
            };
            merge_file_contents(
                store,
                repo,
                path,
                &base_bytes,
                our_entry,
                their_entry,
                sides,
                ConflictType::Content,
                merged,
                conflicts,
            )
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn merge_file_contents(
    store: &impl ObjectStore,
    repo: &str,
    path: &Path,
    base_bytes: &[u8],
    our_entry: Option<&FileEntry>,
    their_entry: Option<&FileEntry>,
    sides: ConflictSides,
    conflict_on_text: ConflictType,
    merged: &mut BTreeMap<PathBuf, FileEntry>,
    conflicts: &mut BTreeMap<PathBuf, Conflict>,
) -> Result<(), MergeError> {
    let ours_bytes = match our_entry {
        Some(entry) => read_blob(store, &entry.blob)?.content,
        None => Vec::new(),
    };
    let theirs_bytes = match their_entry {
        Some(entry) => read_blob(store, &entry.blob)?.content,
        None => Vec::new(),
    };

    // Prefer OURS in the merged tree on any conflict.
    if let Some(entry) = our_entry {
        merged.insert(path.to_path_buf(), entry.clone());
    } else if let Some(entry) = their_entry {
        merged.insert(path.to_path_buf(), entry.clone());
    } else {
        merged.remove(path);
    }

    match merge3::merge3(base_bytes, &ours_bytes, &theirs_bytes) {
        TextMergeOutcome::Merged(bytes) => {
            let mode = our_entry
                .or(their_entry)
                .map(|entry| entry.mode)
                .unwrap_or(EntryMode::Normal);
            let blob = write_blob(store, &bytes)?;
            merged.insert(
                path.to_path_buf(),
                FileEntry {
                    blob: blob.id,
                    mode,
                    size: Some(blob.size()),
                    content_hash: Some(blob.content_hash),
                },
            );
            Ok(())
        }
        TextMergeOutcome::Conflicted { hunks, .. } => {
            let conflict = write_conflict(
                store,
                &conflict_with_type(repo, path, conflict_on_text, sides, hunks),
            )?;
            conflicts.insert(path.to_path_buf(), conflict);
            Ok(())
        }
        TextMergeOutcome::Binary => {
            let conflict = write_conflict(
                store,
                &conflict_with_type(repo, path, ConflictType::Binary, sides, []),
            )?;
            conflicts.insert(path.to_path_buf(), conflict);
            Ok(())
        }
    }
}

/// Flattens a snapshot into `path -> file entry`, walking all trees.
fn collect_file_entries(
    store: &impl ObjectStore,
    snapshot_id: &ObjectId,
) -> Result<BTreeMap<PathBuf, FileEntry>, MergeError> {
    let snapshot = read_snapshot(store, snapshot_id)?;
    let mut files = BTreeMap::new();
    collect_tree_file_entries(store, snapshot.root_tree.id, &mut files)?;
    Ok(files)
}

fn collect_tree_file_entries(
    store: &impl ObjectStore,
    tree_id: ObjectId,
    files: &mut BTreeMap<PathBuf, FileEntry>,
) -> Result<(), MergeError> {
    let tree = read_tree(store, &tree_id)?;
    for entry in tree.entries {
        match entry.entry_type {
            EntryType::Directory => collect_tree_file_entries(store, entry.object.id, files)?,
            EntryType::File => {
                files.insert(
                    entry.path,
                    FileEntry {
                        blob: entry.object.id,
                        mode: entry.mode,
                        size: entry.size,
                        content_hash: entry.content_hash,
                    },
                );
            }
        }
    }
    Ok(())
}

enum Node {
    File(FileEntry),
    Dir(BTreeMap<String, Node>),
}

/// Rebuilds nested tree objects from a flat merged file map and writes them.
fn write_merged_tree(
    store: &impl ObjectStore,
    files: &BTreeMap<PathBuf, FileEntry>,
) -> Result<Tree, MergeError> {
    let mut root: BTreeMap<String, Node> = BTreeMap::new();
    for (path, entry) in files {
        let components: Vec<String> = path
            .components()
            .filter_map(|component| match component {
                std::path::Component::Normal(name) => name.to_str().map(str::to_owned),
                _ => None,
            })
            .collect();
        insert_node(&mut root, &components, entry.clone());
    }
    Ok(write_node_tree(store, &root, Path::new(""))?)
}

fn insert_node(dir: &mut BTreeMap<String, Node>, components: &[String], entry: FileEntry) {
    match components {
        [] => {}
        [name] => {
            dir.insert(name.clone(), Node::File(entry));
        }
        [name, rest @ ..] => {
            let child = dir
                .entry(name.clone())
                .or_insert_with(|| Node::Dir(BTreeMap::new()));
            if let Node::Dir(child_dir) = child {
                insert_node(child_dir, rest, entry);
            }
        }
    }
}

fn write_node_tree(
    store: &impl ObjectStore,
    dir: &BTreeMap<String, Node>,
    dir_path: &Path,
) -> Result<Tree, SnapshotError> {
    let mut entries = Vec::with_capacity(dir.len());
    for (name, node) in dir {
        let path = dir_path.join(name);
        match node {
            Node::File(file) => entries.push(TreeEntry {
                name: name.clone(),
                path,
                entry_type: EntryType::File,
                object: ObjectRef::new(ObjectKind::Blob, file.blob),
                mode: file.mode,
                size: file.size,
                content_hash: file.content_hash,
            }),
            Node::Dir(children) => {
                let child_tree = write_node_tree(store, children, &path)?;
                entries.push(TreeEntry {
                    name: name.clone(),
                    path,
                    entry_type: EntryType::Directory,
                    object: ObjectRef::new(ObjectKind::Tree, child_tree.id),
                    mode: EntryMode::Directory,
                    size: None,
                    content_hash: None,
                });
            }
        }
    }
    write_tree(store, entries)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        materialize_snapshot, read_conflict, read_merge_result, read_snapshot, read_snapshot_files,
        InMemoryObjectStore, Snapshot,
    };

    fn write_file(path: PathBuf, bytes: &[u8]) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, bytes).unwrap();
    }

    fn snapshot(
        store: &InMemoryObjectStore,
        files: &[(&str, &[u8])],
        parents: &[&Snapshot],
    ) -> Snapshot {
        let dir = tempfile::tempdir().unwrap();
        for (name, bytes) in files {
            write_file(dir.path().join(name), bytes);
        }
        let mut options = SnapshotOptions::new("repo");
        options.parents = parents
            .iter()
            .map(|parent| ObjectRef::new(ObjectKind::Snapshot, parent.id))
            .collect();
        materialize_snapshot(store, dir.path(), options).unwrap()
    }

    fn options() -> MergeOptions {
        MergeOptions::new(Principal::system(), "repo", "merge")
    }

    #[test]
    fn merges_disjoint_file_edits_cleanly() {
        let store = InMemoryObjectStore::new();
        let base = snapshot(&store, &[("shared.txt", b"base\n")], &[]);
        let ours = snapshot(
            &store,
            &[("shared.txt", b"base\n"), ("ours.txt", b"o\n")],
            &[&base],
        );
        let theirs = snapshot(
            &store,
            &[("shared.txt", b"base\n"), ("theirs.txt", b"t\n")],
            &[&base],
        );

        let result = merge_snapshots(&store, &base.id, &ours.id, &theirs.id, &options()).unwrap();
        assert_eq!(result.status, MergeResultStatus::Clean);
        assert!(result.conflicts.is_empty());
        let snapshot_id = result.merged_snapshot.unwrap();
        let merged = read_snapshot(&store, &snapshot_id).unwrap();
        assert_eq!(
            merged.parents,
            vec![
                ObjectRef::new(ObjectKind::Snapshot, ours.id),
                ObjectRef::new(ObjectKind::Snapshot, theirs.id)
            ]
        );

        let files = read_snapshot_files(&store, &snapshot_id).unwrap();
        assert_eq!(files.len(), 3);
        assert_eq!(files[&PathBuf::from("shared.txt")], b"base\n");
        assert_eq!(files[&PathBuf::from("ours.txt")], b"o\n");
        assert_eq!(files[&PathBuf::from("theirs.txt")], b"t\n");
    }

    #[test]
    fn both_modified_same_file_different_regions_merges_cleanly() {
        let store = InMemoryObjectStore::new();
        let base = snapshot(&store, &[("a.txt", b"a\nb\nc\nd\n")], &[]);
        let ours = snapshot(&store, &[("a.txt", b"a\nB\nc\nd\n")], &[&base]);
        let theirs = snapshot(&store, &[("a.txt", b"a\nb\nc\nD\n")], &[&base]);

        let result = merge_snapshots(&store, &base.id, &ours.id, &theirs.id, &options()).unwrap();
        assert_eq!(result.status, MergeResultStatus::Clean);
        let files = read_snapshot_files(&store, &result.merged_snapshot.unwrap()).unwrap();
        assert_eq!(files[&PathBuf::from("a.txt")], b"a\nB\nc\nD\n");
    }

    #[test]
    fn both_modified_conflict_writes_conflict_not_snapshot() {
        let store = InMemoryObjectStore::new();
        let base = snapshot(&store, &[("a.txt", b"a\nb\nc\n")], &[]);
        let ours = snapshot(&store, &[("a.txt", b"a\nours\nc\n")], &[&base]);
        let theirs = snapshot(&store, &[("a.txt", b"a\ntheirs\nc\n")], &[&base]);

        let result = merge_snapshots(&store, &base.id, &ours.id, &theirs.id, &options()).unwrap();
        assert_eq!(result.status, MergeResultStatus::Conflicted);
        assert!(result.merged_snapshot.is_none());
        assert_eq!(result.conflicts.len(), 1);
        assert_eq!(result.repo_id, "repo");
        assert_eq!(result.base_snapshot, base.id);
        assert_eq!(result.ours_snapshot, ours.id);
        assert_eq!(result.theirs_snapshot, theirs.id);

        let conflict = read_conflict(&store, &result.conflicts[0]).unwrap();
        assert_eq!(conflict.path, PathBuf::from("a.txt"));
        assert_eq!(conflict.conflict_type, ConflictType::Content);
        assert_eq!(conflict.repo_id, "repo");
        assert!(conflict.sides.base.is_some());
        assert!(conflict.sides.ours.is_some());
        assert!(conflict.sides.theirs.is_some());
        assert!(!conflict.hunks.is_empty());

        let stored = read_merge_result(&store, &result.id).unwrap();
        assert_eq!(stored, result);
    }

    #[test]
    fn add_add_conflict() {
        let store = InMemoryObjectStore::new();
        let base = snapshot(&store, &[("keep.txt", b"k\n")], &[]);
        let ours = snapshot(
            &store,
            &[("keep.txt", b"k\n"), ("new.txt", b"ours\n")],
            &[&base],
        );
        let theirs = snapshot(
            &store,
            &[("keep.txt", b"k\n"), ("new.txt", b"theirs\n")],
            &[&base],
        );

        let result = merge_snapshots(&store, &base.id, &ours.id, &theirs.id, &options()).unwrap();
        assert_eq!(result.status, MergeResultStatus::Conflicted);
        assert!(result.merged_snapshot.is_none());
        assert_eq!(result.conflicts.len(), 1);

        let conflict = read_conflict(&store, &result.conflicts[0]).unwrap();
        assert_eq!(conflict.path, PathBuf::from("new.txt"));
        assert_eq!(conflict.conflict_type, ConflictType::AddAdd);
    }

    #[test]
    fn modify_delete_conflict_keeps_modified_version() {
        let store = InMemoryObjectStore::new();
        let base = snapshot(&store, &[("a.txt", b"base\n")], &[]);
        let ours = snapshot(&store, &[("a.txt", b"ours\n")], &[&base]);
        let theirs = snapshot(&store, &[], &[&base]);

        let result = merge_snapshots(&store, &base.id, &ours.id, &theirs.id, &options()).unwrap();
        assert_eq!(result.status, MergeResultStatus::Conflicted);
        assert!(result.merged_snapshot.is_none());

        let conflict = read_conflict(&store, &result.conflicts[0]).unwrap();
        assert_eq!(conflict.path, PathBuf::from("a.txt"));
        assert_eq!(conflict.conflict_type, ConflictType::ModifyDelete);
        assert!(conflict.sides.base.is_some());
        assert!(conflict.sides.ours.is_some());
        assert_eq!(conflict.sides.theirs, None);
    }

    #[test]
    fn binary_conflict() {
        let store = InMemoryObjectStore::new();
        let base = snapshot(&store, &[("bin.dat", b"ok\n")], &[]);
        let ours = snapshot(&store, &[("bin.dat", b"ours\n")], &[&base]);
        let theirs = snapshot(&store, &[("bin.dat", b"bad\xff\n")], &[&base]);

        let result = merge_snapshots(&store, &base.id, &ours.id, &theirs.id, &options()).unwrap();
        assert_eq!(result.status, MergeResultStatus::Conflicted);
        assert!(result.merged_snapshot.is_none());

        let conflict = read_conflict(&store, &result.conflicts[0]).unwrap();
        assert_eq!(conflict.path, PathBuf::from("bin.dat"));
        assert_eq!(conflict.conflict_type, ConflictType::Binary);
    }

    #[test]
    fn merged_snapshot_parents_are_ours_then_theirs() {
        let store = InMemoryObjectStore::new();
        let base = snapshot(&store, &[("a.txt", b"a\n")], &[]);
        let ours = snapshot(&store, &[("a.txt", b"a\n"), ("o.txt", b"o\n")], &[&base]);
        let theirs = snapshot(&store, &[("a.txt", b"a\n"), ("t.txt", b"t\n")], &[&base]);

        let result = merge_snapshots(&store, &base.id, &ours.id, &theirs.id, &options()).unwrap();
        let merged = read_snapshot(&store, &result.merged_snapshot.unwrap()).unwrap();
        assert_eq!(
            merged.parents,
            vec![
                ObjectRef::new(ObjectKind::Snapshot, ours.id),
                ObjectRef::new(ObjectKind::Snapshot, theirs.id)
            ]
        );
    }

    #[test]
    fn identical_changes_on_both_sides_collapse() {
        let store = InMemoryObjectStore::new();
        let base = snapshot(&store, &[("a.txt", b"a\n")], &[]);
        let ours = snapshot(&store, &[("a.txt", b"same\n"), ("o.txt", b"o\n")], &[&base]);
        let theirs = snapshot(&store, &[("a.txt", b"same\n"), ("t.txt", b"t\n")], &[&base]);

        let result = merge_snapshots(&store, &base.id, &ours.id, &theirs.id, &options()).unwrap();
        assert_eq!(result.status, MergeResultStatus::Clean);
        let files = read_snapshot_files(&store, &result.merged_snapshot.unwrap()).unwrap();
        assert_eq!(files[&PathBuf::from("a.txt")], b"same\n");
    }
}
