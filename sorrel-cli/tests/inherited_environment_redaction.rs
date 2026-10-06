#![cfg(unix)]

use std::{fs, os::unix::fs::PermissionsExt, path::Path};

use assert_cmd::Command;
use serde_json::Value;
use tempfile::TempDir;

const SECRET: &str = "synthetic-inherited-token-734159";
const OVERLAP: &str = "synthetic-inherited-token-734159-extended";

fn run(root: &Path, args: &[&str], path: Option<&str>) -> std::process::Output {
    let mut command = Command::cargo_bin("sorrel").unwrap();
    command.current_dir(root).args(args);
    // Keep all fixtures process-local: no concurrent process-global mutation.
    command.env("SORREL_TEST_TOKEN", SECRET);
    command.env("SORREL_TEST_PASSWORD", OVERLAP);
    command.env("SORREL_TEST_EMPTY_SECRET", "");
    command.env("SORREL_TEST_PUBLIC", "ordinary-inherited-value");
    if let Some(path) = path {
        command.env("PATH", path);
    }
    command.output().unwrap()
}

fn assert_clean(root: &Path, output: &std::process::Output, backend: &str, exit: i32) {
    assert_eq!(output.status.code(), Some(exit));
    let text = String::from_utf8(output.stdout.clone()).unwrap();
    assert!(
        !text.contains(SECRET),
        "captured JSON exposes inherited secret"
    );
    assert!(!text.contains(OVERLAP));
    let json: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(json["backend"], backend);
    assert_eq!(json["job"]["stdout"], "***|***|ordinary-inherited-value");
    assert_eq!(json["job"]["stderr"], "***");
    let run_id = json["runId"].as_str().unwrap();
    let dir = root.join(".sorrel/runs").join(run_id);
    let mut files = 0;
    for entry in fs::read_dir(dir).unwrap() {
        let contents = fs::read(entry.unwrap().path()).unwrap();
        let text = String::from_utf8_lossy(&contents);
        assert!(
            !text.contains(SECRET),
            "persisted run data exposes inherited secret"
        );
        assert!(!text.contains(OVERLAP));
        files += 1;
    }
    let events: Vec<Value> =
        fs::read_to_string(root.join(".sorrel/runs").join(run_id).join("log.jsonl"))
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
    assert!(events
        .iter()
        .any(|event| event["stream"] == "stdout"
            && event["chunk"] == "***|***|ordinary-inherited-value"));
    assert!(events
        .iter()
        .any(|event| event["stream"] == "stderr" && event["chunk"] == "***"));
    assert!(files >= 2, "must inspect real manifest and stream log");
}

fn fixture(devenv: bool, exit: i32) -> (TempDir, Option<String>) {
    let root = TempDir::new().unwrap();
    assert!(run(root.path(), &["init", "--json"], None).status.success());
    fs::write(root.path().join("sorrel.workflow.yml"), format!("jobs:\n  test:\n    command: printf '%s|%s|%s' \"$SORREL_TEST_TOKEN\" \"$SORREL_TEST_PASSWORD\" \"$SORREL_TEST_PUBLIC\"; printf '%s' \"$SORREL_TEST_TOKEN\" >&2; exit {exit}\n")).unwrap();
    let path = if devenv {
        fs::write(root.path().join("devenv.nix"), "{}").unwrap();
        let bin = root.path().join("bin");
        fs::create_dir(&bin).unwrap();
        let executable = bin.join("devenv");
        fs::write(
            &executable,
            "#!/bin/sh\nif [ \"$1\" = --version ]; then exit 0; fi\nshift 2\nexec \"$@\"\n",
        )
        .unwrap();
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o700)).unwrap();
        Some(format!(
            "{}:{}",
            bin.display(),
            std::env::var("PATH").unwrap()
        ))
    } else {
        None
    };
    (root, path)
}

#[test]
fn inherited_values_are_available_but_redacted_before_local_output_and_persistence() {
    for exit in [0, 7] {
        let (root, path) = fixture(false, exit);
        let output = run(
            root.path(),
            &["workflow", "run", "test", "--json"],
            path.as_deref(),
        );
        assert_clean(root.path(), &output, "local-fallback", exit);
    }
}

#[test]
fn devenv_output_is_redacted_before_json_and_persistence_on_success_and_failure() {
    for exit in [0, 7] {
        let (root, path) = fixture(true, exit);
        let output = run(
            root.path(),
            &["workflow", "run", "test", "--json"],
            path.as_deref(),
        );
        assert_clean(root.path(), &output, "devenv", exit);
    }
}
