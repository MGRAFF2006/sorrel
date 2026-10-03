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
fn pull_downloads_snapshot_without_advancing_head() {
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
    let before = repo::load_head().unwrap().unwrap();
    let pull_result =
        sync::pull(&pull_store, &remote, "origin", "HEAD", None).expect("pull succeeds");

    assert_eq!(pull_result.snapshot, head.snapshot);
    assert!(pull_result.downloaded > 0);

    let pulled_head = repo::load_head().expect("head").expect("head exists");
    assert_eq!(pulled_head, before);
    sorrel_core::read_snapshot_files(&pull_store, &snapshot_id).expect("complete downloaded tree");
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

    let read_head = |path: &Path| std::fs::read(path.join(".sorrel/HEAD")).unwrap();
    let before = read_head(pull_path);

    // A cached status result must not hide local edits from pull's safety check.
    AssertCommand::cargo_bin("sorrel")
        .unwrap()
        .current_dir(pull_path)
        .arg("status")
        .assert()
        .success();
    std::fs::write(pull_path.join("hello.txt"), b"local work\n").unwrap();
    let output = AssertCommand::cargo_bin("sorrel")
        .unwrap()
        .current_dir(pull_path)
        .arg("pull")
        .assert()
        .failure()
        .get_output()
        .clone();
    assert!(String::from_utf8_lossy(&output.stderr).contains("uncommitted changes"));
    assert_eq!(read_head(pull_path), before);
    assert_eq!(
        std::fs::read(pull_path.join("hello.txt")).unwrap(),
        b"local work\n"
    );
    std::fs::write(pull_path.join("hello.txt"), b"from-push\n").unwrap();

    std::fs::write(pull_path.join(".sorrel/MERGE_STATE"), b"in progress").unwrap();
    let output = AssertCommand::cargo_bin("sorrel")
        .unwrap()
        .current_dir(pull_path)
        .arg("pull")
        .assert()
        .failure()
        .get_output()
        .clone();
    assert!(String::from_utf8_lossy(&output.stderr).contains("merge in progress"));
    assert_eq!(read_head(pull_path), before);
    std::fs::remove_file(pull_path.join(".sorrel/MERGE_STATE")).unwrap();

    // An ordinary descendant must still fast-forward after the safety checks.
    std::fs::write(push_path.join("hello.txt"), b"incoming change\n").unwrap();
    for args in [vec!["change", "create", "-m", "incoming"], vec!["push"]] {
        AssertCommand::cargo_bin("sorrel")
            .unwrap()
            .current_dir(push_path)
            .args(args)
            .assert()
            .success();
    }
    AssertCommand::cargo_bin("sorrel")
        .unwrap()
        .current_dir(pull_path)
        .arg("pull")
        .assert()
        .success();
    assert_eq!(read_head(pull_path), read_head(push_path));
    assert_eq!(
        std::fs::read(pull_path.join("hello.txt")).unwrap(),
        b"incoming change\n"
    );

    // Reserved paths must fail before remote publication or local checkout.
    let safe_head = read_head(pull_path);
    let parent = parse_object_id_hex(
        serde_json::from_slice::<Value>(&safe_head).unwrap()["snapshot"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    let store = FileObjectStore::new(push_path.join(".sorrel")).unwrap();
    let remote = repo::Remote {
        url: hub.url().to_owned(),
        repo_id: repo_id.clone(),
    };
    for (index, metadata) in [
        ".sorrel/HEAD",
        ".git/config",
        ".GIT/config",
        ".sorrel./HEAD",
        ".sorrel/HEAD/x/",
    ]
    .iter()
    .enumerate()
    {
        let malicious = TempDir::new().unwrap();
        let path = malicious.path().join(metadata);
        if metadata.ends_with('/') {
            std::fs::create_dir_all(path).unwrap();
        } else {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, b"untrusted metadata").unwrap();
        }
        let mut options = SnapshotOptions::new(repo_id.clone());
        options.parents = vec![sorrel_core::ObjectRef::new(
            sorrel_core::ObjectKind::Snapshot,
            parent,
        )];
        let snapshot = materialize_snapshot_excluding(
            &store,
            malicious.path(),
            std::iter::empty::<&str>(),
            options,
        )
        .unwrap();
        let remote_ref = format!("unsafe_{index}");
        let error = sync::push(&store, &remote, "origin", &remote_ref, &snapshot.id, None)
            .expect_err("reserved paths must not leave the source workspace");
        let message = error.to_string();
        let refs = SyncClient::new(&remote).list_refs().unwrap();
        assert!(!refs.to_string().contains(&remote_ref));
        let reserved_root = metadata.split('/').next().unwrap();
        assert!(
            message.contains("invalid snapshot path") && message.contains(reserved_root),
            "unsafe metadata path {metadata} must be rejected with its path: {message}"
        );
        assert_eq!(read_head(pull_path), safe_head);
        assert_eq!(
            std::fs::read(pull_path.join("hello.txt")).unwrap(),
            b"incoming change\n"
        );
    }

    // A clean worktree does not authorize replacing divergent recorded history.
    std::fs::write(pull_path.join("hello.txt"), b"recorded locally\n").unwrap();
    AssertCommand::cargo_bin("sorrel")
        .unwrap()
        .current_dir(pull_path)
        .args(["change", "create", "-m", "local history"])
        .assert()
        .success();
    let local_head = read_head(pull_path);
    std::fs::write(push_path.join("hello.txt"), b"recorded remotely\n").unwrap();
    for args in [
        vec!["change", "create", "-m", "remote history"],
        vec!["push"],
    ] {
        AssertCommand::cargo_bin("sorrel")
            .unwrap()
            .current_dir(push_path)
            .args(args)
            .assert()
            .success();
    }
    let output = AssertCommand::cargo_bin("sorrel")
        .unwrap()
        .current_dir(pull_path)
        .arg("pull")
        .assert()
        .failure()
        .get_output()
        .clone();
    assert!(String::from_utf8_lossy(&output.stderr).contains("only fast-forward"));
    assert_eq!(read_head(pull_path), local_head);
    assert_eq!(
        std::fs::read(pull_path.join("hello.txt")).unwrap(),
        b"recorded locally\n"
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
