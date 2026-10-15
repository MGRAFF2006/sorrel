//! Bounded JSON adapter for the authoritative Core evaluator.
#[path = "support/policy_wire.rs"]
mod policy_wire;
use jiff::Timestamp;
use policy_wire::{evaluate, Result};
use serde_json::{json, Value};
use std::io::{self, Read, Write};
const MAX_BYTES: usize = 1024 * 1024;
fn run() -> Result<Value> {
    let mut bytes = Vec::new();
    io::stdin()
        .take((MAX_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| "cannot read policy request")?;
    if bytes.len() > MAX_BYTES {
        return Err("policy request exceeds byte limit".into());
    }
    let request = serde_json::from_slice(&bytes).map_err(|_| "invalid policy request")?;
    evaluate(request, Timestamp::now())
}

fn main() {
    let (value, mut failed) = match run() {
        Ok(value) => (value, false),
        Err(message) => (
            json!({ "error": { "code": "policy_evaluation_failed", "message": message } }),
            true,
        ),
    };
    let mut bytes = serde_json::to_vec(&value).expect("policy response serializes");
    if bytes.len() > MAX_BYTES {
        failed = true;
        bytes = br#"{"error":{"code":"policy_evaluation_failed","message":"policy response exceeds byte limit"}}"#.to_vec();
    }
    if io::stdout().write_all(&bytes).is_err() || failed {
        std::process::exit(1);
    }
}
