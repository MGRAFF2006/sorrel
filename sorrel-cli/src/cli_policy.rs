//! CLI policy evaluation surface.
//!
//! This module provides the policy API consumed by the `sorrel` CLI and its
//! local `cli_runner` workflow gate. It is a self-contained headless policy
//! evaluator that conforms to the canonical `sorrel-protocol` policy manifest
//! (see `tests/policy_conformance.rs`). It used to live in
//! `sorrel-core::cli_policy`; it now lives in the CLI so the engine crate keeps
//! only its native [`sorrel_core::policy`] / authority API, whose types differ
//! in shape and semantics.

use serde::{Deserialize, Serialize};

/// Portable principal identifier used by CLI-compat policy evaluation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrincipalId {
    pub kind: String,
    pub id: String,
}

impl PrincipalId {
    pub fn parse(value: &str) -> Option<Self> {
        let (kind, id) = value.split_once(':')?;
        if kind.is_empty() || id.is_empty() {
            return None;
        }
        Some(Self {
            kind: kind.to_owned(),
            id: id.to_owned(),
        })
    }

    pub fn to_ref(&self) -> String {
        format!("{}:{}", self.kind, self.id)
    }
}

/// Resource reference for CLI-compat permission evaluation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceRef {
    pub scope: String,
    pub id: String,
}

impl ResourceRef {
    pub fn parse(value: &str) -> Option<Self> {
        let (scope, id) = value.split_once(':')?;
        if scope.is_empty() || id.is_empty() {
            return None;
        }
        Some(Self {
            scope: scope.to_owned(),
            id: id.to_owned(),
        })
    }
}

/// Scoped resource entry attached to a grant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceScope {
    pub scope: String,
    #[serde(flatten)]
    pub fields: serde_json::Map<String, serde_json::Value>,
}

impl ResourceScope {
    pub fn matches(&self, resource: &ResourceRef) -> bool {
        if self.scope != resource.scope {
            return false;
        }

        match self.fields.get("ref").and_then(serde_json::Value::as_str) {
            Some(pattern) => resource.id == pattern || pattern.ends_with("/**"),
            None => match self.fields.get("path").and_then(serde_json::Value::as_str) {
                Some(path) => resource.id == path || path.ends_with("/**"),
                None => true,
            },
        }
    }

    fn covers(&self, other: &ResourceScope) -> bool {
        if self.scope != other.scope {
            return false;
        }

        let self_ref = self
            .fields
            .get("ref")
            .or_else(|| self.fields.get("path"))
            .and_then(serde_json::Value::as_str);
        let other_ref = other
            .fields
            .get("ref")
            .or_else(|| other.fields.get("path"))
            .and_then(serde_json::Value::as_str);

        match (self_ref, other_ref) {
            (Some(base), Some(target)) => base == target || base.ends_with("/**"),
            _ => true,
        }
    }
}

/// CLI-compat permission decision outcomes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Decision {
    Allow,
    Deny,
    Redact,
    NeedsGrant,
    NeedsReview,
}

impl Decision {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Allow => "allow",
            Self::Deny => "deny",
            Self::Redact => "redact",
            Self::NeedsGrant => "needs_grant",
            Self::NeedsReview => "needs_review",
        }
    }

    pub fn effect(self) -> &'static str {
        match self {
            Self::Allow => "allow",
            Self::Deny => "deny",
            Self::Redact => "redact",
            Self::NeedsGrant => "require",
            Self::NeedsReview => "review",
        }
    }
}

/// A grant under the previous effective policy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Grant {
    pub principal: PrincipalId,
    pub capabilities: Vec<String>,
    pub resources: Vec<ResourceScope>,
    #[serde(default)]
    pub issued_by: Option<PrincipalId>,
}

/// Default headless policy rule for baseline actions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyRule {
    pub action: String,
    pub decision: Decision,
    pub reason: String,
}

/// Input for permission evaluation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvaluateInput {
    pub principal: PrincipalId,
    pub action: String,
    pub resource: ResourceRef,
    pub environment: Option<String>,
}

/// Result of evaluating a permission request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicyDecision {
    pub decision: Decision,
    pub reason: String,
    pub action: String,
    pub principal: PrincipalId,
    pub resource: ResourceRef,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub environment: Option<String>,
}

/// Signed policy mutation evaluated against the previous effective policy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicyChange {
    pub actor: PrincipalId,
    pub operation: String,
    #[serde(default)]
    pub grant: Option<ProposedGrant>,
    #[serde(default)]
    pub signatures: Vec<String>,
}

/// Grant payload inside a policy change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProposedGrant {
    pub principal: PrincipalId,
    pub capabilities: Vec<String>,
    pub resources: Vec<ResourceScope>,
}

/// Result of evaluating a policy change before application.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicyChangeEvaluation {
    pub decision: Decision,
    pub reason: String,
    pub trusted: bool,
    pub actor: PrincipalId,
    pub operation: String,
}

/// In-memory policy state used for headless evaluation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyContext {
    pub repo_id: String,
    pub grants: Vec<Grant>,
    pub authority_principals: Vec<PrincipalId>,
    pub default_rules: Vec<PolicyRule>,
}

impl PolicyContext {
    /// Baseline headless policy used by the CLI before persistent storage is wired.
    #[must_use]
    pub fn headless_default() -> Self {
        Self {
            repo_id: "repo_mock_local".to_owned(),
            authority_principals: vec![PrincipalId {
                kind: "user".to_owned(),
                id: "alice".to_owned(),
            }],
            grants: vec![Grant {
                principal: PrincipalId {
                    kind: "user".to_owned(),
                    id: "alice".to_owned(),
                },
                capabilities: vec![
                    "policy.grant".to_owned(),
                    "policy.delegate".to_owned(),
                    "authority.admin".to_owned(),
                ],
                resources: vec![ResourceScope {
                    scope: "repo".to_owned(),
                    fields: serde_json::json!({ "ref": "repo_mock_local" })
                        .as_object()
                        .cloned()
                        .unwrap_or_default(),
                }],
                issued_by: None,
            }],
            default_rules: vec![
                PolicyRule {
                    action: "path.write".to_owned(),
                    decision: Decision::Allow,
                    reason: "Headless Core policy allows agents to write declared paths.".to_owned(),
                },
                PolicyRule {
                    action: "workflow.run".to_owned(),
                    decision: Decision::Allow,
                    reason: "Headless Core policy allows mocked workflow runs.".to_owned(),
                },
                PolicyRule {
                    action: "secret.inject".to_owned(),
                    decision: Decision::NeedsGrant,
                    reason:
                        "Secret injection requires an explicit grant before values are materialized."
                            .to_owned(),
                },
            ],
        }
    }
}

/// Evaluates a permission request against the previous effective policy.
#[must_use]
pub fn evaluate(input: &EvaluateInput, context: &PolicyContext) -> PolicyDecision {
    if let Some(grant) = matching_grant(&input.principal, &input.action, &input.resource, context) {
        return PolicyDecision {
            decision: Decision::Allow,
            reason: format!(
                "Matched grant issued by {}",
                grant
                    .issued_by
                    .as_ref()
                    .map(PrincipalId::to_ref)
                    .unwrap_or_else(|| "authority".to_owned())
            ),
            action: input.action.clone(),
            principal: input.principal.clone(),
            resource: input.resource.clone(),
            environment: input.environment.clone(),
        };
    }

    for rule in &context.default_rules {
        if rule.action == input.action {
            return PolicyDecision {
                decision: rule.decision,
                reason: rule.reason.clone(),
                action: input.action.clone(),
                principal: input.principal.clone(),
                resource: input.resource.clone(),
                environment: input.environment.clone(),
            };
        }
    }

    PolicyDecision {
        decision: Decision::Deny,
        reason: "No matching grant or default rule for this action.".to_owned(),
        action: input.action.clone(),
        principal: input.principal.clone(),
        resource: input.resource.clone(),
        environment: input.environment.clone(),
    }
}

/// Evaluates a signed policy change against the previous effective policy.
///
/// Never evaluates a permission change using permissions created by that same change.
#[must_use]
pub fn evaluate_policy_change(
    change: &PolicyChange,
    context: &PolicyContext,
) -> PolicyChangeEvaluation {
    let base = PolicyChangeEvaluation {
        decision: Decision::Deny,
        reason: String::new(),
        trusted: false,
        actor: change.actor.clone(),
        operation: change.operation.clone(),
    };

    if !is_signed(change) {
        return PolicyChangeEvaluation {
            reason: "PolicyChange is unsigned or explicitly marked untrusted.".to_owned(),
            ..base
        };
    }

    match change.operation.as_str() {
        "grant" => evaluate_grant_change(change, context),
        "revoke" | "update_policy" | "delegate" | "rotate_authority" => {
            // Each operation requires the capability the protocol assigns it.
            // Authority rotation is governed by authority.rotate (or
            // authority.admin); other mutations by policy.grant (or
            // authority.admin). This matches the canonical sorrel-core evaluator.
            let required: &[&str] = if change.operation == "rotate_authority" {
                &["authority.rotate", "authority.admin"]
            } else {
                &["policy.grant", "authority.admin"]
            };

            if required
                .iter()
                .any(|capability| actor_has_capability(&change.actor, capability, None, context))
            {
                PolicyChangeEvaluation {
                    decision: Decision::Allow,
                    reason: format!(
                        "Actor {} is authorized to perform {} under the previous effective policy.",
                        change.actor.to_ref(),
                        change.operation
                    ),
                    trusted: true,
                    actor: change.actor.clone(),
                    operation: change.operation.clone(),
                }
            } else {
                PolicyChangeEvaluation {
                    decision: Decision::Deny,
                    reason: format!(
                        "actor lacks {} on {} under the previous effective policy",
                        change.operation, context.repo_id
                    ),
                    trusted: true,
                    actor: change.actor.clone(),
                    operation: change.operation.clone(),
                }
            }
        }
        _ => PolicyChangeEvaluation {
            reason: format!(
                "Unsupported policy change operation `{}`.",
                change.operation
            ),
            ..base
        },
    }
}

fn evaluate_grant_change(change: &PolicyChange, context: &PolicyContext) -> PolicyChangeEvaluation {
    let Some(proposed) = &change.grant else {
        return PolicyChangeEvaluation {
            decision: Decision::Deny,
            reason: "Grant policy change is missing a proposed grant payload.".to_owned(),
            trusted: true,
            actor: change.actor.clone(),
            operation: change.operation.clone(),
        };
    };

    let is_self_grant = change.actor == proposed.principal;

    if is_self_grant
        && !actor_has_capability(&change.actor, "policy.grant", Some(proposed), context)
        && !actor_has_capability(&change.actor, "authority.admin", Some(proposed), context)
    {
        return PolicyChangeEvaluation {
            decision: Decision::Deny,
            reason: format!(
                "actor lacks policy.grant on {} under the previous effective policy",
                context.repo_id
            ),
            trusted: true,
            actor: change.actor.clone(),
            operation: change.operation.clone(),
        };
    }

    let has_grant_authority = actor_has_capability(&change.actor, "policy.grant", None, context)
        || actor_has_capability(&change.actor, "authority.admin", None, context);

    if !has_grant_authority {
        return PolicyChangeEvaluation {
            decision: Decision::Deny,
            reason: format!(
                "actor lacks policy.grant on {} under the previous effective policy",
                context.repo_id
            ),
            trusted: true,
            actor: change.actor.clone(),
            operation: change.operation.clone(),
        };
    }

    if scope_broadening_violation(&change.actor, proposed, context) {
        return PolicyChangeEvaluation {
            decision: Decision::Deny,
            reason: "Delegated policy.grant cannot broaden beyond its delegated resource scope."
                .to_owned(),
            trusted: true,
            actor: change.actor.clone(),
            operation: change.operation.clone(),
        };
    }

    PolicyChangeEvaluation {
        decision: Decision::Allow,
        reason: format!(
            "Actor {} may grant {:?} to {} under the previous effective policy.",
            change.actor.to_ref(),
            proposed.capabilities,
            proposed.principal.to_ref()
        ),
        trusted: true,
        actor: change.actor.clone(),
        operation: change.operation.clone(),
    }
}

fn is_signed(change: &PolicyChange) -> bool {
    change.signatures.iter().any(|signature| {
        !signature.is_empty()
            && signature != "unsigned"
            && signature != "untrusted"
            && !signature.starts_with("sig_invalid")
    })
}

fn matching_grant<'a>(
    principal: &PrincipalId,
    action: &str,
    resource: &ResourceRef,
    context: &'a PolicyContext,
) -> Option<&'a Grant> {
    context.grants.iter().find(|grant| {
        grant.principal == *principal
            && grant.capabilities.iter().any(|cap| cap == action)
            && grant.resources.iter().any(|scope| scope.matches(resource))
    })
}

fn actor_has_capability(
    actor: &PrincipalId,
    capability: &str,
    proposed: Option<&ProposedGrant>,
    context: &PolicyContext,
) -> bool {
    if context
        .authority_principals
        .iter()
        .any(|principal| principal == actor)
    {
        return true;
    }

    context.grants.iter().any(|grant| {
        if grant.principal != *actor {
            return false;
        }
        if !grant.capabilities.iter().any(|cap| cap == capability) {
            return false;
        }

        match proposed {
            Some(proposed_grant) => proposed_grant.resources.iter().all(|target| {
                grant
                    .resources
                    .iter()
                    .any(|delegated| delegated.covers(target))
            }),
            None => true,
        }
    })
}

fn scope_broadening_violation(
    actor: &PrincipalId,
    proposed: &ProposedGrant,
    context: &PolicyContext,
) -> bool {
    if context
        .authority_principals
        .iter()
        .any(|principal| principal == actor)
    {
        return false;
    }

    let Some(delegated_grant) = context
        .grants
        .iter()
        .find(|grant| grant.principal == *actor)
    else {
        return false;
    };

    !proposed.resources.iter().all(|target| {
        delegated_grant
            .resources
            .iter()
            .any(|delegated| delegated.covers(target))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn agent(id: &str) -> PrincipalId {
        PrincipalId {
            kind: "agent".to_owned(),
            id: id.to_owned(),
        }
    }

    fn user(id: &str) -> PrincipalId {
        PrincipalId {
            kind: "user".to_owned(),
            id: id.to_owned(),
        }
    }

    fn repo_scope() -> ResourceScope {
        ResourceScope {
            scope: "repo".to_owned(),
            fields: serde_json::json!({ "ref": "repo_mock_local" })
                .as_object()
                .cloned()
                .unwrap_or_default(),
        }
    }

    #[test]
    fn evaluate_allows_path_write_by_default() {
        let context = PolicyContext::headless_default();
        let decision = evaluate(
            &EvaluateInput {
                principal: agent("docs"),
                action: "path.write".to_owned(),
                resource: ResourceRef {
                    scope: "path".to_owned(),
                    id: "docs/README.md".to_owned(),
                },
                environment: None,
            },
            &context,
        );

        assert_eq!(decision.decision, Decision::Allow);
    }

    #[test]
    fn evaluate_self_grant_is_denied() {
        let context = PolicyContext::headless_default();
        let change = PolicyChange {
            actor: agent("agent_17"),
            operation: "grant".to_owned(),
            grant: Some(ProposedGrant {
                principal: agent("agent_17"),
                capabilities: vec!["secret.inject".to_owned(), "policy.grant".to_owned()],
                resources: vec![repo_scope()],
            }),
            signatures: vec!["sig_actor".to_owned()],
        };

        let evaluation = evaluate_policy_change(&change, &context);
        assert_eq!(evaluation.decision, Decision::Deny);
        assert!(evaluation.reason.contains("policy.grant"));
    }

    #[test]
    fn evaluate_delegated_grant_is_allowed() {
        let context = PolicyContext::headless_default();
        let change = PolicyChange {
            actor: user("alice"),
            operation: "grant".to_owned(),
            grant: Some(ProposedGrant {
                principal: agent("agent_17"),
                capabilities: vec!["path.write".to_owned()],
                resources: vec![ResourceScope {
                    scope: "path".to_owned(),
                    fields: serde_json::json!({ "ref": "packages/auth/**" })
                        .as_object()
                        .cloned()
                        .unwrap_or_default(),
                }],
            }),
            signatures: vec!["sig_alice".to_owned()],
        };

        let evaluation = evaluate_policy_change(&change, &context);
        assert_eq!(evaluation.decision, Decision::Allow);
        assert!(evaluation.trusted);
    }

    #[test]
    fn evaluate_unsigned_change_is_untrusted() {
        let context = PolicyContext::headless_default();
        let change = PolicyChange {
            actor: agent("agent_17"),
            operation: "grant".to_owned(),
            grant: Some(ProposedGrant {
                principal: agent("agent_17"),
                capabilities: vec!["path.write".to_owned()],
                resources: vec![repo_scope()],
            }),
            signatures: vec![],
        };

        let evaluation = evaluate_policy_change(&change, &context);
        assert_eq!(evaluation.decision, Decision::Deny);
        assert!(!evaluation.trusted);
        assert!(evaluation.reason.contains("unsigned"));
    }

    #[test]
    fn delegated_grant_cannot_broaden_scope() {
        let mut context = PolicyContext::headless_default();
        context.grants.push(Grant {
            principal: user("bob"),
            capabilities: vec!["policy.grant".to_owned()],
            resources: vec![ResourceScope {
                scope: "path".to_owned(),
                fields: serde_json::json!({ "ref": "packages/auth/**" })
                    .as_object()
                    .cloned()
                    .unwrap_or_default(),
            }],
            issued_by: Some(user("alice")),
        });

        let change = PolicyChange {
            actor: user("bob"),
            operation: "grant".to_owned(),
            grant: Some(ProposedGrant {
                principal: agent("agent_17"),
                capabilities: vec!["path.write".to_owned()],
                resources: vec![ResourceScope {
                    scope: "repo".to_owned(),
                    fields: serde_json::json!({ "ref": "repo_mock_local" })
                        .as_object()
                        .cloned()
                        .unwrap_or_default(),
                }],
            }),
            signatures: vec!["sig_bob".to_owned()],
        };

        let evaluation = evaluate_policy_change(&change, &context);
        assert_eq!(evaluation.decision, Decision::Deny);
        assert!(evaluation.reason.contains("broaden"));
    }
}
