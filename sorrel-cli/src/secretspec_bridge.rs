//! Bridge between Sorrel `SecretRef` handles and upstream SecretSpec providers.
//!
//! Sorrel keeps SecretRef ids + Core grants as source of truth. SecretSpec
//! (Apache-2.0, consumed upstream — not forked) resolves and stores values via
//! providers such as `keyring`, `dotenv`, and `env`.

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command as ProcessCommand, Stdio};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::cli_policy::{
    evaluate, Decision, EvaluateInput, Grant, PolicyContext, PrincipalId, ResourceScope,
};
use crate::repo;

/// Default SecretSpec provider for local/dev when a SecretRef still says `sorrel-vault`.
pub const DEFAULT_PROVIDER: &str = "dotenv:.env";

/// Local CLI principal used for secret and workflow authorization.
pub const CLI_SECRET_PRINCIPAL: &str = "agent:agent_mock_cli";

fn with_access_reason(spec: secretspec::Secrets, fallback: &str) -> secretspec::Secrets {
    let reason = env::var("SECRETSPEC_REASON")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| fallback.to_owned());
    spec.with_reason(reason)
}

/// Declared secret handle (values never stored here).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecretHandle {
    pub id: String,
    pub name: String,
    pub provider: String,
    pub uri: String,
    pub environment: String,
    pub required: bool,
    pub description: Option<String>,
}

/// Resolved secret values keyed by env name (e.g. `NPM_TOKEN`).
#[derive(Debug, Clone, Default)]
pub struct ResolvedSecrets {
    pub provider: String,
    pub profile: String,
    /// Env name → value. Treat as sensitive.
    pub values: BTreeMap<String, String>,
    /// SecretRef id → env name for redaction markers.
    pub id_to_name: BTreeMap<String, String>,
}

#[derive(Debug)]
pub enum BridgeError {
    Io(io::Error),
    Spec(String),
    Policy {
        action: String,
        secret_id: String,
        reason: String,
        result: String,
    },
    MissingSecretspec(PathBuf),
    NotFound(String),
}

impl std::fmt::Display for BridgeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(f, "{error}"),
            Self::Spec(message) => write!(f, "secretspec: {message}"),
            Self::Policy {
                action,
                secret_id,
                reason,
                ..
            } => write!(f, "policy denied {action} on secret:{secret_id} ({reason})"),
            Self::MissingSecretspec(path) => write!(
                f,
                "secretspec.toml not found at {} (run `sorrel secret sync`)",
                path.display()
            ),
            Self::NotFound(id) => write!(f, "SecretRef `{id}` not found"),
        }
    }
}

impl std::error::Error for BridgeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

impl From<io::Error> for BridgeError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

/// Load SecretRef handles from `.sorrel/secrets/` and optional `sorrel.secrets.yml`.
pub fn load_secret_handles(cwd: &Path) -> Result<Vec<SecretHandle>, BridgeError> {
    let mut by_id: BTreeMap<String, SecretHandle> = BTreeMap::new();

    if repo::is_initialized() {
        // Registry lives under cwd/.sorrel when callers `chdir`; list_registry uses relative paths.
        for object in repo::list_registry_entries(repo::SECRETS_DIR)? {
            if let Some(handle) = handle_from_json(&object) {
                by_id.insert(handle.id.clone(), handle);
            }
        }
    }

    let yaml_path = cwd.join("sorrel.secrets.yml");
    if yaml_path.is_file() {
        let text = fs::read_to_string(&yaml_path)?;
        for handle in handles_from_secrets_yaml(&text)? {
            by_id.entry(handle.id.clone()).or_insert(handle);
        }
    }

    Ok(by_id.into_values().collect())
}

fn handle_from_json(object: &Value) -> Option<SecretHandle> {
    let id = object.get("id")?.as_str()?.to_owned();
    let name = object.get("name")?.as_str()?.to_owned();
    let provider = object
        .get("provider")
        .and_then(Value::as_str)
        .unwrap_or("sorrel-vault")
        .to_owned();
    let uri = object
        .get("uri")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    let environment = object
        .get("environment")
        .and_then(Value::as_str)
        .unwrap_or("dev")
        .to_owned();
    let required = object
        .get("required")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let description = object
        .get("description")
        .and_then(Value::as_str)
        .map(str::to_owned);
    Some(SecretHandle {
        id,
        name,
        provider,
        uri,
        environment,
        required,
        description,
    })
}

#[derive(Debug, Deserialize)]
struct SecretsYaml {
    #[serde(default, rename = "secretRefs")]
    secret_refs: Vec<SecretsYamlRef>,
}

#[derive(Debug, Deserialize)]
struct SecretsYamlRef {
    id: String,
    name: String,
    #[serde(default = "default_provider")]
    provider: String,
    #[serde(default)]
    uri: String,
    #[serde(default = "default_env")]
    environment: String,
    #[serde(default = "default_required")]
    required: bool,
    description: Option<String>,
}

fn default_provider() -> String {
    "sorrel-vault".to_owned()
}

fn default_env() -> String {
    "dev".to_owned()
}

fn default_required() -> bool {
    true
}

fn handles_from_secrets_yaml(text: &str) -> Result<Vec<SecretHandle>, BridgeError> {
    let parsed: SecretsYaml = serde_yaml_ng::from_str(text).map_err(|error| {
        BridgeError::Spec(format!("failed to parse sorrel.secrets.yml: {error}"))
    })?;
    Ok(parsed
        .secret_refs
        .into_iter()
        .map(|item| SecretHandle {
            id: item.id,
            name: item.name,
            provider: item.provider,
            uri: item.uri,
            environment: item.environment,
            required: item.required,
            description: item.description,
        })
        .collect())
}

/// Map a Sorrel SecretRef provider to a SecretSpec provider name/URI.
#[must_use]
pub fn secretspec_provider_for(handle: &SecretHandle, override_provider: Option<&str>) -> String {
    if let Some(provider) = override_provider {
        if !provider.trim().is_empty() {
            return provider.trim().to_owned();
        }
    }
    match handle.provider.as_str() {
        "keyring" | "keyring://" => "keyring".to_owned(),
        "env" => "env".to_owned(),
        "dotenv" => "dotenv:.env".to_owned(),
        other if other.starts_with("dotenv:") || other.starts_with("dotenv://") => other.to_owned(),
        other if other.starts_with("keyring:") => other.to_owned(),
        // Legacy local vault + unknown → dotenv for offline/dev (or uri override).
        _ => {
            if handle.uri.starts_with("keyring:")
                || handle.uri.starts_with("dotenv:")
                || handle.uri == "env"
            {
                handle.uri.clone()
            } else {
                DEFAULT_PROVIDER.to_owned()
            }
        }
    }
}

/// Write or refresh `secretspec.toml` from declared SecretRef handles.
pub fn sync_secretspec_toml(cwd: &Path, handles: &[SecretHandle]) -> Result<PathBuf, BridgeError> {
    let path = cwd.join("secretspec.toml");
    let project_name = cwd
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("sorrel")
        .to_owned();

    let mut profiles: BTreeMap<String, Vec<&SecretHandle>> = BTreeMap::new();
    for handle in handles {
        profiles
            .entry(profile_for_environment(&handle.environment))
            .or_default()
            .push(handle);
    }
    if profiles.is_empty() {
        profiles.insert("default".to_owned(), Vec::new());
    }

    let mut out = String::new();
    out.push_str("# Generated by `sorrel secret sync` — SecretRef names ↔ SecretSpec.\n");
    out.push_str("# Values live in providers (keyring / dotenv / env); do not commit secrets.\n\n");
    out.push_str("[project]\n");
    out.push_str(&format!("name = \"{}\"\n", escape_toml(&project_name)));
    out.push_str("revision = \"1.0\"\n\n");

    for (profile, profile_handles) in &profiles {
        out.push_str(&format!("[profiles.{profile}]\n"));
        if profile_handles.is_empty() {
            out.push('\n');
            continue;
        }
        for handle in profile_handles {
            let description = handle.description.as_deref().unwrap_or("Sorrel SecretRef");
            out.push_str(&format!(
                "{} = {{ description = \"{}\", required = {} }}\n",
                handle.name,
                escape_toml(description),
                if handle.required { "true" } else { "false" }
            ));
        }
        out.push('\n');
    }

    // Always include a default profile that unions all secrets so unscoped
    // `secretspec` loads work without an explicit profile.
    if !profiles.contains_key("default") {
        out.push_str("[profiles.default]\n");
        for handle in handles {
            let description = handle.description.as_deref().unwrap_or("Sorrel SecretRef");
            out.push_str(&format!(
                "{} = {{ description = \"{}\", required = {} }}\n",
                handle.name,
                escape_toml(description),
                if handle.required { "true" } else { "false" }
            ));
        }
        out.push('\n');
    }

    fs::write(&path, out)?;
    Ok(path)
}

fn profile_for_environment(environment: &str) -> String {
    match environment {
        "development" => "development".to_owned(),
        "dev" => "dev".to_owned(),
        "staging" => "staging".to_owned(),
        "production" | "prod" => "production".to_owned(),
        other => other.to_owned(),
    }
}

fn escape_toml(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

/// Ensure `secretspec.toml` exists (generate from handles when missing).
pub fn ensure_secretspec_toml(cwd: &Path) -> Result<(PathBuf, Vec<SecretHandle>), BridgeError> {
    let handles = load_secret_handles(cwd)?;
    let path = cwd.join("secretspec.toml");
    if !path.is_file() {
        if handles.is_empty() {
            return Err(BridgeError::MissingSecretspec(path));
        }
        sync_secretspec_toml(cwd, &handles)?;
    }
    Ok((path, handles))
}

/// Canonical, value-free secret grant scope. Its hash is bound by signed proposal IDs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SecretGrantScope {
    pub action: String,
    pub agents: Vec<String>,
    pub workflows: Vec<String>,
    pub runners: Vec<String>,
    pub secret: String,
    pub environment: String,
}

impl SecretGrantScope {
    pub fn validate(&self) -> Result<(), BridgeError> {
        if !self.action.starts_with("secret.")
            || self.action.len() <= 7
            || self.secret.is_empty()
            || self.secret.contains('*')
            || self.environment.is_empty()
            || self.agents.is_empty()
            || self
                .agents
                .iter()
                .chain(&self.workflows)
                .chain(&self.runners)
                .any(String::is_empty)
            || self.agents.windows(2).any(|pair| pair[0] >= pair[1])
            || self.workflows.windows(2).any(|pair| pair[0] >= pair[1])
            || self.runners.windows(2).any(|pair| pair[0] >= pair[1])
        {
            return Err(BridgeError::Spec(
                "invalid canonical secret grant scope".to_owned(),
            ));
        }
        Ok(())
    }

    pub fn id(&self) -> Result<String, BridgeError> {
        self.validate()?;
        let payload = serde_json::to_vec(self)
            .map_err(|_| BridgeError::Spec("cannot encode secret grant scope".to_owned()))?;
        Ok(format!(
            "grant_{}",
            sorrel_core::ObjectId::for_bytes(&payload).to_hex()
        ))
    }

    pub fn capabilities(&self) -> Vec<String> {
        if self.action == "secret.inject" {
            vec!["secret.inject".to_owned(), "secret.read".to_owned()]
        } else {
            vec![self.action.clone()]
        }
    }

    pub fn proposals(&self) -> Result<Vec<sorrel_core::authority::ProposedGrant>, BridgeError> {
        let id = self.id()?;
        Ok(self
            .agents
            .iter()
            .enumerate()
            .map(|(index, agent)| {
                sorrel_core::authority::ProposedGrant::new(
                    format!("{id}_{index}"),
                    sorrel_core::policy::PrincipalDescriptor::new(
                        sorrel_core::policy::PrincipalKind::Agent,
                        agent,
                    ),
                    self.capabilities(),
                    sorrel_core::policy::ResourceRef::new(
                        sorrel_core::policy::ResourceKind::Secret,
                        &self.secret,
                    ),
                    sorrel_core::policy::GrantEffect::Allow,
                )
            })
            .collect())
    }
}

/// Explicit operator input, never copied into stored grants or command output.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OperatorAuthorityContext {
    pub authority_root: sorrel_core::authority::AuthorityRoot,
    pub previous_grants: Vec<sorrel_core::policy::Grant>,
    pub context: sorrel_core::authority::PolicyChangeContext,
}

pub fn load_authority_context(path: &Path) -> Result<OperatorAuthorityContext, BridgeError> {
    let bytes = fs::read(path)?;
    serde_json::from_slice(&bytes)
        .map_err(|_| BridgeError::Spec("invalid operator authority context".to_owned()))
}

pub fn verify_secret_grant_approval(
    scope: &SecretGrantScope,
    change: &sorrel_core::authority::PolicyChange,
    authority: &OperatorAuthorityContext,
) -> Result<bool, BridgeError> {
    use sorrel_core::authority::{
        evaluate_policy_change, PolicyChangeAction, PolicyChangeOutcome, PolicyChangeTrust,
    };
    if change.action != PolicyChangeAction::Grant || change.proposed_grants != scope.proposals()? {
        return Ok(false);
    }
    let decision = evaluate_policy_change(
        change,
        &authority.authority_root,
        &authority.previous_grants,
        &authority.context,
    );
    Ok(decision.trust == PolicyChangeTrust::Trusted
        && decision.outcome == PolicyChangeOutcome::Approved)
}

/// Compatibility helper with no workflow/runner/environment assertion.
pub fn secret_policy_context() -> Result<PolicyContext, BridgeError> {
    secret_policy_context_for(None, None, None)
}

/// Load approved grants matching the actual operation dimensions before evaluation.
pub fn secret_policy_context_for(
    environment: Option<&str>,
    workflow: Option<&str>,
    runner: Option<&str>,
) -> Result<PolicyContext, BridgeError> {
    let mut context = PolicyContext::headless_default();
    context.authority_principals.clear();
    context.grants.clear();
    if !repo::is_initialized() {
        return Ok(context);
    }
    let authority = env::var_os("SORREL_AUTHORITY_CONTEXT")
        .map(|path| load_authority_context(Path::new(&path)))
        .transpose()?;
    let demo = env::var_os("SORREL_LOCAL_DEMO").as_deref() == Some(std::ffi::OsStr::new("1"));
    for object in repo::list_registry_entries(repo::GRANTS_DIR)? {
        context.grants.extend(grants_from_persisted(
            &object,
            authority.as_ref(),
            demo,
            environment,
            workflow,
            runner,
        )?);
    }
    Ok(context)
}

fn grants_from_persisted(
    object: &Value,
    authority: Option<&OperatorAuthorityContext>,
    demo: bool,
    environment: Option<&str>,
    workflow: Option<&str>,
    runner: Option<&str>,
) -> Result<Vec<Grant>, BridgeError> {
    // Legacy decisions or serialized strings alone are not authorization evidence.
    if object.get("kind").and_then(Value::as_str) != Some("Grant")
        || object.get("decision").and_then(Value::as_str) != Some("allow")
        || object.pointer("/resource/type").and_then(Value::as_str) != Some("secret")
    {
        return Ok(vec![]);
    }
    let Some(approval) = object.get("approval") else {
        return Ok(vec![]);
    };
    let Some(scope_value) = object.get("scope") else {
        return Ok(vec![]);
    };
    let scope: SecretGrantScope = serde_json::from_value(scope_value.clone())
        .map_err(|_| BridgeError::Spec("invalid stored secret grant scope".to_owned()))?;
    if object.get("id").and_then(Value::as_str) != Some(scope.id()?.as_str())
        || object.get("action").and_then(Value::as_str) != Some(scope.action.as_str())
        || object.pointer("/resource/ref").and_then(Value::as_str) != Some(scope.secret.as_str())
        || object.get("environment").and_then(Value::as_str) != Some(scope.environment.as_str())
    {
        return Ok(vec![]);
    }
    let expected_access = serde_json::json!({
        "agents": scope.agents.iter().map(|id| serde_json::json!({"kind":"AgentPolicy", "id":id})).collect::<Vec<_>>(),
        "workflows": scope.workflows.iter().map(|id| serde_json::json!({"kind":"Workflow", "id":id})).collect::<Vec<_>>(),
        "runners": scope.runners.iter().map(|id| serde_json::json!({"kind":"Runner", "id":id})).collect::<Vec<_>>(),
    });
    if object.get("access") != Some(&expected_access) {
        return Ok(vec![]);
    }
    let approved = match approval.get("mode").and_then(Value::as_str) {
        Some("native") => {
            let Some(authority) = authority else {
                return Ok(vec![]);
            };
            let Some(change) = approval.get("policyChange") else {
                return Ok(vec![]);
            };
            let change = serde_json::from_value(change.clone()).map_err(|_| {
                BridgeError::Spec("invalid stored native grant approval".to_owned())
            })?;
            verify_secret_grant_approval(&scope, &change, authority)?
        }
        Some("local-demo") => {
            demo && object.pointer("/metadata/mocked").and_then(Value::as_bool) == Some(true)
        }
        _ => false,
    };
    if !approved
        || environment != Some(scope.environment.as_str())
        || (!scope.workflows.is_empty()
            && !workflow.is_some_and(|id| scope.workflows.iter().any(|allowed| allowed == id)))
        || (!scope.runners.is_empty()
            && !runner.is_some_and(|id| scope.runners.iter().any(|allowed| allowed == id)))
    {
        return Ok(vec![]);
    }
    let effective_native_grants = authority
        .map(|authority| {
            let mut grants = authority.previous_grants.clone();
            for proposed in scope.proposals()? {
                let mut grant = sorrel_core::policy::Grant::new(
                    proposed.id,
                    proposed.principal,
                    sorrel_core::policy::Capability::new(&scope.action),
                    proposed.resource,
                    proposed.effect,
                );
                grant.capabilities = proposed.capabilities;
                grants.push(grant);
            }
            Ok::<_, BridgeError>(grants)
        })
        .transpose()?;
    Ok(scope
        .agents
        .iter()
        .filter_map(|agent| {
            let capabilities = scope
                .capabilities()
                .into_iter()
                .filter(|capability| {
                    effective_native_grants.as_ref().is_none_or(|grants| {
                        let request = sorrel_core::policy::PolicyEvaluationRequest {
                            principal: sorrel_core::policy::PrincipalDescriptor::new(
                                sorrel_core::policy::PrincipalKind::Agent,
                                agent,
                            ),
                            capability: sorrel_core::policy::Capability::new(capability),
                            resource: sorrel_core::policy::ResourceRef::new(
                                sorrel_core::policy::ResourceKind::Secret,
                                &scope.secret,
                            ),
                        };
                        sorrel_core::policy::evaluate_policy(&request, grants, &[]).decision
                            == sorrel_core::policy::DecisionKind::Allow
                    })
                })
                .collect::<Vec<_>>();
            if capabilities.is_empty() {
                return None;
            }
            Some(Grant {
                principal: PrincipalId {
                    kind: "agent".to_owned(),
                    id: agent.clone(),
                },
                capabilities,
                resources: vec![ResourceScope {
                    scope: "secret".to_owned(),
                    fields: serde_json::json!({"ref":scope.secret})
                        .as_object()
                        .cloned()
                        .unwrap_or_default(),
                }],
                issued_by: None,
            })
        })
        .collect())
}

/// Authorize a secret action for the CLI agent.
pub fn authorize_secret(
    context: &PolicyContext,
    action: &str,
    secret_id: &str,
    environment: Option<&str>,
) -> Result<(), BridgeError> {
    let principal = PrincipalId::parse(CLI_SECRET_PRINCIPAL)
        .ok_or_else(|| BridgeError::Spec("invalid CLI principal".to_owned()))?;
    let decision = evaluate(
        &EvaluateInput {
            principal,
            action: action.to_owned(),
            resource: crate::cli_policy::ResourceRef {
                scope: "secret".to_owned(),
                id: secret_id.to_owned(),
            },
            environment: environment.map(str::to_owned),
        },
        context,
    );
    if decision.decision == Decision::Allow {
        return Ok(());
    }
    Err(BridgeError::Policy {
        action: action.to_owned(),
        secret_id: secret_id.to_owned(),
        reason: decision.reason,
        result: decision.decision.as_str().to_owned(),
    })
}

/// Resolve selected SecretRef ids into env values via SecretSpec (after policy allow).
pub fn resolve_handles(
    cwd: &Path,
    handles: &[SecretHandle],
    selected_ids: &[String],
    provider_override: Option<&str>,
) -> Result<ResolvedSecrets, BridgeError> {
    let selected: Vec<&SecretHandle> = if selected_ids.is_empty() {
        handles.iter().collect()
    } else {
        selected_ids
            .iter()
            .map(|id| {
                handles
                    .iter()
                    .find(|handle| handle.id == *id || handle.name == *id)
                    .ok_or_else(|| BridgeError::NotFound(id.clone()))
            })
            .collect::<Result<Vec<_>, _>>()?
    };

    if selected.is_empty() {
        return Ok(ResolvedSecrets::default());
    }

    let (spec_path, _) = ensure_secretspec_toml(cwd)?;
    let provider = secretspec_provider_for(selected[0], provider_override);
    let profile = profile_for_environment(&selected[0].environment);

    let spec = secretspec::Secrets::load_from(&spec_path)
        .map_err(|error| BridgeError::Spec(error.to_string()))?;
    let mut spec = with_access_reason(
        spec,
        "Sorrel secret resolution after Core grant authorization",
    );
    spec.set_provider(&provider);
    spec.set_profile(&profile);

    let response = spec
        .resolve()
        .map_err(|error| BridgeError::Spec(error.to_string()))?;
    if !response.is_ok() {
        return Err(BridgeError::Spec(format!(
            "missing required secrets: {}",
            response.missing_required.join(", ")
        )));
    }

    let mut values = BTreeMap::new();
    let mut id_to_name = BTreeMap::new();
    for handle in selected {
        id_to_name.insert(handle.id.clone(), handle.name.clone());
        if let Some(resolved) = response.secrets.get(&handle.name) {
            if let Some(value) = &resolved.value {
                values.insert(handle.name.clone(), value.clone());
            }
        } else if handle.required {
            return Err(BridgeError::Spec(format!(
                "required secret `{}` ({}) did not resolve",
                handle.name, handle.id
            )));
        }
    }

    Ok(ResolvedSecrets {
        provider: response.provider,
        profile: response.profile,
        values,
        id_to_name,
    })
}

/// Value-free presence report for `sorrel secret check`.
pub fn check_handles(
    cwd: &Path,
    handles: &[SecretHandle],
    provider_override: Option<&str>,
) -> Result<secretspec::ResolutionReport, BridgeError> {
    if handles.is_empty() {
        return Err(BridgeError::Spec(
            "no SecretRef handles declared (add sorrel.secrets.yml or .sorrel/secrets)".to_owned(),
        ));
    }
    let (spec_path, _) = ensure_secretspec_toml(cwd)?;
    let provider = secretspec_provider_for(&handles[0], provider_override);
    let profile = profile_for_environment(&handles[0].environment);
    let spec = secretspec::Secrets::load_from(&spec_path)
        .map_err(|error| BridgeError::Spec(error.to_string()))?;
    let mut spec = with_access_reason(spec, "Sorrel secret availability check");
    spec.set_provider(&provider);
    spec.set_profile(&profile);
    spec.report()
        .map_err(|error| BridgeError::Spec(error.to_string()))
}

/// Persist a secret value into the configured provider (after policy allow).
pub fn set_secret_value(
    cwd: &Path,
    handle: &SecretHandle,
    value: String,
    provider_override: Option<&str>,
) -> Result<(), BridgeError> {
    let (spec_path, _) = ensure_secretspec_toml(cwd)?;
    let provider = secretspec_provider_for(handle, provider_override);
    let profile = profile_for_environment(&handle.environment);
    let spec = secretspec::Secrets::load_from(&spec_path)
        .map_err(|error| BridgeError::Spec(error.to_string()))?;
    let mut spec = with_access_reason(spec, "Sorrel secret update after Core grant authorization");
    spec.set_provider(&provider);
    spec.set_profile(&profile);
    spec.set(&handle.name, Some(value))
        .map_err(|error| BridgeError::Spec(error.to_string()))
}

/// Run a child command with resolved secrets in its environment only.
pub fn run_with_secrets(
    command: &[String],
    resolved: &ResolvedSecrets,
) -> Result<i32, BridgeError> {
    if command.is_empty() {
        return Err(BridgeError::Spec(
            "no command specified; usage: sorrel secret run -- <command>".to_owned(),
        ));
    }
    let mut child = ProcessCommand::new(&command[0]);
    child.args(&command[1..]);
    child.envs(&resolved.values);
    child.stdin(Stdio::inherit());
    child.stdout(Stdio::inherit());
    child.stderr(Stdio::inherit());
    let status = child.status().map_err(BridgeError::Io)?;
    Ok(status.code().unwrap_or(1))
}

/// Redact known secret values and SecretRef ids from captured output.
#[must_use]
pub fn redact_text(text: &str, resolved: &ResolvedSecrets) -> String {
    let mut out = text.to_owned();
    for (id, name) in &resolved.id_to_name {
        let marker = format!("<sorrel:redacted {id}>");
        if let Some(value) = resolved.values.get(name) {
            if value.len() >= 4 {
                out = out.replace(value, &marker);
            }
        }
        out = out.replace(id, &marker);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approved_fixture() -> (
        SecretGrantScope,
        OperatorAuthorityContext,
        sorrel_core::authority::PolicyChange,
        Value,
    ) {
        use sorrel_core::authority::{
            AuthorityRoot, AuthoritySigningKey, PolicyChange, PolicyChangeAction,
            PolicyChangeContext, PolicyRoot,
        };
        use sorrel_core::policy::{
            Capability, Grant as NativeGrant, GrantEffect, PrincipalDescriptor, PrincipalKind,
            ResourceKind, ResourceRef,
        };
        let scope = SecretGrantScope {
            action: "secret.inject".to_owned(),
            agents: vec!["agent_a".to_owned(), "agent_b".to_owned()],
            workflows: vec!["workflow_test".to_owned()],
            runners: vec!["runner_test".to_owned()],
            secret: "secret_test".to_owned(),
            environment: "dev".to_owned(),
        };
        let actor = PrincipalDescriptor::new(PrincipalKind::User, "operator");
        let org = ResourceRef::new(ResourceKind::Org, "org_test");
        let authority = OperatorAuthorityContext {
            authority_root: AuthorityRoot::new(
                "authority",
                "root",
                vec![AuthoritySigningKey::new(
                    "key",
                    "synthetic-private-material",
                )],
                1,
            ),
            previous_grants: vec![NativeGrant::new(
                "grant_operator",
                actor.clone(),
                Capability::new("policy.grant"),
                org.clone(),
                GrantEffect::Allow,
            )],
            context: PolicyChangeContext::new(PolicyRoot::new("previous", 1), org),
        };
        let mut change = PolicyChange::new(
            "change",
            actor,
            authority.context.current_policy_root.clone(),
            PolicyRoot::new("proposed", 2),
            PolicyChangeAction::Grant,
        );
        change.proposed_grants = scope.proposals().unwrap();
        change.signatures = vec![authority
            .authority_root
            .sign_change(&change, "key")
            .unwrap()];
        let object = serde_json::json!({
            "kind":"Grant", "id":scope.id().unwrap(), "action":scope.action,
            "resource":{"type":"secret", "ref":scope.secret}, "environment":scope.environment,
            "scope":scope,
            "access":{
                "agents":scope.agents.iter().map(|id| serde_json::json!({"kind":"AgentPolicy", "id":id})).collect::<Vec<_>>(),
                "workflows":scope.workflows.iter().map(|id| serde_json::json!({"kind":"Workflow", "id":id})).collect::<Vec<_>>(),
                "runners":scope.runners.iter().map(|id| serde_json::json!({"kind":"Runner", "id":id})).collect::<Vec<_>>()
            },
            "approval":{"mode":"native", "policyChange":change}, "decision":"allow", "metadata":{"mocked":false}
        });
        (scope, authority, change, object)
    }

    #[test]
    fn secret_grant_ids_cannot_expand_into_cli_resource_patterns() {
        let (mut scope, _, _, _) = approved_fixture();
        scope.secret = "secret_test/**".to_owned();
        assert!(scope.id().is_err());
        assert!(scope.proposals().is_err());
    }

    #[test]
    fn native_approval_binds_all_constraints_and_capabilities() {
        let (scope, authority, change, _) = approved_fixture();
        assert!(verify_secret_grant_approval(&scope, &change, &authority).unwrap());
        for changed in [
            SecretGrantScope {
                environment: "prod".to_owned(),
                ..scope.clone()
            },
            SecretGrantScope {
                secret: "other_secret".to_owned(),
                ..scope.clone()
            },
            SecretGrantScope {
                action: "secret.read".to_owned(),
                ..scope.clone()
            },
            SecretGrantScope {
                agents: vec!["agent_a".to_owned()],
                ..scope.clone()
            },
            SecretGrantScope {
                workflows: vec![],
                ..scope.clone()
            },
            SecretGrantScope {
                runners: vec![],
                ..scope.clone()
            },
        ] {
            assert!(!verify_secret_grant_approval(&changed, &change, &authority).unwrap());
        }
        let mut unsigned = change.clone();
        unsigned.signatures.clear();
        assert!(!verify_secret_grant_approval(&scope, &unsigned, &authority).unwrap());
        let mut forged = change.clone();
        forged.signatures[0].value = "synthetic-forged".to_owned();
        assert!(!verify_secret_grant_approval(&scope, &forged, &authority).unwrap());
        let mut missing_read = change;
        missing_read.proposed_grants[0].capabilities.pop();
        missing_read.signatures = vec![authority
            .authority_root
            .sign_change(&missing_read, "key")
            .unwrap()];
        assert!(!verify_secret_grant_approval(&scope, &missing_read, &authority).unwrap());
    }

    #[test]
    fn loaded_native_grants_preserve_all_agents_and_usage_constraints() {
        let (_, authority, _, object) = approved_fixture();
        let grants = grants_from_persisted(
            &object,
            Some(&authority),
            false,
            Some("dev"),
            Some("workflow_test"),
            Some("runner_test"),
        )
        .unwrap();
        assert_eq!(grants.len(), 2);
        for agent in ["agent_a", "agent_b"] {
            assert_eq!(
                evaluate(
                    &EvaluateInput {
                        principal: PrincipalId::parse(&format!("agent:{agent}")).unwrap(),
                        action: "secret.read".to_owned(),
                        resource: crate::cli_policy::ResourceRef::parse("secret:secret_test")
                            .unwrap(),
                        environment: Some("dev".to_owned())
                    },
                    &PolicyContext {
                        grants: grants.clone(),
                        ..PolicyContext::headless_default()
                    }
                )
                .decision,
                Decision::Allow
            );
        }
        for (environment, workflow, runner) in [
            (None, Some("workflow_test"), Some("runner_test")),
            (Some("prod"), Some("workflow_test"), Some("runner_test")),
            (Some("dev"), None, Some("runner_test")),
            (Some("dev"), Some("wrong"), Some("runner_test")),
            (Some("dev"), Some("workflow_test"), None),
            (Some("dev"), Some("workflow_test"), Some("wrong")),
        ] {
            assert!(grants_from_persisted(
                &object,
                Some(&authority),
                false,
                environment,
                workflow,
                runner
            )
            .unwrap()
            .is_empty());
        }
        assert!(grants_from_persisted(
            &object,
            None,
            false,
            Some("dev"),
            Some("workflow_test"),
            Some("runner_test")
        )
        .unwrap()
        .is_empty());
    }

    #[test]
    fn stored_decision_or_legacy_fields_cannot_forge_approval() {
        let (_, mut authority, _, object) = approved_fixture();
        for decision in [
            serde_json::json!("deny"),
            serde_json::json!("needs_grant"),
            Value::Null,
            serde_json::json!(true),
        ] {
            let mut denied = object.clone();
            denied["decision"] = decision;
            assert!(grants_from_persisted(
                &denied,
                Some(&authority),
                false,
                Some("dev"),
                Some("workflow_test"),
                Some("runner_test")
            )
            .unwrap()
            .is_empty());
        }
        let mut legacy = object.clone();
        legacy.as_object_mut().unwrap().remove("approval");
        assert!(grants_from_persisted(
            &legacy,
            Some(&authority),
            false,
            Some("dev"),
            Some("workflow_test"),
            Some("runner_test")
        )
        .unwrap()
        .is_empty());
        let mut changed_access = object.clone();
        changed_access["access"]["agents"][0]["id"] = serde_json::json!("other");
        assert!(grants_from_persisted(
            &changed_access,
            Some(&authority),
            false,
            Some("dev"),
            Some("workflow_test"),
            Some("runner_test")
        )
        .unwrap()
        .is_empty());
        let mut forged = object.clone();
        forged["approval"]["policyChange"]["signatures"][0]["value"] = serde_json::json!("forged");
        assert!(grants_from_persisted(
            &forged,
            Some(&authority),
            false,
            Some("dev"),
            Some("workflow_test"),
            Some("runner_test")
        )
        .unwrap()
        .is_empty());
        authority.previous_grants.clear();
        assert!(grants_from_persisted(
            &object,
            Some(&authority),
            false,
            Some("dev"),
            Some("workflow_test"),
            Some("runner_test")
        )
        .unwrap()
        .is_empty());
    }

    #[test]
    fn native_recipient_restrictions_apply_at_consumption() {
        use sorrel_core::policy::{
            Capability, Grant as NativeGrant, GrantEffect, PrincipalDescriptor, PrincipalKind,
            ResourceKind, ResourceRef,
        };
        for effect in [GrantEffect::Deny, GrantEffect::Redact, GrantEffect::Review] {
            for restricted_capability in ["secret.read", "secret.inject", "*"] {
                for restricted_id in ["secret_test", "*", "other_secret"] {
                    let (scope, mut authority, change, object) = approved_fixture();
                    authority.previous_grants.push(NativeGrant::new(
                        "recipient_restriction",
                        PrincipalDescriptor::new(PrincipalKind::Agent, "agent_a"),
                        Capability::new(restricted_capability),
                        ResourceRef::new(ResourceKind::Secret, restricted_id),
                        effect,
                    ));
                    // Issuance authority and the recipient's permission are distinct.
                    assert!(verify_secret_grant_approval(&scope, &change, &authority).unwrap());
                    let grants = grants_from_persisted(
                        &object,
                        Some(&authority),
                        false,
                        Some("dev"),
                        Some("workflow_test"),
                        Some("runner_test"),
                    )
                    .unwrap();
                    let context = PolicyContext {
                        grants,
                        ..PolicyContext::headless_default()
                    };
                    for recipient in ["agent_a", "agent_b"] {
                        for capability in ["secret.read", "secret.inject"] {
                            let blocked = recipient == "agent_a"
                                && restricted_id != "other_secret"
                                && (restricted_capability == "*"
                                    || restricted_capability == capability);
                            let decision = evaluate(
                                &EvaluateInput {
                                    principal: PrincipalId::parse(&format!("agent:{recipient}"))
                                        .unwrap(),
                                    action: capability.to_owned(),
                                    resource: crate::cli_policy::ResourceRef::parse(
                                        "secret:secret_test",
                                    )
                                    .unwrap(),
                                    environment: Some("dev".to_owned()),
                                },
                                &context,
                            );
                            assert_eq!(decision.decision == Decision::Allow, !blocked, "effect={effect:?}, recipient={recipient}, capability={capability}, restriction={restricted_capability}, resource={restricted_id}");
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn local_demo_requires_explicit_consumption_opt_in() {
        let (_, _, _, mut object) = approved_fixture();
        object["approval"] = serde_json::json!({"mode":"local-demo"});
        object["metadata"]["mocked"] = serde_json::json!(true);
        assert!(grants_from_persisted(
            &object,
            None,
            false,
            Some("dev"),
            Some("workflow_test"),
            Some("runner_test")
        )
        .unwrap()
        .is_empty());
        assert_eq!(
            grants_from_persisted(
                &object,
                None,
                true,
                Some("dev"),
                Some("workflow_test"),
                Some("runner_test")
            )
            .unwrap()
            .len(),
            2
        );
        object["metadata"]["mocked"] = serde_json::json!(false);
        assert!(grants_from_persisted(
            &object,
            None,
            true,
            Some("dev"),
            Some("workflow_test"),
            Some("runner_test")
        )
        .unwrap()
        .is_empty());
    }

    #[test]
    fn maps_legacy_vault_provider_to_dotenv() {
        let handle = SecretHandle {
            id: "secret_npm_token_dev".to_owned(),
            name: "NPM_TOKEN".to_owned(),
            provider: "sorrel-vault".to_owned(),
            uri: "secret://project/dev/NPM_TOKEN".to_owned(),
            environment: "dev".to_owned(),
            required: false,
            description: None,
        };
        assert_eq!(secretspec_provider_for(&handle, None), "dotenv:.env");
        assert_eq!(secretspec_provider_for(&handle, Some("keyring")), "keyring");
    }

    #[test]
    fn parses_secret_refs_from_yaml() {
        let yaml = r#"
schemaVersion: sorrel.vault.v0
kind: SecretSpec
secretRefs:
  - id: secret_npm_token_dev
    name: NPM_TOKEN
    provider: dotenv
    uri: dotenv:.env
    environment: dev
    required: false
"#;
        let handles = handles_from_secrets_yaml(yaml).expect("yaml parses");
        assert_eq!(handles.len(), 1);
        assert_eq!(handles[0].name, "NPM_TOKEN");
        assert_eq!(handles[0].provider, "dotenv");
    }

    #[test]
    fn redacts_resolved_values() {
        let mut resolved = ResolvedSecrets::default();
        resolved
            .values
            .insert("NPM_TOKEN".to_owned(), "super-secret-token".to_owned());
        resolved
            .id_to_name
            .insert("secret_npm_token_dev".to_owned(), "NPM_TOKEN".to_owned());
        let text = redact_text(
            "token=super-secret-token id=secret_npm_token_dev",
            &resolved,
        );
        assert!(!text.contains("super-secret-token"));
        assert!(text.contains("<sorrel:redacted secret_npm_token_dev>"));
    }
}
