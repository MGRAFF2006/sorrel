//! `sorrel secret *` command handlers — SecretSpec-backed under Core policy.

use std::env;
use std::io;
use std::path::PathBuf;

use clap::{Args, Subcommand};
use serde_json::{json, Value};

use crate::secretspec_bridge::{
    authorize_secret, check_handles, load_secret_handles, resolve_handles, run_with_secrets,
    secret_policy_context, set_secret_value, sync_secretspec_toml, BridgeError, SecretHandle,
    DEFAULT_PROVIDER,
};
use crate::{repo, CommandOutput};

#[derive(Debug, Subcommand)]
pub enum SecretCommand {
    /// List known SecretRef handles (alias of `list`).
    Refs,
    /// List known SecretRef handles without materializing values.
    List,
    /// Generate or refresh `secretspec.toml` from declared SecretRefs.
    Sync,
    /// Check SecretSpec resolution presence (handles only; no values).
    Check(SecretProviderArgs),
    /// Show a SecretRef handle; with `--reveal` and a grant, print the value.
    Get(SecretGetArgs),
    /// Store a secret value in the configured SecretSpec provider.
    Set(SecretSetArgs),
    /// Run a command with authorized secrets injected into its environment only.
    Run(SecretRunArgs),
}

#[derive(Debug, Args)]
pub struct SecretProviderArgs {
    /// SecretSpec provider name or URI (default: dotenv:.env).
    #[arg(long)]
    pub provider: Option<String>,
}

#[derive(Debug, Args)]
pub struct SecretGetArgs {
    /// SecretRef id or env name (e.g. secret_npm_token_dev or NPM_TOKEN).
    pub secret: String,

    /// Print the raw value (requires secret.read grant).
    #[arg(long)]
    pub reveal: bool,

    /// SecretSpec provider name or URI.
    #[arg(long)]
    pub provider: Option<String>,
}

#[derive(Debug, Args)]
pub struct SecretSetArgs {
    /// SecretRef id or env name.
    pub secret: String,

    /// Value to store. Prefer piping via stdin in scripts; this flag is for tests.
    #[arg(long)]
    pub value: Option<String>,

    /// SecretSpec provider name or URI.
    #[arg(long)]
    pub provider: Option<String>,
}

#[derive(Debug, Args)]
pub struct SecretRunArgs {
    /// SecretSpec provider name or URI.
    #[arg(long)]
    pub provider: Option<String>,

    /// Limit injection to these SecretRef ids (default: all declared).
    #[arg(long = "secret", value_name = "ID")]
    pub secrets: Vec<String>,

    /// Command and args after `--`.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true, required = true)]
    pub command: Vec<String>,
}

pub fn execute(command: SecretCommand) -> io::Result<CommandOutput> {
    match command {
        SecretCommand::Refs => list_output("secret refs"),
        SecretCommand::List => list_output("secret list"),
        SecretCommand::Sync => sync_output(),
        SecretCommand::Check(args) => check_output(args),
        SecretCommand::Get(args) => get_output(args),
        SecretCommand::Set(args) => set_output(args),
        SecretCommand::Run(args) => run_output(args),
    }
}

/// `secret run` may need a non-zero process exit code while still printing JSON.
pub enum SecretRunResult {
    Output(CommandOutput),
    /// Child process exit code (for inherited stdio path).
    Exit(i32),
}

pub fn execute_run(args: SecretRunArgs, json: bool) -> io::Result<SecretRunResult> {
    let cwd = env::current_dir()?;
    let handles = load_secret_handles(&cwd).map_err(bridge_io)?;
    let context = secret_policy_context().map_err(bridge_io)?;

    let selected: Vec<String> = if args.secrets.is_empty() {
        handles.iter().map(|handle| handle.id.clone()).collect()
    } else {
        args.secrets.clone()
    };

    for id in &selected {
        let handle = find_handle(&handles, id).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("SecretRef `{id}` not found"),
            )
        })?;
        authorize_secret(
            &context,
            "secret.read",
            &handle.id,
            Some(&handle.environment),
        )
        .map_err(bridge_io)?;
        authorize_secret(
            &context,
            "secret.inject",
            &handle.id,
            Some(&handle.environment),
        )
        .map_err(bridge_io)?;
    }

    let provider = args.provider.as_deref().or(Some(DEFAULT_PROVIDER));
    let resolved = resolve_handles(&cwd, &handles, &selected, provider).map_err(bridge_io)?;

    // Strip a leading `--` if clap left it in trailing args.
    let mut command = args.command;
    if command.first().is_some_and(|part| part == "--") {
        command.remove(0);
    }

    if json {
        // For --json, capture is not available with inherit; report planned inject then run.
        let code = run_with_secrets(&command, &resolved).map_err(bridge_io)?;
        let output = CommandOutput {
            json: json!({
                "command": "secret run",
                "mocked": false,
                "status": if code == 0 { "completed" } else { "failed" },
                "exitCode": code,
                "provider": resolved.provider,
                "profile": resolved.profile,
                "injected": resolved.values.keys().cloned().collect::<Vec<_>>(),
                "backend": "secretspec"
            }),
            human: format!("secret run exited with {code}"),
        };
        return Ok(SecretRunResult::Output(output));
    }

    let code = run_with_secrets(&command, &resolved).map_err(bridge_io)?;
    Ok(SecretRunResult::Exit(code))
}

fn list_output(command_name: &str) -> io::Result<CommandOutput> {
    let cwd = env::current_dir()?;
    let handles = load_secret_handles(&cwd).map_err(bridge_io)?;
    // Prefer registry objects for JSON shape when present.
    let registry = if repo::is_initialized() {
        repo::list_registry_entries(repo::SECRETS_DIR)?
    } else {
        Vec::new()
    };
    let objects: Vec<Value> = if registry.is_empty() {
        handles
            .iter()
            .map(|handle| {
                json!({
                    "id": handle.id,
                    "name": handle.name,
                    "provider": handle.provider,
                    "uri": handle.uri,
                    "environment": handle.environment,
                    "required": handle.required
                })
            })
            .collect()
    } else {
        registry
    };

    let mut human = String::new();
    for object in &objects {
        let id = object["id"].as_str().unwrap_or_default();
        let name = object["name"].as_str().unwrap_or_default();
        let environment = object["environment"].as_str().unwrap_or("dev");
        let provider = object["provider"].as_str().unwrap_or("sorrel-vault");
        human.push_str(&format!("{id}  {name}  {environment}  {provider}\n"));
    }
    if objects.is_empty() {
        human = "No SecretRef handles declared. Add sorrel.secrets.yml or import into .sorrel/secrets/."
            .to_owned();
    }
    Ok(CommandOutput {
        json: json!({
            "command": command_name,
            "mocked": false,
            "count": objects.len(),
            "objects": objects
        }),
        human: human.trim_end().to_owned(),
    })
}

fn sync_output() -> io::Result<CommandOutput> {
    let cwd = env::current_dir()?;
    let handles = load_secret_handles(&cwd).map_err(bridge_io)?;
    if handles.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "no SecretRef handles to sync; declare sorrel.secrets.yml or .sorrel/secrets/",
        ));
    }
    let path = sync_secretspec_toml(&cwd, &handles).map_err(bridge_io)?;
    Ok(CommandOutput {
        json: json!({
            "command": "secret sync",
            "mocked": false,
            "path": path.display().to_string(),
            "count": handles.len(),
            "names": handles.iter().map(|handle| handle.name.clone()).collect::<Vec<_>>()
        }),
        human: format!("Wrote {} ({} secret(s))", path.display(), handles.len()),
    })
}

fn check_output(args: SecretProviderArgs) -> io::Result<CommandOutput> {
    let cwd = env::current_dir()?;
    let handles = load_secret_handles(&cwd).map_err(bridge_io)?;
    let context = secret_policy_context().map_err(bridge_io)?;
    for handle in &handles {
        authorize_secret(
            &context,
            "secret.read",
            &handle.id,
            Some(&handle.environment),
        )
        .map_err(bridge_io)?;
    }
    let provider = args.provider.as_deref();
    let report = check_handles(&cwd, &handles, provider).map_err(bridge_io)?;
    let report_json = serde_json::to_value(&report).unwrap_or_else(|_| json!({}));
    Ok(CommandOutput {
        json: json!({
            "command": "secret check",
            "mocked": false,
            "provider": args.provider.as_deref().unwrap_or(DEFAULT_PROVIDER),
            "report": report_json
        }),
        human: format!(
            "Secret check via {} ({} declared handle(s))",
            args.provider.as_deref().unwrap_or(DEFAULT_PROVIDER),
            handles.len()
        ),
    })
}

fn get_output(args: SecretGetArgs) -> io::Result<CommandOutput> {
    let cwd = env::current_dir()?;
    let handles = load_secret_handles(&cwd).map_err(bridge_io)?;
    let handle = find_handle(&handles, &args.secret).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            format!("SecretRef `{}` not found", args.secret),
        )
    })?;
    let context = secret_policy_context().map_err(bridge_io)?;
    authorize_secret(
        &context,
        "secret.read",
        &handle.id,
        Some(&handle.environment),
    )
    .map_err(bridge_io)?;

    if !args.reveal {
        return Ok(CommandOutput {
            json: json!({
                "command": "secret get",
                "mocked": false,
                "revealed": false,
                "object": {
                    "id": handle.id,
                    "name": handle.name,
                    "provider": handle.provider,
                    "uri": handle.uri,
                    "environment": handle.environment,
                    "required": handle.required
                }
            }),
            human: format!(
                "{}  {}  {}  {} (use --reveal with a grant to print the value)",
                handle.id, handle.name, handle.environment, handle.provider
            ),
        });
    }

    let resolved = resolve_handles(
        &cwd,
        &handles,
        std::slice::from_ref(&handle.id),
        args.provider.as_deref(),
    )
    .map_err(bridge_io)?;
    let value = resolved.values.get(&handle.name).cloned().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            format!("secret `{}` has no resolvable value", handle.name),
        )
    })?;

    Ok(CommandOutput {
        json: json!({
            "command": "secret get",
            "mocked": false,
            "revealed": true,
            "name": handle.name,
            "id": handle.id,
            "value": value
        }),
        human: value,
    })
}

fn set_output(args: SecretSetArgs) -> io::Result<CommandOutput> {
    let cwd = env::current_dir()?;
    let handles = load_secret_handles(&cwd).map_err(bridge_io)?;
    let handle = find_handle(&handles, &args.secret).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            format!("SecretRef `{}` not found", args.secret),
        )
    })?;
    let context = secret_policy_context().map_err(bridge_io)?;
    authorize_secret(
        &context,
        "secret.inject",
        &handle.id,
        Some(&handle.environment),
    )
    .map_err(bridge_io)?;

    let value = match args.value {
        Some(value) => value,
        None => {
            let mut buffer = String::new();
            io::stdin().read_line(&mut buffer)?;
            buffer.trim_end_matches(['\r', '\n']).to_owned()
        }
    };
    if value.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "empty secret value",
        ));
    }

    // Ensure the handle is registered and secretspec.toml is synced first.
    let _ = sync_secretspec_toml(&cwd, &handles).map_err(bridge_io)?;
    set_secret_value(&cwd, handle, value, args.provider.as_deref()).map_err(bridge_io)?;

    Ok(CommandOutput {
        json: json!({
            "command": "secret set",
            "mocked": false,
            "id": handle.id,
            "name": handle.name,
            "provider": args.provider.as_deref().unwrap_or(DEFAULT_PROVIDER),
            "stored": true
        }),
        human: format!("Stored {} via SecretSpec (value not printed)", handle.name),
    })
}

fn run_output(args: SecretRunArgs) -> io::Result<CommandOutput> {
    match execute_run(args, true)? {
        SecretRunResult::Output(output) => Ok(output),
        SecretRunResult::Exit(_) => unreachable!("json path returns Output"),
    }
}

fn find_handle<'a>(handles: &'a [SecretHandle], key: &str) -> Option<&'a SecretHandle> {
    handles
        .iter()
        .find(|handle| handle.id == key || handle.name == key)
}

fn bridge_io(error: BridgeError) -> io::Error {
    match error {
        BridgeError::Io(error) => error,
        BridgeError::Policy { .. } => {
            io::Error::new(io::ErrorKind::PermissionDenied, error.to_string())
        }
        BridgeError::NotFound(_) | BridgeError::MissingSecretspec(_) => {
            io::Error::new(io::ErrorKind::NotFound, error.to_string())
        }
        BridgeError::Spec(_) => io::Error::other(error.to_string()),
    }
}

/// Persist a SecretRef handle into `.sorrel/secrets/` for registry-backed workflows.
pub fn register_handle(handle: &SecretHandle) -> io::Result<PathBuf> {
    if !repo::is_initialized() {
        return Err(io::Error::other(
            "run `sorrel init` before registering secrets",
        ));
    }
    let object = json!({
        "schemaVersion": "sorrel.protocol.v0",
        "kind": "SecretRef",
        "id": handle.id,
        "name": handle.name,
        "provider": handle.provider,
        "uri": handle.uri,
        "environment": handle.environment,
        "required": handle.required,
        "valueType": "secret",
        "description": handle.description
    });
    repo::write_registry_entry(repo::SECRETS_DIR, &handle.id, &object)?;
    Ok(repo::sorrel_dir().join(repo::SECRETS_DIR).join(&handle.id))
}
