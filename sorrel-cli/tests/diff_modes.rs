use assert_cmd::Command;
use serde_json::{json, Value};
use std::{fs, path::Path};
use tempfile::TempDir;

fn command(root: &Path, args: &[&str]) -> Value {
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
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn human_diff(root: &Path) -> String {
    let output = Command::cargo_bin("sorrel")
        .unwrap()
        .current_dir(root)
        .arg("diff")
        .output()
        .unwrap();
    assert!(output.status.success());
    String::from_utf8(output.stdout).unwrap()
}

fn file<'a>(diff: &'a Value, path: &str) -> &'a Value {
    diff["files"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["path"] == path)
        .unwrap()
}

#[cfg(unix)]
fn executable(path: &Path, enabled: bool) {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(
        path,
        fs::Permissions::from_mode(if enabled { 0o755 } else { 0o644 }),
    )
    .unwrap();
}

#[cfg(unix)]
#[test]
fn diff_shows_both_executable_transitions_without_content_hunks() {
    let root = TempDir::new().unwrap();
    command(root.path(), &["init"]);
    fs::create_dir(root.path().join("scripts")).unwrap();
    let path = root.path().join("scripts/run.sh");
    fs::write(&path, "echo same bytes\n").unwrap();
    executable(&path, false);
    command(root.path(), &["change", "create", "-m", "base"]);
    executable(&path, true);
    let diff = command(root.path(), &["diff"]);
    let entry = file(&diff, "scripts/run.sh");
    assert_eq!(entry["kind"], "modified");
    assert_eq!(entry["oldMode"], "normal");
    assert_eq!(entry["newMode"], "executable");
    assert_eq!(entry["hunks"], json!([]));
    let human = human_diff(root.path());
    assert!(human.contains("old mode normal\nnew mode executable\n"));
    command(root.path(), &["change", "create", "-m", "executable"]);
    executable(&path, false);
    let diff = command(root.path(), &["diff"]);
    let entry = file(&diff, "scripts/run.sh");
    assert_eq!(entry["oldMode"], "executable");
    assert_eq!(entry["newMode"], "normal");
    assert_eq!(entry["hunks"], json!([]));
    assert!(human_diff(root.path()).contains("old mode executable\nnew mode normal\n"));
}

#[cfg(unix)]
#[test]
fn diff_shows_binary_mode_change_without_claiming_content_changed() {
    let root = TempDir::new().unwrap();
    command(root.path(), &["init"]);
    let path = root.path().join("binary.dat");
    fs::write(&path, b"unchanged\xff").unwrap();
    executable(&path, false);
    command(root.path(), &["change", "create", "-m", "base"]);
    executable(&path, true);
    let diff = command(root.path(), &["diff"]);
    let entry = file(&diff, "binary.dat");
    assert_eq!(entry["binary"], true);
    assert_eq!(entry["oldMode"], "normal");
    assert_eq!(entry["newMode"], "executable");
    assert_eq!(entry["hunks"], json!([]));
    let human = human_diff(root.path());
    assert!(human.contains("new mode executable"));
    assert!(!human.contains("Binary file changed"));
}

#[test]
fn diff_reports_added_deleted_empty_files_and_directory_modes() {
    let root = TempDir::new().unwrap();
    command(root.path(), &["init"]);
    fs::write(root.path().join("deleted.txt"), "").unwrap();
    command(root.path(), &["change", "create", "-m", "base"]);
    fs::remove_file(root.path().join("deleted.txt")).unwrap();
    fs::write(root.path().join("added.txt"), "").unwrap();
    fs::create_dir(root.path().join("empty-directory")).unwrap();
    let diff = command(root.path(), &["diff"]);
    let deleted = file(&diff, "deleted.txt");
    assert_eq!(deleted["oldMode"], "normal");
    assert!(deleted.get("newMode").unwrap().is_null());
    let added = file(&diff, "added.txt");
    assert!(added.get("oldMode").unwrap().is_null());
    assert_eq!(added["newMode"], "normal");
    let directory = file(&diff, "empty-directory");
    assert!(directory.get("oldMode").unwrap().is_null());
    assert_eq!(directory["newMode"], "directory");
    assert_eq!(directory["hunks"], json!([]));
    let human = human_diff(root.path());
    assert!(human.contains("new mode directory"));
    assert!(human.contains("old mode normal"));
}

#[cfg(unix)]
#[test]
fn diff_preserves_content_hunks_when_mode_also_changes() {
    let root = TempDir::new().unwrap();
    command(root.path(), &["init"]);
    let path = root.path().join("script.sh");
    fs::write(&path, "old text\n").unwrap();
    executable(&path, false);
    command(root.path(), &["change", "create", "-m", "base"]);
    fs::write(&path, "new text\n").unwrap();
    executable(&path, true);
    let diff = command(root.path(), &["diff"]);
    let entry = file(&diff, "script.sh");
    assert_eq!(entry["oldMode"], "normal");
    assert_eq!(entry["newMode"], "executable");
    assert_eq!(
        entry["hunks"][0]["lines"],
        json!([
            {"kind":"removed", "text":"old text"},
            {"kind":"added", "text":"new text"}
        ])
    );
    let human = human_diff(root.path());
    assert!(human.contains("old mode normal\nnew mode executable\n@@"));
    assert!(human.contains("-old text\n+new text\n"));
}
