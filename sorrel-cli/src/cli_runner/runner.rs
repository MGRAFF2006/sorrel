use std::collections::BTreeMap;
use std::process::Command as ProcessCommand;

use super::bundle::JobBundle;
use super::policy::{CorePermissionEvaluator, PolicyGateError};

/// Outcome of a local process workflow job.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunOutcome {
    pub status: RunStatus,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    /// SecretSpec / local-fallback marker for JSON consumers.
    pub backend: String,
    /// Env names that were injected (never values).
    pub injected_secrets: Vec<String>,
}

/// High-level status for a workflow job run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunStatus {
    Completed,
    Failed,
}

impl RunStatus {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Completed => "completed",
            Self::Failed => "failed",
        }
    }
}

/// Errors raised while executing a workflow job.
#[derive(Debug)]
pub enum RunError {
    PolicyDenied(PolicyGateError),
    SpawnFailed { message: String },
    SecretResolve { message: String },
}

impl std::fmt::Display for RunError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PolicyDenied(error) => write!(formatter, "{error}"),
            Self::SpawnFailed { message } => write!(formatter, "failed to spawn job: {message}"),
            Self::SecretResolve { message } => {
                write!(formatter, "failed to resolve secrets: {message}")
            }
        }
    }
}

impl std::error::Error for RunError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::PolicyDenied(error) => Some(error),
            Self::SpawnFailed { .. } | Self::SecretResolve { .. } => None,
        }
    }
}

/// Executes workflow jobs as local shell processes.
#[derive(Debug, Default, Clone, Copy)]
pub struct LocalProcessRunner;

impl LocalProcessRunner {
    /// Runs a job bundle after Core policy authorization.
    pub fn run(
        &self,
        bundle: &JobBundle,
        evaluator: &CorePermissionEvaluator<'_>,
    ) -> Result<RunOutcome, RunError> {
        self.run_with_env(bundle, evaluator, BTreeMap::new())
    }

    /// Runs a job with additional environment variables (secret injection).
    pub fn run_with_env(
        &self,
        bundle: &JobBundle,
        evaluator: &CorePermissionEvaluator<'_>,
        env: BTreeMap<String, String>,
    ) -> Result<RunOutcome, RunError> {
        evaluator
            .authorize(bundle)
            .map_err(RunError::PolicyDenied)?;

        let injected_secrets: Vec<String> = env.keys().cloned().collect();
        let mut command = ProcessCommand::new(&bundle.shell);
        command.arg("-c").arg(&bundle.command);
        for (key, value) in &env {
            command.env(key, value);
        }

        let output = command.output().map_err(|error| RunError::SpawnFailed {
            message: error.to_string(),
        })?;

        let exit_code = output.status.code();
        let completed = output.status.success();

        Ok(RunOutcome {
            status: if completed {
                RunStatus::Completed
            } else {
                RunStatus::Failed
            },
            exit_code,
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            backend: "local-fallback".to_owned(),
            injected_secrets,
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::cli_policy::{Grant, PolicyContext, PrincipalId, ResourceScope};

    use super::*;

    fn granted_context() -> PolicyContext {
        let mut context = PolicyContext {
            repo_id: "repo_mock_local".to_owned(),
            authority_principals: vec![],
            grants: vec![],
            default_rules: vec![],
        };
        context.grants.push(Grant {
            principal: PrincipalId {
                kind: "agent".to_owned(),
                id: "agent_mock_cli".to_owned(),
            },
            capabilities: vec!["workflow.run".to_owned(), "runner.use".to_owned()],
            resources: vec![
                ResourceScope {
                    scope: "workflow".to_owned(),
                    fields: serde_json::json!({ "ref": "workflow_validate_protocol" })
                        .as_object()
                        .cloned()
                        .unwrap_or_default(),
                },
                ResourceScope {
                    scope: "runner".to_owned(),
                    fields: serde_json::json!({ "ref": "runner_local_process" })
                        .as_object()
                        .cloned()
                        .unwrap_or_default(),
                },
            ],
            issued_by: None,
        });
        context
    }

    #[test]
    fn run_executes_local_command_after_policy_allows() {
        let bundle = JobBundle {
            workflow_id: "workflow_validate_protocol".to_owned(),
            job_name: "test".to_owned(),
            runner_id: "runner_local_process".to_owned(),
            command: "echo workflow-ok".to_owned(),
            shell: "sh".to_owned(),
            secret_refs: vec![],
            environment: Some("dev".to_owned()),
        };
        let context = granted_context();
        let evaluator = CorePermissionEvaluator {
            context: &context,
            principal: PrincipalId {
                kind: "agent".to_owned(),
                id: "agent_mock_cli".to_owned(),
            },
        };

        let outcome = LocalProcessRunner
            .run(&bundle, &evaluator)
            .expect("run succeeds");
        assert_eq!(outcome.status, RunStatus::Completed);
        assert!(outcome.stdout.contains("workflow-ok"));
        assert_eq!(outcome.backend, "local-fallback");
    }

    #[test]
    fn run_with_env_injects_variables() {
        let bundle = JobBundle {
            workflow_id: "workflow_validate_protocol".to_owned(),
            job_name: "test".to_owned(),
            runner_id: "runner_local_process".to_owned(),
            command: "printf '%s' \"$SORREL_TEST_SECRET\"".to_owned(),
            shell: "sh".to_owned(),
            secret_refs: vec![],
            environment: Some("dev".to_owned()),
        };
        let context = granted_context();
        let evaluator = CorePermissionEvaluator {
            context: &context,
            principal: PrincipalId {
                kind: "agent".to_owned(),
                id: "agent_mock_cli".to_owned(),
            },
        };
        let mut env = BTreeMap::new();
        env.insert("SORREL_TEST_SECRET".to_owned(), "injected-value".to_owned());

        let outcome = LocalProcessRunner
            .run_with_env(&bundle, &evaluator, env)
            .expect("run succeeds");
        assert_eq!(outcome.stdout, "injected-value");
        assert_eq!(
            outcome.injected_secrets,
            vec!["SORREL_TEST_SECRET".to_owned()]
        );
    }
}
