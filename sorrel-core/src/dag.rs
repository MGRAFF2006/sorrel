//! Snapshot DAG helpers: breadth-first merge-base over parent links.
//!
//! Snapshots form a DAG through [`Snapshot::parents`](crate::Snapshot::parents).
//! [`merge_base`] walks parents from both tips and returns the first common
//! ancestor at the earliest generation. This is distinct from
//! [`crate::history::merge_base`], which returns a *best* common ancestor
//! (git `merge-base` style).

use std::collections::{BTreeSet, HashSet, VecDeque};

use crate::{read_snapshot, ObjectId, ObjectStore, ObjectStoreError, SnapshotError};

/// Result type used by DAG operations.
pub type DagResult<T> = Result<T, DagError>;

/// Errors returned while walking the snapshot parent DAG.
#[derive(Debug, thiserror::Error)]
pub enum DagError {
    /// The underlying object store failed.
    #[error(transparent)]
    ObjectStore(#[from] ObjectStoreError),

    /// Reading a snapshot object failed (missing object or decode failure).
    #[error(transparent)]
    Snapshot(#[from] SnapshotError),
}

/// Returns the first common ancestor of two snapshots, or [`None`] when their
/// histories are unrelated.
///
/// Parents are walked breadth-first from both tips. The first generation that
/// yields any common ancestor is selected; if several candidates appear at that
/// generation, the lexicographically smallest hex id is returned so the result
/// is deterministic.
///
/// Special cases:
/// * If `a == b`, returns that id.
/// * If one tip is an ancestor of the other, returns the ancestor.
pub fn merge_base(
    store: &impl ObjectStore,
    a: &ObjectId,
    b: &ObjectId,
) -> Result<Option<ObjectId>, DagError> {
    if a == b {
        return Ok(Some(*a));
    }

    let mut seen_a = HashSet::from([*a]);
    let mut seen_b = HashSet::from([*b]);
    let mut frontier_a = VecDeque::from([*a]);
    let mut frontier_b = VecDeque::from([*b]);

    loop {
        if frontier_a.is_empty() && frontier_b.is_empty() {
            return Ok(None);
        }

        let next_a = expand_frontier(store, &mut frontier_a, &seen_a)?;
        let next_b = expand_frontier(store, &mut frontier_b, &seen_b)?;

        for &id in &next_a {
            seen_a.insert(id);
        }
        for &id in &next_b {
            seen_b.insert(id);
        }

        let mut candidates = BTreeSet::new();
        for &id in &next_a {
            if seen_b.contains(&id) {
                candidates.insert(id);
            }
        }
        for &id in &next_b {
            if seen_a.contains(&id) {
                candidates.insert(id);
            }
        }

        if let Some(best) = candidates.into_iter().next() {
            // `ObjectId` orders by raw bytes, which matches lowercase hex order.
            return Ok(Some(best));
        }

        frontier_a.extend(next_a);
        frontier_b.extend(next_b);
    }
}

fn expand_frontier(
    store: &impl ObjectStore,
    frontier: &mut VecDeque<ObjectId>,
    seen: &HashSet<ObjectId>,
) -> DagResult<Vec<ObjectId>> {
    let mut next = Vec::new();
    let count = frontier.len();
    for _ in 0..count {
        let Some(current) = frontier.pop_front() else {
            break;
        };
        let snapshot = read_snapshot(store, &current)?;
        for parent in &snapshot.parents {
            if seen.contains(&parent.id) || next.contains(&parent.id) {
                continue;
            }
            next.push(parent.id);
        }
    }
    Ok(next)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        write_snapshot, write_tree, InMemoryObjectStore, ObjectKind, ObjectRef, Snapshot,
        SnapshotOptions,
    };

    fn empty_tree(store: &InMemoryObjectStore) -> ObjectId {
        write_tree(store, Vec::new()).unwrap().id
    }

    fn snapshot_with_parents(
        store: &InMemoryObjectStore,
        message: &str,
        parents: &[&Snapshot],
    ) -> Snapshot {
        let mut options = SnapshotOptions::new("repo");
        options.message = Some(message.to_owned());
        options.parents = parents
            .iter()
            .map(|parent| ObjectRef::new(ObjectKind::Snapshot, parent.id))
            .collect();
        write_snapshot(store, empty_tree(store), options).unwrap()
    }

    #[test]
    fn merge_base_of_linear_history_is_the_older_snapshot() {
        let store = InMemoryObjectStore::new();
        let root = snapshot_with_parents(&store, "root", &[]);
        let mid = snapshot_with_parents(&store, "mid", &[&root]);
        let tip = snapshot_with_parents(&store, "tip", &[&mid]);

        assert_eq!(merge_base(&store, &mid.id, &tip.id).unwrap(), Some(mid.id));
        assert_eq!(merge_base(&store, &tip.id, &mid.id).unwrap(), Some(mid.id));
        assert_eq!(
            merge_base(&store, &root.id, &tip.id).unwrap(),
            Some(root.id)
        );
    }

    #[test]
    fn merge_base_of_simple_fork_is_the_fork_point() {
        let store = InMemoryObjectStore::new();
        let root = snapshot_with_parents(&store, "root", &[]);
        let fork = snapshot_with_parents(&store, "fork", &[&root]);
        let left = snapshot_with_parents(&store, "left", &[&fork]);
        let right = snapshot_with_parents(&store, "right", &[&fork]);

        assert_eq!(
            merge_base(&store, &left.id, &right.id).unwrap(),
            Some(fork.id)
        );
        assert_eq!(
            merge_base(&store, &right.id, &left.id).unwrap(),
            Some(fork.id)
        );
    }

    #[test]
    fn unrelated_histories_have_no_merge_base() {
        let store = InMemoryObjectStore::new();
        let one = snapshot_with_parents(&store, "one", &[]);
        let two = snapshot_with_parents(&store, "two", &[]);

        assert_eq!(merge_base(&store, &one.id, &two.id).unwrap(), None);
    }

    #[test]
    fn identical_inputs_return_that_id() {
        let store = InMemoryObjectStore::new();
        let snap = snapshot_with_parents(&store, "same", &[]);

        assert_eq!(
            merge_base(&store, &snap.id, &snap.id).unwrap(),
            Some(snap.id)
        );
    }

    #[test]
    fn one_is_ancestor_returns_the_ancestor() {
        let store = InMemoryObjectStore::new();
        let ancestor = snapshot_with_parents(&store, "ancestor", &[]);
        let child = snapshot_with_parents(&store, "child", &[&ancestor]);
        let tip = snapshot_with_parents(&store, "tip", &[&child]);

        assert_eq!(
            merge_base(&store, &ancestor.id, &tip.id).unwrap(),
            Some(ancestor.id)
        );
        assert_eq!(
            merge_base(&store, &tip.id, &ancestor.id).unwrap(),
            Some(ancestor.id)
        );
    }

    #[test]
    fn equal_distance_ancestors_pick_lexicographically_smallest_hex_id() {
        let store = InMemoryObjectStore::new();
        let root = snapshot_with_parents(&store, "root", &[]);
        let left = snapshot_with_parents(&store, "left", &[&root]);
        let right = snapshot_with_parents(&store, "right", &[&root]);
        // Criss-cross: each side merges both parents, then both tips sit one
        // generation above `left` and `right`.
        let left_merge = snapshot_with_parents(&store, "left-merge", &[&left, &right]);
        let right_merge = snapshot_with_parents(&store, "right-merge", &[&right, &left]);

        let expected = if left.id.to_hex() < right.id.to_hex() {
            left.id
        } else {
            right.id
        };

        assert_eq!(
            merge_base(&store, &left_merge.id, &right_merge.id).unwrap(),
            Some(expected)
        );
        assert_eq!(
            merge_base(&store, &right_merge.id, &left_merge.id).unwrap(),
            Some(expected)
        );
    }
}
