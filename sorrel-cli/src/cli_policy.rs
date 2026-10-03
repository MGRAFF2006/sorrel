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
pub use sorrel_core::policy::GrantEffect;

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
    fn pattern(&self) -> Result<Option<&str>, ()> {
        match self.fields.get("ref").or_else(|| self.fields.get("path")) {
            Some(value) => value.as_str().map(Some).ok_or(()),
            None => Ok(None),
        }
    }

    pub fn matches(&self, resource: &ResourceRef) -> bool {
        self.scope == resource.scope
            && match self.pattern() {
                Ok(Some(pattern)) => pattern_matches(pattern, &resource.id),
                Ok(None) => true,
                Err(()) => false,
            }
    }

    fn covers(&self, other: &ResourceScope) -> bool {
        if self.scope != other.scope {
            return false;
        }
        match (self.pattern(), other.pattern()) {
            (Ok(None), Ok(_)) => true,
            (Ok(Some(base)), Ok(Some(target))) => pattern_matches(base, target),
            _ => false,
        }
    }
}

fn valid_scoped_id(value: &str) -> bool {
    !value.is_empty()
        && !value.contains(['\\', '\0'])
        && value
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
}

fn pattern_matches(pattern: &str, target: &str) -> bool {
    if !valid_scoped_id(pattern) || !valid_scoped_id(target) {
        return false;
    }
    if pattern == target {
        return true;
    }
    pattern.strip_suffix("/**").is_some_and(|prefix| {
        !prefix.is_empty()
            && (target == prefix
                || target
                    .strip_prefix(prefix)
                    .is_some_and(|rest| rest.starts_with('/')))
    })
}

fn allow_effect() -> GrantEffect {
    GrantEffect::Allow
}

/// Converts loosely typed persisted effects without letting unknown values allow.
#[must_use]
pub fn effect_from_value(value: Option<&serde_json::Value>) -> GrantEffect {
    match value {
        None => GrantEffect::Allow,
        Some(value) => serde_json::from_value(value.clone()).unwrap_or(GrantEffect::Deny),
    }
}

fn effect_priority(effect: GrantEffect) -> u8 {
    match effect {
        GrantEffect::Allow => 0,
        GrantEffect::Review => 1,
        GrantEffect::Redact => 2,
        GrantEffect::Deny => 3,
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
    #[serde(default = "allow_effect")]
    pub effect: GrantEffect,
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
                effect: GrantEffect::Allow,
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
            decision: match grant.effect {
                GrantEffect::Allow => Decision::Allow,
                GrantEffect::Deny => Decision::Deny,
                GrantEffect::Redact => Decision::Redact,
                GrantEffect::Review => Decision::NeedsReview,
            },
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

    let has_grant_authority = context.authority_principals.contains(&change.actor)
        || context.grants.iter().any(|grant| {
            grant.principal == change.actor
                && grant.effect == GrantEffect::Allow
                && grant
                    .capabilities
                    .iter()
                    .any(|cap| cap == "policy.grant" || cap == "authority.admin")
        });

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
    context
        .grants
        .iter()
        .filter(|grant| {
            grant.principal == *principal
                && grant.capabilities.iter().any(|cap| cap == action)
                && grant.resources.iter().any(|scope| scope.matches(resource))
        })
        .max_by_key(|grant| effect_priority(grant.effect))
}

fn actor_has_capability(
    actor: &PrincipalId,
    capability: &str,
    proposed: Option<&ProposedGrant>,
    context: &PolicyContext,
) -> bool {
    let relevant = context
        .grants
        .iter()
        .filter(|grant| {
            grant.principal == *actor && grant.capabilities.iter().any(|cap| cap == capability)
        })
        .collect::<Vec<_>>();
    let repository = ResourceRef {
        scope: "repo".to_owned(),
        id: context.repo_id.clone(),
    };
    let applies = |grant: &&Grant| match proposed {
        Some(proposed) => proposed.resources.iter().any(|target| {
            grant
                .resources
                .iter()
                .any(|scope| scope.covers(target) || target.covers(scope))
        }),
        None => grant
            .resources
            .iter()
            .any(|scope| scope.matches(&repository)),
    };
    if relevant
        .iter()
        .any(|grant| grant.effect != GrantEffect::Allow && applies(grant))
    {
        return false;
    }
    if context.authority_principals.contains(actor) {
        return true;
    }
    match proposed {
        Some(proposed) => {
            !proposed.resources.is_empty()
                && proposed.resources.iter().all(|target| {
                    relevant.iter().any(|grant| {
                        grant.effect == GrantEffect::Allow
                            && grant.resources.iter().any(|scope| scope.covers(target))
                    })
                })
        }
        None => relevant
            .iter()
            .any(|grant| grant.effect == GrantEffect::Allow && applies(grant)),
    }
}

fn scope_broadening_violation(
    actor: &PrincipalId,
    proposed: &ProposedGrant,
    context: &PolicyContext,
) -> bool {
    !actor_has_capability(actor, "policy.grant", Some(proposed), context)
        && !actor_has_capability(actor, "authority.admin", Some(proposed), context)
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
            effect: GrantEffect::Allow,
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
    fn path_scope(pattern: &str) -> ResourceScope {
        ResourceScope {
            scope: "path".into(),
            fields: serde_json::json!({"path":pattern})
                .as_object()
                .unwrap()
                .clone(),
        }
    }

    fn scoped_grant(effect: GrantEffect, capability: &str, resource: ResourceScope) -> Grant {
        Grant {
            effect,
            principal: user("bob"),
            capabilities: vec![capability.into()],
            resources: vec![resource],
            issued_by: None,
        }
    }

    #[test]
    fn restrictive_effects_override_allows_in_both_orders() {
        let input = EvaluateInput {
            principal: user("bob"),
            action: "path.write".into(),
            resource: ResourceRef {
                scope: "path".into(),
                id: "src/file.rs".into(),
            },
            environment: None,
        };
        for (effect, expected) in [
            (GrantEffect::Deny, Decision::Deny),
            (GrantEffect::Redact, Decision::Redact),
            (GrantEffect::Review, Decision::NeedsReview),
        ] {
            let allow = scoped_grant(GrantEffect::Allow, "path.write", path_scope("src/**"));
            let restriction = scoped_grant(effect, "path.write", path_scope("src/file.rs"));
            for grants in [
                vec![allow.clone(), restriction.clone()],
                vec![restriction, allow],
            ] {
                let mut context = PolicyContext::headless_default();
                context.grants = grants;
                assert_eq!(evaluate(&input, &context).decision, expected);
            }
        }
    }

    #[test]
    fn missing_effect_is_compatible_but_unknown_effect_never_allows() {
        let value = serde_json::json!({"principal":{"kind":"user","id":"bob"},"capabilities":["path.write"],"resources":[{"scope":"path","ref":"src/**"}]});
        let legacy: Grant = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(legacy.effect, GrantEffect::Allow);
        let mut unknown = value;
        unknown["effect"] = serde_json::json!("surprise");
        assert!(serde_json::from_value::<Grant>(unknown).is_err());
        assert_eq!(
            effect_from_value(Some(&serde_json::json!("surprise"))),
            GrantEffect::Deny
        );
        assert_eq!(
            effect_from_value(Some(&serde_json::json!(null))),
            GrantEffect::Deny
        );
    }

    #[test]
    fn wildcard_paths_require_segment_prefix_and_safe_components() {
        let scope = path_scope("src/**");
        for path in ["src", "src/lib.rs", "src/nested/file.rs"] {
            assert!(scope.matches(&ResourceRef {
                scope: "path".into(),
                id: path.into()
            }));
        }
        for path in [
            "secrets/credentials",
            "src-other/file.rs",
            "src/../secrets",
            "src/./file",
            "src//file",
        ] {
            assert!(
                !scope.matches(&ResourceRef {
                    scope: "path".into(),
                    id: path.into()
                }),
                "scope escaped through {path}"
            );
        }
        assert!(scope.covers(&path_scope("src/nested/**")));
        assert!(!scope.covers(&path_scope("secrets/**")));
        assert!(!scope.covers(&path_scope("src-other/**")));
        assert!(!scope.covers(&ResourceScope {
            scope: "path".into(),
            fields: Default::default()
        }));
    }

    #[test]
    fn delegation_only_uses_allow_grants_and_cannot_escape_prefix() {
        let proposed = |path| ProposedGrant {
            principal: agent("worker"),
            capabilities: vec!["path.write".into()],
            resources: vec![path_scope(path)],
        };
        let change = |path| PolicyChange {
            actor: user("bob"),
            operation: "grant".into(),
            grant: Some(proposed(path)),
            signatures: vec!["sig_bob".into()],
        };
        let mut context = PolicyContext::headless_default();
        context.grants = vec![scoped_grant(
            GrantEffect::Allow,
            "policy.grant",
            path_scope("src/**"),
        )];
        assert_eq!(
            evaluate_policy_change(&change("src/public/**"), &context).decision,
            Decision::Allow
        );
        for path in ["secrets/**", "src-other/**", "src/../secrets/**"] {
            assert_eq!(
                evaluate_policy_change(&change(path), &context).decision,
                Decision::Deny
            );
        }
        for effect in [GrantEffect::Deny, GrantEffect::Redact, GrantEffect::Review] {
            context.grants = vec![scoped_grant(effect, "policy.grant", path_scope("src/**"))];
            assert_eq!(
                evaluate_policy_change(&change("src/public/**"), &context).decision,
                Decision::Deny
            );
        }
        context.grants = vec![
            scoped_grant(GrantEffect::Allow, "policy.grant", path_scope("src/**")),
            scoped_grant(
                GrantEffect::Deny,
                "policy.grant",
                path_scope("src/private/**"),
            ),
        ];
        assert_eq!(
            evaluate_policy_change(&change("src/private/file.rs"), &context).decision,
            Decision::Deny
        );
        assert_eq!(
            evaluate_policy_change(&change("src/public/file.rs"), &context).decision,
            Decision::Allow
        );
        assert_eq!(
            evaluate_policy_change(&change("src/**"), &context).decision,
            Decision::Deny
        );
        context.grants.reverse();
        assert_eq!(
            evaluate_policy_change(&change("src/private/file.rs"), &context).decision,
            Decision::Deny
        );
    }

    #[test]
    fn path_delegation_does_not_authorize_repository_policy_mutations() {
        let mut context = PolicyContext::headless_default();
        context.grants = vec![scoped_grant(
            GrantEffect::Allow,
            "policy.grant",
            path_scope("src/**"),
        )];
        let change = PolicyChange {
            actor: user("bob"),
            operation: "revoke".into(),
            grant: None,
            signatures: vec!["sig_bob".into()],
        };
        assert_eq!(
            evaluate_policy_change(&change, &context).decision,
            Decision::Deny
        );
    }
}
