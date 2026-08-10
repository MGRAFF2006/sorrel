use serde::{Deserialize, Serialize};

/// Portable execution bundle for a single workflow job.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobBundle {
    pub workflow_id: String,
    pub job_name: String,
    pub runner_id: String,
    pub command: String,
    pub shell: String,
    pub secret_refs: Vec<String>,
    pub environment: Option<String>,
}

impl JobBundle {
    /// Secret refs remain refs in any serialized view.
    #[must_use]
    pub fn secret_ref_handles(&self) -> &[String] {
        &self.secret_refs
    }
}
