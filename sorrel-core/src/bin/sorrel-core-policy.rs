//! Bounded JSON adapter for Hub callers of the authoritative Core evaluator.
use jiff::Timestamp;
use serde::Deserialize;
use serde_json::{json, Map, Value};
use sorrel_core::policy::{
    evaluate_policy, Capability, DecisionKind, Grant, GrantEffect, Policy, PolicyEvaluationRequest,
    PrincipalDescriptor, PrincipalKind, ResourceRef, PROTOCOL_VERSION,
};
use std::collections::BTreeMap;
use std::io::{self, Read, Write};

const MAX_BYTES: usize = 1024 * 1024;
type Result<T> = std::result::Result<T, String>;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    principal: Value,
    action: String,
    resource: Value,
    #[serde(default)]
    grants: Vec<Value>,
    #[serde(default)]
    policies: Vec<Value>,
}

fn object<'a>(value: &'a Value, allowed: &[&str]) -> Result<&'a Map<String, Value>> {
    let fields = value.as_object().ok_or("expected an object")?;
    if fields
        .keys()
        .any(|field| !allowed.contains(&field.as_str()))
    {
        return Err("unsupported authorization field".into());
    }
    Ok(fields)
}

fn string(value: Option<&Value>) -> Result<&str> {
    value
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| "expected a non-empty authorization string".into())
}

fn version(fields: &Map<String, Value>, kind: &str, legacy: bool) -> Result<()> {
    if (!legacy || fields.contains_key("schemaVersion"))
        && string(fields.get("schemaVersion"))? != PROTOCOL_VERSION
    {
        return Err("unsupported authorization schemaVersion".into());
    }
    if (!legacy || fields.contains_key("kind")) && string(fields.get("kind"))? != kind {
        return Err("unsupported authorization object kind".into());
    }
    Ok(())
}

fn parse_principal(value: &Value) -> Result<PrincipalDescriptor> {
    let fields = object(value, &["kind", "type", "id", "displayName"])?;
    if fields.contains_key("kind") && fields.contains_key("type") {
        return Err("principal must use either kind or type".into());
    }
    let kind = string(fields.get("kind").or_else(|| fields.get("type")))?;
    let id = string(fields.get("id"))?;
    let (kind, id) = match kind {
        "service" => (PrincipalKind::Service, format!("service:{id}")),
        "workflow" => (PrincipalKind::Service, format!("workflow:{id}")),
        other => (
            serde_json::from_value(Value::String(other.into()))
                .map_err(|_| "unsupported principal kind")?,
            id.to_owned(),
        ),
    };
    let mut principal = PrincipalDescriptor::new(kind, id);
    if let Some(display_name) = fields.get("displayName") {
        principal.display_name = Some(
            display_name
                .as_str()
                .ok_or("displayName must be a string")?
                .to_owned(),
        );
    }
    Ok(principal)
}

fn resource(value: &Value) -> Result<ResourceRef> {
    let fields = object(value, &["kind", "id", "path"])?;
    string(fields.get("id"))?;
    if fields.contains_key("path") {
        return Err("path-scoped resources require the Core path-restriction update".into());
    }
    serde_json::from_value(value.clone()).map_err(|_| "unsupported resource reference".into())
}

fn timestamp(fields: &Map<String, Value>, field: &str) -> Result<Option<Timestamp>> {
    fields
        .get(field)
        .map(|value| {
            string(Some(value))?
                .parse::<Timestamp>()
                .map_err(|_| "invalid authorization timestamp".into())
        })
        .transpose()
}

fn grants(value: &Value, now: Timestamp) -> Result<Vec<Grant>> {
    let fields = object(
        value,
        &[
            "schemaVersion",
            "kind",
            "id",
            "source",
            "principal",
            "capabilities",
            "action",
            "resource",
            "resources",
            "effect",
            "reason",
            "metadata",
            "status",
            "issuedAt",
            "expiresAt",
            "revokedAt",
            "issuedBy",
            "conditions",
        ],
    )?;
    let legacy = fields.contains_key("action");
    version(fields, "Grant", legacy)?;
    if fields.get("source").is_some_and(|value| value != "core") {
        return Err("unsupported authorization source".into());
    }
    if let Some(conditions) = fields.get("conditions") {
        if conditions
            .as_object()
            .is_none_or(|fields| !fields.is_empty())
        {
            return Err("unsupported grant conditions".into());
        }
    }
    let id = string(fields.get("id"))?;
    let principal = parse_principal(fields.get("principal").ok_or("missing principal")?)?;
    if let Some(issued_by) = fields.get("issuedBy") {
        parse_principal(issued_by)?;
    }
    let capabilities = if legacy {
        if fields.contains_key("capabilities") {
            return Err("grant must use either action or capabilities".into());
        }
        vec![Capability::new(string(fields.get("action"))?)]
    } else {
        let values = fields
            .get("capabilities")
            .and_then(Value::as_array)
            .ok_or("missing capabilities")?;
        values
            .iter()
            .map(|value| string(Some(value)).map(Capability::new))
            .collect::<Result<Vec<_>>>()?
    };
    if capabilities.is_empty() {
        return Err("grant capabilities cannot be empty".into());
    }
    let resources = match (fields.get("resource"), fields.get("resources")) {
        (Some(value), None) => vec![resource(value)?],
        (None, Some(values)) => values
            .as_array()
            .ok_or("resources must be an array")?
            .iter()
            .map(resource)
            .collect::<Result<Vec<_>>>()?,
        _ => return Err("grant must specify resource or resources".into()),
    };
    if resources.is_empty() {
        return Err("grant resources cannot be empty".into());
    }
    let effect = match string(fields.get("effect"))? {
        "require-approval" => GrantEffect::Review,
        value => serde_json::from_value(Value::String(value.into()))
            .map_err(|_| "unsupported grant effect")?,
    };
    let metadata: BTreeMap<String, String> = fields
        .get("metadata")
        .map(|value| serde_json::from_value(value.clone()))
        .transpose()
        .map_err(|_| "grant metadata must contain strings")?
        .unwrap_or_default();
    let reason = fields
        .get("reason")
        .map(|value| {
            value
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| "reason must be a string".to_owned())
        })
        .transpose()?;
    let status = fields
        .get("status")
        .map(|value| string(Some(value)))
        .transpose()?
        .unwrap_or("active");
    if !["active", "expired", "revoked"].contains(&status) {
        return Err("unsupported grant status".into());
    }
    let issued_at = timestamp(fields, "issuedAt")?;
    let expires_at = timestamp(fields, "expiresAt")?;
    let revoked_at = timestamp(fields, "revokedAt")?;
    if status != "active"
        || revoked_at.is_some()
        || issued_at.is_some_and(|at| at > now)
        || expires_at.is_some_and(|at| at <= now)
    {
        return Ok(Vec::new());
    }
    Ok(resources
        .into_iter()
        .map(|resource| {
            let mut grant = Grant::new(
                id,
                principal.clone(),
                capabilities[0].clone(),
                resource,
                effect,
            );
            grant.capabilities = capabilities.clone();
            grant.reason = reason.clone();
            grant.metadata = metadata.clone();
            grant
        })
        .collect())
}

fn policy(value: &Value) -> Result<Policy> {
    let fields = object(
        value,
        &[
            "schemaVersion",
            "kind",
            "id",
            "resource",
            "rules",
            "defaultDecision",
        ],
    )?;
    version(fields, "Policy", false)?;
    string(fields.get("id"))?;
    let mut converted = value.clone();
    resource(fields.get("resource").ok_or("missing policy resource")?)?;
    if let Some(rules) = fields.get("rules") {
        for rule in rules.as_array().ok_or("rules must be an array")? {
            let rule = object(
                rule,
                &[
                    "id",
                    "effect",
                    "principal",
                    "capabilities",
                    "resources",
                    "reason",
                ],
            )?;
            string(rule.get("id"))?;
            if let Some(capabilities) = rule.get("capabilities") {
                for value in capabilities
                    .as_array()
                    .ok_or("capabilities must be an array")?
                {
                    string(Some(value))?;
                }
            }
            if let Some(resources) = rule.get("resources") {
                for value in resources.as_array().ok_or("resources must be an array")? {
                    resource(value)?;
                }
            }
        }
        for rule in converted["rules"]
            .as_array_mut()
            .ok_or("rules must be an array")?
        {
            if let Some(value) = rule.get("principal") {
                rule["principal"] = serde_json::to_value(parse_principal(value)?)
                    .map_err(|_| "invalid principal")?;
            }
        }
    }
    serde_json::from_value(converted).map_err(|_| "unsupported native policy".into())
}

fn evaluate(request: Request, now: Timestamp) -> Result<Value> {
    if request.action.trim().is_empty() {
        return Err("missing action".into());
    }
    let request_native = PolicyEvaluationRequest {
        principal: parse_principal(&request.principal)?,
        capability: Capability::new(request.action),
        resource: resource(&request.resource)?,
    };
    let grants = request
        .grants
        .iter()
        .map(|value| grants(value, now))
        .collect::<Result<Vec<_>>>()?
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    let policies = request
        .policies
        .iter()
        .map(policy)
        .collect::<Result<Vec<_>>>()?;
    let mut decision = evaluate_policy(&request_native, &grants, &policies);
    decision.evaluated_at = now.to_string();
    Ok(json!({ "allowed": decision.decision == DecisionKind::Allow, "decision": decision }))
}

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

#[cfg(test)]
mod tests {
    use super::*;
    fn request() -> Value {
        json!({"principal":{"type":"user","id":"local"},"action":"repo.object.write","resource":{"kind":"repo","id":"repo_main"},"grants":[]})
    }
    fn grant(effect: &str) -> Value {
        json!({"schemaVersion":PROTOCOL_VERSION,"kind":"Grant","id":format!("grant_{effect}"),"principal":{"kind":"user","id":"local"},"capabilities":["repo.object.write"],"resource":{"kind":"repo","id":"*"},"effect":effect})
    }
    fn result(value: Value) -> Result<Value> {
        evaluate(
            serde_json::from_value(value).unwrap(),
            "2026-01-01T00:00:00Z".parse().unwrap(),
        )
    }
    #[test]
    fn core_precedence_and_default_are_authoritative() {
        for (effects, expected) in [
            (vec![], "needs_grant"),
            (vec!["allow"], "allow"),
            (vec!["allow", "deny"], "deny"),
            (vec!["allow", "redact"], "redact"),
            (vec!["allow", "review"], "needs_review"),
        ] {
            let mut value = request();
            value["grants"] = json!(effects.into_iter().map(grant).collect::<Vec<_>>());
            let decision = result(value).unwrap();
            assert_eq!(decision["decision"]["decision"], expected);
            assert_eq!(decision["allowed"], expected == "allow");
        }
    }
    #[test]
    fn lifecycle_and_conditions_are_not_discarded() {
        for fields in [
            json!({"status":"revoked"}),
            json!({"status":"expired"}),
            json!({"revokedAt":"2025-01-01T00:00:00Z"}),
            json!({"expiresAt":"2025-12-31T23:59:59Z"}),
            json!({"issuedAt":"2026-01-01T01:00:00+00:00"}),
        ] {
            let mut g = grant("allow");
            g.as_object_mut()
                .unwrap()
                .extend(fields.as_object().unwrap().clone());
            let mut value = request();
            value["grants"] = json!([g]);
            assert_eq!(result(value).unwrap()["allowed"], false);
        }
        for fields in [
            json!({"expiresAt":null}),
            json!({"expiresAt":"invalid"}),
            json!({"conditions":{"runner":"only"}}),
            json!({"schemaVersion":"future"}),
            json!({"kind":"Policy"}),
            json!({"unhandledConstraint":true}),
        ] {
            let mut g = grant("allow");
            g.as_object_mut()
                .unwrap()
                .extend(fields.as_object().unwrap().clone());
            let mut value = request();
            value["grants"] = json!([g]);
            assert!(result(value).is_err());
        }
    }
    #[test]
    fn explicit_legacy_records_and_native_plural_resources_convert_deliberately() {
        let mut value = request();
        value["grants"] = json!([{"id":"legacy","source":"core","principal":{"type":"user","id":"local"},"action":"repo.object.write","resource":{"kind":"repo","id":"*"},"effect":"allow"}]);
        assert_eq!(result(value.clone()).unwrap()["allowed"], true);
        value["grants"][0].as_object_mut().unwrap().remove("effect");
        assert!(result(value).is_err());
        let mut g = grant("allow");
        g.as_object_mut().unwrap().remove("resource");
        g["resources"] =
            json!([{"kind":"repo","id":"repo_other"},{"kind":"repo","id":"repo_main"}]);
        let mut value = request();
        value["grants"] = json!([g]);
        assert_eq!(result(value).unwrap()["allowed"], true);
    }
    #[test]
    fn service_and_workflow_principals_cannot_collide() {
        for (acting, granted, allowed) in [
            ("workflow", "workflow", true),
            ("service", "service", true),
            ("service", "workflow", false),
            ("workflow", "service", false),
        ] {
            let mut value = request();
            value["principal"] = json!({"type":acting,"id":"same"});
            let mut g = grant("allow");
            g["principal"] = json!({"kind":granted,"id":"same"});
            value["grants"] = json!([g]);
            assert_eq!(result(value).unwrap()["allowed"], allowed);
        }
    }
    #[test]
    fn trusted_native_policy_can_deny_and_unknown_rules_fail_closed() {
        let mut value = request();
        value["grants"] = json!([grant("allow")]);
        value["policies"] = json!([{"schemaVersion":PROTOCOL_VERSION,"kind":"Policy","id":"policy","resource":{"kind":"repo","id":"*"},"rules":[{"id":"deny","effect":"deny","principal":{"type":"user","id":"local"},"capabilities":["repo.object.write"],"resources":[]}]}]);
        assert_eq!(result(value.clone()).unwrap()["allowed"], false);
        value["policies"][0]["rules"][0]["conditions"] = json!({"ignored":true});
        assert!(result(value).is_err());
    }
}
