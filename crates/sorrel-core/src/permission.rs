//! Headless permission spine for Sorrel Core.
//!
//! The evaluator in this module is deliberately storage-agnostic: callers pass
//! the complete in-memory grant set they want considered for a request, and the
//! result is a deterministic [`PolicyDecision`]. Sorrel Hub can persist grants
//! and audit events, but Core does not depend on Hub.
//!
//! Suggested call points:
//!
//! - **Lanes** construct a lane or agent [`Principal`], a [`ResourceRef`] for
//!   the lane path/object being touched, and evaluate before reading or writing
//!   lane-scoped state.
//! - **Workflows** evaluate [`Capability::WORKFLOW_RUN`] before starting a run,
//!   then evaluate step-specific capabilities with run metadata in
//!   [`EvaluationContext::attributes`].
//! - **Vault** evaluates [`Capability::SECRET_INJECT`] against
//!   [`ResourceRef::from_secret`] before materializing a secret. Redaction
//!   decisions should be applied to secret metadata and logs, never to raw
//!   secret material after it has been disclosed.
//! - **Hub** may hydrate grants from its policy store, call [`evaluate`], and
//!   persist an [`AuditEvent`] for collaboration history. Hub-specific IDs or
//!   tenancy rules belong in the passed context/grants, not in this crate.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Actor requesting a capability in Sorrel.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
pub struct Principal {
    /// Stable principal identifier within the caller's authority domain.
    pub id: String,
    /// Principal class used to avoid collisions between users, agents, and
    /// service identities that happen to share an identifier.
    pub kind: PrincipalKind,
}

impl Principal {
    /// Creates a principal.
    #[must_use]
    pub fn new(kind: PrincipalKind, id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            kind,
        }
    }
}

/// Coarse principal classes used by the permission spine.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
pub enum PrincipalKind {
    /// Human user.
    User,
    /// Agent acting on a user's or lane's behalf.
    Agent,
    /// Lane-scoped identity.
    Lane,
    /// Workflow identity.
    Workflow,
    /// Runner identity.
    Runner,
    /// Sorrel Hub or another coordination service.
    Service,
}

/// Action requested by a principal.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
pub struct Capability {
    /// Stable action name, for example `workflow.run`.
    pub action: String,
}

impl Capability {
    /// Start or resume a workflow.
    pub const WORKFLOW_RUN: &'static str = "workflow.run";
    /// Use a local or remote runner.
    pub const RUNNER_USE: &'static str = "runner.use";
    /// Inject a vault secret into an execution environment.
    pub const SECRET_INJECT: &'static str = "secret.inject";

    /// Creates a capability from a stable action name.
    #[must_use]
    pub fn new(action: impl Into<String>) -> Self {
        Self {
            action: action.into(),
        }
    }

    /// Capability for [`Self::WORKFLOW_RUN`].
    #[must_use]
    pub fn workflow_run() -> Self {
        Self::new(Self::WORKFLOW_RUN)
    }

    /// Capability for [`Self::RUNNER_USE`].
    #[must_use]
    pub fn runner_use() -> Self {
        Self::new(Self::RUNNER_USE)
    }

    /// Capability for [`Self::SECRET_INJECT`].
    #[must_use]
    pub fn secret_inject() -> Self {
        Self::new(Self::SECRET_INJECT)
    }
}

/// Storage-neutral reference to a protected Sorrel resource.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
pub struct ResourceRef {
    /// Resource kind such as `lane`, `workflow`, `runner`, or `secret`.
    pub kind: String,
    /// Resource identifier within `kind`.
    pub id: String,
    /// Optional path below the resource identifier.
    pub path: Option<String>,
}

impl ResourceRef {
    /// Wildcard used by grants to match any kind or id.
    pub const WILDCARD: &'static str = "*";

    /// Creates a resource reference without a path.
    #[must_use]
    pub fn new(kind: impl Into<String>, id: impl Into<String>) -> Self {
        Self {
            kind: kind.into(),
            id: id.into(),
            path: None,
        }
    }

    /// Adds a path scope to the resource reference.
    #[must_use]
    pub fn with_path(mut self, path: impl Into<String>) -> Self {
        self.path = Some(path.into());
        self
    }

    /// Creates the resource reference used for vault secret injection checks.
    #[must_use]
    pub fn from_secret(secret: &SecretRef) -> Self {
        Self::new("secret", secret.vault.clone()).with_path(secret.name.clone())
    }

    fn matches(&self, requested: &Self) -> bool {
        text_matches(&self.kind, &requested.kind)
            && text_matches(&self.id, &requested.id)
            && path_matches(self.path.as_deref(), requested.path.as_deref())
    }
}

/// Reference to a secret without carrying secret material.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
pub struct SecretRef {
    /// Vault or environment namespace containing the secret.
    pub vault: String,
    /// Secret name within the vault.
    pub name: String,
    /// Optional caller-defined version or rotation marker.
    pub version: Option<String>,
}

impl SecretRef {
    /// Creates a secret reference.
    #[must_use]
    pub fn new(vault: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            vault: vault.into(),
            name: name.into(),
            version: None,
        }
    }

    /// Adds a version or rotation marker to the secret reference.
    #[must_use]
    pub fn with_version(mut self, version: impl Into<String>) -> Self {
        self.version = Some(version.into());
        self
    }
}

/// Redaction instruction returned by policy.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
pub struct Redaction {
    /// Fields, JSON pointers, or caller-defined labels to redact.
    pub fields: Vec<String>,
    /// Human-readable policy reason.
    pub reason: String,
}

impl Redaction {
    /// Creates a redaction instruction.
    #[must_use]
    pub fn new(
        fields: impl IntoIterator<Item = impl Into<String>>,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            fields: fields.into_iter().map(Into::into).collect(),
            reason: reason.into(),
        }
    }
}

/// A policy grant considered by the evaluator.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Grant {
    /// Stable grant id used for audit trails.
    pub id: String,
    /// Principal receiving this grant.
    pub principal: Principal,
    /// Capability covered by this grant.
    pub capability: Capability,
    /// Resource scope covered by this grant.
    pub resource: ResourceRef,
    /// Effect applied when the grant matches.
    pub effect: GrantEffect,
    /// Optional exclusive expiration expressed as Unix epoch seconds.
    pub expires_at_epoch_seconds: Option<u64>,
    /// Exact context attributes required for the grant to match.
    pub conditions: BTreeMap<String, String>,
}

impl Grant {
    /// Creates a grant with no expiration or context conditions.
    #[must_use]
    pub fn new(
        id: impl Into<String>,
        principal: Principal,
        capability: Capability,
        resource: ResourceRef,
        effect: GrantEffect,
    ) -> Self {
        Self {
            id: id.into(),
            principal,
            capability,
            resource,
            effect,
            expires_at_epoch_seconds: None,
            conditions: BTreeMap::new(),
        }
    }

    /// Creates an allow grant.
    #[must_use]
    pub fn allow(
        id: impl Into<String>,
        principal: Principal,
        capability: Capability,
        resource: ResourceRef,
    ) -> Self {
        Self::new(id, principal, capability, resource, GrantEffect::Allow)
    }

    /// Creates a deny grant.
    #[must_use]
    pub fn deny(
        id: impl Into<String>,
        principal: Principal,
        capability: Capability,
        resource: ResourceRef,
    ) -> Self {
        Self::new(id, principal, capability, resource, GrantEffect::Deny)
    }

    /// Creates a redaction grant.
    #[must_use]
    pub fn redact(
        id: impl Into<String>,
        principal: Principal,
        capability: Capability,
        resource: ResourceRef,
        redaction: Redaction,
    ) -> Self {
        Self::new(
            id,
            principal,
            capability,
            resource,
            GrantEffect::Redact(redaction),
        )
    }

    /// Creates a grant that requires manual review.
    #[must_use]
    pub fn needs_review(
        id: impl Into<String>,
        principal: Principal,
        capability: Capability,
        resource: ResourceRef,
        reason: impl Into<String>,
    ) -> Self {
        Self::new(
            id,
            principal,
            capability,
            resource,
            GrantEffect::NeedsReview {
                reason: reason.into(),
            },
        )
    }

    /// Adds an expiration to the grant.
    #[must_use]
    pub const fn expires_at(mut self, epoch_seconds: u64) -> Self {
        self.expires_at_epoch_seconds = Some(epoch_seconds);
        self
    }

    /// Adds an exact-match context condition to the grant.
    #[must_use]
    pub fn when(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.conditions.insert(key.into(), value.into());
        self
    }

    fn matches(
        &self,
        principal: &Principal,
        capability: &Capability,
        resource: &ResourceRef,
        context: &EvaluationContext,
    ) -> bool {
        self.principal == *principal
            && self.capability == *capability
            && self.resource.matches(resource)
            && !self.is_expired(context)
            && self.conditions_match(context)
    }

    fn is_expired(&self, context: &EvaluationContext) -> bool {
        self.expires_at_epoch_seconds
            .is_some_and(|expires_at| context.now_epoch_seconds >= expires_at)
    }

    fn conditions_match(&self, context: &EvaluationContext) -> bool {
        self.conditions
            .iter()
            .all(|(key, value)| context.attributes.get(key) == Some(value))
    }
}

/// Effect a matching grant contributes to a policy decision.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum GrantEffect {
    /// The request is permitted unless a higher-precedence grant applies.
    Allow,
    /// The request is denied. Deny takes highest precedence.
    Deny,
    /// The request is permitted only with the supplied redaction.
    Redact(Redaction),
    /// The request must be sent to a review path.
    NeedsReview {
        /// Human-readable policy reason.
        reason: String,
    },
}

/// Request context supplied by the caller.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct EvaluationContext {
    /// Deterministic evaluation time used for expiration checks.
    pub now_epoch_seconds: u64,
    /// Caller-defined attributes, such as lane id, workflow run id, or tenant.
    pub attributes: BTreeMap<String, String>,
}

impl EvaluationContext {
    /// Creates a context at a deterministic Unix epoch second.
    #[must_use]
    pub fn at(now_epoch_seconds: u64) -> Self {
        Self {
            now_epoch_seconds,
            attributes: BTreeMap::new(),
        }
    }

    /// Adds a caller-defined attribute.
    #[must_use]
    pub fn with_attribute(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.attributes.insert(key.into(), value.into());
        self
    }
}

/// Result of policy evaluation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum PolicyDecision {
    /// The request is permitted.
    Allow,
    /// The request is denied.
    Deny,
    /// The request is permitted only with the supplied redactions.
    Redact {
        /// Redactions callers must apply.
        redactions: Vec<Redaction>,
    },
    /// No active grant covers this request.
    NeedsGrant,
    /// A matching grant requires a human or external review path.
    NeedsReview {
        /// Human-readable policy reason.
        reason: String,
    },
}

/// Audit record callers can persist after evaluation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AuditEvent {
    /// Principal that requested access.
    pub principal: Principal,
    /// Requested capability.
    pub capability: Capability,
    /// Requested resource.
    pub resource: ResourceRef,
    /// Request context used for evaluation.
    pub context: EvaluationContext,
    /// Decision returned by the evaluator.
    pub decision: PolicyDecision,
    /// Active grant ids that matched the request.
    pub matched_grant_ids: Vec<String>,
}

/// Evaluates grants for the requested principal, capability, and resource.
///
/// Precedence is deterministic and independent of grant order:
///
/// 1. any matching deny returns [`PolicyDecision::Deny`]
/// 2. any matching review grant returns [`PolicyDecision::NeedsReview`]
/// 3. matching redaction grants return [`PolicyDecision::Redact`]
/// 4. any matching allow returns [`PolicyDecision::Allow`]
/// 5. otherwise the caller receives [`PolicyDecision::NeedsGrant`]
#[must_use]
pub fn evaluate(
    principal: &Principal,
    capability: &Capability,
    resource: &ResourceRef,
    context: &EvaluationContext,
    grants: &[Grant],
) -> PolicyDecision {
    evaluate_with_audit(principal, capability, resource, context, grants).decision
}

/// Evaluates grants and returns a storage-neutral audit event.
#[must_use]
pub fn evaluate_with_audit(
    principal: &Principal,
    capability: &Capability,
    resource: &ResourceRef,
    context: &EvaluationContext,
    grants: &[Grant],
) -> AuditEvent {
    let mut matched_grant_ids = BTreeSet::new();
    let mut has_deny = false;
    let mut review_reasons = BTreeSet::new();
    let mut redactions = BTreeSet::new();
    let mut has_allow = false;

    for grant in grants {
        if !grant.matches(principal, capability, resource, context) {
            continue;
        }

        matched_grant_ids.insert(grant.id.clone());
        match &grant.effect {
            GrantEffect::Allow => has_allow = true,
            GrantEffect::Deny => has_deny = true,
            GrantEffect::Redact(redaction) => {
                redactions.insert(redaction.clone());
            }
            GrantEffect::NeedsReview { reason } => {
                review_reasons.insert(reason.clone());
            }
        }
    }

    let decision = if has_deny {
        PolicyDecision::Deny
    } else if let Some(reason) = review_reasons.into_iter().next() {
        PolicyDecision::NeedsReview { reason }
    } else if !redactions.is_empty() {
        PolicyDecision::Redact {
            redactions: redactions.into_iter().collect(),
        }
    } else if has_allow {
        PolicyDecision::Allow
    } else {
        PolicyDecision::NeedsGrant
    };

    AuditEvent {
        principal: principal.clone(),
        capability: capability.clone(),
        resource: resource.clone(),
        context: context.clone(),
        decision,
        matched_grant_ids: matched_grant_ids.into_iter().collect(),
    }
}

fn text_matches(grant_value: &str, requested_value: &str) -> bool {
    grant_value == ResourceRef::WILDCARD || grant_value == requested_value
}

fn path_matches(grant_path: Option<&str>, requested_path: Option<&str>) -> bool {
    match (grant_path, requested_path) {
        (None, _) => true,
        (Some(_), None) => false,
        (Some(grant_path), Some(requested_path)) => {
            requested_path == grant_path
                || requested_path
                    .strip_prefix(grant_path.trim_end_matches('/'))
                    .is_some_and(|suffix| suffix.starts_with('/'))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn agent() -> Principal {
        Principal::new(PrincipalKind::Agent, "agent/alice")
    }

    fn context() -> EvaluationContext {
        EvaluationContext::at(1_700_000_000)
    }

    #[test]
    fn allows_matching_grant() {
        let principal = agent();
        let capability = Capability::new("lane.write");
        let resource = ResourceRef::new("lane", "main").with_path("src/lib.rs");
        let grants = vec![Grant::allow(
            "grant-allow",
            principal.clone(),
            capability.clone(),
            ResourceRef::new("lane", "main"),
        )];

        assert_eq!(
            evaluate(&principal, &capability, &resource, &context(), &grants),
            PolicyDecision::Allow
        );
    }

    #[test]
    fn deny_takes_precedence_over_allow() {
        let principal = agent();
        let capability = Capability::new("lane.write");
        let resource = ResourceRef::new("lane", "main").with_path("src/lib.rs");
        let grants = vec![
            Grant::allow(
                "grant-allow",
                principal.clone(),
                capability.clone(),
                ResourceRef::new("lane", "main"),
            ),
            Grant::deny(
                "grant-deny",
                principal.clone(),
                capability.clone(),
                ResourceRef::new("lane", "main").with_path("src"),
            ),
        ];

        assert_eq!(
            evaluate(&principal, &capability, &resource, &context(), &grants),
            PolicyDecision::Deny
        );
    }

    #[test]
    fn returns_redaction_decision() {
        let principal = agent();
        let capability = Capability::new("lane.read");
        let resource = ResourceRef::new("lane", "main").with_path("logs/run-1.json");
        let redaction = Redaction::new(["secrets", "tokens"], "hide sensitive run data");
        let grants = vec![Grant::redact(
            "grant-redact",
            principal.clone(),
            capability.clone(),
            ResourceRef::new("lane", "main").with_path("logs"),
            redaction.clone(),
        )];

        assert_eq!(
            evaluate(&principal, &capability, &resource, &context(), &grants),
            PolicyDecision::Redact {
                redactions: vec![redaction]
            }
        );
    }

    #[test]
    fn expired_grant_does_not_allow_request() {
        let principal = agent();
        let capability = Capability::new("lane.write");
        let resource = ResourceRef::new("lane", "main");
        let grants = vec![Grant::allow(
            "grant-expired",
            principal.clone(),
            capability.clone(),
            resource.clone(),
        )
        .expires_at(10)];

        assert_eq!(
            evaluate(
                &principal,
                &capability,
                &resource,
                &EvaluationContext::at(10),
                &grants
            ),
            PolicyDecision::NeedsGrant
        );
    }

    #[test]
    fn path_scoped_grant_only_matches_descendants() {
        let principal = agent();
        let capability = Capability::new("lane.write");
        let grants = vec![Grant::allow(
            "grant-src",
            principal.clone(),
            capability.clone(),
            ResourceRef::new("lane", "main").with_path("src"),
        )];

        assert_eq!(
            evaluate(
                &principal,
                &capability,
                &ResourceRef::new("lane", "main").with_path("src/lib.rs"),
                &context(),
                &grants
            ),
            PolicyDecision::Allow
        );
        assert_eq!(
            evaluate(
                &principal,
                &capability,
                &ResourceRef::new("lane", "main").with_path("scripts/build.rs"),
                &context(),
                &grants
            ),
            PolicyDecision::NeedsGrant
        );
        assert_eq!(
            evaluate(
                &principal,
                &capability,
                &ResourceRef::new("lane", "main").with_path("src-old/lib.rs"),
                &context(),
                &grants
            ),
            PolicyDecision::NeedsGrant
        );
    }

    #[test]
    fn workflow_run_decision_uses_workflow_capability() {
        let principal = Principal::new(PrincipalKind::User, "user/alice");
        let capability = Capability::workflow_run();
        let resource = ResourceRef::new("workflow", "ci");
        let grants = vec![Grant::allow(
            "grant-workflow-run",
            principal.clone(),
            capability.clone(),
            resource.clone(),
        )];

        assert_eq!(
            evaluate(&principal, &capability, &resource, &context(), &grants),
            PolicyDecision::Allow
        );
    }

    #[test]
    fn runner_use_decision_uses_runner_capability() {
        let principal = Principal::new(PrincipalKind::Workflow, "workflow/ci");
        let capability = Capability::runner_use();
        let resource = ResourceRef::new("runner", "local");
        let grants = vec![Grant::allow(
            "grant-runner-use",
            principal.clone(),
            capability.clone(),
            resource.clone(),
        )];

        assert_eq!(
            evaluate(&principal, &capability, &resource, &context(), &grants),
            PolicyDecision::Allow
        );
    }

    #[test]
    fn secret_inject_decision_uses_secret_resource() {
        let principal = Principal::new(PrincipalKind::Workflow, "workflow/deploy");
        let capability = Capability::secret_inject();
        let secret = SecretRef::new("prod", "deploy/token").with_version("v1");
        let resource = ResourceRef::from_secret(&secret);
        let grants = vec![Grant::allow(
            "grant-secret-inject",
            principal.clone(),
            capability.clone(),
            ResourceRef::new("secret", "prod").with_path("deploy"),
        )];

        assert_eq!(
            evaluate(&principal, &capability, &resource, &context(), &grants),
            PolicyDecision::Allow
        );
    }

    #[test]
    fn needs_review_decision_uses_review_grant_reason() {
        let principal = agent();
        let capability = Capability::new("lane.merge");
        let resource = ResourceRef::new("lane", "main");
        let grants = vec![Grant::needs_review(
            "grant-review",
            principal.clone(),
            capability.clone(),
            resource.clone(),
            "main lane merge requires review",
        )];

        assert_eq!(
            evaluate(&principal, &capability, &resource, &context(), &grants),
            PolicyDecision::NeedsReview {
                reason: "main lane merge requires review".to_owned()
            }
        );
    }

    #[test]
    fn missing_grant_returns_needs_grant() {
        assert_eq!(
            evaluate(
                &agent(),
                &Capability::new("lane.write"),
                &ResourceRef::new("lane", "main"),
                &context(),
                &[]
            ),
            PolicyDecision::NeedsGrant
        );
    }

    #[test]
    fn audit_event_records_sorted_matching_grant_ids() {
        let principal = agent();
        let capability = Capability::new("lane.read");
        let resource = ResourceRef::new("lane", "main");
        let grants = vec![
            Grant::allow(
                "z-grant",
                principal.clone(),
                capability.clone(),
                resource.clone(),
            ),
            Grant::allow(
                "a-grant",
                principal.clone(),
                capability.clone(),
                resource.clone(),
            ),
        ];

        let audit = evaluate_with_audit(&principal, &capability, &resource, &context(), &grants);

        assert_eq!(audit.decision, PolicyDecision::Allow);
        assert_eq!(
            audit.matched_grant_ids,
            vec!["a-grant".to_owned(), "z-grant".to_owned()]
        );
    }

    #[test]
    fn serde_round_trips_policy_types() {
        let grant = Grant::redact(
            "grant-redact",
            agent(),
            Capability::new("lane.read"),
            ResourceRef::new("lane", "main").with_path("logs"),
            Redaction::new(["token"], "mask token"),
        )
        .when("lane", "main")
        .expires_at(1_800_000_000);

        let encoded = serde_json::to_string(&grant).unwrap();
        let decoded: Grant = serde_json::from_str(&encoded).unwrap();

        assert_eq!(decoded, grant);
    }
}
