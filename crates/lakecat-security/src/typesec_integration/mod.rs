use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use lakecat_core::{LakeCatResult, content_hash_json};
use typesec::{CombineStrategy, ComposedEngine, PolicyEngine, PolicyResult, ResourceId, SubjectId};

use crate::{
    AuthorizationReceipt, AuthorizationRequest, GovernanceEngine, action_name, resource_name,
};

pub struct TypeSecGovernanceEngine {
    engine: Arc<dyn PolicyEngine>,
}

impl TypeSecGovernanceEngine {
    pub fn new(engine: Arc<dyn PolicyEngine>) -> Arc<Self> {
        Arc::new(Self { engine })
    }

    pub fn with_fallback(
        primary: Arc<dyn PolicyEngine>,
        fallback: Arc<dyn PolicyEngine>,
    ) -> Arc<Self> {
        Arc::new(Self {
            engine: Arc::new(ComposedEngine::new(
                vec![primary, fallback],
                CombineStrategy::PriorityOrder,
            )),
        })
    }

    pub fn allow_all() -> Arc<Self> {
        Arc::new(Self {
            engine: Arc::new(AllowAllPolicy),
        })
    }

    pub fn rbac_from_yaml(yaml: &str) -> LakeCatResult<Arc<Self>> {
        let engine = typesec::RbacEngine::from_yaml(yaml).map_err(|err| {
            lakecat_core::LakeCatError::InvalidArgument(format!(
                "failed to load TypeSec RBAC policy: {err}"
            ))
        })?;
        Ok(Self::new(Arc::new(engine)))
    }
}

struct AllowAllPolicy;

impl PolicyEngine for AllowAllPolicy {
    fn check(&self, _subject: &SubjectId, _action: &str, _resource: &ResourceId) -> PolicyResult {
        PolicyResult::Allow
    }
}

#[async_trait]
impl GovernanceEngine for TypeSecGovernanceEngine {
    async fn authorize(
        &self,
        request: AuthorizationRequest,
    ) -> LakeCatResult<AuthorizationReceipt> {
        let subject = SubjectId::from(request.principal.subject.clone());
        let action = action_name(&request.action);
        let resource = ResourceId::from(resource_name(&request));
        let decision = self.engine.check(&subject, action, &resource);
        let allowed = matches!(decision, PolicyResult::Allow);
        Ok(AuthorizationReceipt {
            principal: request.principal,
            action: request.action,
            table: request.table,
            allowed,
            engine: "typesec".to_string(),
            policy_hash: Some(content_hash_json(&serde_json::json!({
                "engine": "typesec",
                "subject": subject.as_str(),
                "action": action,
                "resource": resource.as_str(),
                "decision": policy_result_name(&decision),
                "context-hash": content_hash_json(&request.context)?,
            }))?),
            context: request.context,
            checked_at: Utc::now(),
        })
    }
}

fn policy_result_name(result: &PolicyResult) -> &'static str {
    match result {
        PolicyResult::Allow => "allow",
        PolicyResult::Deny(_) => "deny",
        PolicyResult::Delegate(_) => "delegate",
        _ => "unknown",
    }
}

#[cfg(test)]
mod tests;
