use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    process::{Command, Output},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub mod workflow;

pub const WORKFLOW_KIND: &str = "Workflow";

pub const PROTOCOL_VERSION: &str = "sorrel.protocol.v0";
pub const BUNDLE_KIND: &str = "JobBundle";
pub const RUNNER_KIND: &str = "Runner";
pub const LOG_ARTIFACT_FORMAT: &str = "application/vnd.sorrel.runner.log+jsonl;version=0";
pub const CAPABILITY_RUNNER_USE: &str = "runner.use";
pub const CAPABILITY_WORKFLOW_RUN: &str = "workflow.run";
pub const CAPABILITY_SECRET_READ: &str = "secret.read";
pub const CAPABILITY_SECRET_INJECT: &str = "secret.inject";

#[derive(Debug, Error)]
pub enum RunnerError {
    #[error("job bundle must contain at least one job")]
    EmptyBundle,
    #[error("job id must not be empty")]
    EmptyJobId,
    #[error("command argv must contain at least one element")]
    EmptyArgv,
    #[error("shell command must not be empty")]
    EmptyShellCommand,
    #[error("runner capability descriptor is invalid: {0}")]
    InvalidCapabilities(String),
    #[error("job bundle is missing required capability declaration: {0}")]
    MissingRequiredCapability(String),
    #[error("required capability must not be empty")]
    EmptyRequiredCapability,
    #[error("job uses secret ref not declared by bundle: {0}")]
    UndeclaredSecretRef(String),
    #[error("runner does not satisfy bundle requirement: {0}")]
    RunnerRequirementUnsatisfied(String),
    #[error("core policy decision {status:?} for {capability}: {reason}")]
    PolicyDenied {
        capability: String,
        status: PolicyDecisionStatus,
        reason: String,
    },
    #[error("secret references cannot be injected by this runner prototype: {0}")]
    SecretInjectionUnsupported(String),
    #[error("failed to spawn command `{program}`: {source}")]
    Spawn {
        program: String,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to encode or decode runner JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("failed to parse workflow file: {0}")]
    WorkflowParse(String),
    #[error("invalid workflow definition: {0}")]
    Workflow(#[from] workflow::WorkflowError),
}

pub type Result<T> = std::result::Result<T, RunnerError>;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ObjectRef {
    pub kind: String,
    pub id: String,
}

impl ObjectRef {
    pub fn new(kind: impl Into<String>, id: impl Into<String>) -> Self {
        Self {
            kind: kind.into(),
            id: id.into(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrincipalContext {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent: Option<ObjectRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workflow: Option<ObjectRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runner: Option<ObjectRef>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PolicyDecisionStatus {
    Allow,
    Deny,
    NeedsGrant,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PolicyDecision {
    #[serde(default = "default_protocol_version")]
    pub schema_version: String,
    #[serde(default = "default_policy_decision_kind")]
    pub kind: String,
    pub status: PolicyDecisionStatus,
    pub capability: String,
    pub resource: ObjectRef,
    pub principal: PrincipalContext,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: BTreeMap<String, String>,
}

impl PolicyDecision {
    pub fn allow(
        capability: impl Into<String>,
        principal: PrincipalContext,
        resource: ObjectRef,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            schema_version: PROTOCOL_VERSION.to_owned(),
            kind: "PolicyDecision".to_owned(),
            status: PolicyDecisionStatus::Allow,
            capability: capability.into(),
            resource,
            principal,
            reason: Some(reason.into()),
            metadata: BTreeMap::new(),
        }
    }

    pub fn needs_grant(
        capability: impl Into<String>,
        principal: PrincipalContext,
        resource: ObjectRef,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            schema_version: PROTOCOL_VERSION.to_owned(),
            kind: "PolicyDecision".to_owned(),
            status: PolicyDecisionStatus::NeedsGrant,
            capability: capability.into(),
            resource,
            principal,
            reason: Some(reason.into()),
            metadata: BTreeMap::new(),
        }
    }
}

/// Evaluates Core permissions for a `(principal, capability, resource)` tuple.
///
/// Runners must consult a Core-backed evaluator before execution. Bundle-attached
/// `policyDecisions` are audit metadata only and are not trusted on their own.
pub trait CorePermissionEvaluator {
    fn evaluate(
        &self,
        principal: &PrincipalContext,
        capability: &str,
        resource: &ObjectRef,
    ) -> PolicyDecision;
}

/// In-memory grant store mirroring sorrel-core's permission evaluator output.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GrantStoreEvaluator {
    grants: Vec<PolicyDecision>,
}

impl GrantStoreEvaluator {
    pub fn from_grants(grants: impl IntoIterator<Item = PolicyDecision>) -> Self {
        Self {
            grants: grants.into_iter().collect(),
        }
    }

    pub fn deny_all() -> Self {
        Self::default()
    }
}

impl CorePermissionEvaluator for GrantStoreEvaluator {
    fn evaluate(
        &self,
        principal: &PrincipalContext,
        capability: &str,
        resource: &ObjectRef,
    ) -> PolicyDecision {
        self.grants
            .iter()
            .find(|decision| {
                decision.capability == capability
                    && decision.resource == *resource
                    && decision.principal == *principal
            })
            .cloned()
            .unwrap_or_else(|| {
                PolicyDecision::needs_grant(
                    capability,
                    principal.clone(),
                    resource.clone(),
                    "missing Core policy decision",
                )
            })
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunnerRequirements {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runner: Option<ObjectRef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub labels: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub platform_capabilities: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secret_handling: Option<SecretHandling>,
}

impl RunnerRequirements {
    fn is_empty(&self) -> bool {
        self.runner.is_none()
            && self.labels.is_empty()
            && self.platform_capabilities.is_empty()
            && self.secret_handling.is_none()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RedactionMetadata {
    #[serde(default = "default_protocol_version")]
    pub schema_version: String,
    #[serde(default = "default_redaction_kind")]
    pub kind: String,
    pub strategy: String,
    pub mask: String,
    pub min_secret_length: usize,
    pub visible_prefix: usize,
    pub visible_suffix: usize,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub detect_env_keys: Vec<String>,
}

impl Default for RedactionMetadata {
    fn default() -> Self {
        Self {
            schema_version: PROTOCOL_VERSION.to_owned(),
            kind: "RedactionMetadata".to_owned(),
            strategy: "mask".to_owned(),
            mask: "***".to_owned(),
            min_secret_length: 6,
            visible_prefix: 0,
            visible_suffix: 0,
            detect_env_keys: vec![
                "TOKEN".to_owned(),
                "SECRET".to_owned(),
                "PASSWORD".to_owned(),
                "KEY".to_owned(),
            ],
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobBundle {
    #[serde(default = "default_protocol_version")]
    pub schema_version: String,
    #[serde(default = "default_bundle_kind")]
    pub kind: String,
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workflow: Option<ObjectRef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required_capabilities: Vec<String>,
    #[serde(default)]
    pub principal: PrincipalContext,
    #[serde(default, skip_serializing_if = "RunnerRequirements::is_empty")]
    pub runner_requirements: RunnerRequirements,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub secret_refs: Vec<ObjectRef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub policy_decisions: Vec<PolicyDecision>,
    #[serde(default)]
    pub redaction: RedactionMetadata,
    pub jobs: Vec<Job>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: BTreeMap<String, String>,
}

impl JobBundle {
    pub fn single(id: impl Into<String>, job: Job) -> Self {
        Self {
            schema_version: PROTOCOL_VERSION.to_owned(),
            kind: BUNDLE_KIND.to_owned(),
            id: id.into(),
            workflow: None,
            required_capabilities: vec![CAPABILITY_RUNNER_USE.to_owned()],
            principal: PrincipalContext::default(),
            runner_requirements: RunnerRequirements::default(),
            secret_refs: Vec::new(),
            policy_decisions: Vec::new(),
            redaction: RedactionMetadata::default(),
            jobs: vec![job],
            metadata: BTreeMap::new(),
        }
    }

    pub fn validate(&self) -> Result<()> {
        if self.jobs.is_empty() {
            return Err(RunnerError::EmptyBundle);
        }
        if !self
            .required_capabilities
            .iter()
            .any(|capability| capability == CAPABILITY_RUNNER_USE)
        {
            return Err(RunnerError::MissingRequiredCapability(
                CAPABILITY_RUNNER_USE.to_owned(),
            ));
        }
        for capability in &self.required_capabilities {
            if capability.trim().is_empty() {
                return Err(RunnerError::EmptyRequiredCapability);
            }
        }
        if self.workflow.is_some()
            && !self
                .required_capabilities
                .iter()
                .any(|capability| capability == CAPABILITY_WORKFLOW_RUN)
        {
            return Err(RunnerError::MissingRequiredCapability(
                CAPABILITY_WORKFLOW_RUN.to_owned(),
            ));
        }
        validate_secret_declarations(self)?;

        for job in &self.jobs {
            job.validate()?;
        }

        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Job {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub command: CommandSpec,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub working_directory: Option<PathBuf>,
    #[serde(default)]
    pub inputs: Vec<JobInput>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, EnvValue>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub secret_refs: Vec<ObjectRef>,
}

impl Job {
    pub fn shell(
        id: impl Into<String>,
        shell: Shell,
        command: impl Into<String>,
        working_directory: impl Into<Option<PathBuf>>,
    ) -> Self {
        Self {
            id: id.into(),
            name: None,
            command: CommandSpec::Shell {
                shell,
                command: command.into(),
            },
            working_directory: working_directory.into(),
            inputs: Vec::new(),
            env: BTreeMap::new(),
            secret_refs: Vec::new(),
        }
    }

    pub fn exec(
        id: impl Into<String>,
        argv: impl IntoIterator<Item = impl Into<String>>,
        working_directory: impl Into<Option<PathBuf>>,
    ) -> Self {
        Self {
            id: id.into(),
            name: None,
            command: CommandSpec::Exec {
                argv: argv.into_iter().map(Into::into).collect(),
            },
            working_directory: working_directory.into(),
            inputs: Vec::new(),
            env: BTreeMap::new(),
            secret_refs: Vec::new(),
        }
    }

    pub fn validate(&self) -> Result<()> {
        if self.id.trim().is_empty() {
            return Err(RunnerError::EmptyJobId);
        }
        self.command.validate()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum CommandSpec {
    Shell { shell: Shell, command: String },
    Exec { argv: Vec<String> },
}

impl CommandSpec {
    pub fn validate(&self) -> Result<()> {
        match self {
            Self::Shell { command, .. } if command.trim().is_empty() => {
                Err(RunnerError::EmptyShellCommand)
            }
            Self::Exec { argv } if argv.is_empty() || argv[0].trim().is_empty() => {
                Err(RunnerError::EmptyArgv)
            }
            _ => Ok(()),
        }
    }

    fn to_program_and_args(&self) -> Result<(String, Vec<String>)> {
        self.validate()?;

        match self {
            Self::Shell { shell, command } => Ok(shell.program_and_args(command)),
            Self::Exec { argv } => Ok((argv[0].clone(), argv[1..].to_vec())),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Shell {
    Sh,
    Bash,
    Pwsh,
    Cmd,
}

impl Shell {
    fn program_and_args(self, command: &str) -> (String, Vec<String>) {
        match self {
            Self::Sh => ("sh".to_owned(), vec!["-c".to_owned(), command.to_owned()]),
            Self::Bash => (
                "bash".to_owned(),
                vec!["-lc".to_owned(), command.to_owned()],
            ),
            Self::Pwsh => (
                "pwsh".to_owned(),
                vec![
                    "-NoLogo".to_owned(),
                    "-Command".to_owned(),
                    command.to_owned(),
                ],
            ),
            Self::Cmd => ("cmd".to_owned(), vec!["/C".to_owned(), command.to_owned()]),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum JobInput {
    ContentAddressed {
        name: String,
        object: ObjectRef,
        #[serde(skip_serializing_if = "Option::is_none")]
        mount_path: Option<PathBuf>,
    },
    LocalPath {
        name: String,
        path: PathBuf,
        #[serde(default)]
        read_only: bool,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum EnvValue {
    Literal { value: String },
    SecretRef { secret: ObjectRef },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunnerCapabilities {
    #[serde(default = "default_protocol_version")]
    pub schema_version: String,
    #[serde(default = "default_runner_kind")]
    pub kind: String,
    pub id: String,
    pub name: String,
    pub mode: RunnerMode,
    pub runner_type: RunnerType,
    pub platform: PlatformRequirement,
    pub isolation: Isolation,
    pub max_parallel_jobs: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub labels: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trust: Option<RunnerTrust>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<RunnerStatus>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: BTreeMap<String, String>,
}

impl RunnerCapabilities {
    pub fn local_process(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            schema_version: PROTOCOL_VERSION.to_owned(),
            kind: RUNNER_KIND.to_owned(),
            id: id.into(),
            name: name.into(),
            mode: RunnerMode::Local,
            runner_type: RunnerType::Process,
            platform: PlatformRequirement {
                runtime: "shell".to_owned(),
                image: None,
                os: current_os(),
                arch: current_arch(),
                capabilities: vec![
                    "process".to_owned(),
                    "stdout".to_owned(),
                    "stderr".to_owned(),
                ],
            },
            isolation: Isolation::Host,
            max_parallel_jobs: 1,
            labels: vec!["local".to_owned(), "process".to_owned()],
            endpoint: Some("runner://local/process".to_owned()),
            trust: Some(RunnerTrust {
                identity: None,
                attestation: Attestation::None,
                secret_handling: SecretHandling::None,
            }),
            status: Some(RunnerStatus::Online),
            metadata: BTreeMap::new(),
        }
    }

    pub fn local_container(
        id: impl Into<String>,
        name: impl Into<String>,
        engine: ContainerEngine,
        image: impl Into<String>,
    ) -> Self {
        let image = image.into();
        let engine_label = engine.as_str().to_owned();
        Self {
            schema_version: PROTOCOL_VERSION.to_owned(),
            kind: RUNNER_KIND.to_owned(),
            id: id.into(),
            name: name.into(),
            mode: RunnerMode::Local,
            runner_type: RunnerType::Container,
            platform: PlatformRequirement {
                runtime: "container".to_owned(),
                image: Some(image),
                os: current_os(),
                arch: current_arch(),
                capabilities: vec![
                    engine_label.clone(),
                    "stdout".to_owned(),
                    "stderr".to_owned(),
                ],
            },
            isolation: Isolation::Container,
            max_parallel_jobs: 1,
            labels: vec!["local".to_owned(), "container".to_owned(), engine_label],
            endpoint: Some(format!("runner://local/{}", engine.as_str())),
            trust: Some(RunnerTrust {
                identity: None,
                attestation: Attestation::SelfAttested,
                secret_handling: SecretHandling::None,
            }),
            status: Some(RunnerStatus::Online),
            metadata: BTreeMap::new(),
        }
    }

    pub fn validate(&self) -> Result<()> {
        if self.kind != RUNNER_KIND {
            return Err(RunnerError::InvalidCapabilities(format!(
                "kind must be {RUNNER_KIND}"
            )));
        }
        if self.schema_version != PROTOCOL_VERSION {
            return Err(RunnerError::InvalidCapabilities(format!(
                "schemaVersion must be {PROTOCOL_VERSION}"
            )));
        }
        if self.id.trim().is_empty() {
            return Err(RunnerError::InvalidCapabilities(
                "id must not be empty".to_owned(),
            ));
        }
        if self.name.trim().is_empty() {
            return Err(RunnerError::InvalidCapabilities(
                "name must not be empty".to_owned(),
            ));
        }
        if self.max_parallel_jobs == 0 {
            return Err(RunnerError::InvalidCapabilities(
                "maxParallelJobs must be at least 1".to_owned(),
            ));
        }
        if self.platform.runtime.trim().is_empty() {
            return Err(RunnerError::InvalidCapabilities(
                "platform.runtime must not be empty".to_owned(),
            ));
        }
        if self.platform.os.trim().is_empty() {
            return Err(RunnerError::InvalidCapabilities(
                "platform.os must not be empty".to_owned(),
            ));
        }
        if self.platform.arch.trim().is_empty() {
            return Err(RunnerError::InvalidCapabilities(
                "platform.arch must not be empty".to_owned(),
            ));
        }

        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RunnerMode {
    Local,
    Pull,
    Push,
    Ephemeral,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RunnerType {
    Process,
    Container,
    Ssh,
    Kubernetes,
    GithubActions,
    GitlabCi,
    Buildkite,
    RemoteExecution,
    Wasm,
    Browser,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Isolation {
    Host,
    Container,
    Vm,
    Microvm,
    Wasm,
    Browser,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlatformRequirement {
    pub runtime: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    pub os: String,
    pub arch: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub capabilities: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunnerTrust {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub identity: Option<String>,
    pub attestation: Attestation,
    pub secret_handling: SecretHandling,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Attestation {
    None,
    SelfAttested,
    Oidc,
    Tpm,
    Sigstore,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SecretHandling {
    None,
    Environment,
    File,
    Brokered,
    HardwareBacked,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RunnerStatus {
    Online,
    Offline,
    Draining,
    Disabled,
}

pub trait Runner {
    fn capabilities(&self) -> &RunnerCapabilities;
    fn run(
        &self,
        bundle: &JobBundle,
        policy: &dyn CorePermissionEvaluator,
    ) -> Result<BundleRunResult>;
}

#[derive(Clone, Debug)]
pub struct LocalProcessRunner {
    capabilities: RunnerCapabilities,
}

impl LocalProcessRunner {
    pub fn new(capabilities: RunnerCapabilities) -> Result<Self> {
        capabilities.validate()?;
        Ok(Self { capabilities })
    }

    pub fn default_local() -> Self {
        Self::new(RunnerCapabilities::local_process(
            "runner_local_process",
            "local-process",
        ))
        .expect("default local process capabilities are valid")
    }
}

impl Runner for LocalProcessRunner {
    fn capabilities(&self) -> &RunnerCapabilities {
        &self.capabilities
    }

    fn run(
        &self,
        bundle: &JobBundle,
        policy: &dyn CorePermissionEvaluator,
    ) -> Result<BundleRunResult> {
        bundle.validate()?;
        authorize_job_bundle(bundle, &self.capabilities, policy)?;

        let mut jobs = Vec::with_capacity(bundle.jobs.len());
        for job in &bundle.jobs {
            jobs.push(run_process_job(bundle, job)?);
        }

        Ok(BundleRunResult::from_jobs(bundle.id.clone(), jobs))
    }
}

#[derive(Clone, Debug)]
pub struct ContainerRunner {
    capabilities: RunnerCapabilities,
    engine: ContainerEngine,
    image: String,
}

impl ContainerRunner {
    pub fn new(engine: ContainerEngine, image: impl Into<String>) -> Result<Self> {
        let image = image.into();
        let capabilities = RunnerCapabilities::local_container(
            format!("runner_local_{}", engine.as_str()),
            format!("local-{}", engine.as_str()),
            engine,
            image.clone(),
        );
        Self::with_capabilities(engine, image, capabilities)
    }

    pub fn with_capabilities(
        engine: ContainerEngine,
        image: impl Into<String>,
        capabilities: RunnerCapabilities,
    ) -> Result<Self> {
        capabilities.validate()?;
        Ok(Self {
            capabilities,
            engine,
            image: image.into(),
        })
    }
}

impl Runner for ContainerRunner {
    fn capabilities(&self) -> &RunnerCapabilities {
        &self.capabilities
    }

    fn run(
        &self,
        bundle: &JobBundle,
        policy: &dyn CorePermissionEvaluator,
    ) -> Result<BundleRunResult> {
        bundle.validate()?;
        authorize_job_bundle(bundle, &self.capabilities, policy)?;

        let mut jobs = Vec::with_capacity(bundle.jobs.len());
        for job in &bundle.jobs {
            jobs.push(run_container_job(self.engine, &self.image, bundle, job)?);
        }

        Ok(BundleRunResult::from_jobs(bundle.id.clone(), jobs))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContainerEngine {
    Docker,
    Podman,
}

impl ContainerEngine {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Docker => "docker",
            Self::Podman => "podman",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BundleRunResult {
    pub bundle_id: String,
    pub status: RunStatus,
    pub jobs: Vec<JobRunResult>,
}

impl BundleRunResult {
    fn from_jobs(bundle_id: String, jobs: Vec<JobRunResult>) -> Self {
        let status = if jobs.iter().all(|job| job.status == RunStatus::Succeeded) {
            RunStatus::Succeeded
        } else {
            RunStatus::Failed
        };

        Self {
            bundle_id,
            status,
            jobs,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobRunResult {
    pub job_id: String,
    pub status: RunStatus,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub log: LogArtifact,
    pub redaction: RedactionMetadata,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RunStatus {
    Succeeded,
    Failed,
    Terminated,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogArtifact {
    pub format: String,
    pub records: Vec<LogRecord>,
}

impl LogArtifact {
    pub fn to_json_lines(&self) -> Result<String> {
        let mut lines = String::new();
        for record in &self.records {
            lines.push_str(&serde_json::to_string(record)?);
            lines.push('\n');
        }
        Ok(lines)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum LogRecord {
    Started {
        time_unix_ms: u128,
        job_id: String,
        command: CommandSpec,
    },
    Stream {
        time_unix_ms: u128,
        job_id: String,
        stream: OutputStream,
        text: String,
    },
    Finished {
        time_unix_ms: u128,
        job_id: String,
        status: RunStatus,
        exit_code: Option<i32>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OutputStream {
    Stdout,
    Stderr,
}

fn run_process_job(bundle: &JobBundle, job: &Job) -> Result<JobRunResult> {
    ensure_no_secret_injection(job)?;
    let (program, args) = job.command.to_program_and_args()?;
    let mut command = Command::new(&program);
    command.args(args);

    if let Some(working_directory) = &job.working_directory {
        command.current_dir(working_directory);
    }
    apply_literal_env(job, &mut command)?;

    let started_at = unix_millis();
    let output = command
        .output()
        .map_err(|source| RunnerError::Spawn { program, source })?;
    let finished_at = unix_millis();

    result_from_output(bundle, job, output, started_at, finished_at)
}

fn run_container_job(
    engine: ContainerEngine,
    image: &str,
    bundle: &JobBundle,
    job: &Job,
) -> Result<JobRunResult> {
    ensure_no_secret_injection(job)?;
    let workdir = job
        .working_directory
        .clone()
        .unwrap_or_else(|| PathBuf::from("."));
    let absolute_workdir = absolute_path(&workdir);
    let container_workdir = "/workspace";
    let (inner_program, inner_args) = job.command.to_program_and_args()?;

    let mut args = vec![
        "run".to_owned(),
        "--rm".to_owned(),
        "-w".to_owned(),
        container_workdir.to_owned(),
        "-v".to_owned(),
        format!("{}:{container_workdir}", absolute_workdir.display()),
    ];

    for input in &job.inputs {
        if let JobInput::LocalPath {
            path, read_only, ..
        } = input
        {
            let host_path = absolute_path(path);
            let mount_path = format!("/inputs/{}", sanitize_mount_name(path));
            let mode = if *read_only { ":ro" } else { "" };
            args.push("-v".to_owned());
            args.push(format!("{}:{mount_path}{mode}", host_path.display()));
        }
    }

    for (name, value) in &job.env {
        if let EnvValue::Literal { value } = value {
            args.push("-e".to_owned());
            args.push(format!("{name}={value}"));
        }
    }

    args.push(image.to_owned());
    args.push(inner_program);
    args.extend(inner_args);

    let started_at = unix_millis();
    let output = Command::new(engine.as_str())
        .args(args)
        .output()
        .map_err(|source| RunnerError::Spawn {
            program: engine.as_str().to_owned(),
            source,
        })?;
    let finished_at = unix_millis();

    result_from_output(bundle, job, output, started_at, finished_at)
}

fn result_from_output(
    bundle: &JobBundle,
    job: &Job,
    output: Output,
    started_at: u128,
    finished_at: u128,
) -> Result<JobRunResult> {
    let redaction = build_redaction_context(bundle, job);
    let stdout = redact_text(&String::from_utf8_lossy(&output.stdout), &redaction);
    let stderr = redact_text(&String::from_utf8_lossy(&output.stderr), &redaction);
    let exit_code = output.status.code();
    let status = match exit_code {
        Some(0) => RunStatus::Succeeded,
        Some(_) => RunStatus::Failed,
        None => RunStatus::Terminated,
    };

    let mut records = vec![LogRecord::Started {
        time_unix_ms: started_at,
        job_id: job.id.clone(),
        command: redact_command(&job.command, &redaction),
    }];
    if !stdout.is_empty() {
        records.push(LogRecord::Stream {
            time_unix_ms: finished_at,
            job_id: job.id.clone(),
            stream: OutputStream::Stdout,
            text: stdout.clone(),
        });
    }
    if !stderr.is_empty() {
        records.push(LogRecord::Stream {
            time_unix_ms: finished_at,
            job_id: job.id.clone(),
            stream: OutputStream::Stderr,
            text: stderr.clone(),
        });
    }
    records.push(LogRecord::Finished {
        time_unix_ms: finished_at,
        job_id: job.id.clone(),
        status,
        exit_code,
    });

    Ok(JobRunResult {
        job_id: job.id.clone(),
        status,
        exit_code,
        stdout,
        stderr,
        log: LogArtifact {
            format: LOG_ARTIFACT_FORMAT.to_owned(),
            records,
        },
        redaction: redaction.metadata,
    })
}

/// Authorizes a bundle via Core before any job command is spawned.
pub fn authorize_job_bundle(
    bundle: &JobBundle,
    capabilities: &RunnerCapabilities,
    policy: &dyn CorePermissionEvaluator,
) -> Result<()> {
    validate_runner_requirements(&bundle.runner_requirements, capabilities)?;

    let runner_resource = ObjectRef::new(RUNNER_KIND, capabilities.id.clone());
    authorize_capability(bundle, policy, CAPABILITY_RUNNER_USE, &runner_resource)?;

    if let Some(workflow) = bundle
        .workflow
        .as_ref()
        .or(bundle.principal.workflow.as_ref())
    {
        authorize_capability(bundle, policy, CAPABILITY_WORKFLOW_RUN, workflow)?;
    }

    for secret in secret_read_dependencies(bundle) {
        authorize_capability(bundle, policy, CAPABILITY_SECRET_READ, &secret)?;
    }

    for secret in secret_inject_dependencies(bundle) {
        authorize_capability(bundle, policy, CAPABILITY_SECRET_INJECT, &secret)?;
    }

    Ok(())
}

fn authorize_capability(
    bundle: &JobBundle,
    policy: &dyn CorePermissionEvaluator,
    capability: &str,
    resource: &ObjectRef,
) -> Result<()> {
    let decision = policy.evaluate(&bundle.principal, capability, resource);

    match decision.status {
        PolicyDecisionStatus::Allow => Ok(()),
        PolicyDecisionStatus::Deny | PolicyDecisionStatus::NeedsGrant => {
            Err(RunnerError::PolicyDenied {
                capability: capability.to_owned(),
                status: decision.status,
                reason: decision
                    .reason
                    .unwrap_or_else(|| "Core policy did not allow execution".to_owned()),
            })
        }
    }
}

fn validate_runner_requirements(
    requirements: &RunnerRequirements,
    capabilities: &RunnerCapabilities,
) -> Result<()> {
    if let Some(required_runner) = &requirements.runner {
        let actual = ObjectRef::new(RUNNER_KIND, capabilities.id.clone());
        if *required_runner != actual {
            return Err(RunnerError::RunnerRequirementUnsatisfied(format!(
                "runner must be {}",
                required_runner.id
            )));
        }
    }

    for label in &requirements.labels {
        if !capabilities
            .labels
            .iter()
            .any(|candidate| candidate == label)
        {
            return Err(RunnerError::RunnerRequirementUnsatisfied(format!(
                "missing label {label}"
            )));
        }
    }

    for platform_capability in &requirements.platform_capabilities {
        if !capabilities
            .platform
            .capabilities
            .iter()
            .any(|candidate| candidate == platform_capability)
        {
            return Err(RunnerError::RunnerRequirementUnsatisfied(format!(
                "missing platform capability {platform_capability}"
            )));
        }
    }

    if let Some(required_secret_handling) = requirements.secret_handling {
        let actual_secret_handling = capabilities
            .trust
            .as_ref()
            .map(|trust| trust.secret_handling)
            .unwrap_or(SecretHandling::None);
        if actual_secret_handling != required_secret_handling {
            return Err(RunnerError::RunnerRequirementUnsatisfied(format!(
                "secretHandling must be {required_secret_handling:?}"
            )));
        }
    }

    Ok(())
}

fn validate_secret_declarations(bundle: &JobBundle) -> Result<()> {
    let read_dependencies = secret_read_dependencies(bundle);
    let inject_dependencies = secret_inject_dependencies(bundle);

    if !read_dependencies.is_empty()
        && !bundle
            .required_capabilities
            .iter()
            .any(|capability| capability == CAPABILITY_SECRET_READ)
    {
        return Err(RunnerError::MissingRequiredCapability(
            CAPABILITY_SECRET_READ.to_owned(),
        ));
    }

    if !inject_dependencies.is_empty()
        && !bundle
            .required_capabilities
            .iter()
            .any(|capability| capability == CAPABILITY_SECRET_INJECT)
    {
        return Err(RunnerError::MissingRequiredCapability(
            CAPABILITY_SECRET_INJECT.to_owned(),
        ));
    }

    for secret in job_secret_dependencies(bundle) {
        if !bundle
            .secret_refs
            .iter()
            .any(|declared| declared == &secret)
        {
            return Err(RunnerError::UndeclaredSecretRef(secret.id));
        }
    }

    Ok(())
}

fn secret_read_dependencies(bundle: &JobBundle) -> Vec<ObjectRef> {
    unique_refs(
        bundle
            .secret_refs
            .iter()
            .cloned()
            .chain(job_secret_dependencies(bundle))
            .collect(),
    )
}

fn secret_inject_dependencies(bundle: &JobBundle) -> Vec<ObjectRef> {
    unique_refs(
        bundle
            .jobs
            .iter()
            .flat_map(|job| {
                job.env.values().filter_map(|value| match value {
                    EnvValue::SecretRef { secret } => Some(secret.clone()),
                    EnvValue::Literal { .. } => None,
                })
            })
            .collect(),
    )
}

fn job_secret_dependencies(bundle: &JobBundle) -> Vec<ObjectRef> {
    unique_refs(
        bundle
            .jobs
            .iter()
            .flat_map(|job| {
                job.secret_refs
                    .iter()
                    .cloned()
                    .chain(job.env.values().filter_map(|value| match value {
                        EnvValue::SecretRef { secret } => Some(secret.clone()),
                        EnvValue::Literal { .. } => None,
                    }))
            })
            .collect(),
    )
}

fn unique_refs(refs: Vec<ObjectRef>) -> Vec<ObjectRef> {
    let mut unique = Vec::new();
    for reference in refs {
        if !unique.iter().any(|candidate| candidate == &reference) {
            unique.push(reference);
        }
    }
    unique
}

#[derive(Clone, Debug)]
struct RedactionContext {
    metadata: RedactionMetadata,
    terms: Vec<String>,
}

fn build_redaction_context(bundle: &JobBundle, job: &Job) -> RedactionContext {
    let mut terms = Vec::new();

    for secret in bundle
        .secret_refs
        .iter()
        .chain(job.secret_refs.iter())
        .chain(job.env.values().filter_map(|value| match value {
            EnvValue::SecretRef { secret } => Some(secret),
            EnvValue::Literal { .. } => None,
        }))
    {
        terms.push(secret.id.clone());
    }

    for (name, value) in &job.env {
        if let EnvValue::Literal { value } = value {
            if should_redact_env_key(name, &bundle.redaction) {
                terms.push(value.clone());
            }
        }
    }

    terms.sort_by_key(|term| std::cmp::Reverse(term.len()));
    terms.dedup();

    RedactionContext {
        metadata: bundle.redaction.clone(),
        terms,
    }
}

fn redact_command(command: &CommandSpec, redaction: &RedactionContext) -> CommandSpec {
    match command {
        CommandSpec::Shell { shell, command } => CommandSpec::Shell {
            shell: *shell,
            command: redact_text(command, redaction),
        },
        CommandSpec::Exec { argv } => CommandSpec::Exec {
            argv: argv
                .iter()
                .map(|argument| redact_text(argument, redaction))
                .collect(),
        },
    }
}

fn redact_text(text: &str, redaction: &RedactionContext) -> String {
    let mut redacted = text.to_owned();

    for term in &redaction.terms {
        if term.len() < redaction.metadata.min_secret_length {
            continue;
        }
        redacted = redacted.replace(term, &redact_value(term, &redaction.metadata));
    }

    redact_env_assignments(&redacted, &redaction.metadata)
}

fn redact_env_assignments(text: &str, metadata: &RedactionMetadata) -> String {
    text.lines()
        .map(|line| {
            let Some((key, value)) = line.split_once('=') else {
                return line.to_owned();
            };
            if key
                .chars()
                .all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
                && key
                    .chars()
                    .next()
                    .is_some_and(|ch| ch == '_' || ch.is_ascii_alphabetic())
                && should_redact_env_key(key, metadata)
            {
                format!("{key}={}", redact_value(value, metadata))
            } else {
                line.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn redact_value(value: &str, metadata: &RedactionMetadata) -> String {
    let chars = value.chars().collect::<Vec<_>>();
    let prefix_length = metadata.visible_prefix.min(chars.len());
    let suffix_length = metadata
        .visible_suffix
        .min(chars.len().saturating_sub(prefix_length));
    let prefix = chars[..prefix_length].iter().collect::<String>();
    let suffix = if suffix_length == 0 {
        String::new()
    } else {
        chars[chars.len() - suffix_length..]
            .iter()
            .collect::<String>()
    };

    format!("{prefix}{}{suffix}", metadata.mask)
}

fn should_redact_env_key(key: &str, metadata: &RedactionMetadata) -> bool {
    metadata
        .detect_env_keys
        .iter()
        .any(|detector| key.contains(detector))
}

fn apply_literal_env(job: &Job, command: &mut Command) -> Result<()> {
    for (name, value) in &job.env {
        match value {
            EnvValue::Literal { value } => {
                command.env(name, value);
            }
            EnvValue::SecretRef { secret } => {
                return Err(RunnerError::SecretInjectionUnsupported(secret.id.clone()));
            }
        }
    }

    Ok(())
}

fn ensure_no_secret_injection(job: &Job) -> Result<()> {
    if let Some(secret) = job.secret_refs.first() {
        return Err(RunnerError::SecretInjectionUnsupported(secret.id.clone()));
    }

    for value in job.env.values() {
        if let EnvValue::SecretRef { secret } = value {
            return Err(RunnerError::SecretInjectionUnsupported(secret.id.clone()));
        }
    }

    Ok(())
}

fn absolute_path(path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(path)
    }
}

fn sanitize_mount_name(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or("input")
        .chars()
        .map(|ch| match ch {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' | '.' => ch,
            _ => '_',
        })
        .collect()
}

fn unix_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}

fn current_os() -> String {
    match std::env::consts::OS {
        "macos" => "darwin".to_owned(),
        os => os.to_owned(),
    }
}

fn current_arch() -> String {
    match std::env::consts::ARCH {
        "x86_64" => "x64".to_owned(),
        "aarch64" => "arm64".to_owned(),
        arch => arch.to_owned(),
    }
}

fn default_protocol_version() -> String {
    PROTOCOL_VERSION.to_owned()
}

fn default_bundle_kind() -> String {
    BUNDLE_KIND.to_owned()
}

fn default_runner_kind() -> String {
    RUNNER_KIND.to_owned()
}

fn default_policy_decision_kind() -> String {
    "PolicyDecision".to_owned()
}

fn default_redaction_kind() -> String {
    "RedactionMetadata".to_owned()
}
