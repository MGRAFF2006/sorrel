//! Integration tests for sync transport against a **real** sorrel-hub process.
//!
//! No mock HTTP servers: these tests spawn `sorrel-hub/scripts/listen.mjs` and
//! exercise the live sync protocol (bootstrap grants for `user:local`).

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde_json::Value;
use sorrel_cli::repo;
use sorrel_cli::sync::{self, SyncClient};
use sorrel_core::{
    materialize_snapshot_excluding, parse_object_id_hex, write_snapshot, write_tree,
    FileObjectStore, ObjectId, SnapshotOptions,
};
use tempfile::TempDir;

/// Guard so only one Hub child is active at a time (port + process hygiene).
static HUB_LOCK: Mutex<()> = Mutex::new(());

struct HubChild(Child);

impl Drop for HubChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

struct LiveHub {
    url: String,
    _child: HubChild,
    _lock: std::sync::MutexGuard<'static, ()>,
}

impl LiveHub {
    fn start() -> Self {
        let lock = HUB_LOCK.lock().expect("hub lock");
        let hub_dir = hub_repo_dir();
        let listen = hub_dir.join("scripts/listen.mjs");
        assert!(
            listen.is_file(),
            "expected real hub listen script at {} (set SORREL_HUB_DIR)",
            listen.display()
        );

        let mut child = HubChild(
            Command::new("node")
                .arg(&listen)
                .current_dir(&hub_dir)
                .env("SORREL_HUB_SYNC_STORE", "memory")
                .env("SORREL_HUB_BOOTSTRAP_GRANTS", "1")
                .stdout(Stdio::piped())
                .stderr(Stdio::inherit())
                .spawn()
                .expect("spawn sorrel-hub listen.mjs"),
        );

        let stdout = child.0.stdout.take().expect("hub stdout");
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            assert!(
                Instant::now() < deadline,
                "timed out waiting for hub ready line"
            );
            line.clear();
            let bytes = reader.read_line(&mut line).expect("read hub ready line");
            if bytes == 0 {
                let status = child.0.wait().expect("hub exit");
                panic!("hub exited before ready: {status}");
            }
            if let Ok(value) = serde_json::from_str::<Value>(line.trim()) {
                if let Some(url) = value.get("url").and_then(Value::as_str) {
                    return Self {
                        url: url.to_owned(),
                        _child: child,
                        _lock: lock,
                    };
                }
            }
        }
    }

    fn url(&self) -> &str {
        &self.url
    }
}

fn hub_repo_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("SORREL_HUB_DIR") {
        return PathBuf::from(dir);
    }
    // Monorepo sibling layout: sorrel-cli/../sorrel-hub
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../sorrel-hub")
}

#[test]
fn push_then_pull_round_trip_preserves_snapshot_id() {
    let hub = LiveHub::start();

    let local = TempDir::new().expect("tempdir");
    std::env::set_current_dir(local.path()).expect("chdir");
    init_local_repo("repo_roundtrip");

    let remote = repo::Remote {
        url: hub.url().to_owned(),
        repo_id: "repo_roundtrip".to_owned(),
    };
    repo::add_remote("origin", &remote.url, &remote.repo_id).expect("add remote");

    let store = FileObjectStore::new(repo::object_store_root()).expect("store");
    let head = repo::load_head().expect("head").expect("head exists");
    let snapshot_id = parse_object_id_hex(&head.snapshot).expect("valid snapshot id in HEAD");

    let push_result =
        sync::push(&store, &remote, "origin", "HEAD", &snapshot_id, None).expect("push succeeds");
    assert_eq!(push_result.snapshot, head.snapshot);
    assert!(push_result.uploaded > 0);

    let pull_dir = TempDir::new().expect("pull tempdir");
    std::env::set_current_dir(pull_dir.path()).expect("chdir pull");
    init_empty_local_repo("repo_roundtrip");
    repo::add_remote("origin", &remote.url, &remote.repo_id).expect("add remote pull");

    let pull_store = FileObjectStore::new(repo::object_store_root()).expect("pull store");
    let before_pull = repo::load_head().expect("head").expect("head exists");
    let pull_result =
        sync::pull(&pull_store, &remote, "origin", "HEAD", None).expect("pull succeeds");

    assert_eq!(pull_result.snapshot, head.snapshot);
    assert!(pull_result.downloaded > 0);

    let pulled_head = repo::load_head().expect("head").expect("head exists");
    assert_eq!(
        pulled_head, before_pull,
        "transport fetch must not publish HEAD before checkout"
    );
}

#[test]
fn sync_client_list_refs_and_missing_against_live_hub() {
    let hub = LiveHub::start();

    let remote = repo::Remote {
        url: hub.url().to_owned(),
        repo_id: "repo_test".to_owned(),
    };
    let client = SyncClient::new(&remote);
    let refs = client.list_refs().expect("list refs");
    assert!(refs.get("refs").and_then(Value::as_array).is_some());

    let want = ObjectId::for_bytes(b"want-root");
    let missing = client.post_missing(&want, &[]).expect("post missing");
    assert!(missing.get("missing").and_then(Value::as_array).is_some());
}

#[test]
fn cli_push_pull_restores_working_tree_via_live_hub() {
    use assert_cmd::Command as AssertCommand;

    let hub = LiveHub::start();

    let push_dir = TempDir::new().expect("push dir");
    let push_path = push_dir.path();
    AssertCommand::cargo_bin("sorrel")
        .unwrap()
        .current_dir(push_path)
        .arg("init")
        .assert()
        .success();

    std::fs::write(push_path.join("hello.txt"), b"from-push\n").expect("write");
    AssertCommand::cargo_bin("sorrel")
        .unwrap()
        .current_dir(push_path)
        .args(["change", "create", "-m", "add hello"])
        .assert()
        .success();

    let manifest: Value = serde_json::from_str(
        &std::fs::read_to_string(push_path.join(".sorrel/manifest.json")).unwrap(),
    )
    .unwrap();
    let repo_id = manifest["repoId"].as_str().expect("repoId").to_owned();

    AssertCommand::cargo_bin("sorrel")
        .unwrap()
        .current_dir(push_path)
        .args(["remote", "add", "origin", hub.url(), "--repo-id", &repo_id])
        .assert()
        .success();

    AssertCommand::cargo_bin("sorrel")
        .unwrap()
        .current_dir(push_path)
        .args(["push", "origin"])
        .assert()
        .success();

    let pull_dir = TempDir::new().expect("pull dir");
    let pull_path = pull_dir.path();
    AssertCommand::cargo_bin("sorrel")
        .unwrap()
        .current_dir(pull_path)
        .arg("init")
        .assert()
        .success();
    AssertCommand::cargo_bin("sorrel")
        .unwrap()
        .current_dir(pull_path)
        .args(["remote", "add", "origin", hub.url(), "--repo-id", &repo_id])
        .assert()
        .success();
    AssertCommand::cargo_bin("sorrel")
        .unwrap()
        .current_dir(pull_path)
        .args(["pull", "origin"])
        .assert()
        .success();

    let content = std::fs::read_to_string(pull_path.join("hello.txt")).expect("pulled file");
    assert_eq!(content, "from-push\n");
}

fn cli(workspace: &Path, args: &[&str]) -> std::process::Output {
    assert_cmd::Command::cargo_bin("sorrel")
        .unwrap()
        .current_dir(workspace)
        .args(args)
        .output()
        .unwrap()
}

fn cli_ok(workspace: &Path, args: &[&str]) {
    let output = cli(workspace, args);
    assert!(
        output.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn head_bytes(workspace: &Path) -> (Vec<u8>, Vec<u8>) {
    let head = std::fs::read(workspace.join(".sorrel/HEAD")).unwrap();
    let value: Value = serde_json::from_slice(&head).unwrap();
    let lane = std::fs::read(
        workspace
            .join(".sorrel/heads")
            .join(value["lane"].as_str().unwrap()),
    )
    .unwrap();
    (head, lane)
}

fn connected_workspace(hub: &LiveHub, repo_id: &str) -> TempDir {
    let workspace = TempDir::new().unwrap();
    cli_ok(workspace.path(), &["init"]);
    cli_ok(
        workspace.path(),
        &["remote", "add", "origin", hub.url(), "--repo-id", repo_id],
    );
    workspace
}

#[test]
fn cli_pull_rejects_dirty_work_and_preserves_both_heads() {
    let hub = LiveHub::start();
    let source = connected_workspace(&hub, "repo_dirty_pull");
    std::fs::write(source.path().join("tracked.txt"), b"base\n").unwrap();
    cli_ok(source.path(), &["change", "create", "-m", "base"]);
    cli_ok(source.path(), &["push"]);
    let local = connected_workspace(&hub, "repo_dirty_pull");
    cli_ok(local.path(), &["pull"]);
    std::fs::write(source.path().join("tracked.txt"), b"remote\n").unwrap();
    cli_ok(source.path(), &["change", "create", "-m", "remote"]);
    cli_ok(source.path(), &["push"]);

    for edited in [true, false] {
        if edited {
            std::fs::write(local.path().join("tracked.txt"), b"uncommitted\n").unwrap();
        } else {
            std::fs::remove_file(local.path().join("tracked.txt")).unwrap();
        }
        let before = head_bytes(local.path());
        let output = cli(local.path(), &["pull"]);
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("uncommitted"));
        assert_eq!(head_bytes(local.path()), before);
        if edited {
            assert_eq!(
                std::fs::read(local.path().join("tracked.txt")).unwrap(),
                b"uncommitted\n"
            );
        } else {
            assert!(!local.path().join("tracked.txt").exists());
        }
    }
    std::fs::write(local.path().join("tracked.txt"), b"base\n").unwrap();
    cli_ok(local.path(), &["pull"]);
    assert_eq!(
        std::fs::read(local.path().join("tracked.txt")).unwrap(),
        b"remote\n"
    );
    assert_eq!(head_bytes(local.path()), head_bytes(source.path()));

    std::fs::write(local.path().join(".sorrel/MERGE_STATE"), b"{}").unwrap();
    let before = head_bytes(local.path());
    let output = cli(local.path(), &["pull"]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("merge is in progress"));
    assert_eq!(head_bytes(local.path()), before);
}

#[test]
fn cli_pull_rejects_divergence_rewinds_and_unrelated_history() {
    let hub = LiveHub::start();
    for case in ["diverged", "rewound", "unrelated"] {
        let source = connected_workspace(&hub, &format!("repo_pull_{case}"));
        std::fs::write(source.path().join("tracked.txt"), b"base\n").unwrap();
        cli_ok(source.path(), &["change", "create", "-m", "base"]);
        cli_ok(source.path(), &["push"]);
        let local = connected_workspace(&hub, &format!("repo_pull_{case}"));
        if case != "unrelated" {
            cli_ok(local.path(), &["pull"]);
        }
        std::fs::write(local.path().join("tracked.txt"), b"local\n").unwrap();
        cli_ok(local.path(), &["change", "create", "-m", "local"]);
        if case == "diverged" {
            std::fs::write(source.path().join("tracked.txt"), b"remote\n").unwrap();
            cli_ok(source.path(), &["change", "create", "-m", "remote"]);
            cli_ok(source.path(), &["push"]);
        }
        let before = head_bytes(local.path());
        let output = cli(local.path(), &["pull"]);
        assert!(!output.status.success(), "{case}");
        assert!(String::from_utf8_lossy(&output.stderr).contains("not a fast-forward"));
        assert_eq!(head_bytes(local.path()), before);
        assert_eq!(
            std::fs::read(local.path().join("tracked.txt")).unwrap(),
            b"local\n"
        );
    }
}

#[test]
fn cli_pull_rejects_ignored_path_collisions_before_restoring() {
    let hub = LiveHub::start();
    for directory in [false, true] {
        let repo_id = format!("repo_obstructed_pull_{directory}");
        let source = connected_workspace(&hub, &repo_id);
        std::fs::write(source.path().join(".gitignore"), "blocked\n").unwrap();
        cli_ok(
            source.path(),
            &["change", "create", "-m", "ignore obstruction"],
        );
        cli_ok(source.path(), &["push"]);
        let local = connected_workspace(&hub, &repo_id);
        cli_ok(local.path(), &["pull"]);
        let preserved = if directory {
            std::fs::create_dir(local.path().join("blocked")).unwrap();
            local.path().join("blocked/local-only")
        } else {
            local.path().join("blocked")
        };
        std::fs::write(&preserved, b"preserve\n").unwrap();
        std::fs::write(source.path().join(".gitignore"), "").unwrap();
        std::fs::write(source.path().join("blocked"), b"remote\n").unwrap();
        std::fs::write(source.path().join("aaa.txt"), b"earlier checkout path\n").unwrap();
        cli_ok(source.path(), &["change", "create", "-m", "add files"]);
        cli_ok(source.path(), &["push"]);
        let before = head_bytes(local.path());
        let output = cli(local.path(), &["pull"]);
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("untracked path"));
        assert_eq!(head_bytes(local.path()), before);
        assert_eq!(std::fs::read(preserved).unwrap(), b"preserve\n");
        assert_eq!(
            std::fs::read_to_string(local.path().join(".gitignore")).unwrap(),
            "blocked\n"
        );
        assert!(!local.path().join("aaa.txt").exists());
    }
}

#[cfg(unix)]
#[test]
fn cli_pull_restore_io_failure_preserves_both_heads() {
    use std::os::unix::fs::PermissionsExt;

    let hub = LiveHub::start();
    let source = connected_workspace(&hub, "repo_pull_io_failure");
    std::fs::write(source.path().join("tracked.txt"), b"base\n").unwrap();
    cli_ok(source.path(), &["change", "create", "-m", "base"]);
    cli_ok(source.path(), &["push"]);
    let local = connected_workspace(&hub, "repo_pull_io_failure");
    cli_ok(local.path(), &["pull"]);
    std::fs::write(source.path().join("tracked.txt"), b"remote\n").unwrap();
    cli_ok(source.path(), &["change", "create", "-m", "remote"]);
    cli_ok(source.path(), &["push"]);

    let path = local.path().join("tracked.txt");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o444)).unwrap();
    // Privileged test processes can bypass filesystem permissions.
    if std::fs::OpenOptions::new().write(true).open(&path).is_ok() {
        return;
    }
    let before = head_bytes(local.path());
    let output = cli(local.path(), &["pull"]);
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("Permission denied"));
    assert_eq!(head_bytes(local.path()), before);
    assert_eq!(std::fs::read(path).unwrap(), b"base\n");
}

#[test]
fn cli_pull_allows_clean_file_directory_transitions() {
    let hub = LiveHub::start();
    let source = connected_workspace(&hub, "repo_pull_transitions");
    std::fs::write(source.path().join("a"), b"file\n").unwrap();
    cli_ok(source.path(), &["change", "create", "-m", "file"]);
    cli_ok(source.path(), &["push"]);
    let local = connected_workspace(&hub, "repo_pull_transitions");
    cli_ok(local.path(), &["pull"]);

    std::fs::remove_file(source.path().join("a")).unwrap();
    std::fs::create_dir(source.path().join("a")).unwrap();
    std::fs::write(source.path().join("a/child"), b"nested\n").unwrap();
    cli_ok(source.path(), &["change", "create", "-m", "directory"]);
    cli_ok(source.path(), &["push"]);
    cli_ok(local.path(), &["pull"]);
    assert_eq!(
        std::fs::read(local.path().join("a/child")).unwrap(),
        b"nested\n"
    );
    assert_eq!(head_bytes(local.path()), head_bytes(source.path()));

    std::fs::remove_file(source.path().join("a/child")).unwrap();
    std::fs::remove_dir(source.path().join("a")).unwrap();
    std::fs::write(source.path().join("a"), b"file again\n").unwrap();
    cli_ok(source.path(), &["change", "create", "-m", "file again"]);
    cli_ok(source.path(), &["push"]);
    cli_ok(local.path(), &["pull"]);
    assert_eq!(
        std::fs::read(local.path().join("a")).unwrap(),
        b"file again\n"
    );
    assert_eq!(head_bytes(local.path()), head_bytes(source.path()));
}

#[test]
fn cli_pull_preserves_ignored_children_on_directory_to_file() {
    let hub = LiveHub::start();
    let source = connected_workspace(&hub, "repo_pull_ignored_child");
    std::fs::create_dir(source.path().join("a")).unwrap();
    std::fs::write(source.path().join("a/tracked"), b"tracked\n").unwrap();
    std::fs::write(source.path().join(".gitignore"), "a/local-only\n").unwrap();
    cli_ok(source.path(), &["change", "create", "-m", "directory"]);
    cli_ok(source.path(), &["push"]);
    let local = connected_workspace(&hub, "repo_pull_ignored_child");
    cli_ok(local.path(), &["pull"]);
    std::fs::write(local.path().join("a/local-only"), b"preserve\n").unwrap();

    std::fs::remove_file(source.path().join("a/tracked")).unwrap();
    std::fs::remove_dir(source.path().join("a")).unwrap();
    std::fs::write(source.path().join("a"), b"file\n").unwrap();
    cli_ok(
        source.path(),
        &["change", "create", "-m", "replace directory"],
    );
    cli_ok(source.path(), &["push"]);
    let before = head_bytes(local.path());
    let output = cli(local.path(), &["pull"]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("untracked path"));
    assert_eq!(head_bytes(local.path()), before);
    assert_eq!(
        std::fs::read(local.path().join("a/tracked")).unwrap(),
        b"tracked\n"
    );
    assert_eq!(
        std::fs::read(local.path().join("a/local-only")).unwrap(),
        b"preserve\n"
    );
}

fn init_empty_local_repo(repo_id: &str) {
    std::fs::create_dir_all(repo::sorrel_dir().join(repo::SLICES_DIR)).expect("slices dir");
    let store = FileObjectStore::new(repo::object_store_root()).expect("store");
    let mut options = SnapshotOptions::new(repo_id.to_owned());
    options.created_at = repo::now_rfc3339();
    options.message = Some("initial snapshot".to_owned());
    let empty_tree = write_tree(&store, Vec::new()).expect("tree");
    let snapshot = write_snapshot(&store, empty_tree.id, options).expect("snapshot");
    let manifest = repo::build_manifest(repo_id, &repo::now_rfc3339());
    repo::write_manifest(&manifest).expect("manifest");
    repo::write_head(&repo::Head {
        lane: repo::DEFAULT_LANE_ID.to_owned(),
        snapshot: snapshot.id.to_hex(),
    })
    .expect("head");
}

fn init_local_repo(repo_id: &str) {
    init_empty_local_repo(repo_id);

    std::fs::write("tracked.txt", b"sync-me\n").expect("write file");
    let store = FileObjectStore::new(repo::object_store_root()).expect("store");
    let head = repo::load_head().expect("head").expect("head");
    let parent = parse_object_id_hex(&head.snapshot).expect("parent");
    let mut options = SnapshotOptions::new(repo_id.to_owned());
    options.created_at = repo::now_rfc3339();
    options.message = Some("add tracked.txt".to_owned());
    options.parents = vec![sorrel_core::ObjectRef::new(
        sorrel_core::ObjectKind::Snapshot,
        parent,
    )];
    let snap = materialize_snapshot_excluding(&store, Path::new("."), [repo::SORREL_DIR], options)
        .expect("materialize");
    repo::write_head(&repo::Head {
        lane: repo::DEFAULT_LANE_ID.to_owned(),
        snapshot: snap.id.to_hex(),
    })
    .expect("advance head");
}
