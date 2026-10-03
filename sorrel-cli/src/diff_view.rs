//! Shared recorded/working-tree diff presentation. Only changed blobs are read.
use std::collections::BTreeMap;
use std::io;

use serde_json::{json, Value};
use sorrel_core::{
    read_blob, read_snapshot_entries, snapshot_diff, EntryMode, EntryType, ObjectId, ObjectStore,
    PathChangeKind, TreeEntry,
};

use crate::linediff;

pub struct RenderedDiff {
    pub files: Vec<Value>,
    pub human: String,
}

fn mode(entry: Option<&TreeEntry>) -> Option<&'static str> {
    entry.map(|entry| match entry.mode {
        EntryMode::Normal => "normal",
        EntryMode::Executable => "executable",
        EntryMode::Directory => "directory",
    })
}

fn content(store: &impl ObjectStore, entry: Option<&TreeEntry>) -> io::Result<Vec<u8>> {
    let Some(entry) = entry.filter(|entry| entry.entry_type == EntryType::File) else {
        return Ok(Vec::new());
    };
    let blob = read_blob(store, &entry.object.id).map_err(io::Error::other)?;
    check_metadata(entry, blob.content.len(), blob.content_hash)?;
    Ok(blob.content)
}

fn check_metadata(entry: &TreeEntry, length: usize, hash: ObjectId) -> io::Result<()> {
    if entry.size.is_some_and(|size| size != length as u64)
        || entry.content_hash.is_some_and(|expected| expected != hash)
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "changed file metadata does not match its blob",
        ));
    }
    Ok(())
}

pub fn render(
    store: &impl ObjectStore,
    base: &ObjectId,
    tip: &ObjectId,
) -> io::Result<RenderedDiff> {
    let diff = snapshot_diff(store, base, tip).map_err(io::Error::other)?;
    let old_entries: BTreeMap<_, _> = read_snapshot_entries(store, base)
        .map_err(io::Error::other)?
        .into_iter()
        .map(|entry| (entry.path.clone(), entry))
        .collect();
    let new_entries: BTreeMap<_, _> = read_snapshot_entries(store, tip)
        .map_err(io::Error::other)?
        .into_iter()
        .map(|entry| (entry.path.clone(), entry))
        .collect();
    let mut files = Vec::new();
    let mut human = String::new();
    for change in diff.changes {
        let path = change.path.to_string_lossy().replace('\\', "/");
        let kind = match change.kind {
            PathChangeKind::Added => "added",
            PathChangeKind::Modified => "modified",
            PathChangeKind::Deleted => "deleted",
        };
        let old = old_entries.get(&change.path);
        let new = new_entries.get(&change.path);
        let old_mode = mode(old);
        let new_mode = mode(new);
        let old_bytes = content(store, old)?;
        let new_bytes = match (old, new) {
            (Some(a), Some(b))
                if a.entry_type == EntryType::File
                    && b.entry_type == EntryType::File
                    && a.object.id == b.object.id =>
            {
                // Avoid a duplicate payload read, but validate both entries.
                let hash = a
                    .content_hash
                    .unwrap_or_else(|| ObjectId::for_bytes(&old_bytes));
                check_metadata(b, old_bytes.len(), hash)?;
                old_bytes.clone()
            }
            _ => content(store, new)?,
        };
        let binary = old_bytes.contains(&0)
            || new_bytes.contains(&0)
            || std::str::from_utf8(&old_bytes).is_err()
            || std::str::from_utf8(&new_bytes).is_err();
        let mut file = json!({"path":path,"kind":kind,"binary":binary,"hunks":[],"oldMode":old_mode,"newMode":new_mode,"modeChanged":old_mode != new_mode});
        human.push_str(&format!("diff --sorrel {path} ({kind})\n"));
        if old_mode != new_mode {
            human.push_str(&format!(
                "Mode {} -> {}\n",
                old_mode.unwrap_or("absent"),
                new_mode.unwrap_or("absent")
            ));
        }
        if binary {
            if old_bytes != new_bytes {
                human.push_str("Binary file changed\n");
            }
        } else {
            let old_text = std::str::from_utf8(&old_bytes).map_err(io::Error::other)?;
            let new_text = std::str::from_utf8(&new_bytes).map_err(io::Error::other)?;
            let hunks = linediff::hunks(old_text, new_text, 3);
            human.push_str(&linediff::render_unified(&hunks));
            let newline_old = old_text.ends_with('\n');
            let newline_new = new_text.ends_with('\n');
            if newline_old != newline_new {
                human.push_str(if newline_new {
                    "Final newline added\n"
                } else {
                    "Final newline removed\n"
                });
            } else if old_bytes != new_bytes && hunks.is_empty() {
                human.push_str("Line-ending bytes changed\n");
            }
            file["oldFinalNewline"] = json!(newline_old);
            file["newFinalNewline"] = json!(newline_new);
            file["lineEndingsChanged"] = json!(old_bytes != new_bytes && hunks.is_empty());
            file["hunks"] = json!(hunks.iter().map(|hunk| json!({
                "oldStart":hunk.old_start,"oldLen":hunk.old_len,
                "newStart":hunk.new_start,"newLen":hunk.new_len,
                "lines":hunk.lines.iter().map(|line| json!({"kind":match line.kind {
                    linediff::LineKind::Context=>"context",linediff::LineKind::Added=>"added",linediff::LineKind::Removed=>"removed"
                },"text":line.text})).collect::<Vec<_>>()
            })).collect::<Vec<_>>());
        }
        files.push(file);
    }
    if files.is_empty() {
        human.push_str("No recorded file changes");
    }
    Ok(RenderedDiff {
        files,
        human: human.trim_end().to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use sorrel_core::{
        write_blob, write_snapshot, write_tree, InMemoryObjectStore, ObjectKind, ObjectRef,
        SnapshotOptions,
    };
    use std::{cell::RefCell, path::PathBuf};

    #[derive(Default)]
    struct CountingStore {
        inner: InMemoryObjectStore,
        reads: RefCell<BTreeMap<ObjectId, usize>>,
    }
    impl ObjectStore for CountingStore {
        fn write(&self, bytes: &[u8]) -> sorrel_core::ObjectStoreResult<ObjectId> {
            self.inner.write(bytes)
        }
        fn read(&self, id: &ObjectId) -> sorrel_core::ObjectStoreResult<Vec<u8>> {
            *self.reads.borrow_mut().entry(*id).or_default() += 1;
            self.inner.read(id)
        }
        fn has(&self, id: &ObjectId) -> sorrel_core::ObjectStoreResult<bool> {
            self.inner.has(id)
        }
    }

    fn entry(store: &impl ObjectStore, name: &str, bytes: &[u8], mode: EntryMode) -> TreeEntry {
        let blob = write_blob(store, bytes).unwrap();
        TreeEntry {
            name: name.to_owned(),
            path: PathBuf::from(name),
            entry_type: EntryType::File,
            object: ObjectRef::new(ObjectKind::Blob, blob.id),
            mode,
            size: Some(bytes.len() as u64),
            content_hash: Some(blob.content_hash),
        }
    }
    fn snapshot(store: &impl ObjectStore, entries: Vec<TreeEntry>) -> ObjectId {
        let tree = write_tree(store, entries).unwrap();
        write_snapshot(store, tree.id, SnapshotOptions::new("repo_test"))
            .unwrap()
            .id
    }

    #[test]
    fn nul_bytes_are_binary_and_unchanged_payloads_are_not_read() {
        let store = CountingStore::default();
        let unchanged = entry(&store, "unchanged.bin", &[255; 128], EntryMode::Normal);
        let old = entry(&store, "changed.bin", &[0, 1], EntryMode::Normal);
        let new = entry(&store, "changed.bin", &[0, 2], EntryMode::Normal);
        let base = snapshot(&store, vec![unchanged.clone(), old]);
        let tip = snapshot(&store, vec![unchanged.clone(), new]);
        store.reads.borrow_mut().clear();
        let output = render(&store, &base, &tip).unwrap();
        assert_eq!(output.files.len(), 1);
        assert_eq!(output.files[0]["binary"], true);
        assert!(output.human.contains("Binary file changed"));
        assert!(!store.reads.borrow().contains_key(&unchanged.object.id));
    }

    #[test]
    fn mode_only_changes_are_visible_without_text_hunks() {
        let store = InMemoryObjectStore::new();
        let old = entry(&store, "script.sh", b"echo hello\n", EntryMode::Normal);
        let mut new = old.clone();
        new.mode = EntryMode::Executable;
        let base = snapshot(&store, vec![old]);
        let tip = snapshot(&store, vec![new]);
        let output = render(&store, &base, &tip).unwrap();
        assert_eq!(output.files[0]["oldMode"], "normal");
        assert_eq!(output.files[0]["newMode"], "executable");
        assert_eq!(output.files[0]["hunks"], json!([]));
        assert!(output.human.contains("Mode normal -> executable"));
    }

    #[test]
    fn optional_file_metadata_remains_valid() {
        let store = InMemoryObjectStore::new();
        let mut old = entry(&store, "script", b"echo hello\n", EntryMode::Normal);
        old.size = None;
        old.content_hash = None;
        let mut new = old.clone();
        new.mode = EntryMode::Executable;
        let base = snapshot(&store, vec![old]);
        let tip = snapshot(&store, vec![new]);
        assert_eq!(
            render(&store, &base, &tip).unwrap().files[0]["newMode"],
            "executable"
        );
    }

    #[test]
    fn mode_change_cannot_hide_inconsistent_new_metadata() {
        let store = InMemoryObjectStore::new();
        let old = entry(&store, "script", b"echo hello\n", EntryMode::Normal);
        let mut new = old.clone();
        new.mode = EntryMode::Executable;
        new.size = Some(1);
        let base = snapshot(&store, vec![old]);
        let tip = snapshot(&store, vec![new]);
        assert!(render(&store, &base, &tip).is_err());
    }

    #[test]
    fn final_newline_only_changes_are_visible() {
        let store = InMemoryObjectStore::new();
        let base = snapshot(
            &store,
            vec![entry(&store, "text", b"hello\n", EntryMode::Normal)],
        );
        let tip = snapshot(
            &store,
            vec![entry(&store, "text", b"hello", EntryMode::Normal)],
        );
        let output = render(&store, &base, &tip).unwrap();
        assert_eq!(output.files[0]["oldFinalNewline"], true);
        assert_eq!(output.files[0]["newFinalNewline"], false);
        assert!(output.human.contains("Final newline removed"));
    }
}
