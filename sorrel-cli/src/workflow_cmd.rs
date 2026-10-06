use std::path::{Path, PathBuf};

use crate::cli_policy::{Grant, PolicyContext, PrincipalId, ResourceScope};
use crate::cli_runner::{parse_workflow_file_selected, ParsedWorkflow, RunError, WorkflowError};
use crate::cli_runner::{CorePermissionEvaluator, JobBundle, LocalProcessRunner, RunStatus};
use crate::env_cmd::{select_backend, try_devenv_run, RunnerBackendKind};
use crate::run_log::{self, RunManifest};
use crate::secretspec_bridge::{
    load_secret_handles, redact_text, resolve_handles, secret_policy_context_for, BridgeError,
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

    /// Named workflow in a canonical workflow file.
    #[arg(long)]
    pub workflow: Option<String>,
}

#[derive(Debug, Args)]
pub struct WorkflowRunJobArgs {
    /// Job name to run from the workflow file.
    pub job_name: String,

    /// Path to a workflow file. Defaults to ./sorrel.workflow.yml.
    #[arg(long)]
    pub file: Option<PathBuf>,

    /// Named workflow in a canonical workflow file.
    #[arg(long)]
    pub workflow: Option<String>,
}

pub fn workflow_validate_output(args: WorkflowFileArgs) -> CommandOutput {
    match load_workflow(&args.file, args.workflow.as_deref()) {
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
    let workflow = match load_workflow(&args.file, args.workflow.as_deref()) {
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
    let context = match workflow_execution_context(Some(&bundle)) {
        Ok(context) => context,
        Err(error) => {
            return CommandOutput {
                json: json!({"command": "workflow run", "mocked": false, "status": "failed", "error": {"kind": "policy_load_failed", "message": error.to_string()}}),
                human: format!("Cannot load workflow policy: {error}"),
            }
        }
    };
    let evaluator = CorePermissionEvaluator {
        context: &context,
        principal,
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
            let run_id = match persist_run(&workflow, &bundle, &outcome) {
                Ok(id) => id,
                Err(error) => {
                    let mut json = run_success_json(&workflow, &bundle, &outcome);
                    json["status"] = json!("failed");
                    json["error"] = json!({"kind": "run_log_failed", "message": error.to_string()});
                    return CommandOutput {
                        json,
                        human: format!(
                            "Workflow {} job {} {}; failed to save run log: {error}",
                            workflow.id,
                            bundle.job_name,
                            outcome.status.as_str()
                        ),
                    };
                }
            };
            let mut json = run_success_json(&workflow, &bundle, &outcome);
            let mut human = String::new();
            for stream in [&outcome.stdout, &outcome.stderr] {
                if !stream.is_empty() {
                    human.push_str(stream);
                    if !stream.ends_with('\n') {
                        human.push('\n');
                    }
                }
            }
            human.push_str(&format!(
                "Workflow {} job {} {} (backend: {})",
                workflow.id,
                bundle.job_name,
                outcome.status.as_str(),
                outcome.backend
            ));
            if let Some(id) = run_id {
                json["runId"] = json!(id);
                human.push_str(&format!("\nLogs: sorrel run logs {id}"));
            }
            CommandOutput { json, human }
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
    let simple_job = bundle
        .native
        .as_ref()
        .is_none_or(|native| native.jobs.len() == 1 && native.jobs[0].env.is_empty());
    if simple_job
        && secret_env.values.is_empty()
        && select_backend(&cwd) == RunnerBackendKind::Devenv
    {
        match try_devenv_run(&cwd, &bundle.command) {
            Ok(Some(devenv)) => {
                let redaction = bundle
                    .native
                    .as_ref()
                    .map(|native| native.redaction.clone())
                    .unwrap_or_default();
                return Ok(crate::cli_runner::RunOutcome {
                    status: if devenv.success {
                        RunStatus::Completed
                    } else {
                        RunStatus::Failed
                    },
                    exit_code: devenv.exit_code,
                    stdout: sorrel_runners::redact_inherited_env(&devenv.stdout, &redaction),
                    stderr: sorrel_runners::redact_inherited_env(&devenv.stderr, &redaction),
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

    let mut env = secret_env.values.clone();
    let mut bundle = bundle.clone();
    if let Some(native) = &mut bundle.native {
        for job in &mut native.jobs {
            for secret in &job.secret_refs {
                if let Some(name) = secret_env.id_to_name.get(&secret.id) {
                    job.env.entry(name.clone()).or_insert_with(|| {
                        sorrel_runners::EnvValue::SecretRef {
                            secret: secret.clone(),
                        }
                    });
                }
            }
            for (name, value) in &job.env {
                if let sorrel_runners::EnvValue::SecretRef { secret } = value {
                    let provider_name = secret_env.id_to_name.get(&secret.id).unwrap_or(&secret.id);
                    let value = secret_env.values.get(provider_name).ok_or_else(|| {
                        RunError::SecretResolve {
                            message: format!("missing resolved secret {}", secret.id),
                        }
                    })?;
                    env.insert(name.clone(), value.clone());
                }
            }
        }
        if !env.is_empty()
            && !native
                .required_capabilities
                .iter()
                .any(|capability| capability == "secret.inject")
        {
            native
                .required_capabilities
                .push("secret.inject".to_owned());
        }
    }
    LocalProcessRunner.run_with_env(&bundle, evaluator, env)
}

fn persist_run(
    workflow: &ParsedWorkflow,
    bundle: &JobBundle,
    outcome: &crate::cli_runner::RunOutcome,
) -> std::io::Result<Option<String>> {
    if !crate::repo::is_initialized() {
        return Ok(None);
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
    Ok(Some(id))
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

fn load_workflow(
    file: &Option<PathBuf>,
    selected: Option<&str>,
) -> Result<ParsedWorkflow, WorkflowError> {
    let path = resolve_workflow_path(file)?;
    parse_workflow_file_selected(&path, selected)
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

fn workflow_execution_context(bundle: Option<&JobBundle>) -> Result<PolicyContext, BridgeError> {
    if std::env::var_os("SORREL_WORKFLOW_POLICY").as_deref()
        == Some(std::ffi::OsStr::new("restrictive"))
    {
        return Ok(PolicyContext {
            repo_id: "repo_mock_local".to_owned(),
            authority_principals: vec![],
            grants: vec![],
            default_rules: vec![],
        });
    }

    let grants = crate::repo::registry_dir(crate::repo::GRANTS_DIR);
    if grants.try_exists()? && !grants.is_dir() {
        return Err(BridgeError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "workspace grant registry is not a directory",
        )));
    }
    if let Some(bundle) = bundle.filter(|bundle| !bundle.secret_refs.is_empty()) {
        let handles = load_secret_handles(&std::env::current_dir()?)?;
        for id in &bundle.secret_refs {
            let handle = handles
                .iter()
                .find(|handle| handle.id == *id)
                .ok_or_else(|| BridgeError::NotFound(id.clone()))?;
            if bundle.environment.as_deref() != Some(handle.environment.as_str()) {
                return Err(BridgeError::Policy {
                    action: "secret.read".to_owned(),
                    secret_id: id.clone(),
                    reason:
                        "SecretRef environment does not match the workflow execution environment"
                            .to_owned(),
                    result: "deny".to_owned(),
                });
            }
        }
    }
    let mut context = secret_policy_context_for(
        bundle.and_then(|bundle| bundle.environment.as_deref()),
        bundle.map(|bundle| bundle.workflow_id.as_str()),
        bundle.map(|bundle| bundle.runner_id.as_str()),
    )?;
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
    Ok(context)
}

/// Process status for workflow commands, after their structured output is printed.
pub fn exit_code(output: &CommandOutput) -> u8 {
    if matches!(output.json["status"].as_str(), Some("valid" | "completed")) {
        return 0;
    }
    output
        .json
        .pointer("/job/exitCode")
        .and_then(Value::as_u64)
        .and_then(|code| u8::try_from(code).ok())
        .filter(|code| *code != 0)
        .unwrap_or(1)
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
        let context = workflow_execution_context(None).unwrap();
        std::env::remove_var("SORREL_WORKFLOW_POLICY");
        assert!(context.grants.is_empty());
        assert!(context.default_rules.is_empty());
    }
}
