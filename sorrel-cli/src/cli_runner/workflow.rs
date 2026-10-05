use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

use sorrel_runners::workflow::WorkflowFile;

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
    native: WorkflowFile,
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

        let mut selected = self.native.clone();
        let spec = selected
            .workflows
            .get_mut(&self.id)
            .expect("selected workflow exists");
        let mut included = std::collections::BTreeSet::new();
        let mut pending = vec![job_name.to_owned()];
        while let Some(name) = pending.pop() {
            if included.insert(name.clone()) {
                pending.extend(spec.jobs[&name].needs.clone());
            }
        }
        spec.jobs.retain(|name, _| included.contains(name));
        let native = selected.to_bundle(&self.id).map_err(native_error)?;
        let secret_refs = native
            .secret_refs
            .iter()
            .map(|secret| secret.id.clone())
            .collect();
        Ok(JobBundle {
            workflow_id: self.id.clone(),
            job_name: job.name.clone(),
            runner_id: "runner_local_process".to_owned(),
            command: job.command.clone(),
            shell: job.shell.clone().unwrap_or_else(|| "sh".to_owned()),
            secret_refs,
            native: Some(native),
            environment: Some("dev".to_owned()),
        })
    }
}

/// Parses a workflow YAML document.
pub fn parse_workflow_yaml(
    yaml: &str,
    workflow_id: Option<&str>,
) -> Result<ParsedWorkflow, WorkflowError> {
    parse_selected_workflow_yaml(yaml, None, workflow_id)
}

fn native_error(error: sorrel_runners::RunnerError) -> WorkflowError {
    match error {
        sorrel_runners::RunnerError::WorkflowParse(message) => {
            WorkflowError::ParseFailed { message }
        }
        error => WorkflowError::InvalidDocument {
            message: error.to_string(),
        },
    }
}

pub fn parse_selected_workflow_yaml(
    yaml: &str,
    selected: Option<&str>,
    id_override: Option<&str>,
) -> Result<ParsedWorkflow, WorkflowError> {
    let mut native = WorkflowFile::from_yaml(yaml).map_err(native_error)?;
    let name = match selected {
        Some(name) if native.workflows.contains_key(name) => name.to_owned(),
        Some(name) => {
            return Err(WorkflowError::InvalidDocument {
                message: format!("workflow `{name}` was not found"),
            })
        }
        None if native.workflows.len() == 1 => native.workflows.keys().next().unwrap().clone(),
        None => {
            return Err(WorkflowError::InvalidDocument {
                message: "multiple workflows; select one with --workflow <name>".to_owned(),
            })
        }
    };
    native.to_bundle(&name).map_err(native_error)?;
    let spec = native.workflows.remove(&name).unwrap();
    let id = id_override.unwrap_or(&name).to_owned();
    let mut jobs = BTreeMap::new();
    for (name, job) in &spec.jobs {
        let mut secret_refs = job.secrets.clone();
        for value in job.env.values() {
            match value {
                sorrel_runners::workflow::WorkflowEnvValue::Literal(value) => {
                    if let Some(reference) = value.strip_prefix("secret:") {
                        secret_refs.push(reference.to_owned());
                    }
                }
                sorrel_runners::workflow::WorkflowEnvValue::Secret { secret } => {
                    secret_refs.push(secret.id.clone())
                }
            }
        }
        secret_refs.sort();
        secret_refs.dedup();
        jobs.insert(
            name.clone(),
            ParsedJob {
                name: name.clone(),
                command: job.command.clone(),
                shell: job.shell.clone(),
                secret_refs,
            },
        );
    }
    native.workflows.clear();
    native.workflows.insert(id.clone(), spec);
    Ok(ParsedWorkflow {
        id,
        version: native.version,
        jobs,
        source_path: None,
        native,
    })
}

/// Parses a workflow file from disk.
pub fn parse_workflow_file(path: &Path) -> Result<ParsedWorkflow, WorkflowError> {
    parse_workflow_file_selected(path, None)
}

pub fn parse_workflow_file_selected(
    path: &Path,
    selected: Option<&str>,
) -> Result<ParsedWorkflow, WorkflowError> {
    if !path.is_file() {
        return Err(WorkflowError::FileNotFound {
            path: path.to_path_buf(),
        });
    }

    let yaml = std::fs::read_to_string(path).map_err(|error| WorkflowError::ReadFailed {
        path: path.to_path_buf(),
        message: error.to_string(),
    })?;

    let mut workflow = parse_selected_workflow_yaml(&yaml, selected, None)?;
    workflow.source_path = Some(path.to_path_buf());
    Ok(workflow)
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
