use std::path::{Path, PathBuf};

use crate::cli_policy::{Grant, PolicyContext, PrincipalId, ResourceScope};
use crate::cli_runner::{parse_workflow_file, ParsedWorkflow, RunError, WorkflowError};
use crate::cli_runner::{CorePermissionEvaluator, JobBundle, LocalProcessRunner, RunStatus};
use crate::env_cmd::{select_backend, try_devenv_run, RunnerBackendKind};
use crate::run_log::{self, RunManifest};
use crate::secretspec_bridge::{
    load_secret_handles, redact_text, resolve_handles, secret_policy_context, BridgeError,
};
use clap::Args;
use serde_json::{json, Value};

use crate::CommandOutput;

pub const DEFAULT_WORKFLOW_FILE: &str = "sorrel.workflow.yml";
const CLI_AGENT_PRINCIPAL: &str = "agent:agent_mock_cli";

#[derive(Debug, Args)]
pub struct WorkflowFileArgs {
    /// Path to a workflow file. Defaults to ./sorrel.workflow.yml.
    #[arg(long)]
    pub file: Option<PathBuf>,
}

#[derive(Debug, Args)]
pub struct WorkflowRunJobArgs {
    /// Job name to run from the workflow file.
    pub job_name: String,

    /// Path to a workflow file. Defaults to ./sorrel.workflow.yml.
    #[arg(long)]
    pub file: Option<PathBuf>,
}

pub fn workflow_validate_output(args: WorkflowFileArgs) -> CommandOutput {
    match load_workflow(&args.file) {
        Ok(workflow) => CommandOutput {
            json: validation_success_json(&workflow),
            human: format!(
                "Valid workflow {} (version {}) with jobs: {}",
                workflow.id,
                workflow.version,
                workflow.job_names().join(", ")
            ),
        },
        Err(WorkflowError::FileNotFound { path }) => {
            missing_workflow_file_output("workflow validate", &path)
        }
        Err(error) => workflow_error_output("workflow validate", &error),
    }
}

pub fn workflow_run_output(args: WorkflowRunJobArgs) -> CommandOutput {
    let workflow = match load_workflow(&args.file) {
        Ok(workflow) => workflow,
        Err(WorkflowError::FileNotFound { path }) => {
            return missing_workflow_file_output("workflow run", &path);
        }
        Err(error) => return workflow_error_output("workflow run", &error),
    };

    let bundle = match workflow.job_bundle(&args.job_name) {
        Ok(bundle) => bundle,
        Err(WorkflowError::InvalidDocument { message }) if message.contains("was not found") => {
            return missing_job_output(&workflow, &args.job_name);
        }
        Err(error) => return workflow_error_output("workflow run", &error),
    };

    let principal =
        PrincipalId::parse(CLI_AGENT_PRINCIPAL).expect("CLI agent principal is well-formed");
    let context = workflow_execution_context();
    let evaluator = CorePermissionEvaluator {
        context: &context,
        principal: principal.clone(),
    };

    if let Err(denial) = evaluator.authorize(&bundle) {
        return policy_denial_output(&workflow, &bundle, &denial);
    }

    let secret_env = match resolve_job_secrets(&bundle) {
        Ok(env) => env,
        Err(error) => {
            return CommandOutput {
                json: json!({
                    "command": "workflow run",
                    "mocked": false,
                    "status": "failed",
                    "workflow": workflow_summary_json(&workflow),
                    "job": {
                        "name": bundle.job_name,
                        "status": "failed",
                        "command": bundle.command
                    },
                    "bundle": bundle_json(&bundle),
                    "error": {
                        "kind": "secret_resolve_failed",
                        "message": error.to_string()
                    }
                }),
                human: format!(
                    "Workflow {} job {} failed to resolve secrets: {error}",
                    workflow.id, bundle.job_name
                ),
            };
        }
    };

    match run_job_with_backend(&bundle, &evaluator, &secret_env) {
        Ok(mut outcome) => {
            outcome.stdout = redact_text(&outcome.stdout, &secret_env);
            outcome.stderr = redact_text(&outcome.stderr, &secret_env);
            let _ = persist_run(&workflow, &bundle, &outcome);
            CommandOutput {
                json: run_success_json(&workflow, &bundle, &outcome),
                human: format!(
                    "Workflow {} job {} {} (backend: {})",
                    workflow.id,
                    bundle.job_name,
                    outcome.status.as_str(),
                    outcome.backend
                ),
            }
        }
        Err(RunError::PolicyDenied(denial)) => policy_denial_output(&workflow, &bundle, &denial),
        Err(RunError::SpawnFailed { message }) => CommandOutput {
            json: json!({
                "command": "workflow run",
                "mocked": false,
                "status": "failed",
                "workflow": workflow_summary_json(&workflow),
                "job": {
                    "name": bundle.job_name,
                    "status": "failed",
                    "command": bundle.command
                },
                "bundle": bundle_json(&bundle),
                "error": {
                    "kind": "spawn_failed",
                    "message": message
                }
            }),
            human: format!(
                "Workflow {} job {} failed to start",
                workflow.id, bundle.job_name
            ),
        },
        Err(RunError::SecretResolve { message }) => CommandOutput {
            json: json!({
                "command": "workflow run",
                "mocked": false,
                "status": "failed",
                "workflow": workflow_summary_json(&workflow),
                "job": {
                    "name": bundle.job_name,
                    "status": "failed",
                    "command": bundle.command
                },
                "bundle": bundle_json(&bundle),
                "error": {
                    "kind": "secret_resolve_failed",
                    "message": message
                }
            }),
            human: format!(
                "Workflow {} job {} failed to resolve secrets",
                workflow.id, bundle.job_name
            ),
        },
    }
}

fn run_job_with_backend(
    bundle: &JobBundle,
    evaluator: &CorePermissionEvaluator<'_>,
    secret_env: &crate::secretspec_bridge::ResolvedSecrets,
) -> Result<crate::cli_runner::RunOutcome, RunError> {
    let cwd = std::env::current_dir().map_err(|error| RunError::SpawnFailed {
        message: error.to_string(),
    })?;

    // Secret env injection into devenv is deferred: when secrets are required we
    // stay on the local runner so values stay in the child env only.
    if secret_env.values.is_empty() && select_backend(&cwd) == RunnerBackendKind::Devenv {
        match try_devenv_run(&cwd, &bundle.command) {
            Ok(Some(devenv)) => {
                return Ok(crate::cli_runner::RunOutcome {
                    status: if devenv.success {
                        RunStatus::Completed
                    } else {
                        RunStatus::Failed
                    },
                    exit_code: devenv.exit_code,
                    stdout: devenv.stdout,
                    stderr: devenv.stderr,
                    backend: RunnerBackendKind::Devenv.as_str().to_owned(),
                    injected_secrets: vec![],
                });
            }
            Ok(None) => {}
            Err(error) => {
                // Fall through to local with a stderr note via local runner.
                let mut outcome = LocalProcessRunner.run_with_env(
                    bundle,
                    evaluator,
                    secret_env.values.clone(),
                )?;
                outcome.stderr = format!(
                    "devenv backend failed ({error}); used {}\n{}",
                    RunnerBackendKind::LocalFallback.as_str(),
                    outcome.stderr
                );
                outcome.backend = RunnerBackendKind::LocalFallback.as_str().to_owned();
                return Ok(outcome);
            }
        }
    }

    LocalProcessRunner.run_with_env(bundle, evaluator, secret_env.values.clone())
}

fn persist_run(
    workflow: &ParsedWorkflow,
    bundle: &JobBundle,
    outcome: &crate::cli_runner::RunOutcome,
) -> std::io::Result<()> {
    if !crate::repo::is_initialized() {
        return Ok(());
    }
    let id = run_log::new_run_id();
    let mut manifest = RunManifest {
        schema_version: 1,
        id: id.clone(),
        started_at: run_log::now_rfc3339(),
        finished_at: None,
        backend: outcome.backend.clone(),
        principal: CLI_AGENT_PRINCIPAL.to_owned(),
        workflow_id: Some(workflow.id.clone()),
        job_name: Some(bundle.job_name.clone()),
        status: outcome.status.as_str().to_owned(),
        exit_code: outcome.exit_code,
        injected_secrets: outcome.injected_secrets.clone(),
    };
    let dir = run_log::begin_run(&manifest)?;
    if !outcome.stdout.is_empty() {
        run_log::append_stream(&dir, "stdout", &outcome.stdout)?;
    }
    if !outcome.stderr.is_empty() {
        run_log::append_stream(&dir, "stderr", &outcome.stderr)?;
    }
    manifest.status = outcome.status.as_str().to_owned();
    run_log::finish_run(&dir, manifest)?;
    Ok(())
}

fn resolve_job_secrets(
    bundle: &JobBundle,
) -> Result<crate::secretspec_bridge::ResolvedSecrets, BridgeError> {
    if bundle.secret_refs.is_empty() {
        return Ok(crate::secretspec_bridge::ResolvedSecrets::default());
    }
    let cwd = std::env::current_dir().map_err(BridgeError::Io)?;
    let handles = load_secret_handles(&cwd)?;
    resolve_handles(&cwd, &handles, &bundle.secret_refs, None)
}

fn load_workflow(file: &Option<PathBuf>) -> Result<ParsedWorkflow, WorkflowError> {
    let path = resolve_workflow_path(file)?;
    parse_workflow_file(&path)
}

fn resolve_workflow_path(file: &Option<PathBuf>) -> Result<PathBuf, WorkflowError> {
    if let Some(path) = file {
        let path = path.clone();
        if path.is_file() {
            return Ok(path);
        }
        return Err(WorkflowError::FileNotFound { path });
    }

    let cwd = std::env::current_dir().map_err(|error| WorkflowError::ReadFailed {
        path: PathBuf::from(DEFAULT_WORKFLOW_FILE),
        message: error.to_string(),
    })?;
    let default_path = cwd.join(DEFAULT_WORKFLOW_FILE);
    if default_path.is_file() {
        return Ok(default_path);
    }

    Err(WorkflowError::FileNotFound { path: default_path })
}

fn workflow_execution_context() -> PolicyContext {
    if std::env::var_os("SORREL_WORKFLOW_POLICY").as_deref()
        == Some(std::ffi::OsStr::new("restrictive"))
    {
        return PolicyContext {
            repo_id: "repo_mock_local".to_owned(),
            authority_principals: vec![],
            grants: vec![],
            default_rules: vec![],
        };
    }

    let mut context = secret_policy_context().unwrap_or_else(|_| PolicyContext::headless_default());
    context
        .default_rules
        .retain(|rule| rule.action != "workflow.run");

    let principal =
        PrincipalId::parse(CLI_AGENT_PRINCIPAL).expect("CLI agent principal is well-formed");
    context.grants.push(Grant {
        principal,
        capabilities: vec!["workflow.run".to_owned(), "runner.use".to_owned()],
        resources: vec![
            ResourceScope {
                scope: "workflow".to_owned(),
                fields: Default::default(),
            },
            ResourceScope {
                scope: "runner".to_owned(),
                fields: Default::default(),
            },
        ],
        issued_by: None,
    });
    context
}

fn validation_success_json(workflow: &ParsedWorkflow) -> Value {
    json!({
        "command": "workflow validate",
        "mocked": false,
        "status": "valid",
        "workflow": workflow_report_json(workflow)
    })
}

fn workflow_error_output(command: &str, error: &WorkflowError) -> CommandOutput {
    CommandOutput {
        json: json!({
            "command": command,
            "mocked": false,
            "status": "invalid",
            "error": workflow_error_json(error)
        }),
        human: format!("Workflow command failed: {error}"),
    }
}

fn missing_workflow_file_output(command: &str, path: &Path) -> CommandOutput {
    CommandOutput {
        json: json!({
            "command": command,
            "mocked": false,
            "status": "not_found",
            "error": {
                "kind": "workflow_file_not_found",
                "path": path.display().to_string()
            }
        }),
        human: format!("Workflow file not found: {}", path.display()),
    }
}

fn missing_job_output(workflow: &ParsedWorkflow, job_name: &str) -> CommandOutput {
    CommandOutput {
        json: json!({
            "command": "workflow run",
            "mocked": false,
            "status": "not_found",
            "workflow": workflow_summary_json(workflow),
            "error": {
                "kind": "job_not_found",
                "job": job_name,
                "availableJobs": workflow.job_names()
            }
        }),
        human: format!(
            "Job `{job_name}` was not found in workflow `{}`",
            workflow.id
        ),
    }
}

fn policy_denial_output(
    workflow: &ParsedWorkflow,
    bundle: &JobBundle,
    denial: &crate::cli_runner::PolicyGateError,
) -> CommandOutput {
    CommandOutput {
        json: json!({
            "command": "workflow run",
            "mocked": false,
            "status": "denied",
            "workflow": workflow_summary_json(workflow),
            "job": {
                "name": bundle.job_name,
                "command": bundle.command
            },
            "bundle": bundle_json(bundle),
            "decision": {
                "action": denial.action,
                "result": denial.result,
                "reason": denial.reason,
                "resource": {
                    "type": denial.resource_type,
                    "ref": denial.resource_ref
                }
            }
        }),
        human: format!(
            "Policy denied workflow run for job {}: {}",
            bundle.job_name, denial.reason
        ),
    }
}

fn run_success_json(
    workflow: &ParsedWorkflow,
    bundle: &JobBundle,
    outcome: &crate::cli_runner::RunOutcome,
) -> Value {
    json!({
        "command": "workflow run",
        "mocked": false,
        "status": outcome.status.as_str(),
        "backend": outcome.backend,
        "workflow": workflow_summary_json(workflow),
        "job": {
            "name": bundle.job_name,
            "status": outcome.status.as_str(),
            "command": bundle.command,
            "exitCode": outcome.exit_code,
            "stdout": outcome.stdout,
            "stderr": outcome.stderr,
            "injectedSecrets": outcome.injected_secrets
        },
        "bundle": bundle_json(bundle)
    })
}

fn workflow_report_json(workflow: &ParsedWorkflow) -> Value {
    let mut report = workflow_summary_json(workflow);
    if let Some(object) = report.as_object_mut() {
        object.insert(
            "jobs".to_owned(),
            json!(workflow
                .job_names()
                .into_iter()
                .filter_map(|name| workflow.job(&name).map(|job| {
                    json!({
                        "name": job.name,
                        "command": job.command,
                        "shell": job.shell,
                        "secretRefs": job.secret_refs
                    })
                }))
                .collect::<Vec<_>>()),
        );
        if let Some(path) = &workflow.source_path {
            object.insert("path".to_owned(), json!(path.display().to_string()));
        }
    }
    report
}

fn workflow_summary_json(workflow: &ParsedWorkflow) -> Value {
    json!({
        "id": workflow.id,
        "version": workflow.version
    })
}

fn bundle_json(bundle: &JobBundle) -> Value {
    json!({
        "workflowId": bundle.workflow_id,
        "jobName": bundle.job_name,
        "runnerId": bundle.runner_id,
        "command": bundle.command,
        "shell": bundle.shell,
        "secretRefs": bundle.secret_refs,
        "environment": bundle.environment
    })
}

fn workflow_error_json(error: &WorkflowError) -> Value {
    let kind = match error {
        WorkflowError::FileNotFound { .. } => "workflow_file_not_found",
        WorkflowError::ReadFailed { .. } => "workflow_read_failed",
        WorkflowError::ParseFailed { .. } => "workflow_parse_failed",
        WorkflowError::InvalidDocument { .. } => "workflow_invalid_document",
    };

    json!({
        "kind": kind,
        "message": error.to_string()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restrictive_policy_mode_denies_without_grants() {
        std::env::set_var("SORREL_WORKFLOW_POLICY", "restrictive");
        let context = workflow_execution_context();
        std::env::remove_var("SORREL_WORKFLOW_POLICY");
        assert!(context.grants.is_empty());
        assert!(context.default_rules.is_empty());
    }
}
