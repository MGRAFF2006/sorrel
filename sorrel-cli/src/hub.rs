//! Hub collaboration companion client (proposals / lane-submit).
//!
//! Talks to a live `sorrel-hub` over HTTP — no mocks. Sync transport stays in
//! [`crate::sync`]; this module covers the product/metadata collaboration API.

use std::io;

use serde_json::{json, Value};

use crate::repo::Remote;
use crate::sync::DEFAULT_ACTING_PRINCIPAL;

/// Result of `POST /collaboration/lane-submit`.
#[derive(Debug, Clone)]
pub struct LaneSubmitResult {
    pub proposal: Value,
    pub reused: bool,
    pub project_id: String,
}

/// Ensure a Hub project exists for this workspace (create if missing).
pub fn ensure_project(hub_base_url: &str, organization_id: &str, name: &str) -> io::Result<String> {
    let base = hub_base_url.trim_end_matches('/');
    let agent = ureq::Agent::new();

    // Prefer an existing project with the same name in the org.
    let list_url = format!("{base}/projects?organizationId={organization_id}");
    let listed: Value = agent
        .get(&list_url)
        .call()
        .map_err(http_error)?
        .into_json()
        .map_err(|error| io::Error::other(error.to_string()))?;
    if let Some(projects) = listed.get("data").and_then(Value::as_array) {
        for project in projects {
            if project.get("name").and_then(Value::as_str) == Some(name) {
                if let Some(id) = project.get("id").and_then(Value::as_str) {
                    return Ok(id.to_owned());
                }
            }
        }
    }

    let created: Value = agent
        .post(&format!("{base}/projects"))
        .set("Content-Type", "application/json")
        .send_json(json!({
            "organizationId": organization_id,
            "name": name,
            "description": "Auto-created by sorrel lane submit",
        }))
        .map_err(http_error)?
        .into_json()
        .map_err(|error| io::Error::other(error.to_string()))?;

    created
        .pointer("/data/id")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| io::Error::other("hub project create missing data.id"))
}

/// Submit the active lane tip as a Hub proposal via `/collaboration/lane-submit`.
pub fn lane_submit(
    remote: &Remote,
    project_id: &str,
    title: &str,
    source_lane: &str,
    source_snapshot: &str,
    target_lane: &str,
) -> io::Result<LaneSubmitResult> {
    let base = remote.url.trim_end_matches('/');
    let body = json!({
        "projectId": project_id,
        "syncRepoId": remote.repo_id,
        "title": title,
        "sourceLane": source_lane,
        "targetLane": target_lane,
        "sourceSnapshot": source_snapshot,
        "authorPrincipal": serde_json::from_str::<Value>(DEFAULT_ACTING_PRINCIPAL)
            .unwrap_or_else(|_| json!({"type":"user","id":"local"})),
        "open": true,
    });

    let response = ureq::Agent::new()
        .post(&format!("{base}/collaboration/lane-submit"))
        .set("Content-Type", "application/json")
        .set("x-sorrel-acting-principal", DEFAULT_ACTING_PRINCIPAL)
        .send_json(&body)
        .map_err(http_error)?;

    let status = response.status();
    let payload: Value = response
        .into_json()
        .map_err(|error| io::Error::other(error.to_string()))?;

    if status != 200 && status != 201 {
        let message = payload
            .pointer("/error/message")
            .and_then(Value::as_str)
            .unwrap_or("lane-submit failed");
        return Err(io::Error::other(format!("hub lane-submit: {message}")));
    }

    let proposal = payload
        .get("data")
        .cloned()
        .ok_or_else(|| io::Error::other("hub lane-submit missing data"))?;
    let reused = payload
        .get("reused")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    Ok(LaneSubmitResult {
        proposal,
        reused,
        project_id: project_id.to_owned(),
    })
}

fn http_error(error: ureq::Error) -> io::Error {
    match error {
        ureq::Error::Status(code, response) => {
            let body = response.into_string().unwrap_or_default();
            io::Error::other(format!("hub HTTP {code}: {body}"))
        }
        other => io::Error::other(other.to_string()),
    }
}
