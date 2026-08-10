//! Project environment commands — devenv-first with a clear local fallback story.
//!
//! Nix/devenv are preferred for reproducible shells and tasks. They are never
//! mandatory: missing tools get one-shot install guidance from `env ensure`.

use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;

use clap::{Args, Subcommand};
use serde_json::json;

use crate::CommandOutput;

const DEFAULT_DEVENV_NIX: &str = r#"{ pkgs, ... }:

{
  packages = [ pkgs.git ];

  # Optional: enable SecretSpec providers via devenv's secrets integration later.
  # See https://devenv.sh and https://secretspec.dev
}
"#;

const DEFAULT_DEVENV_YAML: &str = r#"# Sorrel-managed devenv project config.
# Prefer `sorrel env ensure` / `sorrel workflow run` over invoking devenv directly.
"#;

#[derive(Debug, Subcommand)]
pub enum EnvCommand {
    /// Write minimal devenv.nix / devenv.yaml when missing.
    Init(EnvInitArgs),
    /// Detect Nix/devenv and print install guidance if missing.
    Ensure(EnvEnsureArgs),
    /// Show detected environment backend status.
    Info,
    /// Enter a devenv shell when available (falls back with a clear error).
    Shell,
}

#[derive(Debug, Args)]
pub struct EnvInitArgs {
    /// Also write a stub secretspec.toml enable note (does not invent SecretRefs).
    #[arg(long)]
    pub with_secretspec: bool,
}

#[derive(Debug, Args)]
pub struct EnvEnsureArgs {
    /// Fail if devenv is unavailable (default: warn and exit 0 with status).
    #[arg(long)]
    pub require_devenv: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvStatus {
    pub nix: bool,
    pub devenv: bool,
    pub devenv_nix: bool,
    pub devenv_yaml: bool,
    pub secretspec_toml: bool,
    pub backend: &'static str,
}

pub fn execute(command: EnvCommand) -> io::Result<CommandOutput> {
    match command {
        EnvCommand::Init(args) => init_output(args),
        EnvCommand::Ensure(args) => ensure_output(args),
        EnvCommand::Info => info_output(),
        EnvCommand::Shell => shell_output(),
    }
}

pub fn detect(cwd: &Path) -> EnvStatus {
    let nix = command_exists("nix");
    let devenv = command_exists("devenv");
    let backend = if devenv && cwd.join("devenv.nix").is_file() {
        "devenv"
    } else {
        "local-fallback"
    };
    EnvStatus {
        nix,
        devenv,
        devenv_nix: cwd.join("devenv.nix").is_file(),
        devenv_yaml: cwd.join("devenv.yaml").is_file(),
        secretspec_toml: cwd.join("secretspec.toml").is_file(),
        backend,
    }
}

fn init_output(args: EnvInitArgs) -> io::Result<CommandOutput> {
    let cwd = env::current_dir()?;
    let mut written = Vec::new();

    let nix_path = cwd.join("devenv.nix");
    if !nix_path.is_file() {
        fs::write(&nix_path, DEFAULT_DEVENV_NIX)?;
        written.push("devenv.nix");
    }
    let yaml_path = cwd.join("devenv.yaml");
    if !yaml_path.is_file() {
        fs::write(&yaml_path, DEFAULT_DEVENV_YAML)?;
        written.push("devenv.yaml");
    }
    if args.with_secretspec {
        let note = cwd.join(".sorrel-secretspec.readme");
        if !note.is_file() {
            fs::write(
                &note,
                "Declare SecretRefs in sorrel.secrets.yml, then run `sorrel secret sync`.\n",
            )?;
            written.push(".sorrel-secretspec.readme");
        }
    }

    let status = detect(&cwd);
    Ok(CommandOutput {
        json: json!({
            "command": "env init",
            "mocked": false,
            "written": written,
            "status": status_json(&status),
            "nixInstall": nix_install_hint(),
            "devenvInstall": devenv_install_hint()
        }),
        human: if written.is_empty() {
            "devenv files already present".to_owned()
        } else {
            format!("Wrote {}", written.join(", "))
        },
    })
}

fn ensure_output(args: EnvEnsureArgs) -> io::Result<CommandOutput> {
    let cwd = env::current_dir()?;
    let status = detect(&cwd);
    let ready = status.devenv && status.nix;
    let mut guidance = Vec::new();
    if !status.nix {
        guidance.push(nix_install_hint());
    }
    if !status.devenv {
        guidance.push(devenv_install_hint());
    }
    if !status.devenv_nix {
        guidance.push("Run `sorrel env init` to create devenv.nix / devenv.yaml.".to_owned());
    }

    if args.require_devenv && !ready {
        return Err(io::Error::other(format!(
            "devenv not ready: {}",
            guidance.join(" ")
        )));
    }

    Ok(CommandOutput {
        json: json!({
            "command": "env ensure",
            "mocked": false,
            "ready": ready,
            "backend": status.backend,
            "status": status_json(&status),
            "guidance": guidance
        }),
        human: if ready {
            format!("Environment ready (backend: {})", status.backend)
        } else {
            format!(
                "Environment not fully ready (backend: {}). {}",
                status.backend,
                guidance.join(" ")
            )
        },
    })
}

fn info_output() -> io::Result<CommandOutput> {
    let cwd = env::current_dir()?;
    let status = detect(&cwd);
    Ok(CommandOutput {
        json: json!({
            "command": "env info",
            "mocked": false,
            "status": status_json(&status)
        }),
        human: format!(
            "backend={} nix={} devenv={} devenv.nix={} secretspec.toml={}",
            status.backend, status.nix, status.devenv, status.devenv_nix, status.secretspec_toml
        ),
    })
}

fn shell_output() -> io::Result<CommandOutput> {
    let cwd = env::current_dir()?;
    let status = detect(&cwd);
    if !status.devenv {
        return Err(io::Error::other(format!(
            "devenv not found. {} Falling back is `sorrel workflow run` with backend local-fallback.",
            devenv_install_hint()
        )));
    }
    // Replace the current process with devenv shell when possible.
    let error = ProcessCommand::new("devenv").arg("shell").status();
    match error {
        Ok(status) => {
            let code = status.code().unwrap_or(1);
            if code == 0 {
                Ok(CommandOutput {
                    json: json!({
                        "command": "env shell",
                        "mocked": false,
                        "status": "exited",
                        "exitCode": 0,
                        "backend": "devenv"
                    }),
                    human: "devenv shell exited".to_owned(),
                })
            } else {
                Err(io::Error::other(format!("devenv shell exited with {code}")))
            }
        }
        Err(error) => Err(error),
    }
}

fn status_json(status: &EnvStatus) -> serde_json::Value {
    json!({
        "nix": status.nix,
        "devenv": status.devenv,
        "devenvNix": status.devenv_nix,
        "devenvYaml": status.devenv_yaml,
        "secretspecToml": status.secretspec_toml,
        "backend": status.backend
    })
}

fn command_exists(name: &str) -> bool {
    ProcessCommand::new(name)
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn nix_install_hint() -> String {
    "Install Nix once: https://nixos.org/download/".to_owned()
}

fn devenv_install_hint() -> String {
    "Install devenv: https://devenv.sh/getting-started/ (pin via your documented channel)"
        .to_owned()
}

/// Preferred execution backend for workflow/task runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunnerBackendKind {
    Devenv,
    LocalFallback,
}

impl RunnerBackendKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Devenv => "devenv",
            Self::LocalFallback => "local-fallback",
        }
    }
}

/// Select backend for the current workspace.
#[must_use]
pub fn select_backend(cwd: &Path) -> RunnerBackendKind {
    let status = detect(cwd);
    if status.devenv && status.devenv_nix {
        RunnerBackendKind::Devenv
    } else {
        RunnerBackendKind::LocalFallback
    }
}

/// Run a shell command through devenv when selected, else return None so callers
/// can use LocalProcessRunner.
pub fn try_devenv_run(cwd: &Path, shell_command: &str) -> io::Result<Option<DevenvRunOutcome>> {
    if select_backend(cwd) != RunnerBackendKind::Devenv {
        return Ok(None);
    }
    let output = ProcessCommand::new("devenv")
        .arg("shell")
        .arg("--")
        .arg("sh")
        .arg("-c")
        .arg(shell_command)
        .current_dir(cwd)
        .output()?;
    Ok(Some(DevenvRunOutcome {
        exit_code: output.status.code(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        success: output.status.success(),
    }))
}

#[derive(Debug, Clone)]
pub struct DevenvRunOutcome {
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub success: bool,
}

#[must_use]
pub fn workspace_root() -> PathBuf {
    env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}
