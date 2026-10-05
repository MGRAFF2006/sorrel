use std::collections::BTreeMap;

use sorrel_runners::{
    CAPABILITY_RUNNER_USE, CAPABILITY_SECRET_INJECT, CAPABILITY_SECRET_READ, EnvValue,
    GrantStoreEvaluator, Job, JobBundle, LocalProcessRunner, ObjectRef, PolicyDecision, Shell,
};

#[test]
fn resolved_secrets_require_fresh_policy_and_are_redacted_from_every_log_record() {
    let secret = ObjectRef::new("SecretRef", "secret_test");
    let mut job = Job::shell(
        "job",
        Shell::Sh,
        "printf '%s' \"$TOKEN\"; printf '\nfixture-secret-value'",
        None,
    );
    job.env.insert(
        "TOKEN".to_owned(),
        EnvValue::SecretRef {
            secret: secret.clone(),
        },
    );
    let mut bundle = JobBundle::single("bundle", job);
    bundle.jobs.insert(
        0,
        Job::shell("earlier", Shell::Sh, "printf '%s' \"$TOKEN\"", None),
    );
    bundle.secret_refs.push(secret.clone());
    bundle.required_capabilities.extend([
        CAPABILITY_SECRET_READ.to_owned(),
        CAPABILITY_SECRET_INJECT.to_owned(),
    ]);
    let env = BTreeMap::from([("TOKEN".to_owned(), "fixture-secret-value".to_owned())]);
    let policy = GrantStoreEvaluator::from_grants([
        PolicyDecision::allow(
            CAPABILITY_RUNNER_USE,
            bundle.principal.clone(),
            ObjectRef::new("Runner", "runner_local_process"),
            "trusted grant",
        ),
        PolicyDecision::allow(
            CAPABILITY_SECRET_READ,
            bundle.principal.clone(),
            secret.clone(),
            "trusted grant",
        ),
        PolicyDecision::allow(
            CAPABILITY_SECRET_INJECT,
            bundle.principal.clone(),
            secret,
            "trusted grant",
        ),
    ]);
    let runner = LocalProcessRunner::default_local();
    assert!(
        runner
            .run_with_env(&bundle, &GrantStoreEvaluator::deny_all(), &env)
            .is_err()
    );
    let result = runner.run_with_env(&bundle, &policy, &env).unwrap();
    assert!(
        result.jobs[0]
            .stdout
            .contains("<sorrel:redacted secret_test>")
    );
    assert!(
        !serde_json::to_string(&result)
            .unwrap()
            .contains("fixture-secret-value")
    );
}
