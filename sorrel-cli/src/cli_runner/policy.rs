use crate::cli_policy::{
    evaluate, Decision, EvaluateInput, PolicyContext, PrincipalId, ResourceRef,
};

use super::bundle::JobBundle;

/// Policy denial returned before a job is executed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyGateError {
    pub action: String,
    pub result: String,
    pub reason: String,
    pub resource_type: String,
    pub resource_ref: String,
}

impl std::fmt::Display for PolicyGateError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "policy denied {} on {}:{} ({})",
            self.action, self.resource_type, self.resource_ref, self.reason
        )
    }
}

impl std::error::Error for PolicyGateError {}

/// Evaluates Core policy for workflow execution.
pub struct CorePermissionEvaluator<'a> {
    pub context: &'a PolicyContext,
    pub principal: PrincipalId,
}

impl CorePermissionEvaluator<'_> {
    /// Checks workflow.run, runner.use, and secret permissions for a bundle.
    pub fn authorize(&self, bundle: &JobBundle) -> Result<(), PolicyGateError> {
        self.check_action(
            "workflow.run",
            "workflow",
            &bundle.workflow_id,
            bundle.environment.as_deref(),
        )?;
        self.check_action(
            "runner.use",
            "runner",
            &bundle.runner_id,
            bundle.environment.as_deref(),
        )?;

        for secret_ref in &bundle.secret_refs {
            self.check_action(
                "secret.read",
                "secret",
                secret_ref,
                bundle.environment.as_deref(),
            )?;
            self.check_action(
                "secret.inject",
                "secret",
                secret_ref,
                bundle.environment.as_deref(),
            )?;
        }

        Ok(())
    }

    fn check_action(
        &self,
        action: &str,
        resource_type: &str,
        resource_ref: &str,
        environment: Option<&str>,
    ) -> Result<(), PolicyGateError> {
        let decision = evaluate(
            &EvaluateInput {
                principal: self.principal.clone(),
                action: action.to_owned(),
                resource: ResourceRef {
                    scope: resource_type.to_owned(),
                    id: resource_ref.to_owned(),
                },
                environment: environment.map(str::to_owned),
            },
            self.context,
        );

        if decision.decision == Decision::Allow {
            return Ok(());
        }

        Err(PolicyGateError {
            action: action.to_owned(),
            result: decision.decision.as_str().to_owned(),
            reason: decision.reason,
            resource_type: resource_type.to_owned(),
            resource_ref: resource_ref.to_owned(),
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::cli_policy::{Grant, PolicyContext, ResourceScope};

    use super::*;

    fn restrictive_context() -> PolicyContext {
        PolicyContext {
            repo_id: "repo_mock_local".to_owned(),
            authority_principals: vec![],
            grants: vec![],
            default_rules: vec![],
        }
    }

    fn granted_context() -> PolicyContext {
        let mut context = restrictive_context();
        context.grants.push(Grant {
            principal: PrincipalId {
                kind: "agent".to_owned(),
                id: "agent_mock_cli".to_owned(),
            },
            capabilities: vec![
                "workflow.run".to_owned(),
                "runner.use".to_owned(),
                "secret.read".to_owned(),
                "secret.inject".to_owned(),
            ],
            resources: vec![
                ResourceScope {
                    scope: "workflow".to_owned(),
                    fields: serde_json::json!({ "ref": "workflow_validate_protocol" })
                        .as_object()
                        .cloned()
                        .unwrap_or_default(),
                },
                ResourceScope {
                    scope: "runner".to_owned(),
                    fields: serde_json::json!({ "ref": "runner_local_process" })
                        .as_object()
                        .cloned()
                        .unwrap_or_default(),
                },
                ResourceScope {
                    scope: "secret".to_owned(),
                    fields: serde_json::json!({ "ref": "secret_npm_token_dev" })
                        .as_object()
                        .cloned()
                        .unwrap_or_default(),
                },
            ],
            issued_by: None,
        });
        context
    }

    fn sample_bundle() -> JobBundle {
        JobBundle {
            workflow_id: "workflow_validate_protocol".to_owned(),
            job_name: "test".to_owned(),
            runner_id: "runner_local_process".to_owned(),
            command: "echo hello".to_owned(),
            shell: "sh".to_owned(),
            secret_refs: vec!["secret_npm_token_dev".to_owned()],
            environment: Some("dev".to_owned()),
        }
    }

    #[test]
    fn missing_grants_deny_execution() {
        let evaluator = CorePermissionEvaluator {
            context: &restrictive_context(),
            principal: PrincipalId {
                kind: "agent".to_owned(),
                id: "agent_mock_cli".to_owned(),
            },
        };

        let error = evaluator
            .authorize(&sample_bundle())
            .expect_err("execution should be denied without grants");
        assert_eq!(error.action, "workflow.run");
        assert_eq!(error.result, "deny");
    }

    #[test]
    fn granted_capabilities_allow_execution() {
        let context = granted_context();
        let evaluator = CorePermissionEvaluator {
            context: &context,
            principal: PrincipalId {
                kind: "agent".to_owned(),
                id: "agent_mock_cli".to_owned(),
            },
        };

        evaluator
            .authorize(&sample_bundle())
            .expect("execution should be allowed with grants");
    }
}
