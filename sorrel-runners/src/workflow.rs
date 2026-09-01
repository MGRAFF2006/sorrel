//! Parsing for the simple `sorrel.workflow.yml` file format and conversion
//! into the portable [`JobBundle`] model.
//!
//! A workflow file declares one or more named workflows. Each workflow declares
//! named jobs with a shell `command`, optional `needs` dependencies (within the
//! same workflow), optional `inputs`, optional `platform`, and optional per-job
//! `env`. A selected workflow is converted into a single [`JobBundle`] whose
//! jobs are topologically sorted by their `needs` edges so that dependencies
//! always precede dependents.
//!
//! The conversion keeps the portable security model intact: secret references
//! remain [`EnvValue::SecretRef`] / [`ObjectRef`] values and are never inlined
//! as raw secrets. The bundle always declares [`CAPABILITY_RUNNER_USE`] and,
//! because every workflow run targets a `Workflow` resource, it additionally
//! declares [`CAPABILITY_WORKFLOW_RUN`] and sets the bundle `workflow`
//! [`ObjectRef`] so the existing Core authorization path runs.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    CAPABILITY_RUNNER_USE, CAPABILITY_WORKFLOW_RUN, CommandSpec, EnvValue, Job, JobBundle,
    JobInput, ObjectRef, PrincipalContext, RunnerError, Shell, WORKFLOW_KIND,
};

/// Errors produced while parsing or converting a workflow file.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum WorkflowError {
    /// The workflow file contained no workflows.
    #[error("workflow file must declare at least one workflow")]
    EmptyFile,
    /// The requested workflow name was not present in the file.
    #[error("workflow `{0}` was not found in the workflow file")]
    UnknownWorkflow(String),
    /// The selected workflow declared no jobs.
    #[error("workflow `{0}` must declare at least one job")]
    EmptyWorkflow(String),
    /// A job declared an empty (whitespace-only) command.
    #[error("job `{0}` must declare a non-empty command")]
    EmptyCommand(String),
    /// Two jobs in the same workflow shared an id.
    #[error("workflow `{workflow}` declares duplicate job id `{job}`")]
    DuplicateJob {
        /// The owning workflow name.
        workflow: String,
        /// The duplicated job id.
        job: String,
    },
    /// A `needs` entry referenced a job that does not exist in the workflow.
    #[error("job `{job}` needs unknown job `{needed}`")]
    UnknownNeeds {
        /// The job declaring the dependency.
        job: String,
        /// The unresolved dependency target.
        needed: String,
    },
    /// The `needs` edges formed a cycle, so no topological order exists.
    #[error("workflow `{workflow}` has a cyclic `needs` dependency involving job `{job}`")]
    CyclicNeeds {
        /// The owning workflow name.
        workflow: String,
        /// A job participating in the cycle.
        job: String,
    },
}

/// A parsed `sorrel.workflow.yml` file.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowFile {
    /// The workflow file format version (currently always `1`).
    pub version: u32,
    /// Named workflows declared by the file.
    #[serde(default)]
    pub workflows: BTreeMap<String, WorkflowSpec>,
}

/// A single named workflow with its jobs.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowSpec {
    /// Named jobs belonging to this workflow.
    #[serde(default)]
    pub jobs: BTreeMap<String, WorkflowJob>,
}

/// A single job within a workflow.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowJob {
    /// Shell command line executed for this job.
    pub command: String,
    /// Ids of jobs (in the same workflow) that must run before this job.
    #[serde(default)]
    pub needs: Vec<String>,
    /// Input path patterns the job depends on.
    #[serde(default)]
    pub inputs: Vec<String>,
    /// Platform requirements for the job.
    #[serde(default)]
    pub platform: Option<WorkflowPlatform>,
    /// Per-job environment values. Secret refs stay portable; literals inline.
    #[serde(default)]
    pub env: BTreeMap<String, WorkflowEnvValue>,
}

/// Platform requirements declared by a workflow job.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowPlatform {
    /// Runtime the job targets (for example `node` or `container`).
    #[serde(default)]
    pub runtime: Option<String>,
    /// Operating system constraint (for example `any` or `linux`).
    #[serde(default)]
    pub os: Option<String>,
    /// CPU architecture constraint (for example `any` or `arm64`).
    #[serde(default)]
    pub arch: Option<String>,
    /// Container image when `runtime` is `container`.
    #[serde(default)]
    pub image: Option<String>,
}

/// A per-job environment value declared in a workflow file.
///
/// Literal values are inlined; secret references stay portable and carry only
/// the secret object kind and id, never a raw secret value.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum WorkflowEnvValue {
    /// A literal string value.
    Literal(String),
    /// A reference to a secret resolved out-of-band by the vault/runner.
    Secret {
        /// The referenced secret object.
        secret: ObjectRef,
    },
}

/// The container runtime token used in `platform.runtime`.
const RUNTIME_CONTAINER: &str = "container";

impl WorkflowFile {
    /// Parses a workflow file from YAML source.
    ///
    /// Returns [`RunnerError::WorkflowParse`] on malformed YAML and
    /// [`WorkflowError::EmptyFile`] when no workflows are declared.
    pub fn from_yaml(source: &str) -> crate::Result<Self> {
        let file: WorkflowFile = serde_yaml_ng::from_str(source)
            .map_err(|error| RunnerError::WorkflowParse(error.to_string()))?;
        if file.workflows.is_empty() {
            return Err(WorkflowError::EmptyFile.into());
        }
        Ok(file)
    }

    /// Converts the named workflow into a single portable [`JobBundle`].
    ///
    /// Jobs are returned in deterministic topologically-sorted order based on
    /// their `needs` edges. Errors are returned for an unknown workflow name,
    /// an empty workflow, empty commands, duplicate job ids, unknown `needs`
    /// targets, and cyclic `needs`.
    pub fn to_bundle(&self, workflow_name: &str) -> crate::Result<JobBundle> {
        let workflow = self
            .workflows
            .get(workflow_name)
            .ok_or_else(|| WorkflowError::UnknownWorkflow(workflow_name.to_owned()))?;

        if workflow.jobs.is_empty() {
            return Err(WorkflowError::EmptyWorkflow(workflow_name.to_owned()).into());
        }

        let order = topological_order(workflow_name, &workflow.jobs)?;

        let mut jobs = Vec::with_capacity(order.len());
        for job_id in &order {
            let spec = &workflow.jobs[job_id];
            jobs.push(convert_job(job_id, spec)?);
        }

        let workflow_ref = ObjectRef::new(WORKFLOW_KIND, workflow_name);
        let bundle = JobBundle {
            schema_version: crate::PROTOCOL_VERSION.to_owned(),
            kind: crate::BUNDLE_KIND.to_owned(),
            id: format!("workflow_{workflow_name}"),
            workflow: Some(workflow_ref.clone()),
            required_capabilities: vec![
                CAPABILITY_RUNNER_USE.to_owned(),
                CAPABILITY_WORKFLOW_RUN.to_owned(),
            ],
            principal: PrincipalContext {
                workflow: Some(workflow_ref),
                ..PrincipalContext::default()
            },
            runner_requirements: crate::RunnerRequirements::default(),
            secret_refs: Vec::new(),
            policy_decisions: Vec::new(),
            redaction: crate::RedactionMetadata::default(),
            jobs,
            metadata: BTreeMap::new(),
        };

        Ok(bundle)
    }
}

/// Converts a single workflow job into a portable [`Job`].
fn convert_job(job_id: &str, spec: &WorkflowJob) -> crate::Result<Job> {
    if spec.command.trim().is_empty() {
        return Err(WorkflowError::EmptyCommand(job_id.to_owned()).into());
    }

    let mut env = BTreeMap::new();
    let mut secret_refs = Vec::new();
    for (key, value) in &spec.env {
        match value {
            WorkflowEnvValue::Literal(value) => {
                env.insert(
                    key.clone(),
                    EnvValue::Literal {
                        value: value.clone(),
                    },
                );
            }
            WorkflowEnvValue::Secret { secret } => {
                if !secret_refs.iter().any(|existing| existing == secret) {
                    secret_refs.push(secret.clone());
                }
                env.insert(
                    key.clone(),
                    EnvValue::SecretRef {
                        secret: secret.clone(),
                    },
                );
            }
        }
    }

    let inputs = spec
        .inputs
        .iter()
        .map(|pattern| JobInput::LocalPath {
            name: pattern.clone(),
            path: std::path::PathBuf::from(pattern),
            read_only: true,
        })
        .collect();

    let mut job = Job {
        id: job_id.to_owned(),
        name: None,
        command: CommandSpec::Shell {
            shell: Shell::Sh,
            command: spec.command.clone(),
        },
        working_directory: None,
        inputs,
        env,
        secret_refs,
    };

    if let Some(platform) = &spec.platform {
        annotate_platform(&mut job, platform);
    }

    Ok(job)
}

/// Records platform requirements as job name metadata.
///
/// Container platforms (`runtime: container`) capture their image so callers
/// can route the job to a [`crate::ContainerRunner`]; the local process runner
/// ignores this metadata and still executes the shell command.
fn annotate_platform(job: &mut Job, platform: &WorkflowPlatform) {
    if platform.runtime.as_deref() == Some(RUNTIME_CONTAINER) {
        if let Some(image) = &platform.image {
            job.name = Some(format!("container:{image}"));
        }
    }
}

/// Returns a deterministic topological ordering of the workflow's jobs.
///
/// The order is stable: ready jobs are emitted in ascending id order. Errors
/// are returned for duplicate job ids (already deduplicated by the parsed map,
/// but `needs` lists are validated), unknown `needs` targets, and cycles.
fn topological_order(
    workflow_name: &str,
    jobs: &BTreeMap<String, WorkflowJob>,
) -> crate::Result<Vec<String>> {
    // Validate that every `needs` target exists and detect intra-list dupes.
    for (job_id, spec) in jobs {
        let mut seen = BTreeSet::new();
        for needed in &spec.needs {
            if needed == job_id || !seen.insert(needed.clone()) {
                return Err(WorkflowError::DuplicateJob {
                    workflow: workflow_name.to_owned(),
                    job: needed.clone(),
                }
                .into());
            }
            if !jobs.contains_key(needed) {
                return Err(WorkflowError::UnknownNeeds {
                    job: job_id.clone(),
                    needed: needed.clone(),
                }
                .into());
            }
        }
    }

    // Kahn's algorithm with a deterministic (BTree-ordered) ready set.
    // indegree[job] = number of prerequisites the job has (its `needs` count).
    let indegree_seed: BTreeMap<&str, usize> = jobs
        .iter()
        .map(|(id, spec)| (id.as_str(), spec.needs.len()))
        .collect();
    let mut indegree = indegree_seed;

    let mut ready: BTreeSet<&str> = indegree
        .iter()
        .filter(|(_, degree)| **degree == 0)
        .map(|(id, _)| *id)
        .collect();

    // Build reverse edges: needed -> dependents.
    let mut dependents: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for (job_id, spec) in jobs {
        for needed in &spec.needs {
            dependents
                .entry(needed.as_str())
                .or_default()
                .push(job_id.as_str());
        }
    }

    let mut order = Vec::with_capacity(jobs.len());
    while let Some(&next) = ready.iter().next() {
        ready.remove(next);
        order.push(next.to_owned());
        if let Some(children) = dependents.get(next) {
            for &child in children {
                let degree = indegree.get_mut(child).unwrap();
                *degree -= 1;
                if *degree == 0 {
                    ready.insert(child);
                }
            }
        }
    }

    if order.len() != jobs.len() {
        // A job that never reached indegree 0 participates in a cycle.
        let stuck = jobs
            .keys()
            .find(|id| !order.iter().any(|emitted| emitted == *id))
            .cloned()
            .unwrap_or_default();
        return Err(WorkflowError::CyclicNeeds {
            workflow: workflow_name.to_owned(),
            job: stuck,
        }
        .into());
    }

    Ok(order)
}
