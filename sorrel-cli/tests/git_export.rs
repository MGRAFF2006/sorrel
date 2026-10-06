//! Integration tests for `sorrel git export` and `sorrel stack`.

use assert_cmd::Command;
use serde_json::Value;
use std::path::Path;
use tempfile::TempDir;

fn command_json(cwd: &Path, args: &[&str]) -> Value {
    let output = Command::cargo_bin("sorrel")
        .expect("sorrel binary")
        .current_dir(cwd)
        .args(args)
        .output()
        .expect("run sorrel");
    assert!(
        output.status.success(),
        "sorrel {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("json stdout")
}

#[test]
fn git_export_writes_branch_and_map() {
    let temp = TempDir::new().expect("temp");
    let root = temp.path();
    command_json(root, &["init", "--json"]);
    std::fs::write(root.join("a.txt"), b"one\n").unwrap();
    command_json(root, &["change", "create", "-m", "first", "--json"]);
    std::fs::write(root.join("a.txt"), b"two\n").unwrap();
    command_json(root, &["change", "create", "-m", "second", "--json"]);

    let dest = root.join("exported.git");
    let exported = command_json(
        root,
        &[
            "git",
            "export",
            dest.to_str().unwrap(),
            "--branch",
            "export-main",
            "--json",
        ],
    );
    assert_eq!(exported["command"], "git export");
    assert_eq!(exported["status"], "exported");
    assert!(exported["createdCommits"].as_u64().unwrap() >= 2);
    assert_eq!(exported["branch"], "export-main");
    assert!(root.join(".sorrel/git-map.json").is_file());
    assert!(dest.join(".git").is_dir() || dest.join("HEAD").is_file());

    // Re-export should reuse map (0 new commits ideally, or at least succeed).
    let again = command_json(
        root,
        &[
            "git",
            "export",
            dest.to_str().unwrap(),
            "--branch",
            "export-main",
            "--json",
        ],
    );
    assert_eq!(again["status"], "exported");
    assert_eq!(again["createdCommits"], 0);
}

#[test]
fn unchanged_exports_keep_mapping_bytes_and_depth_bounded() {
    let workspace = TempDir::new().unwrap();
    let dest = TempDir::new().unwrap();
    let root = workspace.path();
    command_json(root, &["init", "--json"]);
    std::fs::write(root.join("a.txt"), b"one\n").unwrap();
    command_json(root, &["change", "create", "-m", "first", "--json"]);
    let args = ["git", "export", dest.path().to_str().unwrap(), "--json"];
    command_json(root, &args);
    command_json(root, &args); // The first reused export changes `created` to false.
    let map_path = root.join(".sorrel/git-map.json");
    let baseline = std::fs::read(&map_path).unwrap();
    for _ in 0..200 {
        let result = command_json(root, &args);
        assert_eq!(result["createdCommits"], 0);
        assert_eq!(std::fs::read(&map_path).unwrap(), baseline);
    }
    let map: Value = serde_json::from_slice(&baseline).unwrap();
    assert!(map.get("previous").is_none());
    fn depth(value: &Value) -> usize {
        match value {
            Value::Object(fields) => 1 + fields.values().map(depth).max().unwrap_or(0),
            Value::Array(items) => 1 + items.iter().map(depth).max().unwrap_or(0),
            _ => 0,
        }
    }
    assert!(depth(&map) <= 3);
}

#[test]
fn export_and_sync_read_deep_legacy_archives_and_export_flattens_them() {
    let workspace = TempDir::new().unwrap();
    let dest = TempDir::new().unwrap();
    let root = workspace.path();
    command_json(root, &["init", "--json"]);
    std::fs::write(root.join("a.txt"), b"one\n").unwrap();
    command_json(root, &["change", "create", "-m", "first", "--json"]);
    let args = ["git", "export", dest.path().to_str().unwrap(), "--json"];
    command_json(root, &args);
    let map_path = root.join(".sorrel/git-map.json");
    let current: Value = serde_json::from_slice(&std::fs::read(&map_path).unwrap()).unwrap();
    let mut archive = "null".to_owned();
    for _ in 0..300 {
        archive = format!("{{\"previous\":{archive}}}");
    }
    let legacy = format!(
        "{{\"gitToSnapshot\":{},\"commits\":{},\"previous\":{archive}}}",
        current["gitToSnapshot"], current["commits"]
    );
    assert!(serde_json::from_str::<Value>(&legacy).is_err());
    std::fs::write(&map_path, &legacy).unwrap();
    let synced = command_json(
        root,
        &["git", "sync", dest.path().to_str().unwrap(), "--json"],
    );
    assert_eq!(synced["status"], "up-to-date");
    let exported = command_json(root, &args);
    assert_eq!(exported["createdCommits"], 0);
    let bytes = std::fs::read(&map_path).unwrap();
    assert!(bytes.len() < legacy.len());
    let flat: Value = serde_json::from_slice(&bytes).unwrap();
    assert!(flat.get("previous").is_none());
    assert_eq!(flat["gitToSnapshot"], current["gitToSnapshot"]);

    // Ignoring archived values still rejects malformed JSON rather than losing the map.
    let malformed = &legacy[..legacy.len() - 1];
    std::fs::write(&map_path, malformed).unwrap();
    Command::cargo_bin("sorrel")
        .unwrap()
        .current_dir(root)
        .args(args)
        .assert()
        .failure();
    assert_eq!(std::fs::read_to_string(&map_path).unwrap(), malformed);
}

#[test]
fn stack_create_list_show() {
    let temp = TempDir::new().expect("temp");
    let root = temp.path();
    command_json(root, &["init", "--json"]);
    std::fs::write(root.join("x.txt"), b"x\n").unwrap();
    command_json(root, &["change", "create", "-m", "add x", "--json"]);

    let created = command_json(
        root,
        &["stack", "create", "--name", "stack/feature", "--json"],
    );
    assert_eq!(created["command"], "stack create");
    assert_eq!(created["status"], "created");
    let id = created["object"]["id"].as_str().expect("id").to_owned();

    let listed = command_json(root, &["stack", "list", "--json"]);
    assert_eq!(listed["count"], 1);

    let shown = command_json(root, &["stack", "show", &id, "--json"]);
    assert_eq!(shown["object"]["name"], "stack/feature");
    assert_eq!(shown["object"]["id"], id);
}
