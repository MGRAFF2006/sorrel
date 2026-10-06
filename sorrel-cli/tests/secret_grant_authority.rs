use assert_cmd::Command;
use serde_json::{json, Value};
use sorrel_core::authority::{
    AuthorityRoot, AuthoritySigningKey, PolicyChange, PolicyChangeAction, PolicyChangeContext,
    PolicyRoot,
};
use sorrel_core::policy::{
    Capability, Grant, GrantEffect, PrincipalDescriptor, PrincipalKind, ResourceKind, ResourceRef,
};
use std::fs;
use std::path::Path;

fn command(root: &Path) -> Command {
    let mut cmd = Command::cargo_bin("sorrel").unwrap();
    cmd.current_dir(root)
        .env_remove("SORREL_AUTHORITY_CONTEXT")
        .env_remove("SORREL_LOCAL_DEMO");
    cmd
}
fn output(root: &Path, args: &[&str]) -> Value {
    let mut cmd = command(root);
    cmd.args(args);
    if !args.contains(&"--json") {
        cmd.arg("--json");
    }
    let bytes = cmd.assert().success().get_output().stdout.clone();
    serde_json::from_slice(&bytes).unwrap()
}
fn source(root: &Path, request: &Value) -> (tempfile::TempDir, PolicyChange) {
    let outside = tempfile::tempdir().unwrap();
    let actor = PrincipalDescriptor::new(PrincipalKind::User, "operator");
    let org = ResourceRef::new(ResourceKind::Org, "org_test");
    let authority = AuthorityRoot::new(
        "authority_test",
        "authority_hash",
        vec![AuthoritySigningKey::new(
            "key",
            "synthetic-signing-material-never-persist-this",
        )],
        1,
    );
    let previous = PolicyRoot::new("previous", 1);
    let context = PolicyChangeContext::new(previous.clone(), org.clone());
    let grants = vec![Grant::new(
        "operator",
        actor.clone(),
        Capability::new("policy.grant"),
        org,
        GrantEffect::Allow,
    )];
    fs::write(
        outside.path().join("authority.json"),
        serde_json::to_vec(
            &json!({"authorityRoot":authority,"previousGrants":grants,"context":context}),
        )
        .unwrap(),
    )
    .unwrap();
    let mut change = PolicyChange::new(
        "approved_change",
        actor,
        previous,
        PolicyRoot::new("proposed", 2),
        PolicyChangeAction::Grant,
    );
    change.proposed_grants = serde_json::from_value(request["proposedGrants"].clone()).unwrap();
    change.signatures = vec![authority.sign_change(&change, "key").unwrap()];
    fs::write(
        outside.path().join("change.json"),
        serde_json::to_vec(&change).unwrap(),
    )
    .unwrap();
    assert!(!root.starts_with(outside.path()));
    (outside, change)
}
fn approve(root: &Path, source: &Path, extra: &[&str]) -> Value {
    let mut args = vec!["grant", "create", "--json", "--authority-context"];
    let context = source.join("authority.json");
    let change = source.join("change.json");
    args.extend([
        context.to_str().unwrap(),
        "--policy-change",
        change.to_str().unwrap(),
    ]);
    args.extend(extra);
    output(root, &args)
}

#[test]
fn request_and_unapproved_grants_never_persist() {
    let temp = tempfile::tempdir().unwrap();
    output(temp.path(), &["init"]);
    let denied = output(temp.path(), &["grant", "create", "--json"]);
    assert_eq!(denied["status"], "needs_grant");
    assert_eq!(denied["persisted"], false);
    let request = output(
        temp.path(),
        &[
            "grant",
            "create",
            "--request-only",
            "--agent",
            "agent_b",
            "--agent",
            "agent_a",
            "--json",
        ],
    );
    assert_eq!(request["scope"]["agents"], json!(["agent_a", "agent_b"]));
    assert_eq!(
        request["proposedGrants"][0]["capabilities"],
        json!(["secret.inject", "secret.read"])
    );
    assert_eq!(request["persisted"], false);
    assert_eq!(
        output(temp.path(), &["grant", "list", "--json"])["count"],
        0
    );
}

#[test]
fn signed_issuance_rejects_tampering_and_keeps_authority_material_external() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    output(root, &["init"]);
    let request = output(root, &["grant", "create", "--request-only", "--json"]);
    let (outside, mut change) = source(root, &request);
    let approved = approve(root, outside.path(), &[]);
    assert_eq!(approved["persisted"], true);
    assert_eq!(approved["status"], "allow");
    assert_eq!(approved["mocked"], false);
    let persisted = fs::read(root.join(format!(
        ".sorrel/grants/{}.json",
        request["grantId"].as_str().unwrap()
    )))
    .unwrap();
    assert!(!String::from_utf8_lossy(&persisted)
        .contains("synthetic-signing-material-never-persist-this"));
    assert!(!approved
        .to_string()
        .contains("synthetic-signing-material-never-persist-this"));
    command(root)
        .args([
            "grant",
            "create",
            "--environment",
            "prod",
            "--authority-context",
            outside.path().join("authority.json").to_str().unwrap(),
            "--policy-change",
            outside.path().join("change.json").to_str().unwrap(),
        ])
        .assert()
        .failure();
    change.signatures[0].value = "forged".to_owned();
    fs::write(
        outside.path().join("change.json"),
        serde_json::to_vec(&change).unwrap(),
    )
    .unwrap();
    command(root)
        .args([
            "grant",
            "create",
            "--authority-context",
            outside.path().join("authority.json").to_str().unwrap(),
            "--policy-change",
            outside.path().join("change.json").to_str().unwrap(),
        ])
        .assert()
        .failure();
    assert_eq!(output(root, &["grant", "list", "--json"])["count"], 1);
}

#[test]
fn native_grants_require_current_trust_and_workflow_constraints_before_resolution() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    output(root, &["init"]);
    fs::write(root.join("sorrel.secrets.yml"), "secretRefs:\n  - id: secret_test\n    name: TEST_TOKEN\n    provider: dotenv\n    uri: dotenv:.env\n    environment: dev\n    required: true\n").unwrap();
    fs::write(root.join(".gitignore"), ".env\n").unwrap();
    fs::write(root.join(".env"), "TEST_TOKEN=synthetic-token\n").unwrap();
    let scope = [
        "--secret",
        "secret_test",
        "--workflow",
        "workflow_allowed",
        "--runner",
        "runner_local_process",
    ];
    let mut request_args = vec!["grant", "create", "--request-only", "--json"];
    request_args.extend(scope);
    let request = output(root, &request_args);
    let (outside, _) = source(root, &request);
    approve(root, outside.path(), &scope);
    // A direct invocation cannot claim the approved workflow's identity.
    command(root)
        .env(
            "SORREL_AUTHORITY_CONTEXT",
            outside.path().join("authority.json"),
        )
        .args([
            "secret",
            "get",
            "secret_test",
            "--reveal",
            "--provider",
            "dotenv:.env",
        ])
        .assert()
        .failure();
    fs::write(root.join("sorrel.workflow.yml"), "version: 1\nid: workflow_allowed\njobs:\n  test:\n    command: printf '%s' \"$TEST_TOKEN\"\n    secrets: [secret_test]\n").unwrap();
    let denied = command(root)
        .args(["workflow", "run", "test", "--json"])
        .output()
        .unwrap();
    let denied: Value = serde_json::from_slice(&denied.stdout).unwrap();
    assert_eq!(denied["status"], "denied");
    let allowed = command(root)
        .env(
            "SORREL_AUTHORITY_CONTEXT",
            outside.path().join("authority.json"),
        )
        .args(["workflow", "run", "test", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let allowed: Value = serde_json::from_slice(&allowed).unwrap();
    assert_eq!(allowed["status"], "completed");
    assert!(!allowed.to_string().contains("synthetic-token"));
    fs::write(root.join("sorrel.workflow.yml"), "version: 1\nid: workflow_other\njobs:\n  test:\n    command: touch SHOULD_NOT_EXIST\n    secrets: [secret_test]\n").unwrap();
    let denied = command(root)
        .env(
            "SORREL_AUTHORITY_CONTEXT",
            outside.path().join("authority.json"),
        )
        .args(["workflow", "run", "test", "--json"])
        .output()
        .unwrap();
    let denied: Value = serde_json::from_slice(&denied.stdout).unwrap();
    assert_eq!(denied["status"], "denied");
    assert!(!root.join("SHOULD_NOT_EXIST").exists());
    // The provider handle's environment must agree with the authorized bundle;
    // the workflow adapter's default dev cannot authorize a prod provider profile.
    let handles = fs::read_to_string(root.join("sorrel.secrets.yml")).unwrap();
    fs::write(
        root.join("sorrel.secrets.yml"),
        handles.replace("environment: dev", "environment: prod"),
    )
    .unwrap();
    fs::write(root.join("sorrel.workflow.yml"), "version: 1\nid: workflow_allowed\njobs:\n  test:\n    command: touch SHOULD_NOT_EXIST\n    secrets: [secret_test]\n").unwrap();
    let denied = command(root)
        .env(
            "SORREL_AUTHORITY_CONTEXT",
            outside.path().join("authority.json"),
        )
        .args(["workflow", "run", "test", "--json"])
        .output()
        .unwrap();
    let denied: Value = serde_json::from_slice(&denied.stdout).unwrap();
    assert_eq!(denied["status"], "failed");
    assert_eq!(denied["error"]["kind"], "policy_load_failed");
    assert!(!root.join("SHOULD_NOT_EXIST").exists());
}
