use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use super::bundle::JobBundle;

/// Errors raised while locating or parsing workflow files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkflowError {
    FileNotFound { path: PathBuf },
    ReadFailed { path: PathBuf, message: String },
    ParseFailed { message: String },
    InvalidDocument { message: String },
}

impl fmt::Display for WorkflowError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::FileNotFound { path } => {
                write!(formatter, "workflow file not found: {}", path.display())
            }
            Self::ReadFailed { path, message } => {
                write!(
                    formatter,
                    "failed to read workflow file {}: {message}",
                    path.display()
                )
            }
            Self::ParseFailed { message } => {
                write!(formatter, "failed to parse workflow: {message}")
            }
            Self::InvalidDocument { message } => {
                write!(formatter, "invalid workflow document: {message}")
            }
        }
    }
}

impl std::error::Error for WorkflowError {}

/// A parsed job definition from `sorrel.workflow.yml`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedJob {
    pub name: String,
    pub command: String,
    pub shell: Option<String>,
    pub secret_refs: Vec<String>,
}

/// A parsed workflow document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedWorkflow {
    pub id: String,
    pub version: u32,
    pub jobs: BTreeMap<String, ParsedJob>,
    pub source_path: Option<PathBuf>,
}

impl ParsedWorkflow {
    /// Returns the named job if it exists.
    #[must_use]
    pub fn job(&self, name: &str) -> Option<&ParsedJob> {
        self.jobs.get(name)
    }

    /// Returns sorted job names for stable reporting.
    #[must_use]
    pub fn job_names(&self) -> Vec<String> {
        self.jobs.keys().cloned().collect()
    }

    /// Converts a named job into a portable execution bundle.
    pub fn job_bundle(&self, job_name: &str) -> Result<JobBundle, WorkflowError> {
        let job = self
            .job(job_name)
            .ok_or_else(|| WorkflowError::InvalidDocument {
                message: format!("job `{job_name}` was not found in workflow `{}`", self.id),
            })?;

        Ok(JobBundle {
            workflow_id: self.id.clone(),
            job_name: job.name.clone(),
            runner_id: "runner_local_process".to_owned(),
            command: job.command.clone(),
            shell: job.shell.clone().unwrap_or_else(|| "sh".to_owned()),
            secret_refs: job.secret_refs.clone(),
            environment: Some("dev".to_owned()),
        })
    }
}

/// Parses a workflow YAML document.
pub fn parse_workflow_yaml(
    yaml: &str,
    workflow_id: Option<&str>,
) -> Result<ParsedWorkflow, WorkflowError> {
    let document: WorkflowDocument =
        serde_yaml_ng::from_str(yaml).map_err(|error| WorkflowError::ParseFailed {
            message: error.to_string(),
        })?;

    if document.jobs.is_empty() {
        return Err(WorkflowError::InvalidDocument {
            message: "workflow must define at least one job".to_owned(),
        });
    }

    let id = workflow_id
        .map(str::to_owned)
        .or(document.id)
        .unwrap_or_else(|| "workflow_local".to_owned());

    let mut jobs = BTreeMap::new();
    for (name, job) in document.jobs {
        if job.command.trim().is_empty() {
            return Err(WorkflowError::InvalidDocument {
                message: format!("job `{name}` is missing a command"),
            });
        }

        let secret_refs = job.secret_refs();
        jobs.insert(
            name.clone(),
            ParsedJob {
                name,
                command: job.command,
                shell: job.shell,
                secret_refs,
            },
        );
    }

    Ok(ParsedWorkflow {
        id,
        version: document.version,
        jobs,
        source_path: None,
    })
}

/// Parses a workflow file from disk.
pub fn parse_workflow_file(path: &Path) -> Result<ParsedWorkflow, WorkflowError> {
    if !path.is_file() {
        return Err(WorkflowError::FileNotFound {
            path: path.to_path_buf(),
        });
    }

    let yaml = std::fs::read_to_string(path).map_err(|error| WorkflowError::ReadFailed {
        path: path.to_path_buf(),
        message: error.to_string(),
    })?;

    let mut workflow = parse_workflow_yaml(&yaml, None)?;
    workflow.source_path = Some(path.to_path_buf());
    Ok(workflow)
}

#[derive(Debug, Deserialize)]
struct WorkflowDocument {
    #[serde(default = "default_version")]
    version: u32,
    id: Option<String>,
    jobs: BTreeMap<String, WorkflowJobDocument>,
}

fn default_version() -> u32 {
    1
}

#[derive(Debug, Deserialize)]
struct WorkflowJobDocument {
    command: String,
    #[serde(default)]
    shell: Option<String>,
    #[serde(default)]
    secrets: Vec<String>,
    #[serde(default)]
    env: BTreeMap<String, String>,
}

impl WorkflowJobDocument {
    fn secret_refs(&self) -> Vec<String> {
        let mut refs = self.secrets.clone();
        for value in self.env.values() {
            if let Some(secret_ref) = value.strip_prefix("secret:") {
                refs.push(secret_ref.to_owned());
            }
        }
        refs.sort();
        refs.dedup();
        refs
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
version: 1
id: workflow_validate_protocol
jobs:
  test:
    command: echo hello
    shell: sh
    secrets:
      - secret_npm_token_dev
    env:
      NPM_TOKEN: "secret:secret_npm_token_dev"
"#;

    #[test]
    fn parse_workflow_yaml_reports_jobs_and_secret_refs() {
        let workflow = parse_workflow_yaml(SAMPLE, None).expect("workflow parses");
        assert_eq!(workflow.id, "workflow_validate_protocol");
        assert_eq!(workflow.version, 1);
        assert_eq!(workflow.job_names(), vec!["test".to_owned()]);

        let job = workflow.job("test").expect("job exists");
        assert_eq!(job.command, "echo hello");
        assert_eq!(job.secret_refs, vec!["secret_npm_token_dev".to_owned()]);
    }

    #[test]
    fn job_bundle_preserves_secret_refs_without_values() {
        let workflow = parse_workflow_yaml(SAMPLE, None).expect("workflow parses");
        let bundle = workflow.job_bundle("test").expect("bundle builds");
        assert_eq!(bundle.workflow_id, "workflow_validate_protocol");
        assert_eq!(bundle.job_name, "test");
        assert_eq!(bundle.secret_refs, vec!["secret_npm_token_dev".to_owned()]);
    }
}
