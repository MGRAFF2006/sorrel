//! Library surface for `sorrel-cli`.
//!
//! The CLI is primarily a binary (`src/main.rs`), but its modules live here so
//! that integration tests (e.g. the protocol policy-conformance suite) can
//! exercise them directly. The CLI-facing policy and runner modules
//! (`cli_policy`, `cli_runner`) retain CLI-shaped contracts. Workflow adapters
//! delegate parsing/execution to `sorrel-runners` and decisions to Core policy.

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
pub mod workflow_cmd;

/// Structured result of a CLI command: a machine-readable `--json` value and a
/// human-readable line. Shared by the binary and the workflow command module.
pub struct CommandOutput {
    /// Machine-readable JSON output (printed with `--json`).
    pub json: serde_json::Value,
    /// Human-readable summary line.
    pub human: String,
}
