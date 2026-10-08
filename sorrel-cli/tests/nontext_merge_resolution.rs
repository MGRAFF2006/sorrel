use assert_cmd::Command;
use serde_json::Value;
use sorrel_core::{read_snapshot_files, FileObjectStore, ObjectId};
use std::{fs, path::Path, process::Output};
use tempfile::TempDir;

fn run(root: &Path, args: &[&str]) -> Output {
    Command::cargo_bin("sorrel")
        .unwrap()
        .current_dir(root)
        .args(args)
        .arg("--json")
        .output()
        .unwrap()
}
fn command(root: &Path, args: &[&str]) -> Value {
    let output = run(root, args);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}
fn conflicted(case: &str, paths: &[&str]) -> TempDir {
    let root = TempDir::new().unwrap();
    command(root.path(), &["init"]);
    for path in paths {
        fs::write(
            root.path().join(path),
            if case == "binary" || (case == "mixed" && path.ends_with(".bin")) {
                b"base\xff"
            } else {
                b"base\n"
            },
        )
        .unwrap();
    }
    command(root.path(), &["change", "create", "-m", "base"]);
    let lane = command(root.path(), &["lane", "create", "--name", "incoming"])["object"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    for path in paths {
        if case == "ours-delete" {
            fs::remove_file(root.path().join(path)).unwrap();
        } else {
            fs::write(
                root.path().join(path),
                if case == "binary" || (case == "mixed" && path.ends_with(".bin")) {
                    b"ours\xff"
                } else {
                    b"ours\n"
                },
            )
            .unwrap();
        }
    }
    command(root.path(), &["change", "create", "-m", "ours"]);
    command(root.path(), &["lane", "switch", &lane]);
    for path in paths {
        if case == "theirs-delete" {
            fs::remove_file(root.path().join(path)).unwrap();
        } else {
            fs::write(
                root.path().join(path),
                if case == "binary" || (case == "mixed" && path.ends_with(".bin")) {
                    b"theirs\xff"
                } else {
                    b"theirs\n"
                },
            )
            .unwrap();
        }
    }
    command(root.path(), &["change", "create", "-m", "theirs"]);
    command(root.path(), &["lane", "switch", "lane_main"]);
    assert!(!run(root.path(), &["merge", &lane]).status.success());
    assert!(root.path().join(".sorrel/MERGE_STATE").is_file());
    root
}
fn blocked(root: &Path, args: &[&str]) -> String {
    let head = fs::read(root.join(".sorrel/HEAD")).unwrap();
    let state = fs::read(root.join(".sorrel/MERGE_STATE")).unwrap();
    let output = run(root, args);
    assert!(!output.status.success());
    assert_eq!(fs::read(root.join(".sorrel/HEAD")).unwrap(), head);
    assert_eq!(fs::read(root.join(".sorrel/MERGE_STATE")).unwrap(), state);
    String::from_utf8(output.stderr).unwrap()
}

#[test]
fn every_nontext_kind_requires_acknowledgment_and_can_keep_or_delete_default() {
    for case in ["binary", "ours-delete", "theirs-delete"] {
        for delete in [false, true] {
            let root = conflicted(case, &["conflict data.bin"]);
            let path = root.path().join("conflict data.bin");
            let before = fs::read(&path).unwrap();
            assert!(blocked(root.path(), &["merge", "--continue"]).contains("--resolved"));
            assert_eq!(fs::read(&path).unwrap(), before);
            assert!(blocked(
                root.path(),
                &["merge", "--continue", "--resolved", "typo.bin"]
            )
            .contains("conflict"));
            if delete {
                fs::remove_file(&path).unwrap();
            }
            let result = command(
                root.path(),
                &["merge", "--continue", "--resolved", "conflict data.bin"],
            );
            assert_eq!(result["status"], "merged");
            assert!(!root.path().join(".sorrel/MERGE_STATE").exists());
            let id: ObjectId = result["headSnapshot"]["id"]
                .as_str()
                .unwrap()
                .parse()
                .unwrap();
            let store = FileObjectStore::new(root.path().join(".sorrel")).unwrap();
            let files = read_snapshot_files(&store, &id).unwrap();
            if delete {
                assert!(!files.contains_key(Path::new("conflict data.bin")));
            } else {
                assert_eq!(files[Path::new("conflict data.bin")], before);
            }
        }
    }
}

#[test]
fn acknowledgments_must_cover_each_conflict_and_are_not_saved_between_attempts() {
    let root = conflicted("binary", &["a.bin", "b.bin"]);
    assert!(
        blocked(root.path(), &["merge", "--continue", "--resolved", "a.bin"]).contains("b.bin")
    );
    assert!(
        blocked(root.path(), &["merge", "--continue", "--resolved", "b.bin"]).contains("a.bin")
    );
    assert!(blocked(
        root.path(),
        &[
            "merge",
            "--continue",
            "--resolved",
            "a.bin",
            "--resolved",
            "a.bin"
        ]
    )
    .contains("b.bin"));
    command(
        root.path(),
        &[
            "merge",
            "--continue",
            "--resolved",
            "./a.bin",
            "--resolved",
            "b.bin",
        ],
    );
}

#[test]
fn resolved_option_is_only_valid_during_continuation() {
    let root = conflicted("binary", &["a.bin"]);
    blocked(root.path(), &["merge", "--abort", "--resolved", "a.bin"]);
    blocked(
        root.path(),
        &["merge", "--continue", "--resolved", "../a.bin"],
    );
    command(root.path(), &["merge", "--abort"]);
}

#[test]
fn acknowledgment_never_bypasses_text_markers_and_preserves_explicit_edits() {
    let root = conflicted("mixed", &["text.txt", "binary.bin"]);
    assert!(blocked(
        root.path(),
        &["merge", "--continue", "--resolved", "binary.bin"]
    )
    .contains("markers"));
    fs::write(root.path().join("text.txt"), b"chosen text\n").unwrap();
    fs::write(root.path().join("binary.bin"), b"chosen binary\xff").unwrap();
    assert!(blocked(
        root.path(),
        &["merge", "--continue", "--resolved", "text.txt"]
    )
    .contains("binary.bin"));
    let result = command(
        root.path(),
        &[
            "merge",
            "--continue",
            "--resolved",
            "binary.bin",
            "--resolved",
            "text.txt",
        ],
    );
    let id: ObjectId = result["headSnapshot"]["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    let store = FileObjectStore::new(root.path().join(".sorrel")).unwrap();
    let files = read_snapshot_files(&store, &id).unwrap();
    assert_eq!(files[Path::new("text.txt")], b"chosen text\n");
    assert_eq!(files[Path::new("binary.bin")], b"chosen binary\xff");
}

#[test]
fn continuation_rejects_an_unreadable_stored_merge_result() {
    let root = conflicted("binary", &["a.bin"]);
    let path = root.path().join(".sorrel/MERGE_STATE");
    let mut state: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    state["mergeResult"] = Value::from("invalid");
    fs::write(&path, serde_json::to_vec(&state).unwrap()).unwrap();
    assert!(
        blocked(root.path(), &["merge", "--continue", "--resolved", "a.bin"])
            .contains("invalid merge result")
    );
}
