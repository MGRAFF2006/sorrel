use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const PROTOCOL_VERSION: &str = "sorrel.protocol.v0";

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PrincipalId(pub String);

impl PrincipalId {
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PrincipalKind {
    User,
    Agent,
    Team,
    Service,
    Runner,
    System,
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrincipalDescriptor {
    pub kind: PrincipalKind,
    pub id: PrincipalId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
}

impl PrincipalDescriptor {
    #[must_use]
    pub fn new(kind: PrincipalKind, id: impl Into<String>) -> Self {
        Self {
            kind,
            id: PrincipalId::new(id),
            display_name: None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Capability(pub String);

impl Capability {
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ResourceKind {
    Org,
    Project,
    Repo,
    Path,
    File,
    Symbol,
    Change,
    Stack,
    Proposal,
    CommentThread,
    Secret,
    Environment,
    Agent,
    Runner,
    Workflow,
    MarketplaceApp,
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceRef {
    pub kind: ResourceKind,
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

impl ResourceRef {
    #[must_use]
    pub fn new(kind: ResourceKind, id: impl Into<String>) -> Self {
        Self {
            kind,
            id: id.into(),
            path: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecisionKind {
    Allow,
    Deny,
    Redact,
    NeedsGrant,
    NeedsReview,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GrantEffect {
    Allow,
    Deny,
    Redact,
    Review,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Grant {
    #[serde(default = "protocol_version")]
    pub schema_version: String,
    #[serde(default = "grant_kind")]
    pub kind: String,
    pub id: String,
    pub principal: PrincipalDescriptor,
    pub capabilities: Vec<Capability>,
    pub resource: ResourceRef,
    pub effect: GrantEffect,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: BTreeMap<String, String>,
}

impl Grant {
    #[must_use]
    pub fn new(
        id: impl Into<String>,
        principal: PrincipalDescriptor,
        capability: Capability,
        resource: ResourceRef,
        effect: GrantEffect,
    ) -> Self {
        Self {
            schema_version: PROTOCOL_VERSION.to_owned(),
            kind: "Grant".to_owned(),
            id: id.into(),
            principal,
            capabilities: vec![capability],
            resource,
            effect,
            reason: None,
            metadata: BTreeMap::new(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Policy {
    #[serde(default = "protocol_version")]
    pub schema_version: String,
    #[serde(default = "policy_kind")]
    pub kind: String,
    pub id: String,
    pub resource: ResourceRef,
    #[serde(default)]
    pub rules: Vec<PolicyRule>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_decision: Option<DecisionKind>,
}

impl Policy {
    #[must_use]
    pub fn new(id: impl Into<String>, resource: ResourceRef) -> Self {
        Self {
            schema_version: PROTOCOL_VERSION.to_owned(),
            kind: "Policy".to_owned(),
            id: id.into(),
            resource,
            rules: Vec::new(),
            default_decision: None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PolicyRule {
    pub id: String,
    pub effect: GrantEffect,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub principal: Option<PrincipalDescriptor>,
    #[serde(default)]
    pub capabilities: Vec<Capability>,
    #[serde(default)]
    pub resources: Vec<ResourceRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SecretRef {
    #[serde(default = "protocol_version")]
    pub schema_version: String,
    #[serde(default = "secret_ref_kind")]
    pub kind: String,
    pub id: String,
    pub name: String,
    pub uri: String,
    pub environment: String,
    pub required: bool,
    pub value_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resource: Option<ResourceRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub redaction: Option<RedactionMarker>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RedactionMarker {
    #[serde(default = "protocol_version")]
    pub schema_version: String,
    #[serde(default = "redaction_kind")]
    pub kind: String,
    pub id: String,
    pub resource: ResourceRef,
    pub marker: String,
    pub reason: String,
}

impl RedactionMarker {
    #[must_use]
    pub fn for_resource(resource: ResourceRef) -> Self {
        let id = format!("redaction_{}", sanitize_id(&resource.id));
        Self {
            schema_version: PROTOCOL_VERSION.to_owned(),
            kind: "RedactionMarker".to_owned(),
            marker: format!("<sorrel:redacted {}>", resource.id),
            reason: "Policy requires redaction for this resource".to_owned(),
            id,
            resource,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PolicyEvaluationRequest {
    pub principal: PrincipalDescriptor,
    pub capability: Capability,
    pub resource: ResourceRef,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ObjectRef {
    pub kind: String,
    pub id: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PolicyDecision {
    #[serde(default = "protocol_version")]
    pub schema_version: String,
    #[serde(default = "policy_decision_kind")]
    pub kind: String,
    pub id: String,
    pub principal: PrincipalDescriptor,
    pub capability: Capability,
    pub resource: ResourceRef,
    pub decision: DecisionKind,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub matched_grants: Vec<ObjectRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matched_policy: Option<ObjectRef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub redactions: Vec<RedactionMarker>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    pub evaluated_at: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditEvent {
    #[serde(default = "protocol_version")]
    pub schema_version: String,
    #[serde(default = "audit_event_kind")]
    pub kind: String,
    pub id: String,
    pub actor: PrincipalDescriptor,
    pub capability: Capability,
    pub resource: ResourceRef,
    pub decision: ObjectRef,
    pub outcome: DecisionKind,
    pub occurred_at: String,
}

pub fn evaluate_policy(
    request: &PolicyEvaluationRequest,
    grants: &[Grant],
    policies: &[Policy],
) -> PolicyDecision {
    let mut effects = Vec::new();
    let mut matched_grants = Vec::new();
    let mut matched_policy = None;

    let mut sorted_grants = grants.iter().collect::<Vec<_>>();
    sorted_grants.sort_by(|left, right| left.id.cmp(&right.id));
    for grant in sorted_grants {
        if grant_matches(grant, request) {
            effects.push(grant.effect);
            matched_grants.push(ObjectRef {
                kind: "Grant".to_owned(),
                id: grant.id.clone(),
            });
        }
    }

    let mut sorted_policies = policies.iter().collect::<Vec<_>>();
    sorted_policies.sort_by(|left, right| left.id.cmp(&right.id));
    for policy in sorted_policies {
        if !resource_matches(&policy.resource, &request.resource) {
            continue;
        }

        matched_policy.get_or_insert_with(|| ObjectRef {
            kind: "Policy".to_owned(),
            id: policy.id.clone(),
        });

        for rule in policy
            .rules
            .iter()
            .filter(|rule| rule_matches(rule, request))
        {
            effects.push(rule.effect);
            matched_grants.push(ObjectRef {
                kind: "PolicyRule".to_owned(),
                id: rule.id.clone(),
            });
        }

        if let Some(default_decision) = policy.default_decision {
            effects.push(match default_decision {
                DecisionKind::Allow => GrantEffect::Allow,
                DecisionKind::Deny => GrantEffect::Deny,
                DecisionKind::Redact => GrantEffect::Redact,
                DecisionKind::NeedsReview => GrantEffect::Review,
                DecisionKind::NeedsGrant => continue,
            });
        }
    }

    let decision = if effects.contains(&GrantEffect::Deny) {
        DecisionKind::Deny
    } else if effects.contains(&GrantEffect::Redact) {
        DecisionKind::Redact
    } else if effects.contains(&GrantEffect::Review) {
        DecisionKind::NeedsReview
    } else if effects.contains(&GrantEffect::Allow) {
        DecisionKind::Allow
    } else {
        DecisionKind::NeedsGrant
    };

    let redactions = if decision == DecisionKind::Redact {
        vec![RedactionMarker::for_resource(request.resource.clone())]
    } else {
        Vec::new()
    };

    PolicyDecision {
        schema_version: PROTOCOL_VERSION.to_owned(),
        kind: "PolicyDecision".to_owned(),
        id: format!(
            "decision_{}_{}_{}",
            sanitize_id(&request.principal.id.0),
            sanitize_id(&request.capability.0),
            sanitize_id(&request.resource.id)
        ),
        principal: request.principal.clone(),
        capability: request.capability.clone(),
        resource: request.resource.clone(),
        decision,
        matched_grants,
        matched_policy,
        redactions,
        reason: Some(reason_for(decision).to_owned()),
        evaluated_at: "1970-01-01T00:00:00Z".to_owned(),
    }
}

impl AuditEvent {
    #[must_use]
    pub fn from_decision(
        id: impl Into<String>,
        decision: &PolicyDecision,
        occurred_at: impl Into<String>,
    ) -> Self {
        Self {
            schema_version: PROTOCOL_VERSION.to_owned(),
            kind: "AuditEvent".to_owned(),
            id: id.into(),
            actor: decision.principal.clone(),
            capability: decision.capability.clone(),
            resource: decision.resource.clone(),
            decision: ObjectRef {
                kind: "PolicyDecision".to_owned(),
                id: decision.id.clone(),
            },
            outcome: decision.decision,
            occurred_at: occurred_at.into(),
        }
    }
}

fn grant_matches(grant: &Grant, request: &PolicyEvaluationRequest) -> bool {
    principal_matches(&grant.principal, &request.principal)
        && capability_matches(&grant.capabilities, &request.capability)
        && resource_matches(&grant.resource, &request.resource)
}

fn rule_matches(rule: &PolicyRule, request: &PolicyEvaluationRequest) -> bool {
    rule.principal
        .as_ref()
        .map(|principal| principal_matches(principal, &request.principal))
        .unwrap_or(true)
        && (rule.capabilities.is_empty()
            || capability_matches(&rule.capabilities, &request.capability))
        && (rule.resources.is_empty()
            || rule
                .resources
                .iter()
                .any(|resource| resource_matches(resource, &request.resource)))
}

fn principal_matches(left: &PrincipalDescriptor, right: &PrincipalDescriptor) -> bool {
    left.kind == right.kind && left.id == right.id
}

fn capability_matches(allowed: &[Capability], requested: &Capability) -> bool {
    allowed
        .iter()
        .any(|capability| capability.0 == requested.0 || capability.0 == "*")
}

fn resource_matches(allowed: &ResourceRef, requested: &ResourceRef) -> bool {
    allowed.kind == requested.kind && (allowed.id == requested.id || allowed.id == "*")
}

fn reason_for(decision: DecisionKind) -> &'static str {
    match decision {
        DecisionKind::Allow => "A matching grant or policy rule allows the capability.",
        DecisionKind::Deny => "A matching deny grant or policy rule blocks the capability.",
        DecisionKind::Redact => {
            "A matching grant or policy rule allows metadata only with redaction."
        }
        DecisionKind::NeedsGrant => "No grant or policy rule matched the requested capability.",
        DecisionKind::NeedsReview => "A matching grant or policy rule requires human review.",
    }
}

fn sanitize_id(value: &str) -> String {
    value
        .chars()
        .map(|character| match character {
            'a'..='z' | 'A'..='Z' | '0'..='9' => character.to_ascii_lowercase(),
            _ => '_',
        })
        .collect()
}

fn protocol_version() -> String {
    PROTOCOL_VERSION.to_owned()
}

fn grant_kind() -> String {
    "Grant".to_owned()
}

fn policy_kind() -> String {
    "Policy".to_owned()
}

fn secret_ref_kind() -> String {
    "SecretRef".to_owned()
}

fn redaction_kind() -> String {
    "RedactionMarker".to_owned()
}

fn policy_decision_kind() -> String {
    "PolicyDecision".to_owned()
}

fn audit_event_kind() -> String {
    "AuditEvent".to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn serializes_grant_with_core_permission_shape() {
        let grant = Grant::new(
            "grant_agent_repo_read",
            agent(),
            Capability::new("repo.read"),
            ResourceRef::new(ResourceKind::Repo, "repo_main"),
            GrantEffect::Allow,
        );

        let value = serde_json::to_value(grant).unwrap();

        assert_eq!(
            value,
            json!({
                "schemaVersion": "sorrel.protocol.v0",
                "kind": "Grant",
                "id": "grant_agent_repo_read",
                "principal": {
                    "kind": "agent",
                    "id": "agent_policy_eval"
                },
                "capabilities": ["repo.read"],
                "resource": {
                    "kind": "repo",
                    "id": "repo_main"
                },
                "effect": "allow"
            })
        );
    }

    #[test]
    fn evaluator_returns_all_decision_kinds_deterministically() {
        assert_decision(GrantEffect::Allow, DecisionKind::Allow);
        assert_decision(GrantEffect::Deny, DecisionKind::Deny);
        assert_decision(GrantEffect::Redact, DecisionKind::Redact);
        assert_decision(GrantEffect::Review, DecisionKind::NeedsReview);

        let request = request("repo.read", ResourceKind::Repo, "repo_main");
        let decision = evaluate_policy(&request, &[], &[]);
        assert_eq!(decision.decision, DecisionKind::NeedsGrant);
        assert!(decision.matched_grants.is_empty());
    }

    #[test]
    fn deny_precedes_allow_and_redact_emits_marker() {
        let request = request("secret.read", ResourceKind::Secret, "secret_npm_token");
        let allow = grant(
            "grant_b_allow",
            GrantEffect::Allow,
            "secret.read",
            ResourceKind::Secret,
            "secret_npm_token",
        );
        let deny = grant(
            "grant_a_deny",
            GrantEffect::Deny,
            "secret.read",
            ResourceKind::Secret,
            "secret_npm_token",
        );
        let redact = grant(
            "grant_c_redact",
            GrantEffect::Redact,
            "secret.read",
            ResourceKind::Secret,
            "secret_npm_token",
        );

        let denied = evaluate_policy(&request, &[allow, deny], &[]);
        assert_eq!(denied.decision, DecisionKind::Deny);
        assert_eq!(denied.matched_grants[0].id, "grant_a_deny");

        let redacted = evaluate_policy(&request, &[redact], &[]);
        assert_eq!(redacted.decision, DecisionKind::Redact);
        assert_eq!(
            redacted.redactions[0].marker,
            "<sorrel:redacted secret_npm_token>"
        );
    }

    fn assert_decision(effect: GrantEffect, expected: DecisionKind) {
        let request = request("repo.read", ResourceKind::Repo, "repo_main");
        let grant = grant(
            "grant_eval",
            effect,
            "repo.read",
            ResourceKind::Repo,
            "repo_main",
        );

        let decision = evaluate_policy(&request, &[grant], &[]);

        assert_eq!(decision.decision, expected);
    }

    fn request(capability: &str, kind: ResourceKind, id: &str) -> PolicyEvaluationRequest {
        PolicyEvaluationRequest {
            principal: agent(),
            capability: Capability::new(capability),
            resource: ResourceRef::new(kind, id),
        }
    }

    fn grant(
        id: &str,
        effect: GrantEffect,
        capability: &str,
        kind: ResourceKind,
        resource_id: &str,
    ) -> Grant {
        Grant::new(
            id,
            agent(),
            Capability::new(capability),
            ResourceRef::new(kind, resource_id),
            effect,
        )
    }

    fn agent() -> PrincipalDescriptor {
        PrincipalDescriptor::new(PrincipalKind::Agent, "agent_policy_eval")
    }
}
