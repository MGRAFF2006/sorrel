//! CLI-facing workflow + runner surface.
//!
//! Thin compatibility adapters preserve CLI JSON while using the canonical
//! `sorrel-runners` parser/executor and `sorrel-core` policy evaluator.

mod bundle;
mod policy;
mod runner;
mod workflow;

pub use bundle::JobBundle;
pub use policy::{CorePermissionEvaluator, PolicyGateError};
pub use runner::{LocalProcessRunner, RunError, RunOutcome, RunStatus};
pub use workflow::{
    parse_workflow_file, parse_workflow_file_selected, parse_workflow_yaml, ParsedJob,
    ParsedWorkflow, WorkflowError,
};
