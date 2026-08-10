//! Authority roots, signed policy changes, and hardened change evaluation.
//!
//! Core invariant: permission changes are authorized using `previous_grants` only.
//! Grants proposed by the same change must never be treated as actor authority.

use crate::policy::{
    evaluate_policy, Capability, DecisionKind, Grant, GrantEffect, PolicyEvaluationRequest,
    PrincipalDescriptor, ResourceRef, PROTOCOL_VERSION,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const CAP_POLICY_GRANT: &str = "policy.grant";
pub const CAP_POLICY_DELEGATE: &str = "policy.delegate";
pub const CAP_AUTHORITY_ADMIN: &str = "authority.admin";
pub const CAP_AUTHORITY_ROTATE: &str = "authority.rotate";

/// Content-addressed anchor for an immutable policy state.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PolicyRoot {
    pub hash: String,
    #[serde(default)]
    pub version: u64,
}

impl PolicyRoot {
    #[must_use]
    pub fn new(hash: impl Into<String>, version: u64) -> Self {
        Self {
            hash: hash.into(),
            version,
        }
    }
}

/// Trust anchor describing who may sign policy changes.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthorityRoot {
    #[serde(default = "protocol_version")]
    pub schema_version: String,
    #[serde(default = "authority_root_kind")]
    pub kind: String,
    pub id: String,
    pub root_hash: String,
    pub signing_keys: Vec<AuthoritySigningKey>,
    pub rotation_threshold: u32,
}

impl AuthorityRoot {
    #[must_use]
    pub fn new(
        id: impl Into<String>,
        root_hash: impl Into<String>,
        signing_keys: Vec<AuthoritySigningKey>,
        rotation_threshold: u32,
    ) -> Self {
        Self {
            schema_version: PROTOCOL_VERSION.to_owned(),
            kind: "AuthorityRoot".to_owned(),
            id: id.into(),
            root_hash: root_hash.into(),
            signing_keys,
            rotation_threshold,
        }
    }

    /// Deterministic test signature for `change` using `key_id`.
    #[must_use]
    pub fn sign_change(&self, change: &PolicyChange, key_id: &str) -> Option<AuthoritySignature> {
        let key = self.signing_keys.iter().find(|key| key.id == key_id)?;
        Some(AuthoritySignature {
            key_id: key_id.to_owned(),
            value: compute_signature(change, key),
        })
    }

    fn verify_signature(&self, change: &PolicyChange, signature: &AuthoritySignature) -> bool {
        self.signing_keys
            .iter()
            .find(|key| key.id == signature.key_id)
            .is_some_and(|key| compute_signature(change, key) == signature.value)
    }
}

/// Deterministic signing key material for headless tests.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthoritySigningKey {
    pub id: String,
    pub material: String,
}

impl AuthoritySigningKey {
    #[must_use]
    pub fn new(id: impl Into<String>, material: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            material: material.into(),
        }
    }
}

/// Signature over a policy change payload.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthoritySignature {
    pub key_id: String,
    pub value: String,
}

/// Grant proposed by a signed policy change.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProposedGrant {
    pub id: String,
    pub principal: PrincipalDescriptor,
    pub capabilities: Vec<Capability>,
    pub resource: ResourceRef,
    pub effect: GrantEffect,
}

impl ProposedGrant {
    #[must_use]
    pub fn new(
        id: impl Into<String>,
        principal: PrincipalDescriptor,
        capabilities: impl IntoIterator<Item = impl Into<String>>,
        resource: ResourceRef,
        effect: GrantEffect,
    ) -> Self {
        Self {
            id: id.into(),
            principal,
            capabilities: capabilities
                .into_iter()
                .map(|capability| Capability::new(capability))
                .collect(),
            resource,
            effect,
        }
    }
}

/// Action carried by a policy change.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PolicyChangeAction {
    Grant,
    Delegate,
    Revoke,
    Rotate,
}

/// Signed proposal to move from `previous_policy_root` to `proposed_policy_root`.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PolicyChange {
    #[serde(default = "protocol_version")]
    pub schema_version: String,
    #[serde(default = "policy_change_kind")]
    pub kind: String,
    pub id: String,
    pub actor: PrincipalDescriptor,
    pub previous_policy_root: PolicyRoot,
    pub proposed_policy_root: PolicyRoot,
    #[serde(default)]
    pub proposed_grants: Vec<ProposedGrant>,
    pub action: PolicyChangeAction,
    #[serde(default)]
    pub signatures: Vec<AuthoritySignature>,
}

impl PolicyChange {
    #[must_use]
    pub fn new(
        id: impl Into<String>,
        actor: PrincipalDescriptor,
        previous_policy_root: PolicyRoot,
        proposed_policy_root: PolicyRoot,
        action: PolicyChangeAction,
    ) -> Self {
        Self {
            schema_version: PROTOCOL_VERSION.to_owned(),
            kind: "PolicyChange".to_owned(),
            id: id.into(),
            actor,
            previous_policy_root,
            proposed_policy_root,
            proposed_grants: Vec::new(),
            action,
            signatures: Vec::new(),
        }
    }
}

/// Evaluation context for a policy change.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PolicyChangeContext {
    pub current_policy_root: PolicyRoot,
    pub org_resource: ResourceRef,
}

impl PolicyChangeContext {
    #[must_use]
    pub fn new(current_policy_root: PolicyRoot, org_resource: ResourceRef) -> Self {
        Self {
            current_policy_root,
            org_resource,
        }
    }
}

/// Whether signatures on a change are trusted.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PolicyChangeTrust {
    Trusted,
    Untrusted,
}

/// Whether a trusted change is approved for application.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PolicyChangeOutcome {
    Approved,
    Denied,
}

/// Result of evaluating a policy change.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PolicyChangeEvaluation {
    pub trust: PolicyChangeTrust,
    pub outcome: PolicyChangeOutcome,
    pub reason: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub denied_grant_ids: Vec<String>,
}

impl PolicyChangeEvaluation {
    fn untrusted(reason: impl Into<String>) -> Self {
        Self {
            trust: PolicyChangeTrust::Untrusted,
            outcome: PolicyChangeOutcome::Denied,
            reason: reason.into(),
            denied_grant_ids: Vec::new(),
        }
    }

    fn denied(reason: impl Into<String>, denied_grant_ids: Vec<String>) -> Self {
        Self {
            trust: PolicyChangeTrust::Trusted,
            outcome: PolicyChangeOutcome::Denied,
            reason: reason.into(),
            denied_grant_ids,
        }
    }

    fn approved(reason: impl Into<String>) -> Self {
        Self {
            trust: PolicyChangeTrust::Trusted,
            outcome: PolicyChangeOutcome::Approved,
            reason: reason.into(),
            denied_grant_ids: Vec::new(),
        }
    }
}

/// Evaluates a signed policy change against prior grants only.
///
/// Proposed grants from `change` are never merged into the grant set used to
/// authorize the actor.
#[must_use]
pub fn evaluate_policy_change(
    change: &PolicyChange,
    authority_root: &AuthorityRoot,
    previous_grants: &[Grant],
    context: &PolicyChangeContext,
) -> PolicyChangeEvaluation {
    if context.current_policy_root != change.previous_policy_root {
        return PolicyChangeEvaluation::denied(
            "current policy root does not match change previous policy root",
            Vec::new(),
        );
    }

    let signature_check = verify_signatures(change, authority_root);
    if signature_check.trust == PolicyChangeTrust::Untrusted {
        return signature_check;
    }

    match change.action {
        PolicyChangeAction::Rotate => {
            evaluate_rotate(change, authority_root, previous_grants, context)
        }
        PolicyChangeAction::Revoke => evaluate_revoke(change, previous_grants, context),
        PolicyChangeAction::Grant => evaluate_grant(change, previous_grants, context),
        PolicyChangeAction::Delegate => evaluate_delegate(change, previous_grants, context),
    }
}

fn verify_signatures(
    change: &PolicyChange,
    authority_root: &AuthorityRoot,
) -> PolicyChangeEvaluation {
    if change.signatures.is_empty() {
        return PolicyChangeEvaluation::untrusted("policy change is unsigned");
    }

    let mut valid_key_ids = BTreeSet::new();
    for signature in &change.signatures {
        if authority_root.verify_signature(change, signature) {
            valid_key_ids.insert(signature.key_id.clone());
        }
    }

    if valid_key_ids.is_empty() {
        return PolicyChangeEvaluation::untrusted("policy change signatures are invalid or forged");
    }

    let required = match change.action {
        PolicyChangeAction::Rotate => authority_root.rotation_threshold.max(1),
        _ => 1,
    };

    if valid_key_ids.len() < required as usize {
        return PolicyChangeEvaluation::untrusted(format!(
            "policy change requires {required} valid signature(s); received {}",
            valid_key_ids.len()
        ));
    }

    PolicyChangeEvaluation::approved("signatures verified")
}

fn evaluate_rotate(
    change: &PolicyChange,
    authority_root: &AuthorityRoot,
    previous_grants: &[Grant],
    context: &PolicyChangeContext,
) -> PolicyChangeEvaluation {
    let rotate_request = PolicyEvaluationRequest {
        principal: change.actor.clone(),
        capability: Capability::new(CAP_AUTHORITY_ROTATE),
        resource: context.org_resource.clone(),
    };
    let rotate_decision = evaluate_policy(&rotate_request, previous_grants, &[]);
    if rotate_decision.decision != DecisionKind::Allow {
        return PolicyChangeEvaluation::denied(
            "actor lacks authority.rotate capability in previous grants",
            Vec::new(),
        );
    }

    if change.proposed_grants.is_empty() {
        return PolicyChangeEvaluation::approved(format!(
            "authority rotation approved with {} signature(s) meeting threshold {}",
            change.signatures.len(),
            authority_root.rotation_threshold
        ));
    }

    evaluate_proposed_grants(change, previous_grants, context, PolicyChangeAction::Rotate)
}

fn evaluate_revoke(
    change: &PolicyChange,
    previous_grants: &[Grant],
    context: &PolicyChangeContext,
) -> PolicyChangeEvaluation {
    evaluate_proposed_grants(change, previous_grants, context, PolicyChangeAction::Revoke)
}

fn evaluate_grant(
    change: &PolicyChange,
    previous_grants: &[Grant],
    context: &PolicyChangeContext,
) -> PolicyChangeEvaluation {
    evaluate_proposed_grants(change, previous_grants, context, PolicyChangeAction::Grant)
}

fn evaluate_delegate(
    change: &PolicyChange,
    previous_grants: &[Grant],
    context: &PolicyChangeContext,
) -> PolicyChangeEvaluation {
    if !actor_has_any_capability(
        &change.actor,
        previous_grants,
        context,
        &[CAP_POLICY_DELEGATE, CAP_AUTHORITY_ADMIN],
    ) {
        return PolicyChangeEvaluation::denied(
            "actor lacks policy.delegate or authority.admin in previous grants",
            change
                .proposed_grants
                .iter()
                .map(|grant| grant.id.clone())
                .collect(),
        );
    }

    evaluate_proposed_grants(
        change,
        previous_grants,
        context,
        PolicyChangeAction::Delegate,
    )
}

fn evaluate_proposed_grants(
    change: &PolicyChange,
    previous_grants: &[Grant],
    context: &PolicyChangeContext,
    action: PolicyChangeAction,
) -> PolicyChangeEvaluation {
    let mut denied = Vec::new();

    for proposed in &change.proposed_grants {
        if proposed.effect != GrantEffect::Allow {
            denied.push(proposed.id.clone());
            continue;
        }

        if is_self_grant(&change.actor, &proposed.principal)
            && !actor_has_any_capability(
                &change.actor,
                previous_grants,
                context,
                &[CAP_POLICY_GRANT, CAP_POLICY_DELEGATE, CAP_AUTHORITY_ADMIN],
            )
        {
            denied.push(proposed.id.clone());
            continue;
        }

        if action == PolicyChangeAction::Delegate
            && !delegation_within_scope(&change.actor, proposed, previous_grants)
        {
            denied.push(proposed.id.clone());
            continue;
        }

        if action == PolicyChangeAction::Grant
            && !actor_has_any_capability(
                &change.actor,
                previous_grants,
                context,
                &[CAP_POLICY_GRANT, CAP_AUTHORITY_ADMIN],
            )
        {
            denied.push(proposed.id.clone());
            continue;
        }

        if action == PolicyChangeAction::Revoke
            && !actor_has_any_capability(
                &change.actor,
                previous_grants,
                context,
                &[CAP_POLICY_GRANT, CAP_AUTHORITY_ADMIN],
            )
        {
            denied.push(proposed.id.clone());
            continue;
        }
    }

    if denied.is_empty() {
        PolicyChangeEvaluation::approved("all proposed grants authorized by previous grants")
    } else {
        PolicyChangeEvaluation::denied(
            "one or more proposed grants exceed actor authority from previous grants",
            denied,
        )
    }
}

fn actor_has_any_capability(
    actor: &PrincipalDescriptor,
    previous_grants: &[Grant],
    context: &PolicyChangeContext,
    capabilities: &[&str],
) -> bool {
    capabilities.iter().any(|capability| {
        let request = PolicyEvaluationRequest {
            principal: actor.clone(),
            capability: Capability::new(*capability),
            resource: context.org_resource.clone(),
        };
        evaluate_policy(&request, previous_grants, &[]).decision == DecisionKind::Allow
    })
}

fn delegation_within_scope(
    actor: &PrincipalDescriptor,
    proposed: &ProposedGrant,
    previous_grants: &[Grant],
) -> bool {
    previous_grants.iter().any(|grant| {
        grant.principal == *actor
            && grant.effect == GrantEffect::Allow
            && resource_within(&proposed.resource, &grant.resource)
            && proposed
                .capabilities
                .iter()
                .all(|capability| capability_within(capability, &grant.capabilities))
    })
}

fn resource_within(requested: &ResourceRef, allowed: &ResourceRef) -> bool {
    requested.kind == allowed.kind && (requested.id == allowed.id || allowed.id == "*")
}

fn capability_within(requested: &Capability, allowed: &[Capability]) -> bool {
    allowed
        .iter()
        .any(|capability| capability.0 == requested.0 || capability.0 == "*")
}

fn is_self_grant(actor: &PrincipalDescriptor, recipient: &PrincipalDescriptor) -> bool {
    actor.kind == recipient.kind && actor.id == recipient.id
}

fn compute_signature(change: &PolicyChange, key: &AuthoritySigningKey) -> String {
    let payload = signing_payload(change);
    let digest = blake3::hash(format!("{}:{}:{}", payload, key.id, key.material).as_bytes());
    digest.to_hex().to_string()
}

fn signing_payload(change: &PolicyChange) -> String {
    let mut clone = change.clone();
    clone.signatures.clear();
    serde_json::to_string(&clone).unwrap_or_default()
}

fn protocol_version() -> String {
    PROTOCOL_VERSION.to_owned()
}

fn authority_root_kind() -> String {
    "AuthorityRoot".to_owned()
}

fn policy_change_kind() -> String {
    "PolicyChange".to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::{PrincipalId, PrincipalKind, ResourceKind};
    use serde_json::json;

    fn org() -> ResourceRef {
        ResourceRef::new(ResourceKind::Org, "org_sorrel")
    }

    fn root(version: u64) -> PolicyRoot {
        PolicyRoot::new(format!("policy_root_v{version}"), version)
    }

    fn context(version: u64) -> PolicyChangeContext {
        PolicyChangeContext::new(root(version), org())
    }

    fn authority(threshold: u32) -> AuthorityRoot {
        AuthorityRoot::new(
            "authority_root_main",
            "authority_root_hash_v1",
            vec![
                AuthoritySigningKey::new("key_alpha", "alpha-secret"),
                AuthoritySigningKey::new("key_beta", "beta-secret"),
            ],
            threshold,
        )
    }

    fn user(id: &str) -> PrincipalDescriptor {
        PrincipalDescriptor::new(PrincipalKind::User, id)
    }

    fn agent(id: &str) -> PrincipalDescriptor {
        PrincipalDescriptor::new(PrincipalKind::Agent, id)
    }

    fn policy_grant(id: &str, subject: PrincipalDescriptor, capability: &str) -> Grant {
        Grant::new(
            id,
            subject,
            Capability::new(capability),
            org(),
            GrantEffect::Allow,
        )
    }

    fn repo_grant(id: &str, subject: PrincipalDescriptor, capability: &str, repo: &str) -> Grant {
        Grant::new(
            id,
            subject,
            Capability::new(capability),
            ResourceRef::new(ResourceKind::Repo, repo),
            GrantEffect::Allow,
        )
    }

    fn signed_change(
        authority: &AuthorityRoot,
        mut change: PolicyChange,
        key_ids: &[&str],
    ) -> PolicyChange {
        change.signatures = key_ids
            .iter()
            .filter_map(|key_id| authority.sign_change(&change, key_id))
            .collect();
        change
    }

    #[test]
    fn serializes_policy_change_with_policy_roots() {
        let change = PolicyChange::new(
            "change_grant_001",
            agent("agent_a"),
            root(1),
            root(2),
            PolicyChangeAction::Grant,
        );

        let value = serde_json::to_value(&change).unwrap();
        assert_eq!(value["kind"], "PolicyChange");
        assert_eq!(value["previousPolicyRoot"]["hash"], "policy_root_v1");
        assert_eq!(value["proposedPolicyRoot"]["hash"], "policy_root_v2");
    }

    #[test]
    fn self_grant_denied_without_policy_capabilities() {
        let authority = authority(2);
        let actor = agent("agent_self");
        let mut change = PolicyChange::new(
            "change_self_grant",
            actor.clone(),
            root(1),
            root(2),
            PolicyChangeAction::Grant,
        );
        change.proposed_grants.push(ProposedGrant::new(
            "proposed_self_repo_read",
            actor.clone(),
            ["repo.read"],
            ResourceRef::new(ResourceKind::Repo, "repo_main"),
            GrantEffect::Allow,
        ));
        let change = signed_change(&authority, change, &["key_alpha"]);

        let evaluation = evaluate_policy_change(&change, &authority, &[], &context(1));

        assert_eq!(evaluation.trust, PolicyChangeTrust::Trusted);
        assert_eq!(evaluation.outcome, PolicyChangeOutcome::Denied);
        assert_eq!(evaluation.denied_grant_ids, vec!["proposed_self_repo_read"]);
    }

    #[test]
    fn unsigned_change_is_untrusted_and_denied() {
        let authority = authority(1);
        let actor = user("user_admin");
        let mut change = PolicyChange::new(
            "change_unsigned",
            actor,
            root(1),
            root(2),
            PolicyChangeAction::Grant,
        );
        change.proposed_grants.push(ProposedGrant::new(
            "proposed_grant",
            agent("agent_b"),
            [CAP_POLICY_GRANT],
            org(),
            GrantEffect::Allow,
        ));
        let previous = vec![policy_grant(
            "grant_admin",
            user("user_admin"),
            CAP_AUTHORITY_ADMIN,
        )];

        let evaluation = evaluate_policy_change(&change, &authority, &previous, &context(1));

        assert_eq!(evaluation.trust, PolicyChangeTrust::Untrusted);
        assert_eq!(evaluation.outcome, PolicyChangeOutcome::Denied);
        assert!(evaluation.reason.contains("unsigned"));
    }

    #[test]
    fn forged_signature_is_untrusted_and_denied() {
        let authority = authority(1);
        let actor = user("user_admin");
        let mut change = PolicyChange::new(
            "change_forged",
            actor,
            root(1),
            root(2),
            PolicyChangeAction::Grant,
        );
        change.signatures.push(AuthoritySignature {
            key_id: "key_alpha".to_owned(),
            value: "deadbeef".to_owned(),
        });

        let evaluation = evaluate_policy_change(&change, &authority, &[], &context(1));

        assert_eq!(evaluation.trust, PolicyChangeTrust::Untrusted);
        assert_eq!(evaluation.outcome, PolicyChangeOutcome::Denied);
        assert!(evaluation.reason.contains("forged") || evaluation.reason.contains("invalid"));
    }

    #[test]
    fn valid_delegated_grant_succeeds() {
        let authority = authority(1);
        let delegator = user("user_delegator");
        let delegatee = agent("agent_delegatee");
        let mut change = PolicyChange::new(
            "change_delegate",
            delegator.clone(),
            root(3),
            root(4),
            PolicyChangeAction::Delegate,
        );
        change.proposed_grants.push(ProposedGrant::new(
            "proposed_delegate_repo_read",
            delegatee,
            ["repo.read"],
            ResourceRef::new(ResourceKind::Repo, "repo_main"),
            GrantEffect::Allow,
        ));
        let change = signed_change(&authority, change, &["key_alpha"]);
        let previous = vec![
            policy_grant("grant_delegate_cap", delegator.clone(), CAP_POLICY_DELEGATE),
            repo_grant("grant_repo_read", delegator, "repo.read", "repo_main"),
        ];

        let evaluation = evaluate_policy_change(&change, &authority, &previous, &context(3));

        assert_eq!(evaluation.trust, PolicyChangeTrust::Trusted);
        assert_eq!(evaluation.outcome, PolicyChangeOutcome::Approved);
    }

    #[test]
    fn delegated_grant_cannot_exceed_received_scope() {
        let authority = authority(1);
        let delegator = user("user_delegator");
        let delegatee = agent("agent_delegatee");
        let mut change = PolicyChange::new(
            "change_delegate_exceed",
            delegator.clone(),
            root(5),
            root(6),
            PolicyChangeAction::Delegate,
        );
        change.proposed_grants.push(ProposedGrant::new(
            "proposed_delegate_repo_write",
            delegatee,
            ["repo.write"],
            ResourceRef::new(ResourceKind::Repo, "repo_main"),
            GrantEffect::Allow,
        ));
        let change = signed_change(&authority, change, &["key_alpha"]);
        let previous = vec![
            policy_grant("grant_delegate_cap", delegator.clone(), CAP_POLICY_DELEGATE),
            repo_grant("grant_repo_read_only", delegator, "repo.read", "repo_main"),
        ];

        let evaluation = evaluate_policy_change(&change, &authority, &previous, &context(5));

        assert_eq!(evaluation.trust, PolicyChangeTrust::Trusted);
        assert_eq!(evaluation.outcome, PolicyChangeOutcome::Denied);
        assert_eq!(
            evaluation.denied_grant_ids,
            vec!["proposed_delegate_repo_write"]
        );
    }

    #[test]
    fn authority_rotate_requires_configured_threshold() {
        let authority = authority(2);
        let actor = user("user_rotator");
        let change = PolicyChange::new(
            "change_rotate",
            actor.clone(),
            root(7),
            root(8),
            PolicyChangeAction::Rotate,
        );
        let change_one_sig = signed_change(&authority, change.clone(), &["key_alpha"]);
        let change_two_sigs = signed_change(&authority, change, &["key_alpha", "key_beta"]);
        let previous = vec![policy_grant("grant_rotate", actor, CAP_AUTHORITY_ROTATE)];

        let insufficient =
            evaluate_policy_change(&change_one_sig, &authority, &previous, &context(7));
        assert_eq!(insufficient.trust, PolicyChangeTrust::Untrusted);
        assert_eq!(insufficient.outcome, PolicyChangeOutcome::Denied);

        let sufficient =
            evaluate_policy_change(&change_two_sigs, &authority, &previous, &context(7));
        assert_eq!(sufficient.trust, PolicyChangeTrust::Trusted);
        assert_eq!(sufficient.outcome, PolicyChangeOutcome::Approved);
    }

    #[test]
    fn proposed_grants_are_never_used_as_actor_authority() {
        let authority = authority(1);
        let actor = agent("agent_attacker");
        let mut change = PolicyChange::new(
            "change_bootstrap",
            actor.clone(),
            root(9),
            root(10),
            PolicyChangeAction::Grant,
        );
        change.proposed_grants.push(ProposedGrant::new(
            "proposed_bootstrap_admin",
            actor.clone(),
            [CAP_AUTHORITY_ADMIN],
            org(),
            GrantEffect::Allow,
        ));
        change.proposed_grants.push(ProposedGrant::new(
            "proposed_other_grant",
            user("user_victim"),
            ["repo.read"],
            ResourceRef::new(ResourceKind::Repo, "repo_main"),
            GrantEffect::Allow,
        ));
        let change = signed_change(&authority, change, &["key_alpha"]);

        let evaluation = evaluate_policy_change(&change, &authority, &[], &context(9));

        assert_eq!(evaluation.trust, PolicyChangeTrust::Trusted);
        assert_eq!(evaluation.outcome, PolicyChangeOutcome::Denied);
        assert_eq!(evaluation.denied_grant_ids.len(), 2);
    }

    #[test]
    fn self_grant_allowed_with_policy_grant_capability() {
        let authority = authority(1);
        let actor = user("user_granter");
        let mut change = PolicyChange::new(
            "change_self_with_cap",
            actor.clone(),
            root(11),
            root(12),
            PolicyChangeAction::Grant,
        );
        change.proposed_grants.push(ProposedGrant::new(
            "proposed_self_allowed",
            actor.clone(),
            ["repo.read"],
            ResourceRef::new(ResourceKind::Repo, "repo_main"),
            GrantEffect::Allow,
        ));
        let change = signed_change(&authority, change, &["key_alpha"]);
        let previous = vec![policy_grant("grant_policy_grant", actor, CAP_POLICY_GRANT)];

        let evaluation = evaluate_policy_change(&change, &authority, &previous, &context(11));

        assert_eq!(evaluation.outcome, PolicyChangeOutcome::Approved);
    }

    #[test]
    fn authority_root_serializes_expected_shape() {
        let root = authority(2);
        let value = serde_json::to_value(&root).unwrap();
        assert_eq!(
            value,
            json!({
                "schemaVersion": "sorrel.protocol.v0",
                "kind": "AuthorityRoot",
                "id": "authority_root_main",
                "rootHash": "authority_root_hash_v1",
                "signingKeys": [
                    { "id": "key_alpha", "material": "alpha-secret" },
                    { "id": "key_beta", "material": "beta-secret" }
                ],
                "rotationThreshold": 2
            })
        );
    }

    #[test]
    fn mismatched_policy_root_is_denied_before_signature_checks() {
        let authority = authority(1);
        let actor = user("user_admin");
        let mut change = PolicyChange::new(
            "change_root_mismatch",
            actor,
            root(20),
            root(21),
            PolicyChangeAction::Grant,
        );
        change.proposed_grants.push(ProposedGrant::new(
            "proposed_grant",
            agent("agent_x"),
            ["repo.read"],
            ResourceRef::new(ResourceKind::Repo, "repo_main"),
            GrantEffect::Allow,
        ));
        let change = signed_change(&authority, change, &["key_alpha"]);
        let previous = vec![policy_grant(
            "grant_admin",
            user("user_admin"),
            CAP_AUTHORITY_ADMIN,
        )];
        let stale_context = context(19);

        let evaluation = evaluate_policy_change(&change, &authority, &previous, &stale_context);

        assert_eq!(evaluation.outcome, PolicyChangeOutcome::Denied);
        assert!(evaluation.reason.contains("previous policy root"));
    }

    #[test]
    fn principal_id_round_trip_in_proposed_grant() {
        let grant = ProposedGrant::new(
            "pg_1",
            PrincipalDescriptor {
                kind: PrincipalKind::Service,
                id: PrincipalId::new("svc_test"),
                display_name: None,
            },
            ["repo.read"],
            ResourceRef::new(ResourceKind::Repo, "repo_a"),
            GrantEffect::Allow,
        );
        assert_eq!(grant.principal.id.0, "svc_test");
    }
}
