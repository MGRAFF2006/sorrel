//! Object-transfer helpers for the sync transport (push / pull).
//!
//! These functions compute the set of object ids reachable from a set of
//! snapshots (their *closure*) and provide content-verified batch get/put over
//! any [`ObjectStore`]. They are the engine half of the push/pull protocol
//! documented in `sorrel-protocol/docs/sync-transport.md`: a remote is a
//! transport + ref store, and these helpers let a client and a remote negotiate
//! exactly which objects must move.
//!
//! The closure of a snapshot is:
//!
//! - the snapshot object itself,
//! - its root tree and, transitively, every subtree and blob, and
//! - its ancestor snapshots (via `parents`) and *their* trees/blobs.
//!
//! Walking ancestor snapshots means pushing a ref also transfers the history
//! reachable from it, so a pulled repo can walk `log` back to the root.

use std::collections::BTreeSet;

use crate::{
    read_snapshot, read_tree, validate_snapshot, EntryType, ObjectId, ObjectKind, ObjectStore,
    ObjectStoreError, SnapshotError,
};

/// Errors returned while computing closures or transferring objects.
#[derive(Debug, thiserror::Error)]
pub enum TransportError {
    /// The underlying object store failed.
    #[error(transparent)]
    Store(#[from] ObjectStoreError),
    /// Reading a snapshot/tree object failed.
    #[error(transparent)]
    Snapshot(#[from] SnapshotError),
}

/// Result type for transport operations.
pub type TransportResult<T> = Result<T, TransportError>;

/// Computes the full object closure reachable from `roots` (snapshot ids).
///
/// The returned set includes the snapshots themselves, all ancestor snapshots,
/// and every tree and blob reachable from each. Ids are returned sorted for
/// deterministic output. Objects already known to be present can be excluded
/// with [`missing_objects`]. Every reachable snapshot's tree/blob graph and
/// declared reference kinds are validated; native validation bounds tree depth.
pub fn collect_closure<S: ObjectStore>(
    store: &S,
    roots: &[ObjectId],
) -> TransportResult<BTreeSet<ObjectId>> {
    let mut closure = BTreeSet::new();
    let mut snapshot_queue: Vec<ObjectId> = roots.to_vec();
    let mut visited_snapshots = BTreeSet::new();

    while let Some(snapshot_id) = snapshot_queue.pop() {
        if !visited_snapshots.insert(snapshot_id) {
            continue;
        }
        // Validate every ancestor's typed tree/blob graph, not only the selected tip.
        // Native validation also bounds tree depth before any downstream consumer.
        validate_snapshot(store, &snapshot_id)?;
        closure.insert(snapshot_id);
        let snapshot = read_snapshot(store, &snapshot_id)?;
        // Its content tree closure.
        collect_tree_closure(store, snapshot.root_tree.id, &mut closure)?;
        // Its ancestors.
        for parent in &snapshot.parents {
            snapshot_queue.push(parent.id);
        }
    }

    Ok(closure)
}

/// Adds a tree and all reachable subtrees/blobs to `closure`.
fn collect_tree_closure<S: ObjectStore>(
    store: &S,
    tree_id: ObjectId,
    closure: &mut BTreeSet<ObjectId>,
) -> TransportResult<()> {
    let mut pending = vec![tree_id];
    while let Some(id) = pending.pop() {
        if !closure.insert(id) {
            continue;
        }
        let tree = read_tree(store, &id)?;
        for entry in tree.entries {
            let expected_kind = match entry.entry_type {
                EntryType::Directory => ObjectKind::Tree,
                EntryType::File => ObjectKind::Blob,
            };
            if entry.object.kind != expected_kind {
                return Err(SnapshotError::InvalidObjectKind {
                    expected: if expected_kind == ObjectKind::Tree {
                        "Tree"
                    } else {
                        "Blob"
                    },
                    actual: format!("{:?}", entry.object.kind),
                }
                .into());
            }
            match entry.entry_type {
                EntryType::Directory => pending.push(entry.object.id),
                EntryType::File => {
                    closure.insert(entry.object.id);
                }
            }
        }
    }
    Ok(())
}

/// Returns the subset of `want`'s closure that `store` does not already contain.
///
/// This is the server side of `POST /objects/missing`: given the snapshots the
/// caller wants present (`want`) and the objects already known present (`have`,
/// a cheap client hint), return exactly the object ids that must be uploaded.
pub fn missing_objects<S: ObjectStore>(
    store: &S,
    want: &[ObjectId],
    have: &[ObjectId],
) -> TransportResult<Vec<ObjectId>> {
    let have: BTreeSet<ObjectId> = have.iter().copied().collect();
    let closure = collect_closure(store, want)?;

    let mut missing = Vec::new();
    for id in closure {
        if have.contains(&id) {
            continue;
        }
        if !store.has(&id)? {
            missing.push(id);
        }
    }
    Ok(missing)
}

/// Returns the object ids from `want`'s closure that a *target* store is missing.
///
/// This is the client side of a pull: `source` holds the objects (e.g. a local
/// store after download, or a remote mirror), and we ask which of the closure of
/// `want` the `target` store lacks. Equivalent to [`missing_objects`] evaluated
/// against `target` using `source` to walk the graph.
pub fn missing_in_target<S: ObjectStore, T: ObjectStore>(
    source: &S,
    target: &T,
    want: &[ObjectId],
) -> TransportResult<Vec<ObjectId>> {
    let closure = collect_closure(source, want)?;
    let mut missing = Vec::new();
    for id in closure {
        if !target.has(&id)? {
            missing.push(id);
        }
    }
    Ok(missing)
}

/// Returns whether `ancestor` is reachable from `descendant` by walking
/// [`Snapshot::parents`](crate::Snapshot::parents).
///
/// Returns `true` when `ancestor == descendant`, when `ancestor` is a direct
/// parent of `descendant`, or when it appears on any ancestor chain. This
/// mirrors the remote's non-fast-forward ref rule for push/pull.
pub fn is_descendant<S: ObjectStore>(
    store: &S,
    ancestor: ObjectId,
    descendant: ObjectId,
) -> TransportResult<bool> {
    let mut queue = vec![descendant];
    let mut visited = BTreeSet::new();

    while let Some(current) = queue.pop() {
        if !visited.insert(current) {
            continue;
        }
        let snapshot = read_snapshot(store, &current)?;
        if snapshot.root_tree.kind != ObjectKind::Tree {
            return Err(SnapshotError::InvalidObjectKind {
                expected: "Tree",
                actual: format!("{:?}", snapshot.root_tree.kind),
            }
            .into());
        }
        for parent in &snapshot.parents {
            if parent.kind != ObjectKind::Snapshot {
                return Err(SnapshotError::InvalidObjectKind {
                    expected: "Snapshot",
                    actual: format!("{:?}", parent.kind),
                }
                .into());
            }
        }
        // Equality must still refer to an actual, well-typed Snapshot header.
        if current == ancestor {
            return Ok(true);
        }
        queue.extend(snapshot.parents.iter().map(|parent| parent.id));
    }

    Ok(false)
}

/// Copies the given objects from `source` into `target`, content-verified.
///
/// Each object is read from `source` (which verifies its content id) and written
/// to `target` (which re-derives and stores by content id). Returns the ids
/// actually transferred.
pub fn transfer_objects<S: ObjectStore, T: ObjectStore>(
    source: &S,
    target: &T,
    ids: &[ObjectId],
) -> TransportResult<Vec<ObjectId>> {
    let mut transferred = Vec::with_capacity(ids.len());
    for id in ids {
        let bytes = source.read(id)?;
        let written = target.write(&bytes)?;
        // `write` is content-addressed, so this equals `id` for honest bytes.
        transferred.push(written);
    }
    Ok(transferred)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        materialize_snapshot, read_snapshot, read_tree, FileObjectStore, InMemoryObjectStore,
        SnapshotOptions,
    };

    fn write_file(path: std::path::PathBuf, bytes: &[u8]) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, bytes).unwrap();
    }

    fn sample_snapshot(store: &InMemoryObjectStore) -> crate::Snapshot {
        let dir = tempfile::tempdir().unwrap();
        write_file(dir.path().join("a.txt"), b"hello\n");
        materialize_snapshot(store, dir.path(), SnapshotOptions::new("repo")).unwrap()
    }

    fn child_snapshot(
        store: &InMemoryObjectStore,
        parent: &crate::Snapshot,
        filename: &str,
        bytes: &[u8],
    ) -> crate::Snapshot {
        let dir = tempfile::tempdir().unwrap();
        write_file(dir.path().join("a.txt"), b"hello\n");
        write_file(dir.path().join(filename), bytes);
        let mut options = SnapshotOptions::new("repo");
        options.parents = vec![crate::ObjectRef::new(
            crate::ObjectKind::Snapshot,
            parent.id,
        )];
        materialize_snapshot(store, dir.path(), options).unwrap()
    }

    fn blob_ids_in_tree(store: &impl ObjectStore, tree_id: ObjectId) -> Vec<ObjectId> {
        let tree = read_tree(store, &tree_id).unwrap();
        let mut ids = Vec::new();
        for entry in tree.entries {
            match entry.entry_type {
                EntryType::Directory => ids.extend(blob_ids_in_tree(store, entry.object.id)),
                EntryType::File => ids.push(entry.object.id),
            }
        }
        ids
    }

    #[test]
    fn closure_includes_snapshot_trees_blobs_and_ancestors() {
        let store = InMemoryObjectStore::new();
        let first = sample_snapshot(&store);
        let second = child_snapshot(&store, &first, "b.txt", b"world\n");

        let closure = collect_closure(&store, &[second.id]).unwrap();
        let first_snapshot = read_snapshot(&store, &first.id).unwrap();
        let second_snapshot = read_snapshot(&store, &second.id).unwrap();

        assert!(closure.contains(&first.id));
        assert!(closure.contains(&second.id));
        assert!(closure.contains(&first_snapshot.root_tree.id));
        assert!(closure.contains(&second_snapshot.root_tree.id));
        for blob_id in blob_ids_in_tree(&store, second_snapshot.root_tree.id) {
            assert!(closure.contains(&blob_id));
        }
        assert!(closure.len() > 4);
    }

    #[test]
    fn missing_objects_excludes_present_and_have() {
        let source = InMemoryObjectStore::new();
        let snap = sample_snapshot(&source);
        let full = collect_closure(&source, &[snap.id]).unwrap();

        let target = InMemoryObjectStore::new();
        let missing = missing_in_target(&source, &target, &[snap.id]).unwrap();
        assert_eq!(missing.len(), full.len());

        transfer_objects(&source, &target, &missing).unwrap();
        let missing_after = missing_in_target(&source, &target, &[snap.id]).unwrap();
        assert!(missing_after.is_empty());

        let have: Vec<ObjectId> = full.iter().copied().collect();
        let none_missing = missing_objects(&source, &[snap.id], &have).unwrap();
        assert!(none_missing.is_empty());

        // Partial `have` hint: only hinted ids are skipped.
        let partial_have = vec![snap.id];
        let still_missing = missing_objects(&source, &[snap.id], &partial_have).unwrap();
        assert!(!still_missing.contains(&snap.id));
        assert!(still_missing.len() < full.len());
    }

    #[test]
    fn transfer_objects_round_trips_between_memory_and_file_store() {
        let source = InMemoryObjectStore::new();
        let first = sample_snapshot(&source);
        let second = child_snapshot(&source, &first, "b.txt", b"world\n");
        let ids = collect_closure(&source, &[second.id]).unwrap();
        let id_list: Vec<ObjectId> = ids.iter().copied().collect();

        let file_dir = tempfile::tempdir().unwrap();
        let file_store = FileObjectStore::new(file_dir.path()).unwrap();
        let transferred = transfer_objects(&source, &file_store, &id_list).unwrap();
        assert_eq!(transferred.len(), id_list.len());
        for id in &id_list {
            assert!(file_store.has(id).unwrap());
            assert_eq!(source.read(id).unwrap(), file_store.read(id).unwrap());
        }

        let round_trip = InMemoryObjectStore::new();
        transfer_objects(&file_store, &round_trip, &id_list).unwrap();
        for id in &id_list {
            assert!(round_trip.has(id).unwrap());
            assert_eq!(source.read(id).unwrap(), round_trip.read(id).unwrap());
        }
    }

    fn rewrite_json(
        store: &InMemoryObjectStore,
        id: ObjectId,
        mutate: impl FnOnce(&mut serde_json::Value),
    ) -> ObjectId {
        let mut value = serde_json::from_slice(&store.read(&id).unwrap()).unwrap();
        mutate(&mut value);
        store.write(&serde_json::to_vec(&value).unwrap()).unwrap()
    }

    #[test]
    fn closure_rejects_wrong_typed_refs_in_tips_and_ancestor_trees() {
        let store = InMemoryObjectStore::new();
        let parent = sample_snapshot(&store);
        let child = child_snapshot(&store, &parent, "b.txt", b"world\n");
        let wrong_root = rewrite_json(&store, child.id, |value| {
            value["rootTree"]["kind"] = serde_json::json!("Blob");
        });
        let wrong_parent = rewrite_json(&store, child.id, |value| {
            value["parents"][0]["kind"] = serde_json::json!("Tree");
        });
        let wrong_leaf_tree = rewrite_json(&store, parent.root_tree.id, |value| {
            value["entries"][0]["object"]["kind"] = serde_json::json!("Tree");
        });
        let invalid_ancestor = rewrite_json(&store, parent.id, |value| {
            value["rootTree"]["id"] = serde_json::json!(wrong_leaf_tree.to_hex());
        });
        let valid_tip_bad_ancestor = rewrite_json(&store, child.id, |value| {
            value["parents"][0]["id"] = serde_json::json!(invalid_ancestor.to_hex());
        });
        // The selected tip alone is valid; transport must inspect ancestor trees too.
        validate_snapshot(&store, &valid_tip_bad_ancestor).unwrap();
        for id in [wrong_root, wrong_parent, valid_tip_bad_ancestor] {
            assert!(matches!(
                collect_closure(&store, &[id]),
                Err(TransportError::Snapshot(
                    SnapshotError::InvalidObjectKind { .. }
                ))
            ));
        }
    }

    #[test]
    fn closure_rejects_malicious_tree_depth_without_recursive_stack_growth() {
        use crate::{write_snapshot, write_tree, EntryMode, ObjectRef, TreeEntry};
        use std::path::PathBuf;
        let store = InMemoryObjectStore::new();
        let mut tree = write_tree(&store, Vec::new()).unwrap();
        // Construct a content-addressed, typed graph deeper than native's 256 limit.
        // No filesystem recursion or unusually long single filename is involved.
        for depth in (1..=1_024).rev() {
            let path: PathBuf = std::iter::repeat_n("d", depth).collect();
            tree = write_tree(
                &store,
                vec![TreeEntry {
                    name: "d".into(),
                    path,
                    entry_type: EntryType::Directory,
                    object: ObjectRef::new(ObjectKind::Tree, tree.id),
                    mode: EntryMode::Directory,
                    size: None,
                    content_hash: None,
                }],
            )
            .unwrap();
        }
        let snapshot = write_snapshot(&store, tree.id, SnapshotOptions::new("repo")).unwrap();
        assert!(matches!(
            collect_closure(&store, &[snapshot.id]),
            Err(TransportError::Snapshot(SnapshotError::InvalidPath { .. }))
        ));
    }

    #[test]
    fn ancestry_rejects_equal_non_snapshots_and_wrong_parent_reference_kinds() {
        let store = InMemoryObjectStore::new();
        let root = sample_snapshot(&store);
        let blob = crate::write_blob(&store, b"not a snapshot").unwrap();
        assert!(is_descendant(&store, blob.id, blob.id).is_err());
        let child = child_snapshot(&store, &root, "b.txt", b"child");
        let wrong_parent = rewrite_json(&store, child.id, |value| {
            value["parents"][0]["kind"] = serde_json::json!("Tree");
        });
        for ancestor in [root.id, wrong_parent] {
            assert!(matches!(
                is_descendant(&store, ancestor, wrong_parent),
                Err(TransportError::Snapshot(
                    SnapshotError::InvalidObjectKind { .. }
                ))
            ));
        }
    }

    #[test]
    fn is_descendant_covers_parent_multi_hop_unrelated_and_same_id() {
        let store = InMemoryObjectStore::new();
        let root = sample_snapshot(&store);
        let child = child_snapshot(&store, &root, "b.txt", b"world\n");
        let grandchild = child_snapshot(&store, &child, "c.txt", b"!\n");

        let unrelated_dir = tempfile::tempdir().unwrap();
        write_file(unrelated_dir.path().join("x.txt"), b"other\n");
        let unrelated =
            materialize_snapshot(&store, unrelated_dir.path(), SnapshotOptions::new("repo"))
                .unwrap();

        assert!(is_descendant(&store, root.id, root.id).unwrap());
        assert!(is_descendant(&store, root.id, child.id).unwrap());
        assert!(is_descendant(&store, root.id, grandchild.id).unwrap());
        assert!(is_descendant(&store, child.id, grandchild.id).unwrap());
        assert!(!is_descendant(&store, child.id, root.id).unwrap());
        assert!(!is_descendant(&store, root.id, unrelated.id).unwrap());
        assert!(!is_descendant(&store, unrelated.id, root.id).unwrap());
    }
}
