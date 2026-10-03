//! Shared local agent registry. Claims coordinate work; they do not enforce policy.
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::Path;
use std::thread;
use std::time::{Duration, Instant};

use clap::Subcommand;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::{repo, CommandOutput};

#[derive(Debug, Subcommand)]
pub enum AgentCommand {
    /// Register an agent, its lane, and optional workspace/task.
    Register {
        id: String,
        #[arg(long, default_value = "lane_main")]
        lane: String,
        #[arg(long)]
        display_name: Option<String>,
        #[arg(long)]
        workspace: Option<String>,
        #[arg(long)]
        task: Option<String>,
    },
    /// Record an advisory claim; overlapping work remains allowed and visible.
    Claim { agent_id: String, path: String },
    /// Release an advisory claim.
    Release { agent_id: String, path: String },
    /// List registered agents, claims, and overlapping paths.
    Active,
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message.into())
}

pub fn valid_id(id: &str) -> io::Result<()> {
    if id.is_empty()
        || id.len() > 64
        || !id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    {
        return Err(invalid(
            "agent id must contain 1–64 ASCII letters, digits, underscores or hyphens",
        ));
    }
    Ok(())
}

pub fn normalized_path(path: &str) -> io::Result<String> {
    let path = path.replace('\\', "/");
    if path.is_empty()
        || path.chars().any(char::is_control)
        || path.starts_with('/')
        || (path.as_bytes().get(1) == Some(&b':')
            && path.as_bytes().first().is_some_and(u8::is_ascii_alphabetic))
    {
        return Err(invalid(
            "claim path must be a nonempty relative path without control characters",
        ));
    }
    let parts: Vec<_> = path
        .split('/')
        .filter(|part| !part.is_empty() && *part != ".")
        .collect();
    if parts.is_empty() || parts.contains(&"..") {
        return Err(invalid(
            "claim path must name a file or directory without parent traversal",
        ));
    }
    Ok(parts.join("/"))
}

fn string<'a>(record: &'a Value, key: &str) -> io::Result<&'a str> {
    record
        .get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| invalid(format!("invalid {key} in agent registry")))
}

fn timestamp(record: &Value, key: &str) -> io::Result<()> {
    let value = string(record, key)?;
    let bytes = value.as_bytes();
    let shape = bytes.len() >= 20
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes[10] == b'T'
        && bytes[13] == b':'
        && bytes[16] == b':'
        && bytes.last() == Some(&b'Z')
        && bytes[..19]
            .iter()
            .enumerate()
            .all(|(i, b)| [4, 7, 10, 13, 16].contains(&i) || b.is_ascii_digit())
        && (bytes.len() == 20
            || (bytes[19] == b'.'
                && bytes.len() > 21
                && bytes[20..bytes.len() - 1].iter().all(u8::is_ascii_digit)));
    if !shape {
        return Err(invalid(format!(
            "invalid {key}; expected UTC ISO timestamp"
        )));
    }
    for (range, min, max) in [
        (5..7, 1, 12),
        (8..10, 1, 31),
        (11..13, 0, 23),
        (14..16, 0, 59),
        (17..19, 0, 59),
    ] {
        let number = value[range]
            .parse::<u32>()
            .map_err(|_| invalid(format!("invalid {key}")))?;
        if !(min..=max).contains(&number) {
            return Err(invalid(format!("invalid {key}")));
        }
    }
    Ok(())
}

fn validate_agent(record: &Value) -> io::Result<String> {
    let id = string(record, "id")?;
    valid_id(id)?;
    string(record, "lane")?;
    string(record, "displayName")?;
    timestamp(record, "registeredAt")?;
    for key in ["workspace", "task"] {
        if record.get(key).is_some() {
            string(record, key)?;
        }
    }
    Ok(id.to_owned())
}

fn claim_id(agent_id: &str, path: &str) -> io::Result<String> {
    let bytes = serde_json::to_vec(&[agent_id, path])?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

fn validate_claim(record: &Value) -> io::Result<String> {
    let id = string(record, "agentId")?;
    valid_id(id)?;
    let path = string(record, "path")?;
    if normalized_path(path)? != path {
        return Err(invalid("noncanonical claim path"));
    }
    if record.get("mode").and_then(Value::as_str) != Some("advisory") {
        return Err(invalid("blocking claims are not implemented; use advisory"));
    }
    timestamp(record, "claimedAt")?;
    claim_id(id, path)
}

fn sync_dir(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    File::open(path)?.sync_all()?;
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

fn directory(path: &Path) -> io::Result<()> {
    fs::create_dir_all(path)?;
    if !fs::symlink_metadata(path)?.file_type().is_dir() {
        return Err(invalid(format!(
            "registry directory must not be a symlink: {}",
            path.display()
        )));
    }
    Ok(())
}

fn create_record(path: &Path, record: &Value) -> io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| invalid("record has no parent"))?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    serde_json::to_writer_pretty(&mut temporary, record)?;
    writeln!(temporary)?;
    temporary.as_file().sync_all()?;
    match fs::hard_link(temporary.path(), path) {
        Ok(()) => sync_dir(parent),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => Ok(()),
        Err(error) => Err(error),
    }
}

fn migrate(state_dir: &Path) -> io::Result<()> {
    let legacy = state_dir.join("state.json");
    if !legacy.try_exists()? {
        return Ok(());
    }
    let lock = state_dir.join(".migration.lock");
    let start = Instant::now();
    loop {
        match fs::create_dir(&lock) {
            Ok(()) => break,
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                if start.elapsed() >= Duration::from_secs(2) {
                    return Err(invalid("agent registry migration is locked; inspect .migration.lock before retrying"));
                }
                thread::sleep(Duration::from_millis(10));
            }
            Err(error) => return Err(error),
        }
    }
    let result = (|| {
        if !legacy.try_exists()? {
            return Ok(());
        }
        if !fs::symlink_metadata(&legacy)?.file_type().is_file() {
            return Err(invalid("legacy registry must be a regular file"));
        }
        let backup = state_dir.join("state.migrated.json");
        if backup.try_exists()? {
            return Err(invalid(
                "legacy migration backup already exists; inspect state.json",
            ));
        }
        let value: Value = serde_json::from_slice(&fs::read(&legacy)?)?;
        let agents = value
            .get("agents")
            .and_then(Value::as_array)
            .ok_or_else(|| invalid("invalid legacy agents"))?;
        let claims = value
            .get("claims")
            .and_then(Value::as_array)
            .ok_or_else(|| invalid("invalid legacy claims"))?;
        let mut ids = BTreeSet::new();
        for record in agents {
            ids.insert(validate_agent(record)?);
        }
        let mut normalized_claims = Vec::new();
        for record in claims {
            let mut record = record.clone();
            record["path"] = json!(normalized_path(string(&record, "path")?)?);
            validate_claim(&record)?;
            if !ids.contains(string(&record, "agentId")?) {
                return Err(invalid("legacy claim references unknown agent"));
            }
            normalized_claims.push(record);
        }
        for record in agents {
            create_record(
                &state_dir
                    .join("agents")
                    .join(format!("{}.json", validate_agent(record)?)),
                record,
            )?;
        }
        for record in &normalized_claims {
            create_record(
                &state_dir
                    .join("claims")
                    .join(format!("{}.json", validate_claim(record)?)),
                record,
            )?;
        }
        fs::rename(legacy, backup)?;
        sync_dir(state_dir)
    })();
    let cleanup = fs::remove_dir(lock);
    result.and(cleanup)
}

fn prepare(state_dir: &Path) -> io::Result<()> {
    for path in [
        state_dir.to_path_buf(),
        state_dir.join("agents"),
        state_dir.join("claims"),
    ] {
        directory(&path)?;
    }
    migrate(state_dir)
}

fn records(
    directory: &Path,
    validate: fn(&Value) -> io::Result<String>,
) -> io::Result<BTreeMap<String, Value>> {
    let mut records = BTreeMap::new();
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|v| v.to_str()) != Some("json") {
            continue;
        }
        let load = (|| {
            if !fs::symlink_metadata(&path)?.file_type().is_file() {
                return Err(invalid("record must be a regular file"));
            }
            let value = serde_json::from_slice(&fs::read(&path)?)?;
            let id = validate(&value)?;
            if path.file_name().and_then(|v| v.to_str()) != Some(format!("{id}.json").as_str()) {
                return Err(invalid("registry filename does not match record"));
            }
            Ok((id, value))
        })();
        match load {
            Ok((id, record)) => {
                records.insert(id, record);
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => {
                return Err(invalid(format!(
                    "invalid registry record {}: {error}",
                    path.display()
                )))
            }
        }
    }
    Ok(records)
}

pub fn load_active(state_dir: &Path) -> io::Result<Value> {
    prepare(state_dir)?;
    // Claims are published after agent records, so read agents last.
    let claims = records(&state_dir.join("claims"), validate_claim)?;
    let agents = records(&state_dir.join("agents"), validate_agent)?;
    for claim in claims.values() {
        if !agents.contains_key(string(claim, "agentId")?) {
            return Err(invalid("claim references unknown agent"));
        }
    }
    let claims: Vec<Value> = claims.into_values().collect();
    let mut overlaps: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    // ponytail: pairwise scan; use a trie if thousands of live claims need it.
    for (index, claim) in claims.iter().enumerate() {
        for other in &claims[index + 1..] {
            let a = string(claim, "agentId")?;
            let b = string(other, "agentId")?;
            if a == b {
                continue;
            }
            let path = string(claim, "path")?;
            let other_path = string(other, "path")?;
            let ancestor = if path == other_path || other_path.starts_with(&format!("{path}/")) {
                Some(path)
            } else if path.starts_with(&format!("{other_path}/")) {
                Some(other_path)
            } else {
                None
            };
            if let Some(ancestor) = ancestor {
                let ids = overlaps.entry(ancestor.to_owned()).or_default();
                ids.insert(a.to_owned());
                ids.insert(b.to_owned());
            }
        }
    }
    let overlaps: Vec<_> = overlaps
        .into_iter()
        .map(|(path, ids)| json!({"path":path,"agentIds":ids}))
        .collect();
    Ok(
        json!({"agents": agents.into_values().collect::<Vec<_>>(), "claims":claims, "overlaps":overlaps}),
    )
}

pub fn register_record(state_dir: &Path, record: &Value) -> io::Result<()> {
    let id = validate_agent(record)?;
    load_active(state_dir)?;
    repo::write_json_atomic(&state_dir.join("agents").join(format!("{id}.json")), record)
}

pub fn execute(command: AgentCommand) -> io::Result<CommandOutput> {
    if !repo::is_initialized() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "not a Sorrel repository; run sorrel init",
        ));
    }
    let state_dir = repo::sorrel_dir().join("agents");
    let active = load_active(&state_dir)?;
    let agents = active["agents"]
        .as_array()
        .ok_or_else(|| invalid("invalid agents"))?;
    match command {
        AgentCommand::Register {
            id,
            lane,
            display_name,
            workspace,
            task,
        } => {
            valid_id(&id)?;
            let previous = agents.iter().find(|record| record["id"] == id);
            let mut record = previous.cloned().unwrap_or_else(|| {
                json!({
                    "id":id,"registeredAt":repo::now_rfc3339()
                })
            });
            record["lane"] = json!(lane);
            record["displayName"] = json!(display_name.unwrap_or_else(|| previous
                .and_then(|r| r["displayName"].as_str())
                .unwrap_or(&id)
                .to_owned()));
            if let Some(workspace) = workspace {
                record["workspace"] = json!(workspace);
            }
            if let Some(task) = task {
                record["task"] = json!(task);
            }
            register_record(&state_dir, &record)?;
            Ok(CommandOutput {
                json: json!({"agent":record}),
                human: format!("Registered agent {id}"),
            })
        }
        AgentCommand::Claim { agent_id, path } => {
            valid_id(&agent_id)?;
            let path = normalized_path(&path)?;
            if !agents.iter().any(|r| r["id"] == agent_id) {
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("unknown agent {agent_id}"),
                ));
            }
            let record = json!({"agentId":agent_id,"path":path,"mode":"advisory","claimedAt":repo::now_rfc3339()});
            repo::write_json_atomic(
                &state_dir
                    .join("claims")
                    .join(format!("{}.json", validate_claim(&record)?)),
                &record,
            )?;
            Ok(CommandOutput {
                json: json!({"claim":record}),
                human: format!("Agent {agent_id} claimed {path} (advisory)"),
            })
        }
        AgentCommand::Release { agent_id, path } => {
            valid_id(&agent_id)?;
            let path = normalized_path(&path)?;
            let released = match fs::remove_file(
                state_dir
                    .join("claims")
                    .join(format!("{}.json", claim_id(&agent_id, &path)?)),
            ) {
                Ok(()) => {
                    sync_dir(&state_dir.join("claims"))?;
                    true
                }
                Err(error) if error.kind() == io::ErrorKind::NotFound => false,
                Err(error) => return Err(error),
            };
            Ok(CommandOutput {
                json: json!({"agentId":agent_id,"path":path,"released":released}),
                human: format!(
                    "Claim {path}: {}",
                    if released { "released" } else { "absent" }
                ),
            })
        }
        AgentCommand::Active => Ok(CommandOutput {
            human: format!(
                "{} agents, {} claims, {} overlapping paths",
                agents.len(),
                active["claims"].as_array().map_or(0, Vec::len),
                active["overlaps"].as_array().map_or(0, Vec::len)
            ),
            json: active,
        }),
    }
}
