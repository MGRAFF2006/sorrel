//! Snapshot-DAG history operations: ancestry sets and merge bases.
//!
//! Snapshots form a DAG through [`Snapshot::parents`](crate::Snapshot::parents).
//! These helpers answer the two questions merges and lane workflows need:
//! *which snapshots are reachable from here* ([`collect_ancestors`]) and *where
//! do two lines of history meet* ([`merge_bases`] / [`merge_base`]).

use std::collections::BTreeSet;

use crate::{read_snapshot, ObjectId, ObjectStore, ObjectStoreError, SnapshotError};

/// Errors returned while walking snapshot history.
#[derive(Debug, thiserror::Error)]
pub enum HistoryError {
    /// The underlying object store failed.
    #[error(transparent)]
    Store(#[from] ObjectStoreError),
    /// Reading a snapshot object failed.
    #[error(transparent)]
    Snapshot(#[from] SnapshotError),
}

/// Result type for history operations.
pub type HistoryResult<T> = Result<T, HistoryError>;

/// Returns every snapshot reachable from `snapshot_id` via parent links,
/// including `snapshot_id` itself.
///
/// The result is a sorted set, so iteration order is deterministic.
pub fn collect_ancestors<S: ObjectStore>(
    store: &S,
    snapshot_id: ObjectId,
) -> HistoryResult<BTreeSet<ObjectId>> {
    let mut ancestors = BTreeSet::new();
    let mut queue = vec![snapshot_id];

    while let Some(current) = queue.pop() {
        if !ancestors.insert(current) {
            continue;
        }
        let snapshot = read_snapshot(store, &current)?;
        for parent in &snapshot.parents {
            queue.push(parent.id);
        }
    }

    Ok(ancestors)
}

/// Returns all *best* common ancestors of two snapshots, sorted by id.
///
/// A best common ancestor is a common ancestor that is not itself a strict
/// ancestor of another common ancestor (the equivalent of
/// `git merge-base --all`). The result is empty when the two snapshots share
/// no history. Criss-cross histories can legitimately return more than one
/// candidate; callers that need exactly one should use [`merge_base`], which
/// picks deterministically.
pub fn merge_bases<S: ObjectStore>(
    store: &S,
    left: ObjectId,
    right: ObjectId,
) -> HistoryResult<Vec<ObjectId>> {
    let left_ancestors = collect_ancestors(store, left)?;
    let right_ancestors = collect_ancestors(store, right)?;
    let common: BTreeSet<ObjectId> = left_ancestors
        .intersection(&right_ancestors)
        .copied()
        .collect();

    if common.is_empty() {
        return Ok(Vec::new());
    }

    // Every ancestor of a common ancestor is itself a common ancestor, so a
    // multi-source walk from the parents of each common ancestor stays within
    // `common` and marks exactly the redundant (non-best) candidates.
    let mut redundant = BTreeSet::new();
    let mut queue: Vec<ObjectId> = Vec::new();
    for candidate in &common {
        let snapshot = read_snapshot(store, candidate)?;
        for parent in &snapshot.parents {
            queue.push(parent.id);
        }
    }
    while let Some(current) = queue.pop() {
        if !redundant.insert(current) {
            continue;
        }
        let snapshot = read_snapshot(store, &current)?;
        for parent in &snapshot.parents {
            queue.push(parent.id);
        }
    }

    Ok(common
        .into_iter()
        .filter(|candidate| !redundant.contains(candidate))
        .collect())
}

/// Returns one deterministic best common ancestor of two snapshots, or `None`
/// when they share no history.
///
/// When a criss-cross history yields several best candidates, the smallest id
/// (in sorted order) is chosen so that repeated runs agree.
pub fn merge_base<S: ObjectStore>(
    store: &S,
    left: ObjectId,
    right: ObjectId,
) -> HistoryResult<Option<ObjectId>> {
    Ok(merge_bases(store, left, right)?.into_iter().next())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        materialize_snapshot, InMemoryObjectStore, ObjectKind, ObjectRef, Snapshot, SnapshotOptions,
    };
    use std::path::PathBuf;

    fn write_file(path: PathBuf, bytes: &[u8]) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, bytes).unwrap();
    }

    fn snapshot_with_parents(
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

    #[test]
    fn collect_ancestors_includes_self_and_all_parents() {
        let store = InMemoryObjectStore::new();
        let root = snapshot_with_parents(&store, &[("a.txt", b"a\n")], &[]);
        let child =
            snapshot_with_parents(&store, &[("a.txt", b"a\n"), ("b.txt", b"b\n")], &[&root]);
        let grandchild =
            snapshot_with_parents(&store, &[("a.txt", b"a\n"), ("c.txt", b"c\n")], &[&child]);

        let ancestors = collect_ancestors(&store, grandchild.id).unwrap();

        assert!(ancestors.contains(&grandchild.id));
        assert!(ancestors.contains(&child.id));
        assert!(ancestors.contains(&root.id));
        assert_eq!(ancestors.len(), 3);
    }

    #[test]
    fn merge_base_of_linear_history_is_the_older_snapshot() {
        let store = InMemoryObjectStore::new();
        let root = snapshot_with_parents(&store, &[("a.txt", b"a\n")], &[]);
        let child = snapshot_with_parents(&store, &[("a.txt", b"a2\n")], &[&root]);

        assert_eq!(
            merge_base(&store, root.id, child.id).unwrap(),
            Some(root.id)
        );
        assert_eq!(
            merge_base(&store, child.id, root.id).unwrap(),
            Some(root.id)
        );
        assert_eq!(
            merge_base(&store, child.id, child.id).unwrap(),
            Some(child.id)
        );
    }

    #[test]
    fn merge_base_of_simple_fork_is_the_fork_point() {
        let store = InMemoryObjectStore::new();
        let root = snapshot_with_parents(&store, &[("a.txt", b"a\n")], &[]);
        let fork = snapshot_with_parents(&store, &[("a.txt", b"a\n"), ("f.txt", b"f\n")], &[&root]);
        let left = snapshot_with_parents(&store, &[("l.txt", b"l\n")], &[&fork]);
        let right = snapshot_with_parents(&store, &[("r.txt", b"r\n")], &[&fork]);

        let bases = merge_bases(&store, left.id, right.id).unwrap();
        assert_eq!(bases, vec![fork.id]);
    }

    #[test]
    fn merge_base_walks_through_merge_snapshots() {
        let store = InMemoryObjectStore::new();
        let root = snapshot_with_parents(&store, &[("a.txt", b"a\n")], &[]);
        let left = snapshot_with_parents(&store, &[("l.txt", b"l\n")], &[&root]);
        let right = snapshot_with_parents(&store, &[("r.txt", b"r\n")], &[&root]);
        let merged = snapshot_with_parents(
            &store,
            &[("l.txt", b"l\n"), ("r.txt", b"r\n")],
            &[&left, &right],
        );
        let after_right = snapshot_with_parents(&store, &[("r2.txt", b"r2\n")], &[&right]);

        // `right` is an ancestor of `merged`, so it is the best common ancestor.
        let bases = merge_bases(&store, merged.id, after_right.id).unwrap();
        assert_eq!(bases, vec![right.id]);
    }

    #[test]
    fn criss_cross_yields_multiple_best_bases_and_deterministic_pick() {
        let store = InMemoryObjectStore::new();
        let root = snapshot_with_parents(&store, &[("a.txt", b"a\n")], &[]);
        let left = snapshot_with_parents(&store, &[("l.txt", b"l\n")], &[&root]);
        let right = snapshot_with_parents(&store, &[("r.txt", b"r\n")], &[&root]);
        // Criss-cross: each side merges the other, then both advance.
        let left_merge = snapshot_with_parents(&store, &[("lm.txt", b"lm\n")], &[&left, &right]);
        let right_merge = snapshot_with_parents(&store, &[("rm.txt", b"rm\n")], &[&right, &left]);

        let mut expected = vec![left.id, right.id];
        expected.sort();
        let bases = merge_bases(&store, left_merge.id, right_merge.id).unwrap();
        assert_eq!(bases, expected);
        assert_eq!(
            merge_base(&store, left_merge.id, right_merge.id).unwrap(),
            Some(expected[0])
        );
    }

    #[test]
    fn unrelated_histories_have_no_merge_base() {
        let store = InMemoryObjectStore::new();
        let one = snapshot_with_parents(&store, &[("a.txt", b"a\n")], &[]);
        let two = snapshot_with_parents(&store, &[("b.txt", b"b\n")], &[]);

        assert!(merge_bases(&store, one.id, two.id).unwrap().is_empty());
        assert_eq!(merge_base(&store, one.id, two.id).unwrap(), None);
    }
}
