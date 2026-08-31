//! Local structured execution logs under `.sorrel/runs/<id>/`.
//!
//! Media type: `application/vnd.sorrel.runner.log+jsonl;version=1`

use std::fs::{self, File, OpenOptions};
use std::io::{self, BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use clap::Subcommand;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::repo;
use crate::CommandOutput;

pub const LOG_MEDIA_TYPE: &str = "application/vnd.sorrel.runner.log+jsonl;version=1";
pub const RUNS_DIR: &str = "runs";

#[derive(Debug, Subcommand)]
pub enum RunCommand {
    /// List recent local runs.
    List,
    /// Show run metadata and step summary.
    Show { run_id: String },
    /// Print run log lines (JSONL).
    Logs {
        run_id: String,
        /// Reserved for streaming support; currently returns an unsupported error.
        #[arg(long)]
        follow: bool,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunManifest {
    pub schema_version: u32,
    pub id: String,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub backend: String,
    pub principal: String,
    pub workflow_id: Option<String>,
    pub job_name: Option<String>,
    pub status: String,
    pub exit_code: Option<i32>,
    pub injected_secrets: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum LogEvent {
    #[serde(rename = "started")]
    Started {
        ts: String,
        run_id: String,
        backend: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        workflow_id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        job_name: Option<String>,
    },
    #[serde(rename = "stream")]
    Stream {
        ts: String,
        stream: String,
        chunk: String,
    },
    #[serde(rename = "finished")]
    Finished {
        ts: String,
        status: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        exit_code: Option<i32>,
    },
}

pub fn execute(command: RunCommand) -> io::Result<CommandOutput> {
    match command {
        RunCommand::List => list_output(),
        RunCommand::Show { run_id } => show_output(&run_id),
        RunCommand::Logs { run_id, follow } => logs_output(&run_id, follow),
    }
}

pub fn runs_dir() -> PathBuf {
    repo::sorrel_dir().join(RUNS_DIR)
}

pub fn run_dir(run_id: &str) -> PathBuf {
    runs_dir().join(sanitize(run_id))
}

fn sanitize(id: &str) -> String {
    id.chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

/// Create a new run directory and write the started event.
pub fn begin_run(manifest: &RunManifest) -> io::Result<PathBuf> {
    if !repo::is_initialized() {
        return Err(io::Error::other("run `sorrel init` before recording runs"));
    }
    let dir = run_dir(&manifest.id);
    fs::create_dir_all(&dir)?;
    fs::write(
        dir.join("manifest.json"),
        serde_json::to_vec_pretty(manifest)?,
    )?;
    append_event(
        &dir,
        &LogEvent::Started {
            ts: manifest.started_at.clone(),
            run_id: manifest.id.clone(),
            backend: manifest.backend.clone(),
            workflow_id: manifest.workflow_id.clone(),
            job_name: manifest.job_name.clone(),
        },
    )?;
    Ok(dir)
}

pub fn append_stream(dir: &Path, stream: &str, chunk: &str) -> io::Result<()> {
    append_event(
        dir,
        &LogEvent::Stream {
            ts: now_rfc3339(),
            stream: stream.to_owned(),
            chunk: chunk.to_owned(),
        },
    )
}

pub fn finish_run(dir: &Path, mut manifest: RunManifest) -> io::Result<()> {
    let finished_at = now_rfc3339();
    append_event(
        dir,
        &LogEvent::Finished {
            ts: finished_at.clone(),
            status: manifest.status.clone(),
            exit_code: manifest.exit_code,
        },
    )?;
    manifest.finished_at = Some(finished_at);
    fs::write(
        dir.join("manifest.json"),
        serde_json::to_vec_pretty(&manifest)?,
    )?;
    Ok(())
}

fn append_event(dir: &Path, event: &LogEvent) -> io::Result<()> {
    let path = dir.join("log.jsonl");
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    serde_json::to_writer(&mut file, event)?;
    file.write_all(b"\n")?;
    Ok(())
}

#[must_use]
pub fn new_run_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    format!("run_{nanos}")
}

#[must_use]
pub fn now_rfc3339() -> String {
    repo::now_rfc3339()
}

fn list_output() -> io::Result<CommandOutput> {
    let dir = runs_dir();
    let mut runs = Vec::new();
    if dir.is_dir() {
        for entry in fs::read_dir(&dir)? {
            let entry = entry?;
            let manifest_path = entry.path().join("manifest.json");
            if !manifest_path.is_file() {
                continue;
            }
            let text = fs::read_to_string(manifest_path)?;
            if let Ok(value) = serde_json::from_str::<Value>(&text) {
                runs.push(value);
            }
        }
    }
    runs.sort_by(|left, right| {
        right["startedAt"]
            .as_str()
            .unwrap_or_default()
            .cmp(left["startedAt"].as_str().unwrap_or_default())
    });
    Ok(CommandOutput {
        json: json!({
            "command": "run list",
            "mocked": false,
            "mediaType": LOG_MEDIA_TYPE,
            "count": runs.len(),
            "runs": runs
        }),
        human: if runs.is_empty() {
            "No local runs under .sorrel/runs/".to_owned()
        } else {
            runs.iter()
                .filter_map(|run| {
                    Some(format!(
                        "{}  {}  {}",
                        run.get("id")?.as_str()?,
                        run.get("status")?.as_str().unwrap_or("?"),
                        run.get("backend")?.as_str().unwrap_or("?")
                    ))
                })
                .collect::<Vec<_>>()
                .join("\n")
        },
    })
}

fn show_output(run_id: &str) -> io::Result<CommandOutput> {
    let path = run_dir(run_id).join("manifest.json");
    if !path.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("run `{run_id}` not found"),
        ));
    }
    let manifest: Value = serde_json::from_str(&fs::read_to_string(path)?)?;
    Ok(CommandOutput {
        json: json!({
            "command": "run show",
            "mocked": false,
            "mediaType": LOG_MEDIA_TYPE,
            "run": manifest
        }),
        human: serde_json::to_string_pretty(&manifest).unwrap_or_default(),
    })
}

fn logs_output(run_id: &str, follow: bool) -> io::Result<CommandOutput> {
    if follow {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "run log following is not implemented; omit `--follow` for a one-shot read",
        ));
    }

    let path = run_dir(run_id).join("log.jsonl");
    if !path.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("run `{run_id}` log not found"),
        ));
    }
    let file = File::open(&path)?;
    let reader = BufReader::new(file);
    let mut lines = Vec::new();
    let mut human = String::new();
    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        if let Ok(value) = serde_json::from_str::<Value>(&line) {
            lines.push(value.clone());
            human.push_str(&format_log_line(&value));
            human.push('\n');
        } else {
            lines.push(json!({ "raw": line }));
        }
    }
    Ok(CommandOutput {
        json: json!({
            "command": "run logs",
            "mocked": false,
            "mediaType": LOG_MEDIA_TYPE,
            "runId": run_id,
            "follow": false,
            "events": lines
        }),
        human: human.trim_end().to_owned(),
    })
}

fn format_log_line(value: &Value) -> String {
    match value.get("type").and_then(Value::as_str) {
        Some("started") => format!(
            "started backend={} job={}",
            value.get("backend").and_then(Value::as_str).unwrap_or("?"),
            value.get("jobName").and_then(Value::as_str).unwrap_or("-")
        ),
        Some("stream") => format!(
            "[{}] {}",
            value.get("stream").and_then(Value::as_str).unwrap_or("out"),
            value.get("chunk").and_then(Value::as_str).unwrap_or("")
        ),
        Some("finished") => format!(
            "finished status={} exit={:?}",
            value.get("status").and_then(Value::as_str).unwrap_or("?"),
            value.get("exitCode")
        ),
        _ => value.to_string(),
    }
}
