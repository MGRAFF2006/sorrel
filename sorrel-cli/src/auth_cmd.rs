//! `sorrel login`, `sorrel logout` and `sorrel auth status`.
//!
//! Login uses the Hub's device flow: the CLI requests a code, the user approves
//! it in a browser that is signed in to the Hub, and the CLI receives an opaque
//! `srl_cli_…` token that it stores for that Hub only. Tokens and device codes
//! are never printed.

use std::io::{self, Write};
use std::thread;
use std::time::{Duration, Instant};

use clap::{Args, Subcommand};
use serde_json::{json, Value};

use crate::credentials::{self, StoredCredential};
use crate::sync::HUB_TOKEN_ENV;
use crate::CommandOutput;

const DEVICE_GRANT_TYPE: &str = "urn:ietf:params:oauth:grant-type:device_code";
const CLI_TOKEN_PREFIX: &str = "srl_cli_";

/// Arguments for `sorrel login`.
#[derive(Debug, Args)]
pub struct LoginArgs {
    /// Hub URL, the same URL used for remotes (for example https://hub.example/api).
    pub hub_url: String,

    /// Label shown in the Hub UI for this token (default: "Sorrel CLI on <host>").
    #[arg(long)]
    pub label: Option<String>,

    /// Open the approval page in the default browser.
    #[arg(long)]
    pub open: bool,
}

/// Arguments for `sorrel logout`.
#[derive(Debug, Args)]
pub struct LogoutArgs {
    /// Hub URL (default: the only stored Hub).
    pub hub_url: Option<String>,
}

/// `sorrel auth` subcommands.
#[derive(Debug, Subcommand)]
pub enum AuthCommand {
    /// Show stored Hub credentials (never the token itself).
    Status,
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(30))
        .redirects(0)
        .build()
}

/// Send a request and return (status, JSON body) for 2xx and 4xx alike.
fn send(request: ureq::Request, body: Option<&Value>) -> io::Result<(u16, Value)> {
    let result = match body {
        Some(body) => request.send_json(body),
        None => request.call(),
    };
    let response = match result {
        Ok(response) => response,
        Err(ureq::Error::Status(_, response)) => response,
        Err(error) => return Err(io::Error::other(format!("cannot reach Hub: {error}"))),
    };
    let status = response.status();
    let text = response.into_string().unwrap_or_default();
    let value = serde_json::from_str(&text).unwrap_or(Value::Null);
    Ok((status, value))
}

fn hub_error(context: &str, status: u16, body: &Value) -> io::Error {
    let message = body
        .pointer("/error/message")
        .or_else(|| body.get("error_description"))
        .or_else(|| body.get("error"))
        .and_then(Value::as_str)
        .unwrap_or("unexpected response");
    io::Error::other(format!("{context}: Hub HTTP {status}: {message}"))
}

fn default_label() -> String {
    let host = ["HOSTNAME", "COMPUTERNAME"]
        .iter()
        .find_map(|name| std::env::var(name).ok())
        .map(|value| value.trim().to_owned())
        .filter(|value| {
            !value.is_empty() && value.len() <= 60 && !value.chars().any(char::is_control)
        });
    host.map_or_else(
        || "Sorrel CLI".to_owned(),
        |host| format!("Sorrel CLI on {host}"),
    )
}

/// Open `url` without a shell. Only called for an https or loopback URL.
fn open_browser(url: &str) -> bool {
    #[cfg(target_os = "macos")]
    let mut command = std::process::Command::new("open");
    #[cfg(windows)]
    let mut command = std::process::Command::new("explorer.exe");
    #[cfg(not(any(windows, target_os = "macos")))]
    let mut command = std::process::Command::new("xdg-open");
    command
        .arg(url)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .is_ok()
}

fn require_str<'a>(value: &'a Value, field: &str) -> io::Result<&'a str> {
    value
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| io::Error::other(format!("Hub device response is missing {field}")))
}

/// `sorrel login <hub-url>`.
pub fn login(args: LoginArgs) -> io::Result<CommandOutput> {
    let hub = credentials::normalize_hub_url(&args.hub_url)?;
    if !credentials::is_secure_hub_url(&hub) {
        return Err(io::Error::other(format!(
            "refusing to sign in to {hub} over plain HTTP; use https:// (http is allowed only for loopback Hubs)"
        )));
    }
    // Fail before contacting the Hub if stored credentials are unusable.
    let mut stored = credentials::load()?;
    let agent = agent();

    let (status, capabilities) = send(agent.get(&format!("{hub}/capabilities")), None)?;
    if status != 200 {
        return Err(hub_error("reading Hub capabilities", status, &capabilities));
    }
    if capabilities
        .pointer("/data/auth/cliLogin")
        .and_then(Value::as_str)
        != Some("device")
    {
        return Err(io::Error::other(format!(
            "{hub} does not offer CLI login; ask the Hub operator to enable browser sign-in, or set {HUB_TOKEN_ENV}"
        )));
    }

    let label = args.label.unwrap_or_else(default_label);
    let (status, device) = send(
        agent.post(&format!("{hub}/auth/cli/device")),
        Some(&json!({ "label": label })),
    )?;
    if status != 200 {
        return Err(hub_error("starting login", status, &device));
    }
    let device_code = require_str(&device, "device_code")?.to_owned();
    let user_code = require_str(&device, "user_code")?.to_owned();
    let verification = device
        .get("verification_uri_complete")
        .or_else(|| device.get("verification_uri"))
        .and_then(Value::as_str)
        .ok_or_else(|| io::Error::other("Hub device response is missing verification_uri"))?
        .to_owned();
    let expires_in = device
        .get("expires_in")
        .and_then(Value::as_u64)
        .unwrap_or(600);
    let mut interval = device
        .get("interval")
        .and_then(Value::as_u64)
        .unwrap_or(5)
        .max(1);

    // Instructions go to stderr so `--json` stdout stays machine-readable.
    let mut stderr = io::stderr().lock();
    writeln!(stderr, "To sign in, open {verification}")?;
    writeln!(stderr, "and confirm the code {user_code}")?;
    let safe_to_open = credentials::normalize_hub_url(verification.split('?').next().unwrap_or(""))
        .is_ok_and(|url| credentials::is_secure_hub_url(&url));
    if args.open && !(safe_to_open && open_browser(&verification)) {
        writeln!(stderr, "(could not open a browser; open the link manually)")?;
    }
    writeln!(stderr, "Waiting for approval…")?;
    drop(stderr);

    let deadline = Instant::now() + Duration::from_secs(expires_in);
    let token = loop {
        thread::sleep(Duration::from_secs(interval));
        if Instant::now() >= deadline {
            return Err(io::Error::other(
                "the login code expired; run sorrel login again",
            ));
        }
        let (status, body) = send(
            agent.post(&format!("{hub}/auth/cli/token")),
            Some(&json!({ "grant_type": DEVICE_GRANT_TYPE, "device_code": device_code })),
        )?;
        if status == 200 {
            break body;
        }
        match body.get("error").and_then(Value::as_str) {
            Some("authorization_pending") => {}
            Some("slow_down") => interval += 5,
            Some("access_denied") => {
                return Err(io::Error::other(
                    "the login request was denied in the browser",
                ))
            }
            Some("expired_token" | "invalid_grant") => {
                return Err(io::Error::other(
                    "the login code expired or was already used; run sorrel login again",
                ))
            }
            _ => return Err(hub_error("finishing login", status, &body)),
        }
    };

    let access_token = require_str(&token, "access_token")?;
    if !access_token.starts_with(CLI_TOKEN_PREFIX) {
        return Err(io::Error::other("Hub returned an unexpected token type"));
    }
    let credential = StoredCredential {
        token: access_token.to_owned(),
        token_id: require_str(&token, "token_id")?.to_owned(),
        principal: token.get("principal").cloned().unwrap_or(Value::Null),
        label: token
            .get("label")
            .and_then(Value::as_str)
            .unwrap_or(&label)
            .to_owned(),
        expires_at: require_str(&token, "expires_at")?.to_owned(),
    };
    let json = credential.public_json(&hub);
    let previous = stored.hubs.insert(hub.clone(), credential);
    credentials::save(&stored)?;
    if let Some(previous) = previous {
        // Best effort: retire the token this login replaced.
        let _ = revoke_on_hub(&hub, &previous.token);
    }
    let principal = principal_ref(&json["principal"]);
    let mut human = format!(
        "Signed in to {hub} as {principal} (expires {}).",
        json["expiresAt"].as_str().unwrap_or("?")
    );
    if std::env::var_os(HUB_TOKEN_ENV).is_some_and(|value| !value.is_empty()) {
        human.push_str(&format!(
            "\nNote: {HUB_TOKEN_ENV} is set and overrides this login."
        ));
    }
    Ok(CommandOutput { json, human })
}

fn principal_ref(principal: &Value) -> String {
    match (
        principal.get("type").and_then(Value::as_str),
        principal.get("id").and_then(Value::as_str),
    ) {
        (Some(kind), Some(id)) => format!("{kind}:{id}"),
        _ => "unknown principal".to_owned(),
    }
}

/// Revoke `token` on `hub`. Ok(true) when the Hub no longer accepts it.
fn revoke_on_hub(hub: &str, token: &str) -> io::Result<bool> {
    let request = agent()
        .delete(&format!("{hub}/auth/cli/tokens/current"))
        .set("Authorization", &format!("Bearer {token}"));
    let (status, _) = send(request, None)?;
    // 401: already revoked or expired, so it is unusable either way.
    Ok(status == 200 || status == 401)
}

/// `sorrel logout [<hub-url>]`.
pub fn logout(args: LogoutArgs) -> io::Result<CommandOutput> {
    let mut stored = credentials::load()?;
    let hub = match args.hub_url {
        Some(url) => credentials::normalize_hub_url(&url)?,
        None => match stored.hubs.len() {
            0 => return Err(io::Error::other("not signed in to any Hub")),
            1 => stored.hubs.keys().next().cloned().unwrap_or_default(),
            _ => {
                return Err(io::Error::other(format!(
                    "signed in to several Hubs; choose one: {}",
                    stored.hubs.keys().cloned().collect::<Vec<_>>().join(", ")
                )))
            }
        },
    };
    let Some(credential) = stored.hubs.remove(&hub) else {
        return Err(io::Error::other(format!("not signed in to {hub}")));
    };
    let revoked = match revoke_on_hub(&hub, &credential.token) {
        Ok(revoked) => revoked,
        Err(error) => {
            eprintln!("sorrel: {error}");
            false
        }
    };
    // Remove locally even if the Hub was unreachable: the user asked to sign out.
    credentials::save(&stored)?;
    let human = if revoked {
        format!("Signed out of {hub}; the token was revoked.")
    } else {
        format!(
            "Removed the stored token for {hub}, but the Hub did not confirm revocation; revoke token {} on your Hub profile.",
            credential.token_id
        )
    };
    Ok(CommandOutput {
        json: json!({ "hub": hub, "removed": true, "revoked": revoked, "tokenId": credential.token_id }),
        human,
    })
}

/// `sorrel auth status`.
pub fn status(command: &AuthCommand) -> io::Result<CommandOutput> {
    let AuthCommand::Status = command;
    let stored = credentials::load()?;
    let path = credentials::credentials_path()?;
    let env_override = std::env::var_os(HUB_TOKEN_ENV).is_some_and(|value| !value.is_empty());
    let hubs: Vec<Value> = stored
        .hubs
        .iter()
        .map(|(hub, credential)| credential.public_json(hub))
        .collect();
    let mut lines: Vec<String> = stored
        .hubs
        .iter()
        .map(|(hub, credential)| {
            format!(
                "{hub}: {} · {} · expires {}",
                principal_ref(&credential.principal),
                credential.label,
                credential.expires_at
            )
        })
        .collect();
    if lines.is_empty() {
        lines.push("Not signed in to any Hub. Run `sorrel login <hub-url>`.".to_owned());
    }
    if env_override {
        lines.push(format!(
            "{HUB_TOKEN_ENV} is set and overrides stored credentials for every Hub."
        ));
    }
    Ok(CommandOutput {
        json: json!({
            "credentialsFile": path.display().to_string(),
            "envTokenOverride": env_override,
            "hubs": hubs,
        }),
        human: lines.join("\n"),
    })
}
