//! Recovery metadata survives unrelated commands and refuses symlink redirection.

use assert_cmd::Command;
use serde_json::{json, Value};
use std::{fs, path::Path, process::Output};

fn output(root: &Path, args: &[&str]) -> Output {
    Command::cargo_bin("sorrel")
        .unwrap()
        .current_dir(root)
        .args(args)
        .arg("--json")
        .output()
        .unwrap()
}

fn run(root: &Path, args: &[&str]) -> Value {
    let result = output(root, args);
    assert!(
        result.status.success(),
        "sorrel {args:?}: stdout={} stderr={}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    serde_json::from_slice(&result.stdout).unwrap()
}

fn failure(root: &Path, args: &[&str]) -> Value {
    let result = output(root, args);
    assert!(
        !result.status.success(),
        "sorrel {args:?} unexpectedly succeeded"
    );
    let value: Value = serde_json::from_slice(&result.stdout)
        .expect("JSON failures must be readable without parsing stderr");
    assert_eq!(value["schemaVersion"], "sorrel.cli.v1");
    assert_eq!(value["status"], "error");
    assert!(value["error"]["message"]
        .as_str()
        .is_some_and(|message| !message.is_empty()));
    value
}

#[test]
fn commands_without_vcs_lock_preserve_pending_checkout_recovery() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    run(root, &["init"]);
    let original_head = fs::read(root.join(".sorrel/HEAD")).unwrap();
    let head: Value = serde_json::from_slice(&original_head).unwrap();
    let journal = serde_json::to_vec(&json!({
        "schemaVersion": "sorrel.protocol.v0",
        "kind": "CheckoutRecovery",
        "current": head["snapshot"],
        "target": head["snapshot"],
        "head": head,
        "mergeState": null,
    }))
    .unwrap();
    fs::write(root.join(".sorrel/CHECKOUT_STATE"), &journal).unwrap();

    for args in [
        &["env", "info"][..],
        &["run", "list"][..],
        &["policy", "evaluate"][..],
    ] {
        run(root, args);
        assert_eq!(
            fs::read(root.join(".sorrel/CHECKOUT_STATE")).unwrap(),
            journal
        );
        assert_eq!(fs::read(root.join(".sorrel/HEAD")).unwrap(), original_head);
    }
    assert_eq!(
        failure(root, &["status"])["error"]["code"],
        "recovery_required"
    );
    assert_eq!(
        fs::read(root.join(".sorrel/CHECKOUT_STATE")).unwrap(),
        journal
    );
}

#[cfg(unix)]
#[test]
fn symlinked_lane_head_directory_cannot_redirect_a_recording_write() {
    use std::os::unix::fs::symlink;
    let dir = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let root = dir.path();
    run(root, &["init"]);
    let original_head = fs::read(root.join(".sorrel/HEAD")).unwrap();
    fs::write(outside.path().join("sentinel"), b"untouched").unwrap();
    fs::rename(root.join(".sorrel/heads"), root.join(".sorrel/saved-heads")).unwrap();
    symlink(outside.path(), root.join(".sorrel/heads")).unwrap();
    fs::write(root.join("tracked.txt"), b"record this").unwrap();

    let error = failure(root, &["change", "create", "-m", "must not escape"]);
    assert!(error["error"]["message"]
        .as_str()
        .unwrap()
        .contains("symlink"));
    assert_eq!(fs::read(root.join(".sorrel/HEAD")).unwrap(), original_head);
    assert_eq!(
        fs::read(outside.path().join("sentinel")).unwrap(),
        b"untouched"
    );
    assert_eq!(fs::read_dir(outside.path()).unwrap().count(), 1);
}

#[cfg(unix)]
#[test]
fn symlinked_recovery_journals_are_neither_replayed_nor_modified() {
    use std::os::unix::fs::symlink;
    for (journal_name, args) in [
        ("HEAD_TRANSACTION", &["status"][..]),
        ("CHECKOUT_STATE", &["recover", "--abort"][..]),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let root = dir.path();
        run(root, &["init"]);
        let original_head = fs::read(root.join(".sorrel/HEAD")).unwrap();
        let head: Value = serde_json::from_slice(&original_head).unwrap();
        let external_bytes = serde_json::to_vec(&if journal_name == "HEAD_TRANSACTION" {
            head.clone()
        } else {
            json!({
                "schemaVersion": "sorrel.protocol.v0", "kind": "CheckoutRecovery",
                "current": head["snapshot"], "target": head["snapshot"], "head": head,
                "mergeState": null,
            })
        })
        .unwrap();
        let external = outside.path().join("journal.json");
        fs::write(&external, &external_bytes).unwrap();
        let local = root.join(".sorrel").join(journal_name);
        symlink(&external, &local).unwrap();

        let error = failure(root, args);
        assert!(error["error"]["message"]
            .as_str()
            .unwrap()
            .contains("symlink"));
        assert!(fs::symlink_metadata(&local)
            .unwrap()
            .file_type()
            .is_symlink());
        assert_eq!(fs::read(&external).unwrap(), external_bytes);
        assert_eq!(fs::read(root.join(".sorrel/HEAD")).unwrap(), original_head);
    }
}

#[test]
fn invalid_arguments_emit_the_same_json_error_envelope_as_runtime_failures() {
    let dir = tempfile::tempdir().unwrap();
    for args in [
        &["not-a-command"][..],
        &["workspace", "create"][..],
        &["change", "create"][..],
        &["--not-a-real-option"][..],
    ] {
        assert_eq!(failure(dir.path(), args)["error"]["code"], "invalid_input");
    }
}

#[test]
fn initialization_refuses_to_reset_refs_when_the_manifest_is_missing() {
    let dir = tempfile::tempdir().unwrap();
    run(dir.path(), &["init"]);
    fs::write(dir.path().join("recorded.txt"), "recorded work").unwrap();
    run(dir.path(), &["change", "create", "-m", "Keep this"]);
    let before = fs::read(dir.path().join(".sorrel/HEAD")).unwrap();
    fs::remove_file(dir.path().join(".sorrel/manifest.json")).unwrap();
    assert_eq!(
        failure(dir.path(), &["init"])["error"]["code"],
        "invalid_workspace"
    );
    assert_eq!(fs::read(dir.path().join(".sorrel/HEAD")).unwrap(), before);
}
