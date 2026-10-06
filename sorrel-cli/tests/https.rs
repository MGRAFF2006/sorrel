//! Local TLS regressions; certificate keys are generated only in a temporary directory.

use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};

use assert_cmd::Command as AssertCommand;
use serde_json::Value;
use tempfile::TempDir;

struct TlsHub {
    url: String,
    _child: HubChild,
    directory: TempDir,
}

struct HubChild(Child);

impl Drop for HubChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

impl TlsHub {
    fn start() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let openssl = |args: &[&str]| {
            let output = Command::new("openssl")
                .args(args)
                .current_dir(directory.path())
                .output()
                .expect("OpenSSL is required for local TLS tests");
            assert!(
                output.status.success(),
                "OpenSSL certificate generation failed"
            );
        };
        openssl(&[
            "req",
            "-x509",
            "-newkey",
            "rsa:2048",
            "-nodes",
            "-days",
            "2",
            "-subj",
            "/CN=Sorrel temporary test CA",
            "-addext",
            "basicConstraints=critical,CA:TRUE",
            "-keyout",
            "ca.key",
            "-out",
            "ca.pem",
        ]);
        openssl(&[
            "req",
            "-newkey",
            "rsa:2048",
            "-nodes",
            "-subj",
            "/CN=localhost",
            "-keyout",
            "server.key",
            "-out",
            "server.csr",
        ]);
        std::fs::write(
            directory.path().join("server.ext"),
            "basicConstraints=critical,CA:FALSE\nkeyUsage=critical,digitalSignature,keyEncipherment\nextendedKeyUsage=serverAuth\nsubjectAltName=DNS:localhost\n",
        )
        .unwrap();
        openssl(&[
            "x509",
            "-req",
            "-in",
            "server.csr",
            "-CA",
            "ca.pem",
            "-CAkey",
            "ca.key",
            "-set_serial",
            "1",
            "-days",
            "2",
            "-extfile",
            "server.ext",
            "-out",
            "server.pem",
        ]);
        openssl(&["x509", "-in", "ca.pem", "-outform", "DER", "-out", "ca.der"]);
        let hub_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../sorrel-hub");
        let mut child = HubChild(
            Command::new("node")
                .args([
                    "--input-type=module",
                    "--eval",
                    r#"
import https from 'node:https';
import fs from 'node:fs';
import { createApp } from './src/app.js';
const app = createApp();
const directory = process.env.SORREL_TLS_TEST_DIR;
const server = https.createServer({
  key: fs.readFileSync(`${directory}/server.key`),
  cert: fs.readFileSync(`${directory}/server.pem`),
}, app.handleRequest);
server.listen(0, '127.0.0.1', () => {
  console.log(JSON.stringify({url: `https://127.0.0.1:${server.address().port}`}));
});
"#,
                ])
                .current_dir(hub_dir)
                .env("SORREL_TLS_TEST_DIR", directory.path())
                .stdout(Stdio::piped())
                .stderr(Stdio::inherit())
                .spawn()
                .expect("start local HTTPS Hub"),
        );
        let mut line = String::new();
        BufReader::new(child.0.stdout.take().unwrap())
            .read_line(&mut line)
            .unwrap();
        let ready: Value = serde_json::from_str(&line).expect("HTTPS Hub ready message");
        Self {
            url: ready["url"].as_str().unwrap().to_owned(),
            _child: child,
            directory,
        }
    }
}

#[test]
fn cli_http_clients_reject_untrusted_https_certificates() {
    let hub = TlsHub::start();
    let workspace = tempfile::tempdir().unwrap();
    let cli = || {
        let mut command = AssertCommand::cargo_bin("sorrel").unwrap();
        command
            .current_dir(workspace.path())
            .env_remove(sorrel_cli::sync::HUB_TOKEN_ENV);
        command
    };
    cli().arg("init").assert().success();
    cli()
        .args([
            "remote",
            "add",
            "origin",
            &hub.url,
            "--repo-id",
            "repo_tls_test",
        ])
        .assert()
        .success();
    for args in [
        &["pull"][..],
        &["lane", "submit", "--no-push"][..],
        &[
            "lane",
            "submit",
            "--no-push",
            "--project-id",
            "project_tls_test",
        ][..],
    ] {
        let result = cli().args(args).assert().failure();
        let stderr = String::from_utf8_lossy(&result.get_output().stderr);
        assert!(
            stderr.contains("invalid peer certificate"),
            "expected certificate verification failure for {args:?}, got: {stderr}"
        );
    }
}

#[test]
fn https_transport_checks_the_certificate_chain_and_hostname() {
    use std::sync::Arc;
    use ureq::rustls::{self, pki_types::CertificateDer};

    let hub = TlsHub::start();
    let mut roots = rustls::RootCertStore::empty();
    roots
        .add(CertificateDer::from(
            std::fs::read(hub.directory.path().join("ca.der")).unwrap(),
        ))
        .unwrap();
    let config = rustls::ClientConfig::builder_with_provider(
        rustls::crypto::ring::default_provider().into(),
    )
    .with_safe_default_protocol_versions()
    .unwrap()
    .with_root_certificates(roots)
    .with_no_client_auth();
    let agent = ureq::AgentBuilder::new()
        .tls_config(Arc::new(config))
        .build();
    let response: Value = agent
        .get(&format!(
            "{}/projects",
            hub.url.replace("127.0.0.1", "localhost")
        ))
        .call()
        .expect("TLS handshake with a trusted local CA and matching hostname")
        .into_json()
        .unwrap();
    assert!(response["data"].is_array());

    let error = agent
        .get(&format!("{}/projects", hub.url))
        .call()
        .expect_err("a trusted CA must not bypass hostname verification");
    assert!(
        error.to_string().contains("invalid peer certificate"),
        "expected hostname verification failure, got: {error}"
    );
}
