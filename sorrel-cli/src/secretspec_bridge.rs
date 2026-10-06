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

use serde::Deserialize;
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

/// Build a policy context that includes persisted `.sorrel/grants` for secret actions.
pub fn secret_policy_context() -> Result<PolicyContext, BridgeError> {
    let mut context = PolicyContext::headless_default();
    if !repo::is_initialized() {
        return Ok(context);
    }
    for object in repo::list_registry_entries(repo::GRANTS_DIR)? {
        if let Some(grant) = grant_from_persisted(&object) {
            context.grants.push(grant);
        }
    }
    Ok(context)
}

fn grant_from_persisted(object: &Value) -> Option<Grant> {
    let action = object.get("action")?.as_str()?;
    if !action.starts_with("secret.") {
        return None;
    }
    let secret_id = object.get("resource")?.get("ref")?.as_str()?.to_owned();
    let agent_id = object
        .pointer("/access/agents/0/id")
        .and_then(Value::as_str)
        .unwrap_or("agent_mock_cli");
    let mut capabilities = vec![action.to_owned()];
    // Inject implies read for the same handle.
    if action == "secret.inject" {
        capabilities.push("secret.read".to_owned());
    }
    Some(Grant {
        principal: PrincipalId {
            kind: "agent".to_owned(),
            id: agent_id.to_owned(),
        },
        capabilities,
        resources: vec![ResourceScope {
            scope: "secret".to_owned(),
            fields: serde_json::json!({ "ref": secret_id })
                .as_object()
                .cloned()
                .unwrap_or_default(),
        }],
        issued_by: None,
    })
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

    let groups = group_handles(&selected, provider_override)?;
    let (spec_path, _) = ensure_secretspec_toml(cwd)?;
    let config = secretspec::Config::try_from(spec_path.as_path())
        .map_err(|error| BridgeError::Spec(error.to_string()))?;
    let mut result = ResolvedSecrets::default();
    let mut providers = std::collections::BTreeSet::new();
    let mut profiles = std::collections::BTreeSet::new();
    for ((provider, profile), group) in groups {
        let (_manifest, spec) = selected_spec(
            cwd,
            &config,
            &group,
            &provider,
            &profile,
            "Sorrel secret resolution after Core grant authorization",
        )?;
        let response = spec
            .resolve()
            .map_err(|error| BridgeError::Spec(error.to_string()))?;
        if !response.is_ok() {
            return Err(BridgeError::Spec(format!(
                "missing required secrets: {}",
                response.missing_required.join(", ")
            )));
        }
        providers.insert(response.provider);
        profiles.insert(response.profile);
        for handle in group {
            result
                .id_to_name
                .insert(handle.id.clone(), handle.name.clone());
            if let Some(value) = response
                .secrets
                .get(&handle.name)
                .and_then(|secret| secret.value.as_ref())
            {
                result.values.insert(handle.name.clone(), value.clone());
            } else if handle.required {
                return Err(BridgeError::Spec(format!(
                    "required secret `{}` ({}) did not resolve",
                    handle.name, handle.id
                )));
            }
        }
    }
    result.provider = context_summary(providers);
    result.profile = context_summary(profiles);
    Ok(result)
}

fn group_handles<'a>(
    selected: &[&'a SecretHandle],
    provider_override: Option<&str>,
) -> Result<BTreeMap<(String, String), Vec<&'a SecretHandle>>, BridgeError> {
    let mut names = BTreeMap::new();
    let mut groups: BTreeMap<(String, String), Vec<&SecretHandle>> = BTreeMap::new();
    for &handle in selected {
        if let Some(previous) = names.insert(&handle.name, &handle.id) {
            if previous == &handle.id {
                continue;
            }
            return Err(BridgeError::Spec(format!(
                "selected SecretRefs `{previous}` and `{}` share environment name `{}`; select only one",
                handle.id, handle.name
            )));
        }
        groups
            .entry((
                secretspec_provider_for(handle, provider_override),
                profile_for_environment(&handle.environment),
            ))
            .or_default()
            .push(handle);
    }
    Ok(groups)
}

fn selected_spec(
    cwd: &Path,
    config: &secretspec::Config,
    handles: &[&SecretHandle],
    provider: &str,
    profile: &str,
    reason: &str,
) -> Result<(tempfile::NamedTempFile, secretspec::Secrets), BridgeError> {
    use std::io::Write;

    let names: std::collections::BTreeSet<&str> =
        handles.iter().map(|handle| handle.name.as_str()).collect();
    let mut config = config.clone();
    // Inheritance has already been loaded. Re-loading it would restore excluded names.
    config.project.extends = None;
    config.scopes = None;
    let mut aliases = secretspec::GlobalConfig::load()
        .map_err(|error| BridgeError::Spec(error.to_string()))?
        .and_then(|global| global.defaults.providers)
        .unwrap_or_default();
    aliases.extend(config.providers.take().unwrap_or_default());
    for alias in aliases.values_mut() {
        let mut value =
            serde_json::to_value(&*alias).map_err(|error| BridgeError::Spec(error.to_string()))?;
        root_provider_route(&mut value, cwd)?;
        *alias =
            serde_json::from_value(value).map_err(|error| BridgeError::Spec(error.to_string()))?;
    }
    config.providers = Some(aliases);
    for profile in config.profiles.values_mut() {
        profile
            .secrets
            .retain(|name, _| names.contains(name.as_str()));
        if let Some(providers) = profile
            .defaults
            .as_mut()
            .and_then(|defaults| defaults.providers.as_mut())
        {
            for provider in providers {
                *provider = rooted_provider(provider, cwd)?;
            }
        }
        for secret in profile.secrets.values_mut() {
            if let Some(providers) = &mut secret.providers {
                for provider in providers {
                    *provider = rooted_provider(provider, cwd)?;
                }
            }
        }
    }
    let text = toml::to_string(&config).map_err(|error| BridgeError::Spec(error.to_string()))?;
    // Keep defaults in excluded metadata or outside the workspace: they may be sensitive.
    // NamedTempFile restricts permissions and removes the manifest on every exit.
    let metadata_dir = cwd.join(repo::SORREL_DIR);
    let metadata_is_directory =
        fs::symlink_metadata(&metadata_dir).is_ok_and(|metadata| metadata.file_type().is_dir());
    let temporary_dir = if metadata_is_directory {
        metadata_dir
    } else {
        let directory = env::temp_dir();
        if directory.canonicalize()?.starts_with(cwd.canonicalize()?) {
            return Err(BridgeError::Spec(
                "temporary directory must be outside the workspace for secret resolution"
                    .to_owned(),
            ));
        }
        directory
    };
    let mut manifest = tempfile::Builder::new()
        .prefix("sorrel-secrets-")
        .suffix(".toml")
        .tempfile_in(temporary_dir)?;
    manifest.write_all(text.as_bytes())?;
    let spec = secretspec::Secrets::load_from(manifest.path())
        .map_err(|error| BridgeError::Spec(error.to_string()))?;
    let mut spec = with_access_reason(spec, reason);
    spec.set_provider(rooted_provider(provider, cwd)?);
    spec.set_profile(profile);
    spec.set_ignore_ambient_scope(true);
    Ok((manifest, spec))
}

fn rooted_provider(provider: &str, cwd: &Path) -> Result<String, BridgeError> {
    // Dotenv is the only file-backed provider enabled in this build. Let its
    // upstream parser handle URI, shorthand, home-relative and Windows paths.
    if provider != "dotenv" && !provider.starts_with("dotenv:") {
        return Ok(provider.to_owned());
    }
    let mut provider = Box::<dyn secretspec::Provider>::try_from(provider.to_owned())
        .map_err(|error| BridgeError::Spec(error.to_string()))?;
    provider.with_base_dir(&std::path::absolute(cwd)?);
    Ok(provider.uri())
}

fn root_provider_route(value: &mut Value, cwd: &Path) -> Result<(), BridgeError> {
    match value {
        Value::String(provider) => *provider = rooted_provider(provider, cwd)?,
        Value::Object(route) => {
            for key in ["uri", "provider", "cache"] {
                if let Some(value) = route.get_mut(key) {
                    root_provider_route(value, cwd)?;
                }
            }
            if let Some(Value::Array(fallback)) = route.get_mut("fallback") {
                for provider in fallback {
                    root_provider_route(provider, cwd)?;
                }
            }
            if let Some(Value::Object(credentials)) = route.get_mut("credentials") {
                for source in credentials.values_mut() {
                    root_provider_route(source, cwd)?;
                }
            }
        }
        _ => {}
    }
    Ok(())
}

fn context_summary(values: std::collections::BTreeSet<String>) -> String {
    if values.len() == 1 {
        values.into_iter().next().unwrap_or_default()
    } else {
        "mixed".to_owned()
    }
}

/// Value-free presence report for the supplied handles only.
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
    let selected = handles.iter().collect::<Vec<_>>();
    let groups = group_handles(&selected, provider_override)?;
    let (spec_path, _) = ensure_secretspec_toml(cwd)?;
    let config = secretspec::Config::try_from(spec_path.as_path())
        .map_err(|error| BridgeError::Spec(error.to_string()))?;
    let mut providers = std::collections::BTreeSet::new();
    let mut profiles = std::collections::BTreeSet::new();
    let mut secrets = Vec::new();
    let mut violations = Vec::new();
    for ((provider, profile), group) in groups {
        let (_manifest, spec) = selected_spec(
            cwd,
            &config,
            &group,
            &provider,
            &profile,
            "Sorrel secret availability check",
        )?;
        let report = spec
            .report()
            .map_err(|error| BridgeError::Spec(error.to_string()))?;
        providers.insert(report.provider);
        profiles.insert(report.profile);
        secrets.extend(report.secrets);
        violations.extend(report.constraint_violations);
    }
    let mut report = secretspec::ResolutionReport::new(
        context_summary(providers),
        context_summary(profiles),
        secrets,
    );
    report.constraint_violations = violations;
    Ok(report)
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

    #[test]
    fn narrowed_manifest_is_excluded_private_and_cleaned_after_failure() {
        let directory = tempfile::tempdir().unwrap();
        let metadata = directory.path().join(repo::SORREL_DIR);
        fs::create_dir(&metadata).unwrap();
        let config: secretspec::Config = r#"
[project]
name = "synthetic-private-manifest-test"
revision = "1.0"
[profiles.default]
TOKEN = { description = "Synthetic", default = "synthetic-default" }
[profiles.dev]
"#
        .parse()
        .unwrap();
        let handle = SecretHandle {
            id: "secret_token".into(),
            name: "TOKEN".into(),
            provider: "dotenv:.env".into(),
            uri: String::new(),
            environment: "dev".into(),
            required: true,
            description: None,
        };
        let (manifest, _spec) = selected_spec(
            directory.path(),
            &config,
            &[&handle],
            "dotenv:.env",
            "dev",
            "synthetic test",
        )
        .unwrap();
        assert_eq!(manifest.path().parent(), Some(metadata.as_path()));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                manifest.as_file().metadata().unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        let store = sorrel_core::InMemoryObjectStore::new();
        let snapshot = sorrel_core::materialize_snapshot_excluding(
            &store,
            directory.path(),
            [repo::SORREL_DIR],
            sorrel_core::SnapshotOptions::new("repo_test"),
        )
        .unwrap();
        assert!(sorrel_core::read_snapshot_files(&store, &snapshot.id)
            .unwrap()
            .is_empty());
        let path = manifest.path().to_owned();
        drop(manifest);
        assert!(!path.exists());
        let mut invalid = config;
        invalid
            .profiles
            .get_mut("default")
            .unwrap()
            .secrets
            .get_mut("TOKEN")
            .unwrap()
            .description = None;
        assert!(selected_spec(
            directory.path(),
            &invalid,
            &[&handle],
            "dotenv:.env",
            "dev",
            "synthetic test"
        )
        .is_err());
        assert!(fs::read_dir(metadata).unwrap().next().is_none());
    }
}
