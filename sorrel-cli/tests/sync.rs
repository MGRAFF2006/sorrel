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
    let pull_result =
        sync::pull(&pull_store, &remote, "origin", "HEAD", None).expect("pull succeeds");

    assert_eq!(pull_result.snapshot, head.snapshot);
    assert!(pull_result.downloaded > 0);

    let pulled_head = repo::load_head().expect("head").expect("head exists");
    assert_eq!(pulled_head.snapshot, head.snapshot);
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
