//! Library surface for `sorrel-cli`.
//!
//! The CLI is primarily a binary (`src/main.rs`), but its modules live here so
//! that integration tests (e.g. the protocol policy-conformance suite) can
//! exercise them directly. The CLI-facing policy and runner modules
//! (`cli_policy`, `cli_runner`) previously lived in `sorrel-core::cli_policy`
//! and `sorrel-runners::cli_runner`; they now live in the CLI so the engine
//! crates carry only their native, protocol-conformant APIs.

pub mod agent_cmd;
pub mod cli_policy;
pub mod cli_runner;
pub mod env_cmd;
pub mod hub;
pub mod linediff;
pub mod repo;
pub mod run_log;
pub mod secret_cmd;
pub mod secretspec_bridge;
pub mod sync;
pub mod tracking;
pub mod workflow_cmd;
pub mod workspace_cmd;

/// Stable CLI error category carried through the existing I/O result surface.
#[derive(Debug)]
pub struct CommandError {
    pub code: &'static str,
    pub message: String,
}

impl std::fmt::Display for CommandError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for CommandError {}

pub fn command_error(code: &'static str, message: impl Into<String>) -> std::io::Error {
    std::io::Error::other(CommandError {
        code,
        message: message.into(),
    })
}

pub fn error_code(error: &std::io::Error) -> &'static str {
    if let Some(error) = error
        .get_ref()
        .and_then(|error| error.downcast_ref::<CommandError>())
    {
        return error.code;
    }
    match error.kind() {
        std::io::ErrorKind::NotFound => "not_found",
        std::io::ErrorKind::InvalidInput => "invalid_input",
        std::io::ErrorKind::InvalidData => "invalid_data",
        std::io::ErrorKind::PermissionDenied => "permission_denied",
        std::io::ErrorKind::WouldBlock => "busy",
        _ => "operation_failed",
    }
}

/// Structured result of a CLI command: a machine-readable `--json` value and a
/// human-readable line. Shared by the binary and the workflow command module.
pub struct CommandOutput {
    /// Machine-readable JSON output (printed with `--json`).
    pub json: serde_json::Value,
    /// Human-readable summary line.
    pub human: String,
}
