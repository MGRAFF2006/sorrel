//! CLI-facing workflow + runner surface.
//!
//! This module provides the simplified workflow-parsing and local-process
//! execution API consumed by `sorrel-cli`, wired to the local
//! [`crate::cli_policy`] evaluator. It previously lived in `sorrel-runners`
//! as a `cli_runner` compat module; it now lives in the CLI so the engine
//! crates carry only their native, protocol-conformant APIs.

mod bundle;
mod policy;
mod runner;
mod workflow;

pub use bundle::JobBundle;
pub use policy::{CorePermissionEvaluator, PolicyGateError};
pub use runner::{LocalProcessRunner, RunError, RunOutcome, RunStatus};
pub use workflow::{
    parse_workflow_file, parse_workflow_yaml, ParsedJob, ParsedWorkflow, WorkflowError,
};
