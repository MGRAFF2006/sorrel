use std::fs;
use std::path::Path;

use assert_cmd::Command;
use serde_json::{json, Value};

const SECRET_ID: &str = "secret_provider_fixture";
const SECRET_NAME: &str = "SORREL_SECRET_PROVIDER_FIXTURE";

#[test]
fn secret_run_respects_declared_providers_and_explicit_overrides() {
    for (provider, dotenv_path, environment_value, override_provider, expected) in [
        ("env", None, Some("fixture-env"), None, "fixture-env"),
        ("sorrel-vault", Some(".env"), None, None, "fixture-dotenv"),
        (
            "dotenv:fixture.env",
            Some("fixture.env"),
            None,
            None,
            "fixture-dotenv",
        ),
        (
            "env",
            Some(".env"),
            Some("fixture-env"),
            Some("dotenv:.env"),
            "fixture-dotenv",
        ),
        (
            "dotenv",
            Some(".env"),
            Some("fixture-env"),
            Some("env"),
            "fixture-env",
        ),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        cli(root, None).arg("init").assert().success();
        fs::write(
            root.join("sorrel.secrets.yml"),
            format!(
                "secretRefs:\n  - id: {SECRET_ID}\n    name: {SECRET_NAME}\n    provider: {provider}\n    environment: dev\n    required: true\n"
            ),
        )
        .unwrap();
        if let Some(path) = dotenv_path {
            fs::write(root.join(path), format!("{SECRET_NAME}=fixture-dotenv\n")).unwrap();
        }
        cli(root, None)
            .args([
                "grant",
                "create",
                "--action",
                "secret.inject",
                "--secret",
                SECRET_ID,
                "--agent",
                "agent_mock_cli",
                "--environment",
                "dev",
            ])
            .assert()
            .success();

        let mut get = cli(root, environment_value);
        get.args(["secret", "get", SECRET_ID, "--reveal", "--json"]);
        if let Some(override_provider) = override_provider {
            get.args(["--provider", override_provider]);
        }
        let get_output = get.assert().success();
        let revealed: Value = serde_json::from_slice(&get_output.get_output().stdout).unwrap();
        assert_eq!(revealed["value"], expected);

        let mut run = cli(root, environment_value);
        run.args(["secret", "run", "--secret", SECRET_ID, "--json"]);
        if let Some(override_provider) = override_provider {
            run.args(["--provider", override_provider]);
        }
        // The child verifies injection without printing even synthetic secret values.
        run.args([
            "--",
            "node",
            "--eval",
            &format!("process.exit(process.env.{SECRET_NAME} === '{expected}' ? 0 : 17)"),
        ]);
        let run_output = run.assert().success();
        let result: Value = serde_json::from_slice(&run_output.get_output().stdout).unwrap();
        assert_eq!(result["exitCode"], 0);
        assert_eq!(result["status"], "completed");
        assert_eq!(result["injected"], json!([SECRET_NAME]));
        let expected_provider = override_provider.unwrap_or(match provider {
            "sorrel-vault" | "dotenv" => "dotenv:.env",
            other => other,
        });
        assert_eq!(result["provider"], expected_provider);
        assert!(!String::from_utf8_lossy(&run_output.get_output().stdout).contains(expected));
    }
}

fn cli(root: &Path, environment_value: Option<&str>) -> Command {
    let mut command = Command::cargo_bin("sorrel").unwrap();
    command.current_dir(root).env_remove(SECRET_NAME);
    if let Some(value) = environment_value {
        command.env(SECRET_NAME, value);
    }
    command
}
