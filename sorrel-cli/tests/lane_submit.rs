//! Integration test: `sorrel lane submit` against a live Hub (no mocks).

use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use assert_cmd::Command as AssertCommand;
use serde_json::Value;
use tempfile::TempDir;

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
        let hub_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../sorrel-hub");
        let listen = hub_dir.join("scripts/listen.mjs");
        assert!(listen.is_file(), "missing {}", listen.display());

        let mut child = HubChild(
            Command::new("node")
                .arg(&listen)
                .current_dir(&hub_dir)
                .env("SORREL_HUB_SYNC_STORE", "memory")
                .env("SORREL_HUB_BOOTSTRAP_GRANTS", "1")
                .stdout(Stdio::piped())
                .stderr(Stdio::inherit())
                .spawn()
                .expect("spawn hub"),
        );

        let stdout = child.0.stdout.take().expect("stdout");
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            assert!(Instant::now() < deadline, "hub ready timeout");
            line.clear();
            let n = reader.read_line(&mut line).expect("read");
            if n == 0 {
                panic!("hub exited early");
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

#[test]
fn lane_submit_creates_hub_proposal_via_live_api() {
    let hub = LiveHub::start();
    let dir = TempDir::new().unwrap();
    let path = dir.path();

    AssertCommand::cargo_bin("sorrel")
        .unwrap()
        .current_dir(path)
        .arg("init")
        .assert()
        .success();

    std::fs::write(path.join("feature.txt"), b"submit-me\n").unwrap();
    AssertCommand::cargo_bin("sorrel")
        .unwrap()
        .current_dir(path)
        .args(["change", "create", "-m", "add feature"])
        .assert()
        .success();

    let status: Value = serde_json::from_slice(
        &AssertCommand::cargo_bin("sorrel")
            .unwrap()
            .current_dir(path)
            .args(["status", "--json"])
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap();
    let repo_id = status["repoId"].as_str().unwrap();

    AssertCommand::cargo_bin("sorrel")
        .unwrap()
        .current_dir(path)
        .args(["remote", "add", "origin", hub.url(), "--repo-id", repo_id])
        .assert()
        .success();

    let submit: Value = serde_json::from_slice(
        &AssertCommand::cargo_bin("sorrel")
            .unwrap()
            .current_dir(path)
            .args(["lane", "submit", "--json"])
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap();

    assert_eq!(submit["command"], "lane submit");
    assert_eq!(submit["status"], "submitted");
    assert_eq!(submit["reused"], false);
    assert!(submit["proposal"]["id"]
        .as_str()
        .unwrap()
        .starts_with("prop_"));
    assert_eq!(submit["proposal"]["status"], "open");
    assert_eq!(submit["proposal"]["syncRepoId"], repo_id);
    assert!(submit["uploaded"].as_u64().unwrap() > 0);

    let again: Value = serde_json::from_slice(
        &AssertCommand::cargo_bin("sorrel")
            .unwrap()
            .current_dir(path)
            .args(["lane", "submit", "--json", "--no-push"])
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap();
    assert_eq!(again["status"], "reused");
    assert_eq!(again["proposal"]["id"], submit["proposal"]["id"]);
}
