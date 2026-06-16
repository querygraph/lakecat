use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use lakecat_core::{LakeCatError, LakeCatResult, Principal, TableIdent};
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
    pub table: Option<TableIdent>,
    pub allowed: bool,
    pub engine: String,
    pub policy_hash: Option<String>,
    pub checked_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Capability<Action, Resource> {
    receipt: AuthorizationReceipt,
    resource: Resource,
    _action: std::marker::PhantomData<Action>,
}

impl<Action, Resource> Capability<Action, Resource> {
    pub fn receipt(&self) -> &AuthorizationReceipt {
        &self.receipt
    }

    pub fn resource(&self) -> &Resource {
        &self.resource
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanCreateTable;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanLoadTable;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanCommitTable;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanPlanScan;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanReadGraph;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanReadCatalogConfig;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanCreateNamespace;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanListNamespaces;

pub type TableCreateCapability = Capability<CanCreateTable, TableIdent>;
pub type TableLoadCapability = Capability<CanLoadTable, TableIdent>;
pub type TableCommitCapability = Capability<CanCommitTable, TableIdent>;
pub type TableScanCapability = Capability<CanPlanScan, TableIdent>;
pub type GraphReadCapability = Capability<CanReadGraph, ()>;
pub type CatalogConfigCapability = Capability<CanReadCatalogConfig, ()>;
pub type NamespaceCreateCapability = Capability<CanCreateNamespace, ()>;
pub type NamespaceListCapability = Capability<CanListNamespaces, ()>;

impl TableCreateCapability {
    pub fn from_receipt(receipt: AuthorizationReceipt, table: TableIdent) -> LakeCatResult<Self> {
        table_capability_from_receipt(receipt, table, CatalogAction::TableCreate, "create table")
    }

    pub fn table(&self) -> &TableIdent {
        self.resource()
    }
}

impl TableLoadCapability {
    pub fn from_receipt(receipt: AuthorizationReceipt, table: TableIdent) -> LakeCatResult<Self> {
        table_capability_from_receipt(receipt, table, CatalogAction::TableLoad, "load table")
    }

    pub fn table(&self) -> &TableIdent {
        self.resource()
    }
}

impl TableCommitCapability {
    pub fn from_receipt(receipt: AuthorizationReceipt, table: TableIdent) -> LakeCatResult<Self> {
        table_capability_from_receipt(receipt, table, CatalogAction::TableCommit, "commit table")
    }

    pub fn table(&self) -> &TableIdent {
        self.resource()
    }
}

impl TableScanCapability {
    pub fn from_receipt(receipt: AuthorizationReceipt, table: TableIdent) -> LakeCatResult<Self> {
        table_capability_from_receipt(
            receipt,
            table,
            CatalogAction::TablePlanScan,
            "plan table scans",
        )
    }

    pub fn table(&self) -> &TableIdent {
        self.resource()
    }
}

impl GraphReadCapability {
    pub fn from_receipt(receipt: AuthorizationReceipt) -> LakeCatResult<Self> {
        catalog_capability_from_receipt(receipt, CatalogAction::GraphRead, "read catalog graph")
    }
}

impl CatalogConfigCapability {
    pub fn from_receipt(receipt: AuthorizationReceipt) -> LakeCatResult<Self> {
        catalog_capability_from_receipt(
            receipt,
            CatalogAction::CatalogConfig,
            "read catalog config",
        )
    }
}

impl NamespaceCreateCapability {
    pub fn from_receipt(receipt: AuthorizationReceipt) -> LakeCatResult<Self> {
        catalog_capability_from_receipt(
            receipt,
            CatalogAction::NamespaceCreate,
            "create namespaces",
        )
    }
}

impl NamespaceListCapability {
    pub fn from_receipt(receipt: AuthorizationReceipt) -> LakeCatResult<Self> {
        catalog_capability_from_receipt(receipt, CatalogAction::NamespaceList, "list namespaces")
    }
}

fn catalog_capability_from_receipt<Action>(
    receipt: AuthorizationReceipt,
    expected_action: CatalogAction,
    action_description: &str,
) -> LakeCatResult<Capability<Action, ()>> {
    if !receipt.allowed {
        return Err(LakeCatError::Conflict(
            "authorization receipt is not allowed".to_string(),
        ));
    }
    if receipt.action != expected_action {
        return Err(LakeCatError::InvalidArgument(format!(
            "authorization receipt action {:?} cannot {action_description}",
            receipt.action,
        )));
    }
    if receipt.table.is_some() {
        return Err(LakeCatError::InvalidArgument(
            "catalog authorization receipt must not be table-scoped".to_string(),
        ));
    }
    Ok(Capability {
        receipt,
        resource: (),
        _action: std::marker::PhantomData,
    })
}

fn table_capability_from_receipt<Action>(
    receipt: AuthorizationReceipt,
    table: TableIdent,
    expected_action: CatalogAction,
    action_description: &str,
) -> LakeCatResult<Capability<Action, TableIdent>> {
    if !receipt.allowed {
        return Err(LakeCatError::Conflict(
            "authorization receipt is not allowed".to_string(),
        ));
    }
    if receipt.action != expected_action {
        return Err(LakeCatError::InvalidArgument(format!(
            "authorization receipt action {:?} cannot {action_description}",
            receipt.action,
        )));
    }
    if receipt.table.as_ref() != Some(&table) {
        return Err(LakeCatError::InvalidArgument(
            "authorization receipt table does not match scan table".to_string(),
        ));
    }
    Ok(Capability {
        receipt,
        resource: table,
        _action: std::marker::PhantomData,
    })
}

#[cfg(test)]
mod tests {
    use lakecat_core::{Namespace, PrincipalKind, TableName, WarehouseName};

    use super::*;

    #[test]
    fn table_capabilities_require_matching_allowed_receipts() {
        let table = TableIdent::new(
            WarehouseName::new("local").unwrap(),
            "default".parse::<Namespace>().unwrap(),
            TableName::new("events").unwrap(),
        );
        let receipt = AuthorizationReceipt {
            principal: Principal {
                subject: "agent:reader".to_string(),
                kind: PrincipalKind::Agent,
            },
            action: CatalogAction::TablePlanScan,
            table: Some(table.clone()),
            allowed: true,
            engine: "test".to_string(),
            policy_hash: None,
            checked_at: Utc::now(),
        };

        let capability = TableScanCapability::from_receipt(receipt.clone(), table.clone())
            .expect("matching receipt should mint capability");
        assert_eq!(capability.table(), &table);
        assert_eq!(capability.receipt(), &receipt);

        let other_table = TableIdent::new(
            WarehouseName::new("local").unwrap(),
            "default".parse::<Namespace>().unwrap(),
            TableName::new("other").unwrap(),
        );
        assert!(TableScanCapability::from_receipt(receipt.clone(), other_table).is_err());

        let mut wrong_action_receipt = receipt;
        wrong_action_receipt.action = CatalogAction::TableLoad;
        assert!(TableScanCapability::from_receipt(wrong_action_receipt, table).is_err());

        let load_receipt = AuthorizationReceipt {
            principal: Principal {
                subject: "agent:reader".to_string(),
                kind: PrincipalKind::Agent,
            },
            action: CatalogAction::TableLoad,
            table: Some(capability.table().clone()),
            allowed: true,
            engine: "test".to_string(),
            policy_hash: None,
            checked_at: Utc::now(),
        };
        let load_capability =
            TableLoadCapability::from_receipt(load_receipt, capability.table().clone())
                .expect("matching load receipt should mint capability");
        assert_eq!(load_capability.table(), capability.table());

        let commit_receipt = AuthorizationReceipt {
            principal: Principal {
                subject: "agent:writer".to_string(),
                kind: PrincipalKind::Agent,
            },
            action: CatalogAction::TableCommit,
            table: Some(capability.table().clone()),
            allowed: true,
            engine: "test".to_string(),
            policy_hash: None,
            checked_at: Utc::now(),
        };
        let commit_capability =
            TableCommitCapability::from_receipt(commit_receipt, capability.table().clone())
                .expect("matching commit receipt should mint capability");
        assert_eq!(commit_capability.table(), capability.table());

        let create_receipt = AuthorizationReceipt {
            principal: Principal {
                subject: "agent:writer".to_string(),
                kind: PrincipalKind::Agent,
            },
            action: CatalogAction::TableCreate,
            table: Some(capability.table().clone()),
            allowed: true,
            engine: "test".to_string(),
            policy_hash: None,
            checked_at: Utc::now(),
        };
        let create_capability =
            TableCreateCapability::from_receipt(create_receipt, capability.table().clone())
                .expect("matching create receipt should mint capability");
        assert_eq!(create_capability.table(), capability.table());

        let graph_receipt = AuthorizationReceipt {
            principal: Principal {
                subject: "agent:querygraph".to_string(),
                kind: PrincipalKind::Agent,
            },
            action: CatalogAction::GraphRead,
            table: None,
            allowed: true,
            engine: "test".to_string(),
            policy_hash: None,
            checked_at: Utc::now(),
        };
        let graph_capability = GraphReadCapability::from_receipt(graph_receipt.clone())
            .expect("matching graph-read receipt should mint capability");
        assert_eq!(graph_capability.receipt(), &graph_receipt);

        let mut table_scoped_graph_receipt = graph_receipt;
        table_scoped_graph_receipt.table = Some(capability.table().clone());
        assert!(GraphReadCapability::from_receipt(table_scoped_graph_receipt).is_err());

        let config_receipt = AuthorizationReceipt {
            principal: Principal {
                subject: "agent:catalog".to_string(),
                kind: PrincipalKind::Agent,
            },
            action: CatalogAction::CatalogConfig,
            table: None,
            allowed: true,
            engine: "test".to_string(),
            policy_hash: None,
            checked_at: Utc::now(),
        };
        assert!(CatalogConfigCapability::from_receipt(config_receipt).is_ok());

        let namespace_create_receipt = AuthorizationReceipt {
            principal: Principal {
                subject: "agent:catalog".to_string(),
                kind: PrincipalKind::Agent,
            },
            action: CatalogAction::NamespaceCreate,
            table: None,
            allowed: true,
            engine: "test".to_string(),
            policy_hash: None,
            checked_at: Utc::now(),
        };
        assert!(NamespaceCreateCapability::from_receipt(namespace_create_receipt).is_ok());

        let namespace_list_receipt = AuthorizationReceipt {
            principal: Principal {
                subject: "agent:catalog".to_string(),
                kind: PrincipalKind::Agent,
            },
            action: CatalogAction::NamespaceList,
            table: None,
            allowed: true,
            engine: "test".to_string(),
            policy_hash: None,
            checked_at: Utc::now(),
        };
        assert!(NamespaceListCapability::from_receipt(namespace_list_receipt).is_ok());
    }
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
            table: request.table,
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
                table: request.table,
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
