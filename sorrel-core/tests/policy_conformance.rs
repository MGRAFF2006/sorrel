//! Conformance tests proving the Core evaluator agrees with the canonical
//! `sorrel-protocol` policy conformance manifest.
//!
//! The manifest (`tests/conformance/policy-conformance.json`) is a vendored copy
//! of `sorrel-protocol/conformance/policy-conformance.json`. Because the protocol
//! manifest shape and the Core Rust structs differ, this file contains a small
//! mapping layer from manifest cases to Core `evaluate` / `evaluate_policy_change`
//! inputs. The documented gap is acceptable for v0 (see
//! `sorrel-protocol/docs/policy-conformance.md`).

use serde_json::Value;
use sorrel_core::{
    evaluate_policy, evaluate_policy_change, AuthorityRoot, AuthoritySignature,
    AuthoritySigningKey, Capability, DecisionKind, GrantEffect, PolicyChange, PolicyChangeAction,
    PolicyChangeContext, PolicyChangeOutcome, PolicyChangeTrust, PolicyEvaluationRequest,
    PolicyGrant as Grant, PolicyResourceRef as ResourceRef, PolicyRoot, PrincipalDescriptor,
    PrincipalKind, ProposedGrant, ResourceKind,
};

const MANIFEST: &str = include_str!("conformance/policy-conformance.json");

fn manifest() -> Value {
    serde_json::from_str(MANIFEST).expect("manifest is valid JSON")
}

fn principal_kind(value: &str) -> PrincipalKind {
    match value {
        "user" => PrincipalKind::User,
        "agent" => PrincipalKind::Agent,
        "team" => PrincipalKind::Team,
        "runner" => PrincipalKind::Runner,
        "system" => PrincipalKind::System,
        // The protocol models `workflow` principals; Core has no dedicated kind,
        // so map deterministically to Service. Requests and grants in a case use
        // the same mapping, so matching is preserved.
        "workflow" | "service" => PrincipalKind::Service,
        other => panic!("unsupported principal type in manifest: {other}"),
    }
}

fn resource_kind(value: &str) -> ResourceKind {
    match value {
        "repo" => ResourceKind::Repo,
        "path" => ResourceKind::Path,
        "secret" => ResourceKind::Secret,
        "workflow" => ResourceKind::Workflow,
        "runner" => ResourceKind::Runner,
        other => panic!("unsupported resource kind in manifest: {other}"),
    }
}

fn grant_effect(value: &str) -> GrantEffect {
    match value {
        "allow" => GrantEffect::Allow,
        "deny" => GrantEffect::Deny,
        "redact" => GrantEffect::Redact,
        "review" => GrantEffect::Review,
        other => panic!("unsupported grant effect in manifest: {other}"),
    }
}

fn principal_from(value: &Value) -> PrincipalDescriptor {
    PrincipalDescriptor::new(
        principal_kind(value["type"].as_str().unwrap()),
        value["id"].as_str().unwrap(),
    )
}

fn resource_from(value: &Value) -> ResourceRef {
    ResourceRef::new(
        resource_kind(value["kind"].as_str().unwrap()),
        value["id"].as_str().unwrap(),
    )
}

fn grant_from(value: &Value) -> Grant {
    Grant::new(
        value["id"].as_str().unwrap(),
        principal_from(&value["principal"]),
        Capability::new(value["capability"].as_str().unwrap()),
        resource_from(&value["resource"]),
        grant_effect(value["effect"].as_str().unwrap()),
    )
}

#[test]
fn core_agrees_with_permission_decision_vectors() {
    let manifest = manifest();
    let cases = manifest["permissionDecisions"].as_array().unwrap();
    assert!(!cases.is_empty(), "expected permission decision vectors");

    for case in cases {
        let id = case["id"].as_str().unwrap();
        let request = PolicyEvaluationRequest {
            principal: principal_from(&case["request"]["principal"]),
            capability: Capability::new(case["request"]["capability"].as_str().unwrap()),
            resource: resource_from(&case["request"]["resource"]),
        };
        let grants: Vec<Grant> = case["grants"]
            .as_array()
            .unwrap()
            .iter()
            .map(grant_from)
            .collect();

        let decision = evaluate_policy(&request, &grants, &[]);
        let expected = case["expected"].as_str().unwrap();

        let actual_allowed = decision.decision == DecisionKind::Allow;
        match expected {
            "allow" => assert!(
                actual_allowed,
                "case {id}: expected allow, got {:?}",
                decision.decision
            ),
            "deny" | "needs_grant" => assert!(
                !actual_allowed,
                "case {id}: expected not-allowed ({expected}), got {:?}",
                decision.decision
            ),
            other => panic!("case {id}: unsupported expected decision {other}"),
        }
    }
}

/// Builds an `AuthorityRoot` with two deterministic test keys and the requested
/// rotation threshold, matching the existing Core test-signature pattern.
fn authority(threshold: u32) -> AuthorityRoot {
    AuthorityRoot::new(
        "authority_root_repo_main",
        "authority_root_hash_v1",
        vec![
            AuthoritySigningKey::new("key_alpha", "alpha-secret"),
            AuthoritySigningKey::new("key_beta", "beta-secret"),
        ],
        threshold,
    )
}

fn org_resource() -> ResourceRef {
    ResourceRef::new(ResourceKind::Repo, "resource_repo_main")
}

fn policy_change_action(operation: &str) -> PolicyChangeAction {
    match operation {
        "policy.grant" => PolicyChangeAction::Grant,
        "policy.delegate" => PolicyChangeAction::Delegate,
        "policy.revoke" => PolicyChangeAction::Revoke,
        "authority.rotate" => PolicyChangeAction::Rotate,
        other => panic!("unsupported policy change operation: {other}"),
    }
}

/// Maps the manifest grant-rule capability (e.g. `path.write`) onto the resource
/// scope the proposed grant targets, so previous-grant authority can be modeled.
fn previous_grant_from(value: &Value) -> Grant {
    Grant::new(
        value["id"].as_str().unwrap(),
        principal_from(&value["principal"]),
        Capability::new(value["capability"].as_str().unwrap()),
        // Core's `actor_has_any_capability` checks against `context.org_resource`,
        // so previous policy/authority grants must target that resource id.
        match value["capability"].as_str().unwrap() {
            "policy.grant" | "policy.delegate" | "authority.rotate" | "authority.admin" => {
                org_resource()
            }
            _ => resource_from(&value["resource"]),
        },
        grant_effect(value["effect"].as_str().unwrap()),
    )
}

#[test]
fn core_agrees_with_policy_change_vectors() {
    let manifest = manifest();
    let cases = manifest["policyChanges"].as_array().unwrap();
    assert!(!cases.is_empty(), "expected policy change vectors");

    for case in cases {
        let id = case["id"].as_str().unwrap();
        let operation = case["operation"].as_str().unwrap();
        let action = policy_change_action(operation);
        let actor = principal_from(&case["actor"]);

        // Rotation threshold: the manifest may specify a sub-threshold case.
        let threshold = case
            .get("thresholdMinimum")
            .and_then(Value::as_u64)
            .unwrap_or(1) as u32;
        let signature_weight = case
            .get("signatureWeight")
            .and_then(Value::as_u64)
            .unwrap_or(1) as usize;
        let authority = authority(if action == PolicyChangeAction::Rotate {
            threshold
        } else {
            1
        });

        let mut change = PolicyChange::new(
            format!("change_{id}"),
            actor.clone(),
            PolicyRoot::new("authority_root_hash_v1", 1),
            PolicyRoot::new("authority_root_hash_v2", 2),
            action,
        );

        if let Some(proposed) = case.get("proposedGrant") {
            let capabilities: Vec<String> = proposed["capabilities"]
                .as_array()
                .unwrap()
                .iter()
                .map(|capability| capability.as_str().unwrap().to_owned())
                .collect();
            change.proposed_grants.push(ProposedGrant::new(
                format!("proposed_{id}"),
                principal_from(&proposed["principal"]),
                capabilities,
                resource_from(&proposed["resource"]),
                grant_effect(proposed["effect"].as_str().unwrap()),
            ));
        }

        // Apply signatures according to manifest posture.
        let signed = case["signed"].as_bool().unwrap_or(false);
        let forged = case
            .get("forgedSignature")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if forged {
            // Attach a signature whose value cannot be produced by any authority
            // key, so verification fails and the change is untrusted.
            change.signatures = vec![AuthoritySignature {
                key_id: "key_alpha".to_owned(),
                value: "forged-signature-value".to_owned(),
            }];
        } else if signed {
            // For rotation, attach enough valid signatures to reach the modeled
            // weight; otherwise a single valid signature.
            let key_ids: &[&str] = if action == PolicyChangeAction::Rotate {
                match signature_weight {
                    0 => &[],
                    1 => &["key_alpha"],
                    _ => &["key_alpha", "key_beta"],
                }
            } else {
                &["key_alpha"]
            };
            change.signatures = key_ids
                .iter()
                .filter_map(|key_id| authority.sign_change(&change, key_id))
                .collect();
        }

        let previous_grants: Vec<Grant> = case["previousGrants"]
            .as_array()
            .unwrap()
            .iter()
            .map(previous_grant_from)
            .collect();

        let context =
            PolicyChangeContext::new(PolicyRoot::new("authority_root_hash_v1", 1), org_resource());

        let evaluation = evaluate_policy_change(&change, &authority, &previous_grants, &context);

        let expected_trust = case["expected"]["trust"].as_str().unwrap();
        let expected_outcome = case["expected"]["outcome"].as_str().unwrap();

        let actual_trust = match evaluation.trust {
            PolicyChangeTrust::Trusted => "trusted",
            PolicyChangeTrust::Untrusted => "untrusted",
        };
        // The protocol distinguishes `denied` from `untrusted` for trust; Core
        // folds a trusted-but-denied change into (trusted, denied). Treat the
        // protocol `denied` trust value as "trusted signatures, denied outcome".
        let trust_matches = match expected_trust {
            "untrusted" => actual_trust == "untrusted",
            "trusted" | "denied" => actual_trust == "trusted",
            other => panic!("case {id}: unsupported expected trust {other}"),
        };
        assert!(
            trust_matches,
            "case {id}: expected trust {expected_trust}, got {actual_trust}"
        );

        let actual_outcome = match evaluation.outcome {
            PolicyChangeOutcome::Approved => "allow",
            PolicyChangeOutcome::Denied => "deny",
        };
        assert_eq!(
            actual_outcome, expected_outcome,
            "case {id}: expected outcome {expected_outcome}, got {actual_outcome} (reason: {})",
            evaluation.reason
        );
    }
}
