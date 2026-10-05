use crate::cli_policy::{Decision, PolicyContext, PrincipalId};
use sorrel_core::policy as core;

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
        let kind: core::ResourceKind = serde_json::from_value(serde_json::json!(resource_type))
            .map_err(|_| PolicyGateError {
                action: action.to_owned(),
                result: "deny".to_owned(),
                reason: "unsupported resource kind".to_owned(),
                resource_type: resource_type.to_owned(),
                resource_ref: resource_ref.to_owned(),
            })?;
        let principal: core::PrincipalDescriptor = serde_json::from_value(
            serde_json::json!({"kind": self.principal.kind, "id": self.principal.id}),
        )
        .map_err(|_| PolicyGateError {
            action: action.to_owned(),
            result: "deny".to_owned(),
            reason: "unsupported principal kind".to_owned(),
            resource_type: resource_type.to_owned(),
            resource_ref: resource_ref.to_owned(),
        })?;
        let mut grants = Vec::new();
        for (index, grant) in self.context.grants.iter().enumerate() {
            if grant.principal != self.principal {
                continue;
            }
            for resource in &grant.resources {
                if resource.scope != resource_type
                    || !resource.fields.iter().all(|(field, value)| {
                        let Some(value) = value.as_str() else {
                            return false;
                        };
                        match field.as_str() {
                            "ref" | "path" => value == "*" || value == resource_ref,
                            "environment" => environment == Some(value),
                            _ => false,
                        }
                    })
                {
                    continue;
                }
                // Scope constraints were checked above; Core remains the decision authority.
                let id = resource_ref;
                let mut native = core::Grant::new(
                    format!("cli_grant_{index}"),
                    principal.clone(),
                    core::Capability::new(action),
                    core::ResourceRef::new(kind, id),
                    core::GrantEffect::Allow,
                );
                native.capabilities = grant
                    .capabilities
                    .iter()
                    .map(core::Capability::new)
                    .collect();
                grants.push(native);
            }
        }
        let mut policies = Vec::new();
        for (index, rule) in self.context.default_rules.iter().enumerate() {
            if rule.action != action || rule.decision == Decision::NeedsGrant {
                continue;
            }
            let mut policy = core::Policy::new(
                format!("cli_default_{index}"),
                core::ResourceRef::new(kind, "*"),
            );
            policy.default_decision = Some(match rule.decision {
                Decision::Allow => core::DecisionKind::Allow,
                Decision::Deny => core::DecisionKind::Deny,
                Decision::Redact => core::DecisionKind::Redact,
                Decision::NeedsReview => core::DecisionKind::NeedsReview,
                Decision::NeedsGrant => unreachable!(),
            });
            policies.push(policy);
        }
        let request = core::PolicyEvaluationRequest {
            principal,
            capability: core::Capability::new(action),
            resource: core::ResourceRef::new(kind, resource_ref),
        };
        let decision = core::evaluate_policy(&request, &grants, &policies);
        if decision.decision == core::DecisionKind::Allow {
            return Ok(());
        }
        let result = match decision.decision {
            core::DecisionKind::Redact => "redact",
            core::DecisionKind::NeedsReview => "needs_review",
            _ => "deny",
        };
        Err(PolicyGateError {
            action: action.to_owned(),
            result: result.to_owned(),
            reason: decision
                .reason
                .unwrap_or_else(|| "Core policy did not allow execution".to_owned()),
            resource_type: resource_type.to_owned(),
            resource_ref: resource_ref.to_owned(),
        })
    }
}

struct WorkflowPermissionEvaluator<'a, 'context> {
    evaluator: &'a CorePermissionEvaluator<'context>,
    environment: Option<&'a str>,
}

impl CorePermissionEvaluator<'_> {
    pub fn with_environment<'a>(
        &'a self,
        environment: Option<&'a str>,
    ) -> impl sorrel_runners::CorePermissionEvaluator + 'a {
        WorkflowPermissionEvaluator {
            evaluator: self,
            environment,
        }
    }

    fn evaluate_for_environment(
        &self,
        principal: &sorrel_runners::PrincipalContext,
        capability: &str,
        resource: &sorrel_runners::ObjectRef,
        environment: Option<&str>,
    ) -> sorrel_runners::PolicyDecision {
        let scope = match resource.kind.as_str() {
            "SecretRef" => "secret",
            "Runner" => "runner",
            "Workflow" => "workflow",
            _ => "unsupported",
        };
        match self.check_action(capability, scope, &resource.id, environment) {
            Ok(()) => sorrel_runners::PolicyDecision::allow(
                capability,
                principal.clone(),
                resource.clone(),
                "authorized by Core policy",
            ),
            Err(error) => {
                let mut denied = sorrel_runners::PolicyDecision::needs_grant(
                    capability,
                    principal.clone(),
                    resource.clone(),
                    error.reason,
                );
                denied.status = sorrel_runners::PolicyDecisionStatus::Deny;
                denied
            }
        }
    }
}

impl sorrel_runners::CorePermissionEvaluator for CorePermissionEvaluator<'_> {
    fn evaluate(
        &self,
        principal: &sorrel_runners::PrincipalContext,
        capability: &str,
        resource: &sorrel_runners::ObjectRef,
    ) -> sorrel_runners::PolicyDecision {
        self.evaluate_for_environment(principal, capability, resource, None)
    }
}

impl sorrel_runners::CorePermissionEvaluator for WorkflowPermissionEvaluator<'_, '_> {
    fn evaluate(
        &self,
        principal: &sorrel_runners::PrincipalContext,
        capability: &str,
        resource: &sorrel_runners::ObjectRef,
    ) -> sorrel_runners::PolicyDecision {
        self.evaluator
            .evaluate_for_environment(principal, capability, resource, self.environment)
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
            native: None,
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

    #[test]
    fn native_core_deny_precedes_a_matching_allow_grant() {
        let mut context = granted_context();
        context.default_rules.push(crate::cli_policy::PolicyRule {
            action: "workflow.run".to_owned(),
            decision: Decision::Deny,
            reason: "blocked workflow".to_owned(),
        });
        let evaluator = CorePermissionEvaluator {
            context: &context,
            principal: PrincipalId {
                kind: "agent".to_owned(),
                id: "agent_mock_cli".to_owned(),
            },
        };
        assert_eq!(
            evaluator.authorize(&sample_bundle()).unwrap_err().result,
            "deny"
        );
    }
    #[test]
    fn environment_scopes_apply_to_cli_and_shared_runner_authorization() {
        use sorrel_runners::CorePermissionEvaluator as _;
        let mut context = granted_context();
        context.grants[0].resources[2]
            .fields
            .insert("environment".to_owned(), serde_json::json!("prod"));
        let evaluator = CorePermissionEvaluator {
            context: &context,
            principal: context.grants[0].principal.clone(),
        };
        assert_eq!(
            evaluator.authorize(&sample_bundle()).unwrap_err().action,
            "secret.read"
        );
        let principal = sorrel_runners::PrincipalContext::default();
        let secret = sorrel_runners::ObjectRef::new("SecretRef", "secret_npm_token_dev");
        assert_eq!(
            evaluator
                .with_environment(Some("dev"))
                .evaluate(&principal, "secret.read", &secret)
                .status,
            sorrel_runners::PolicyDecisionStatus::Deny
        );
        assert_eq!(
            evaluator
                .with_environment(Some("prod"))
                .evaluate(&principal, "secret.read", &secret)
                .status,
            sorrel_runners::PolicyDecisionStatus::Allow
        );
        assert_eq!(
            evaluator
                .evaluate(&principal, "secret.read", &secret)
                .status,
            sorrel_runners::PolicyDecisionStatus::Deny
        );
    }

    #[test]
    fn additional_scope_constraints_cannot_broaden_a_grant() {
        for fields in [
            serde_json::json!({"ref": "secret_npm_token_dev", "path": "other"}),
            serde_json::json!({"ref": "secret_npm_token_dev", "unknown": "constraint"}),
            serde_json::json!({"ref": 42}),
        ] {
            let mut context = granted_context();
            context.grants[0].resources[2].fields = fields.as_object().unwrap().clone();
            let evaluator = CorePermissionEvaluator {
                context: &context,
                principal: context.grants[0].principal.clone(),
            };
            assert_eq!(
                evaluator.authorize(&sample_bundle()).unwrap_err().action,
                "secret.read"
            );
        }
    }
}
