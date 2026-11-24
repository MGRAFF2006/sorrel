//! Integration test: `sorrel login`/`logout`/`auth status` against a real Hub
//! whose browser sign-in is stubbed (`tests/e2e/cli-login-hub.mjs`).

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use assert_cmd::Command as AssertCommand;
use serde_json::Value;
use tempfile::TempDir;

const REPO_ID: &str = "repo_cli_login_rust";

struct HubChild(Child);

impl Drop for HubChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

struct LoginHub {
    url: String,
    cookie: String,
    _child: HubChild,
}

impl LoginHub {
    fn start() -> Self {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
        let mut child = HubChild(
            Command::new("node")
                .arg(root.join("tests/e2e/cli-login-hub.mjs"))
                .args(["--repo", REPO_ID])
                .current_dir(&root)
                .stdout(Stdio::piped())
                .stderr(Stdio::inherit())
                .spawn()
                .expect("spawn hub"),
        );
        let mut reader = BufReader::new(child.0.stdout.take().expect("stdout"));
        let mut line = String::new();
        reader.read_line(&mut line).expect("hub ready line");
        let ready: Value = serde_json::from_str(line.trim()).expect("hub ready json");
        // Keep draining audit output so the Hub never blocks on a full pipe.
        std::thread::spawn(move || std::io::copy(&mut reader, &mut std::io::sink()));
        Self {
            url: ready["url"].as_str().unwrap().to_owned(),
            cookie: ready["cookie"].as_str().unwrap().to_owned(),
            _child: child,
        }
    }

    fn approve(&self, user_code: &str) {
        let response = ureq::post(&format!("{}/auth/cli/approve", self.url))
            .set("cookie", &self.cookie)
            .set("origin", &self.url)
            .send_json(serde_json::json!({ "user_code": user_code, "decision": "approve" }))
            .expect("approve");
        assert_eq!(response.status(), 200);
    }

    fn session_for(&self, token: &str) -> Value {
        let response: Value = ureq::get(&format!("{}/session", self.url))
            .set("Authorization", &format!("Bearer {token}"))
            .call()
            .expect("session")
            .into_json()
            .expect("session json");
        response["data"]["session"].clone()
    }
}

fn sorrel(cwd: &Path, config: &Path) -> AssertCommand {
    let mut command = AssertCommand::cargo_bin("sorrel").unwrap();
    command
        .current_dir(cwd)
        .env("SORREL_CONFIG_DIR", config)
        .env_remove("SORREL_HUB_TOKEN");
    command
}

fn json_of(output: &std::process::Output) -> Value {
    serde_json::from_slice(&output.stdout).expect("json stdout")
}

/// Run `sorrel login`, approving the printed code in the stubbed browser.
fn login(hub: &LoginHub, cwd: &Path, config: &Path) -> (Value, String) {
    let mut child = Command::new(assert_cmd::cargo::cargo_bin("sorrel"))
        .args(["login", &hub.url, "--label", "rust test", "--json"])
        .current_dir(cwd)
        .env("SORREL_CONFIG_DIR", config)
        .env_remove("SORREL_HUB_TOKEN")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn login");
    let mut stderr = BufReader::new(child.stderr.take().unwrap());
    let mut transcript = String::new();
    let deadline = Instant::now() + Duration::from_secs(15);
    let code = loop {
        assert!(Instant::now() < deadline, "no login code printed");
        let mut line = String::new();
        assert!(
            stderr.read_line(&mut line).unwrap() > 0,
            "login exited: {transcript}"
        );
        transcript.push_str(&line);
        if let Some(code) = line.trim().strip_prefix("and confirm the code ") {
            break code.to_owned();
        }
    };
    hub.approve(&code);
    let mut rest = String::new();
    stderr.read_to_string(&mut rest).unwrap();
    transcript.push_str(&rest);
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success(), "login failed: {transcript}");
    (json_of(&output), transcript)
}

fn stored_token(config: &Path, hub: &str) -> String {
    let file: Value =
        serde_json::from_slice(&std::fs::read(config.join("credentials.json")).unwrap()).unwrap();
    file["hubs"][hub]["token"].as_str().unwrap().to_owned()
}

/// A plain HTTP listener that records request heads and answers 401.
fn recording_server() -> (String, mpsc::Receiver<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let mut stream = stream;
            let mut head = Vec::new();
            let mut byte = [0_u8; 1];
            while !head.ends_with(b"\r\n\r\n") && stream.read(&mut byte).unwrap_or(0) == 1 {
                head.push(byte[0]);
            }
            let _ = sender.send(String::from_utf8_lossy(&head).to_lowercase());
            let body = br#"{"error":{"code":"authentication_required","message":"no"}}"#;
            let _ = write!(
                stream,
                "HTTP/1.1 401 Unauthorized\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
                body.len()
            );
            let _ = stream.write_all(body);
        }
    });
    (url, receiver)
}

#[test]
fn login_stores_a_private_hub_scoped_token_used_only_for_that_hub() {
    let hub = LoginHub::start();
    let temp = TempDir::new().unwrap();
    let config = temp.path().join("config");
    let work = temp.path().join("work");
    std::fs::create_dir(&work).unwrap();

    let (login_json, transcript) = login(&hub, &work, &config);
    let token = stored_token(&config, &hub.url);
    assert!(token.starts_with("srl_cli_"));
    assert_eq!(login_json["hub"], hub.url);
    assert_eq!(login_json["label"], "rust test");
    assert_eq!(login_json["principal"]["id"], "workos:user_cli_login");
    assert!(!login_json.to_string().contains(&token));
    assert!(!transcript.contains(&token), "login never prints the token");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = |path: &Path| std::fs::metadata(path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&config), 0o700);
        assert_eq!(mode(&config.join("credentials.json")), 0o600);
    }

    // auth status shows the Hub, principal, label and expiry, never the token.
    let status = sorrel(&work, &config)
        .args(["auth", "status", "--json"])
        .output()
        .unwrap();
    assert!(status.status.success());
    let status_json = json_of(&status);
    assert_eq!(status_json["hubs"][0]["hub"], hub.url);
    assert_eq!(status_json["hubs"][0]["label"], "rust test");
    assert!(status_json["hubs"][0]["expiresAt"].is_string());
    assert_eq!(status_json["envTokenOverride"], false);
    assert!(!String::from_utf8_lossy(&status.stdout).contains(&token));

    // The stored token authenticates push for exactly this Hub URL…
    sorrel(&work, &config).arg("init").assert().success();
    std::fs::write(work.join("a.txt"), "one\n").unwrap();
    sorrel(&work, &config)
        .args(["change", "create", "-m", "one"])
        .assert()
        .success();
    // A trailing slash normalizes to the same Hub.
    let origin = format!("{}/", hub.url);
    sorrel(&work, &config)
        .args(["remote", "add", "origin", &origin, "--repo-id", REPO_ID])
        .assert()
        .success();
    let push = sorrel(&work, &config)
        .args(["push", "origin", "--json"])
        .output()
        .unwrap();
    assert!(
        push.status.success(),
        "{}",
        String::from_utf8_lossy(&push.stderr)
    );
    assert_eq!(json_of(&push)["status"], "pushed");

    // …and is never sent to another remote.
    let (other_url, requests) = recording_server();
    sorrel(&work, &config)
        .args(["remote", "add", "other", &other_url, "--repo-id", REPO_ID])
        .assert()
        .success();
    let other = sorrel(&work, &config)
        .args(["push", "other"])
        .output()
        .unwrap();
    assert!(!other.status.success());
    assert!(String::from_utf8_lossy(&other.stderr).contains("run `sorrel login"));
    let head = requests
        .recv_timeout(Duration::from_secs(5))
        .expect("request to other remote");
    assert!(!head.contains("authorization:"), "{head}");

    // SORREL_HUB_TOKEN overrides the stored credential.
    let overridden = sorrel(&work, &config)
        .env("SORREL_HUB_TOKEN", "not-a-real-token")
        .args(["push", "origin"])
        .output()
        .unwrap();
    assert!(
        !overridden.status.success(),
        "the env token must be used instead"
    );

    // A credentials file readable by others is refused, not used.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let file = config.join("credentials.json");
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).unwrap();
        let refused = sorrel(&work, &config)
            .args(["push", "origin"])
            .output()
            .unwrap();
        assert!(!refused.status.success());
        assert!(String::from_utf8_lossy(&refused.stderr).contains("chmod 600"));
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600)).unwrap();
    }

    // Logout revokes the token on the Hub and removes it locally.
    assert!(hub.session_for(&token).is_object());
    let logout = sorrel(&work, &config)
        .args(["logout", "--json"])
        .output()
        .unwrap();
    assert!(
        logout.status.success(),
        "{}",
        String::from_utf8_lossy(&logout.stderr)
    );
    let logout_json = json_of(&logout);
    assert_eq!(logout_json["revoked"], true);
    assert_eq!(logout_json["hub"], hub.url);
    assert!(hub.session_for(&token).is_null(), "revoked on the Hub");
    let after = json_of(
        &sorrel(&work, &config)
            .args(["auth", "status", "--json"])
            .output()
            .unwrap(),
    );
    assert_eq!(after["hubs"], serde_json::json!([]));
}

#[test]
fn login_refuses_plain_http_hubs_and_hubs_without_device_login() {
    let temp = TempDir::new().unwrap();
    let config = temp.path().join("config");
    let insecure = sorrel(temp.path(), &config)
        .args(["login", "http://hub.example.com/api"])
        .output()
        .unwrap();
    assert!(!insecure.status.success());
    assert!(String::from_utf8_lossy(&insecure.stderr).contains("plain HTTP"));

    // A Hub without browser sign-in advertises cliLogin "none".
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    std::thread::spawn(move || {
        if let Some(Ok(mut stream)) = listener.incoming().next() {
            let mut buffer = [0_u8; 1024];
            let _ = stream.read(&mut buffer);
            let body = br#"{"data":{"auth":{"mode":"dev","cliLogin":"none"}}}"#;
            let _ = write!(
                stream,
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
                body.len()
            );
            let _ = stream.write_all(body);
        }
    });
    let unsupported = sorrel(temp.path(), &config)
        .args(["login", &url])
        .output()
        .unwrap();
    assert!(!unsupported.status.success());
    assert!(String::from_utf8_lossy(&unsupported.stderr).contains("does not offer CLI login"));
    assert!(!config.join("credentials.json").exists());
}
