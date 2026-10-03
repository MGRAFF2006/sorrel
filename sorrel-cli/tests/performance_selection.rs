//! Optimizing tracked-path selection must retain payload checks during recording.

use assert_cmd::Command;
use serde_json::Value;
use std::{fs, path::Path};

fn run(root: &Path, args: &[&str]) -> Value {
    let output = Command::cargo_bin("sorrel")
        .unwrap()
        .current_dir(root)
        .args(args)
        .arg("--json")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn warm_status_still_rejects_corrupt_cached_payload_without_changing_head() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    run(root, &["init"]);
    fs::write(root.join("tracked.bin"), vec![42; 256 * 1024]).unwrap();
    run(root, &["change", "create", "-m", "record payload"]);
    run(root, &["status"]);
    let head_before = fs::read(root.join(".sorrel/HEAD")).unwrap();
    let cache: Value =
        serde_json::from_slice(&fs::read(root.join(".sorrel/stat-cache.json")).unwrap()).unwrap();
    let blob = cache["entries"]["tracked.bin"]["objectId"]
        .as_str()
        .unwrap();
    let object = root
        .join(".sorrel/objects")
        .join(&blob[..2])
        .join(&blob[2..]);
    fs::write(object, b"corrupt object bytes").unwrap();

    let output = Command::cargo_bin("sorrel")
        .unwrap()
        .current_dir(root)
        .args(["status", "--json"])
        .output()
        .unwrap();
    assert!(
        !output.status.success(),
        "path-only selection cannot authorize a corrupt cached blob"
    );
    let error: Value =
        serde_json::from_slice(&output.stdout).expect("structured payload integrity failure");
    assert_eq!(error["status"], "error");
    assert!(error["error"]["message"]
        .as_str()
        .unwrap()
        .contains("content digest mismatch"));
    assert_eq!(fs::read(root.join(".sorrel/HEAD")).unwrap(), head_before);
    assert_eq!(
        fs::read(root.join("tracked.bin")).unwrap(),
        vec![42; 256 * 1024]
    );
}
