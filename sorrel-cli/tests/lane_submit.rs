//! Integration test: `sorrel lane submit` against a live Hub (no mocks).

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
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
    fn start(authenticated: bool) -> Self {
        let lock = HUB_LOCK.lock().expect("hub lock");
        let hub_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../sorrel-hub");
        let listen = hub_dir.join("scripts/listen.mjs");
        assert!(listen.is_file(), "missing {}", listen.display());

        let mut command = Command::new("node");
        if authenticated {
            command.args(["--input-type=module", "-e", r#"
                import http from 'node:http';
                import { createApp } from './src/app.js';
                import { resolveTrustedGrants } from './src/bootstrap-grants.js';
                const authAdapter = {
                    mode: 'oidc',
                    async resolveSession(request) {
                        if (request.headers.authorization !== 'Bearer fixture-access-token') return null;
                        return { principal: { type: 'user', id: 'local' }, sessionId: 'fixture', authMode: 'oidc' };
                    },
                };
                const app = createApp({ authAdapter, trustedGrantsById: resolveTrustedGrants() });
                const server = http.createServer((request, response) => {
                    if (request.headers.authorization !== 'Bearer fixture-access-token') {
                        response.writeHead(401, { 'content-type': 'application/json' });
                        response.end('{"error":{"message":"Bearer authentication required"}}');
                        return;
                    }
                    return app.handleRequest(request, response);
                });
                server.listen(0, '127.0.0.1', () => {
                    console.log(JSON.stringify({ url: `http://127.0.0.1:${server.address().port}` }));
                });
            "#]);
        } else {
            command.arg(&listen);
        }
        let mut child = HubChild(
            command
                .current_dir(&hub_dir)
                .env("SORREL_HUB_SYNC_STORE", "memory")
                .env("SORREL_HUB_BOOTSTRAP_GRANTS", "1")
                .env("SORREL_HUB_LOCAL_DEMO", "1")
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
    run_lane_submit(false);
}

#[test]
fn lane_submit_authenticates_discovery_creation_sync_and_collaboration() {
    run_lane_submit(true);
}

fn sorrel(path: &Path, authenticated: bool) -> AssertCommand {
    let mut command = AssertCommand::cargo_bin("sorrel").unwrap();
    command.current_dir(path).env_remove("SORREL_HUB_TOKEN");
    if authenticated {
        command.env("SORREL_HUB_TOKEN", "  fixture-access-token  ");
    }
    command
}

fn run_lane_submit(authenticated: bool) {
    let hub = LiveHub::start(authenticated);
    let dir = TempDir::new().unwrap();
    let path = dir.path();

    sorrel(path, authenticated).arg("init").assert().success();

    std::fs::write(path.join("README.md"), b"main baseline\n").unwrap();
    sorrel(path, authenticated)
        .args(["change", "create", "-m", "main baseline"])
        .assert()
        .success();
    let feature_lane: Value = serde_json::from_slice(
        &sorrel(path, authenticated)
            .args(["lane", "create", "--name", "feature", "--json"])
            .assert()
            .success()
            .get_output()
            .stdout,
    )
    .unwrap();
    let feature_lane_id = feature_lane["object"]["id"].as_str().unwrap();
    sorrel(path, authenticated)
        .args(["lane", "switch", feature_lane_id])
        .assert()
        .success();

    std::fs::write(path.join("feature.txt"), b"submit-me\n").unwrap();
    sorrel(path, authenticated)
        .args(["change", "create", "-m", "add feature"])
        .assert()
        .success();

    // Main advances after the feature fork, so its tip is outside the source closure.
    sorrel(path, authenticated)
        .args(["lane", "switch", "lane_main"])
        .assert()
        .success();
    std::fs::write(path.join("README.md"), b"main advanced\n").unwrap();
    sorrel(path, authenticated)
        .args(["change", "create", "-m", "advance main after fork"])
        .assert()
        .success();
    sorrel(path, authenticated)
        .args(["lane", "switch", feature_lane_id])
        .assert()
        .success();

    let status: Value = serde_json::from_slice(
        &sorrel(path, authenticated)
            .args(["status", "--json"])
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap();
    let repo_id = status["repoId"].as_str().unwrap();

    sorrel(path, authenticated)
        .args(["remote", "add", "origin", hub.url(), "--repo-id", repo_id])
        .assert()
        .success();

    let submit: Value = serde_json::from_slice(
        &sorrel(path, authenticated)
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
    assert!(submit["proposal"]["targetSnapshot"].is_string());
    assert_ne!(
        submit["proposal"]["targetSnapshot"],
        submit["proposal"]["sourceSnapshot"]
    );
    let proposal_id = submit["proposal"]["id"].as_str().unwrap();
    let hub_get = |path: &str| {
        let request = ureq::get(&format!("{}{path}", hub.url()));
        if authenticated {
            request.set("Authorization", "Bearer fixture-access-token")
        } else {
            request
        }
    };
    let comparison: Value = hub_get(&format!("/admin/proposals/{proposal_id}/changes"))
        .call()
        .unwrap()
        .into_json()
        .unwrap();
    let changes = comparison["data"]["changes"].as_array().unwrap();
    let feature = changes
        .iter()
        .find(|change| change["path"] == "feature.txt")
        .unwrap();
    assert_eq!(feature["status"], "added");
    let readme = changes
        .iter()
        .find(|change| change["path"] == "README.md")
        .unwrap();
    assert_eq!(readme["before"]["content"], "main advanced\n");
    assert_eq!(readme["after"]["content"], "main baseline\n");
    let refs: Value = hub_get(&format!("/{repo_id}/refs"))
        .call()
        .unwrap()
        .into_json()
        .unwrap();
    let refs = refs["refs"].as_array().unwrap();
    assert_eq!(
        refs.len(),
        1,
        "uploading the target must not publish another ref"
    );
    assert_eq!(refs[0]["name"], "HEAD");
    assert_eq!(refs[0]["snapshot"], submit["proposal"]["sourceSnapshot"]);

    let again: Value = serde_json::from_slice(
        &sorrel(path, authenticated)
            .args(["lane", "submit", "--json", "--no-push"])
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap();
    assert_eq!(again["status"], "reused");
    assert_eq!(again["proposal"]["id"], submit["proposal"]["id"]);
}
