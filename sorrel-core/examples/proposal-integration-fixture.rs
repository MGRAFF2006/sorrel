//! Disposable real-Core snapshot graph for the Hub integration acceptance suite.
use sorrel_core::{materialize_snapshot, FileObjectStore, ObjectKind, ObjectRef, SnapshotOptions};
use std::{collections::BTreeMap, path::PathBuf};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(
        std::env::args()
            .nth(1)
            .ok_or("temporary output directory required")?,
    );
    let store = FileObjectStore::new(root.join("core"))?;
    let mut ids = BTreeMap::new();
    let mut snapshot = |name: &str,
                        files: &[(&str, &[u8])],
                        parents: &[&str]|
     -> Result<(), Box<dyn std::error::Error>> {
        let dir = root.join(name);
        std::fs::create_dir_all(&dir)?;
        for (path, bytes) in files {
            let path = dir.join(path);
            std::fs::create_dir_all(path.parent().unwrap())?;
            std::fs::write(path, bytes)?;
        }
        let mut options = SnapshotOptions::new("repo_test");
        options.message = Some(name.to_owned());
        options.parents = parents
            .iter()
            .map(|name| ObjectRef::new(ObjectKind::Snapshot, ids[*name]))
            .collect();
        let result = materialize_snapshot(&store, dir, options)?;
        ids.insert(name.to_owned(), result.id);
        Ok(())
    };
    snapshot(
        "base",
        &[("shared", b"a\nb\nc\n"), ("binary", b"\0\xff")],
        &[],
    )?;
    snapshot(
        "source",
        &[
            ("shared", b"a\nB\nc\n"),
            ("binary", b"\0\xff"),
            ("source-only", b"source\n"),
        ],
        &["base"],
    )?;
    snapshot(
        "target",
        &[
            ("shared", b"a\nb\nC\n"),
            ("binary", b"\0\xff"),
            ("target-only", b"target\n"),
        ],
        &["base"],
    )?;
    snapshot(
        "conflict",
        &[("shared", b"a\nother\nc\n"), ("binary", b"\0\xff")],
        &["base"],
    )?;
    snapshot("deleted", &[("binary", b"\0\xff")], &["base"])?;
    snapshot(
        "binary1",
        &[("binary", b"\0\xfe"), ("shared", b"a\nb\nc\n")],
        &["base"],
    )?;
    snapshot(
        "binary2",
        &[("binary", b"\0\xfd"), ("shared", b"a\nb\nc\n")],
        &["base"],
    )?;
    snapshot("add1", &[("added", b"first\n")], &["base"])?;
    snapshot("add2", &[("added", b"second\n")], &["base"])?;
    snapshot("file", &[("entry", b"file\n")], &["base"])?;
    snapshot("directory", &[("entry/child", b"child\n")], &["base"])?;
    println!(
        "{}",
        serde_json::to_string(
            &ids.iter()
                .map(|(key, id)| (key, id.to_string()))
                .collect::<BTreeMap<_, _>>()
        )?
    );
    Ok(())
}
