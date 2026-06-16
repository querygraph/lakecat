use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use lakecat_core::{LakeCatResult, Principal, TableIdent};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[async_trait]
pub trait GovernanceEngine: Send + Sync + 'static {
    async fn authorize(&self, request: AuthorizationRequest)
    -> LakeCatResult<AuthorizationReceipt>;
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AuthorizationRequest {
    pub principal: Principal,
    pub action: CatalogAction,
    pub table: Option<TableIdent>,
    pub context: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum CatalogAction {
    CatalogConfig,
    NamespaceCreate,
    NamespaceList,
    TableCreate,
    TableRegister,
    TableLoad,
    TablePlanScan,
    TableCommit,
    TableDrop,
    CredentialsVend,
    GraphRead,
    LineageRead,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AuthorizationReceipt {
    pub principal: Principal,
    pub action: CatalogAction,
    pub allowed: bool,
    pub engine: String,
    pub policy_hash: Option<String>,
    pub checked_at: DateTime<Utc>,
}

#[derive(Debug, Default)]
pub struct AllowAllGovernanceEngine;

impl AllowAllGovernanceEngine {
    pub fn new() -> Arc<Self> {
        Arc::new(Self)
    }
}

#[async_trait]
impl GovernanceEngine for AllowAllGovernanceEngine {
    async fn authorize(
        &self,
        request: AuthorizationRequest,
    ) -> LakeCatResult<AuthorizationReceipt> {
        Ok(AuthorizationReceipt {
            principal: request.principal,
            action: request.action,
            allowed: true,
            engine: "lakecat-allow-all-typesec-placeholder".to_string(),
            policy_hash: None,
            checked_at: Utc::now(),
        })
    }
}

#[cfg(feature = "typesec-local")]
pub mod typesec_integration {
    use std::sync::Arc;

    use async_trait::async_trait;
    use chrono::Utc;
    use lakecat_core::{LakeCatResult, content_hash_json};
    use typesec::{PolicyEngine, PolicyResult, ResourceId, SubjectId};

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

        pub fn allow_all() -> Arc<Self> {
            Arc::new(Self {
                engine: Arc::new(AllowAllPolicy),
            })
        }
    }

    struct AllowAllPolicy;

    impl PolicyEngine for AllowAllPolicy {
        fn check(
            &self,
            _subject: &SubjectId,
            _action: &str,
            _resource: &ResourceId,
        ) -> PolicyResult {
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
                allowed,
                engine: "typesec".to_string(),
                policy_hash: Some(content_hash_json(&serde_json::json!({
                    "engine": "typesec",
                    "subject": subject.as_str(),
                    "action": action,
                    "resource": resource.as_str(),
                    "decision": policy_result_name(&decision),
                }))?),
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
    mod tests {
        use std::sync::Arc;

        use lakecat_core::{Principal, PrincipalKind};
        use typesec::{PolicyEngine, PolicyResult, ResourceId, SubjectId};

        use super::*;
        use crate::{AuthorizationRequest, CatalogAction};

        struct AllowRead;

        impl PolicyEngine for AllowRead {
            fn check(
                &self,
                _subject: &SubjectId,
                action: &str,
                _resource: &ResourceId,
            ) -> PolicyResult {
                if action == "table.load" {
                    PolicyResult::Allow
                } else {
                    PolicyResult::Deny("not granted".to_string())
                }
            }
        }

        #[tokio::test]
        async fn delegates_authorization_to_typesec_policy_engine() {
            let engine = TypeSecGovernanceEngine::new(Arc::new(AllowRead));
            let receipt = engine
                .authorize(AuthorizationRequest {
                    principal: Principal {
                        subject: "agent:reader".to_string(),
                        kind: PrincipalKind::Agent,
                    },
                    action: CatalogAction::TableLoad,
                    table: None,
                    context: serde_json::json!({}),
                })
                .await
                .expect("authorization should run");
            assert!(receipt.allowed);
            assert_eq!(receipt.engine, "typesec");
            assert!(receipt.policy_hash.is_some());
        }
    }
}

pub fn action_name(action: &CatalogAction) -> &'static str {
    match action {
        CatalogAction::CatalogConfig => "catalog.config",
        CatalogAction::NamespaceCreate => "namespace.create",
        CatalogAction::NamespaceList => "namespace.list",
        CatalogAction::TableCreate => "table.create",
        CatalogAction::TableRegister => "table.register",
        CatalogAction::TableLoad => "table.load",
        CatalogAction::TablePlanScan => "table.plan_scan",
        CatalogAction::TableCommit => "table.commit",
        CatalogAction::TableDrop => "table.drop",
        CatalogAction::CredentialsVend => "credentials.vend",
        CatalogAction::GraphRead => "graph.read",
        CatalogAction::LineageRead => "lineage.read",
    }
}

pub fn resource_name(request: &AuthorizationRequest) -> String {
    request
        .table
        .as_ref()
        .map(TableIdent::stable_id)
        .unwrap_or_else(|| "lakecat:catalog".to_string())
}
