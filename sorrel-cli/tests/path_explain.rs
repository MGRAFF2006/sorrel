use assert_cmd::Command;
use serde_json::Value;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};
use tempfile::TempDir;

fn run(root: &Path, args: &[&str]) -> std::process::Output {
    Command::cargo_bin("sorrel")
        .unwrap()
        .current_dir(root)
        .args(args)
        .output()
        .unwrap()
}
fn explain(root: &Path, path: &str) -> Value {
    let output = run(root, &["path", "explain", path, "--json"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["command"], "path explain");
    assert_eq!(value["mocked"], false);
    value
}
fn state(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fn visit(root: &Path, dir: &Path, files: &mut BTreeMap<PathBuf, Vec<u8>>) {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                visit(root, &path, files);
            } else {
                files.insert(
                    path.strip_prefix(root).unwrap().into(),
                    fs::read(path).unwrap(),
                );
            }
        }
    }
    let mut files = BTreeMap::new();
    visit(root, root, &mut files);
    files
}

#[test]
fn explanation_works_without_initializing_or_reading_target_contents() {
    let root = TempDir::new().unwrap();
    fs::write(root.path().join(".sorrelignore"), "*.tmp\n!.env\n").unwrap();
    fs::write(root.path().join(".env"), "SYNTHETIC_CONTENT_NOT_FOR_OUTPUT").unwrap();
    let before = state(root.path());
    assert_eq!(explain(root.path(), "missing.txt")["included"], true);
    assert_eq!(explain(root.path(), "missing.tmp")["ignored"], true);
    let protected = explain(root.path(), ".env");
    assert_eq!(protected["protected"], true);
    assert_eq!(protected["included"], false);
    assert!(!protected
        .to_string()
        .contains("SYNTHETIC_CONTENT_NOT_FOR_OUTPUT"));
    assert_eq!(explain(root.path(), ".sorrel/objects")["metadata"], true);
    assert_eq!(state(root.path()), before);
    assert!(!root.path().join(".sorrel").exists());
}

#[test]
fn initialized_explanation_preserves_objects_cache_journal_and_files() {
    let root = TempDir::new().unwrap();
    assert!(run(root.path(), &["init", "--json"]).status.success());
    fs::write(root.path().join("tracked.txt"), "original content").unwrap();
    assert!(run(
        root.path(),
        &["change", "create", "--message", "track file", "--json"]
    )
    .status
    .success());
    fs::write(root.path().join(".gitignore"), "tracked.txt\n*.tmp\n").unwrap();
    fs::write(
        root.path().join("secretspec.toml"),
        "provider = 'dotenv:credentials.data'\n",
    )
    .unwrap();
    fs::write(
        root.path().join("credentials.data"),
        "SYNTHETIC_VALUE_NEVER_READ",
    )
    .unwrap();
    fs::remove_dir(root.path().join(".sorrel/tmp")).unwrap();
    // A writer would attempt journal recovery. Explanation must leave it alone.
    fs::write(
        root.path().join(".sorrel/metadata-transaction.json"),
        "pending malformed journal",
    )
    .unwrap();
    let before = state(root.path());
    for _ in 0..3 {
        let tracked = explain(root.path(), "./tracked.txt");
        assert_eq!(tracked["path"], "tracked.txt");
        assert_eq!(tracked["included"], true);
        assert_eq!(tracked["tracked"], true);
        assert_eq!(tracked["ignored"], true);
        assert_eq!(explain(root.path(), "new.tmp")["included"], false);
        assert_eq!(explain(root.path(), "credentials.data")["protected"], true);
        assert_eq!(explain(root.path(), ".sorrel/HEAD")["metadata"], true);
    }
    let human = run(root.path(), &["path", "explain", "tracked.txt"]);
    assert!(human.status.success());
    assert!(String::from_utf8(human.stdout)
        .unwrap()
        .contains("tracked=true ignored=true"));
    assert_eq!(state(root.path()), before);
    assert!(!root.path().join(".sorrel/tmp").exists());
}

#[test]
fn invalid_paths_and_unknown_manifest_versions_fail_without_mutations() {
    let root = TempDir::new().unwrap();
    assert!(run(root.path(), &["init", "--json"]).status.success());
    let before = state(root.path());
    for path in ["../outside", root.path().to_str().unwrap()] {
        assert!(!run(root.path(), &["path", "explain", path, "--json"])
            .status
            .success());
        assert_eq!(state(root.path()), before);
    }
    let manifest = root.path().join(".sorrel/manifest.json");
    let mut value: Value = serde_json::from_slice(&fs::read(&manifest).unwrap()).unwrap();
    value["schemaVersion"] = "sorrel.protocol.v99".into();
    fs::write(manifest, serde_json::to_vec(&value).unwrap()).unwrap();
    let before = state(root.path());
    let output = run(root.path(), &["path", "explain", "file.txt", "--json"]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("unsupported workspace schema"));
    assert_eq!(state(root.path()), before);
}
