//! Conformance tests proving the CLI's `cli_policy` evaluator agrees with the
//! canonical `sorrel-protocol` policy conformance manifest.
//!
//! `cli_policy` is the CLI's self-contained policy surface (it used to live in
//! `sorrel-core::cli_policy`). These tests prove it produces the same decisions
//! as the canonical protocol vectors, so CLI output cannot silently drift from
//! the protocol contract that Core also conforms to.
//!
//! The manifest (`tests/conformance/policy-conformance.json`) is a vendored copy
//! of `sorrel-protocol/conformance/policy-conformance.json`. A small mapping
//! layer bridges the manifest shape and the `cli_policy` types.

use serde_json::{Map, Value};
use sorrel_cli::cli_policy::{
    evaluate, evaluate_policy_change, Decision, EvaluateInput, Grant, PolicyChange, PolicyContext,
    PrincipalId, ProposedGrant, ResourceRef, ResourceScope,
};

const MANIFEST: &str = include_str!("conformance/policy-conformance.json");

fn manifest() -> Value {
    serde_json::from_str(MANIFEST).expect("manifest is valid JSON")
}

fn principal_from(value: &Value) -> PrincipalId {
    PrincipalId {
        kind: value["type"].as_str().unwrap().to_owned(),
        id: value["id"].as_str().unwrap().to_owned(),
    }
}

fn resource_ref_from(value: &Value) -> ResourceRef {
    ResourceRef {
        scope: value["kind"].as_str().unwrap().to_owned(),
        id: value["id"].as_str().unwrap().to_owned(),
    }
}

fn resource_scope_from(value: &Value) -> ResourceScope {
    let mut fields = Map::new();
    fields.insert(
        "ref".to_owned(),
        Value::String(value["id"].as_str().unwrap().to_owned()),
    );
    ResourceScope {
        scope: value["kind"].as_str().unwrap().to_owned(),
        fields,
    }
}

fn grant_from(value: &Value) -> Grant {
    Grant {
        principal: principal_from(&value["principal"]),
        capabilities: vec![value["capability"].as_str().unwrap().to_owned()],
        resources: vec![resource_scope_from(&value["resource"])],
        issued_by: None,
    }
}

/// Builds a context that decides purely from the case's grants: no default rules
/// and no implicit authority principals. This isolates grant-based conformance.
fn grant_only_context(grants: Vec<Grant>) -> PolicyContext {
    PolicyContext {
        repo_id: "resource_repo_main".to_owned(),
        grants,
        authority_principals: Vec::new(),
        default_rules: Vec::new(),
    }
}

#[test]
fn embedded_core_agrees_with_permission_decision_vectors() {
    let manifest = manifest();
    let cases = manifest["permissionDecisions"].as_array().unwrap();
    assert!(!cases.is_empty());

    for case in cases {
        let id = case["id"].as_str().unwrap();
        let input = EvaluateInput {
            principal: principal_from(&case["request"]["principal"]),
            action: case["request"]["capability"].as_str().unwrap().to_owned(),
            resource: resource_ref_from(&case["request"]["resource"]),
            environment: None,
        };
        let grants: Vec<Grant> = case["grants"]
            .as_array()
            .unwrap()
            .iter()
            .map(grant_from)
            .collect();
        let context = grant_only_context(grants);

        let decision = evaluate(&input, &context);
        let expected = case["expected"].as_str().unwrap();
        let allowed = decision.decision == Decision::Allow;

        match expected {
            "allow" => assert!(
                allowed,
                "case {id}: expected allow, got {:?}",
                decision.decision
            ),
            "deny" | "needs_grant" => assert!(
                !allowed,
                "case {id}: expected not-allowed ({expected}), got {:?}",
                decision.decision
            ),
            other => panic!("case {id}: unsupported expected {other}"),
        }
    }
}

/// Maps a manifest "previous grant" rule into an embedded Core grant. Policy and
/// authority capabilities are scoped to their declared resource so delegation and
/// scope-broadening checks behave correctly.
fn previous_grant_from(value: &Value) -> Grant {
    Grant {
        principal: principal_from(&value["principal"]),
        capabilities: vec![value["capability"].as_str().unwrap().to_owned()],
        resources: vec![resource_scope_from(&value["resource"])],
        issued_by: None,
    }
}

fn proposed_grant_from(value: &Value) -> ProposedGrant {
    let capabilities = value["capabilities"]
        .as_array()
        .unwrap()
        .iter()
        .map(|capability| capability.as_str().unwrap().to_owned())
        .collect();
    ProposedGrant {
        principal: principal_from(&value["principal"]),
        capabilities,
        resources: vec![resource_scope_from(&value["resource"])],
    }
}

#[test]
fn embedded_core_agrees_with_policy_change_vectors() {
    let manifest = manifest();
    let cases = manifest["policyChanges"].as_array().unwrap();
    assert!(!cases.is_empty());

    for case in cases {
        let id = case["id"].as_str().unwrap();
        let operation = match case["operation"].as_str().unwrap() {
            "policy.grant" => "grant",
            "policy.delegate" => "delegate",
            "policy.revoke" => "revoke",
            "authority.rotate" => "rotate_authority",
            other => panic!("case {id}: unsupported operation {other}"),
        };

        let signed = case["signed"].as_bool().unwrap_or(false);
        let forged = case
            .get("forgedSignature")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let threshold_met = case
            .get("signatureWeight")
            .and_then(Value::as_u64)
            .zip(case.get("thresholdMinimum").and_then(Value::as_u64))
            .map(|(weight, minimum)| weight >= minimum)
            .unwrap_or(true);

        // The embedded Core models signature trust via string markers.
        let signatures = if forged {
            vec!["sig_invalid_forged".to_owned()]
        } else if !signed || !threshold_met {
            // Unsigned, or rotation below threshold: model as untrusted.
            Vec::new()
        } else {
            vec!["sig_valid_maintainer".to_owned()]
        };

        let change = PolicyChange {
            actor: principal_from(&case["actor"]),
            operation: operation.to_owned(),
            grant: case.get("proposedGrant").map(proposed_grant_from),
            signatures,
        };

        let grants: Vec<Grant> = case["previousGrants"]
            .as_array()
            .unwrap()
            .iter()
            .map(previous_grant_from)
            .collect();

        let context = PolicyContext {
            repo_id: "resource_repo_main".to_owned(),
            grants,
            authority_principals: Vec::new(),
            default_rules: Vec::new(),
        };

        let evaluation = evaluate_policy_change(&change, &context);

        let expected_trust = case["expected"]["trust"].as_str().unwrap();
        let expected_outcome = case["expected"]["outcome"].as_str().unwrap();

        // Protocol `trust`: untrusted | trusted | denied. Embedded Core exposes a
        // bool `trusted`; protocol `denied` means trusted-signatures-but-denied.
        let trust_ok = match expected_trust {
            "untrusted" => !evaluation.trusted,
            "trusted" | "denied" => evaluation.trusted,
            other => panic!("case {id}: unsupported expected trust {other}"),
        };
        assert!(
            trust_ok,
            "case {id}: expected trust {expected_trust}, embedded trusted={}",
            evaluation.trusted
        );

        let actual_outcome = match evaluation.decision {
            Decision::Allow => "allow",
            _ => "deny",
        };
        assert_eq!(
            actual_outcome, expected_outcome,
            "case {id}: expected outcome {expected_outcome}, got {actual_outcome} (reason: {})",
            evaluation.reason
        );
    }
}
