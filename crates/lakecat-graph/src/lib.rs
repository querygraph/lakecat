use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use lakecat_core::{LakeCatResult, Namespace, Principal, TableIdent, WarehouseName};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[async_trait]
pub trait CatalogGraphSink: Send + Sync + 'static {
    async fn emit(&self, event: GraphEvent) -> LakeCatResult<()>;
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GraphEvent {
    pub event_id: Option<String>,
    pub subject: String,
    pub label: GraphNodeLabel,
    pub action: GraphAction,
    pub table: Option<TableIdent>,
    pub properties: Value,
    pub emitted_at: DateTime<Utc>,
}

impl GraphEvent {
    pub fn server(action: GraphAction, server_id: impl Into<String>, properties: Value) -> Self {
        let server_id = server_id.into();
        Self {
            event_id: None,
            subject: server_stable_id(&server_id),
            label: GraphNodeLabel::Server,
            action,
            table: None,
            properties,
            emitted_at: Utc::now(),
        }
    }

    pub fn project(action: GraphAction, project_id: impl Into<String>, properties: Value) -> Self {
        let project_id = project_id.into();
        Self {
            event_id: None,
            subject: project_stable_id(&project_id),
            label: GraphNodeLabel::Project,
            action,
            table: None,
            properties,
            emitted_at: Utc::now(),
        }
    }

    pub fn table(action: GraphAction, table: TableIdent, properties: Value) -> Self {
        Self {
            event_id: None,
            subject: table.stable_id(),
            label: GraphNodeLabel::Table,
            action,
            table: Some(table),
            properties,
            emitted_at: Utc::now(),
        }
    }

    pub fn namespace(
        action: GraphAction,
        warehouse: WarehouseName,
        namespace: Namespace,
        properties: Value,
    ) -> Self {
        Self {
            event_id: None,
            subject: namespace_stable_id(&warehouse, &namespace),
            label: GraphNodeLabel::Namespace,
            action,
            table: None,
            properties,
            emitted_at: Utc::now(),
        }
    }

    pub fn warehouse(action: GraphAction, warehouse: WarehouseName, properties: Value) -> Self {
        Self {
            event_id: None,
            subject: warehouse_stable_id(&warehouse),
            label: GraphNodeLabel::Warehouse,
            action,
            table: None,
            properties,
            emitted_at: Utc::now(),
        }
    }

    pub fn view(
        action: GraphAction,
        warehouse: WarehouseName,
        namespace: Namespace,
        name: impl Into<String>,
        properties: Value,
    ) -> Self {
        let name = name.into();
        Self {
            event_id: None,
            subject: view_stable_id(&warehouse, &namespace, &name),
            label: GraphNodeLabel::View,
            action,
            table: None,
            properties,
            emitted_at: Utc::now(),
        }
    }

    pub fn policy(
        action: GraphAction,
        warehouse: WarehouseName,
        policy_id: impl Into<String>,
        properties: Value,
    ) -> Self {
        let policy_id = policy_id.into();
        Self {
            event_id: None,
            subject: policy_stable_id(&warehouse, &policy_id),
            label: GraphNodeLabel::Policy,
            action,
            table: None,
            properties,
            emitted_at: Utc::now(),
        }
    }

    pub fn storage_profile(
        action: GraphAction,
        warehouse: WarehouseName,
        profile_id: impl Into<String>,
        properties: Value,
    ) -> Self {
        let profile_id = profile_id.into();
        Self {
            event_id: None,
            subject: storage_profile_stable_id(&warehouse, &profile_id),
            label: GraphNodeLabel::StorageProfile,
            action,
            table: None,
            properties,
            emitted_at: Utc::now(),
        }
    }

    pub fn scan_plan(action: GraphAction, plan_id: impl Into<String>, properties: Value) -> Self {
        let plan_id = plan_id.into();
        Self {
            event_id: None,
            subject: scan_plan_stable_id(&plan_id),
            label: GraphNodeLabel::ScanPlan,
            action,
            table: None,
            properties,
            emitted_at: Utc::now(),
        }
    }

    pub fn commit(
        action: GraphAction,
        table: &TableIdent,
        sequence_number: u64,
        properties: Value,
    ) -> Self {
        Self {
            event_id: None,
            subject: commit_stable_id(table, sequence_number),
            label: GraphNodeLabel::Commit,
            action,
            table: Some(table.clone()),
            properties,
            emitted_at: Utc::now(),
        }
    }

    pub fn column(
        action: GraphAction,
        table: &TableIdent,
        column_id: impl Into<String>,
        properties: Value,
    ) -> Self {
        let column_id = column_id.into();
        Self {
            event_id: None,
            subject: column_stable_id(table, &column_id),
            label: GraphNodeLabel::Column,
            action,
            table: Some(table.clone()),
            properties,
            emitted_at: Utc::now(),
        }
    }

    pub fn snapshot(
        action: GraphAction,
        table: &TableIdent,
        snapshot_id: impl Into<String>,
        properties: Value,
    ) -> Self {
        let snapshot_id = snapshot_id.into();
        Self {
            event_id: None,
            subject: snapshot_stable_id(table, &snapshot_id),
            label: GraphNodeLabel::Snapshot,
            action,
            table: Some(table.clone()),
            properties,
            emitted_at: Utc::now(),
        }
    }

    pub fn principal(action: GraphAction, principal: &Principal, properties: Value) -> Self {
        Self {
            event_id: None,
            subject: principal_stable_id(principal),
            label: GraphNodeLabel::Principal,
            action,
            table: None,
            properties,
            emitted_at: Utc::now(),
        }
    }

    pub fn with_event_id(mut self, event_id: impl Into<String>) -> Self {
        self.event_id = Some(event_id.into());
        self
    }
}

pub fn server_stable_id(server_id: &str) -> String {
    format!("lakecat:server:{server_id}")
}

pub fn project_stable_id(project_id: &str) -> String {
    format!("lakecat:project:{project_id}")
}

pub fn warehouse_stable_id(warehouse: &WarehouseName) -> String {
    format!("lakecat:warehouse:{}", warehouse.as_str())
}

pub fn namespace_stable_id(warehouse: &WarehouseName, namespace: &Namespace) -> String {
    format!(
        "lakecat:warehouse:{}:namespace:{}",
        warehouse.as_str(),
        namespace.path()
    )
}

pub fn view_stable_id(warehouse: &WarehouseName, namespace: &Namespace, name: &str) -> String {
    format!(
        "{}:view:{}",
        namespace_stable_id(warehouse, namespace),
        name
    )
}

pub fn policy_stable_id(warehouse: &WarehouseName, policy_id: &str) -> String {
    format!(
        "lakecat:warehouse:{}:policy:{}",
        warehouse.as_str(),
        policy_id
    )
}

pub fn storage_profile_stable_id(warehouse: &WarehouseName, profile_id: &str) -> String {
    format!(
        "lakecat:warehouse:{}:storage-profile:{}",
        warehouse.as_str(),
        profile_id
    )
}

pub fn scan_plan_stable_id(plan_id: &str) -> String {
    format!("lakecat:scan-plan:{plan_id}")
}

pub fn commit_stable_id(table: &TableIdent, sequence_number: u64) -> String {
    format!("lakecat:commit:{}:{sequence_number}", table.stable_id())
}

pub fn column_stable_id(table: &TableIdent, column_id: &str) -> String {
    format!("lakecat:column:{}:{column_id}", table.stable_id())
}

pub fn snapshot_stable_id(table: &TableIdent, snapshot_id: &str) -> String {
    format!("lakecat:snapshot:{}:{snapshot_id}", table.stable_id())
}

pub fn principal_stable_id(principal: &Principal) -> String {
    format!("lakecat:principal:{}", principal.subject)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum GraphNodeLabel {
    Server,
    Project,
    Warehouse,
    Namespace,
    Table,
    View,
    Column,
    Snapshot,
    Manifest,
    DataFile,
    DeleteFile,
    Policy,
    StorageProfile,
    Principal,
    ScanPlan,
    Commit,
    LineageRun,
    QueryGraphModel,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum GraphAction {
    Created,
    Upserted,
    Loaded,
    PlannedScan,
    Committed,
    Deleted,
}

#[derive(Debug, Default)]
pub struct NoopCatalogGraphSink;

impl NoopCatalogGraphSink {
    pub fn new() -> Arc<Self> {
        Arc::new(Self)
    }
}

#[async_trait]
impl CatalogGraphSink for NoopCatalogGraphSink {
    async fn emit(&self, _event: GraphEvent) -> LakeCatResult<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_event_uses_stable_catalog_subject() {
        let event = GraphEvent::server(
            GraphAction::Upserted,
            "prod",
            serde_json::json!({"kind": "test"}),
        );

        assert_eq!(event.label, GraphNodeLabel::Server);
        assert_eq!(event.subject, "lakecat:server:prod");
        assert!(event.table.is_none());
    }

    #[test]
    fn project_event_uses_stable_catalog_subject() {
        let event = GraphEvent::project(
            GraphAction::Upserted,
            "default",
            serde_json::json!({"kind": "test"}),
        );

        assert_eq!(event.label, GraphNodeLabel::Project);
        assert_eq!(event.subject, "lakecat:project:default");
        assert!(event.table.is_none());
    }

    #[test]
    fn namespace_event_uses_stable_catalog_subject() {
        let warehouse = WarehouseName::new("local").unwrap();
        let namespace = "default.ops".parse::<Namespace>().unwrap();
        let event = GraphEvent::namespace(
            GraphAction::Created,
            warehouse,
            namespace,
            serde_json::json!({"kind": "test"}),
        );

        assert_eq!(event.label, GraphNodeLabel::Namespace);
        assert_eq!(
            event.subject,
            "lakecat:warehouse:local:namespace:default.ops"
        );
        assert!(event.table.is_none());
    }

    #[test]
    fn policy_event_uses_stable_catalog_subject() {
        let warehouse = WarehouseName::new("local").unwrap();
        let event = GraphEvent::policy(
            GraphAction::Upserted,
            warehouse,
            "agent-read",
            serde_json::json!({"kind": "test"}),
        );

        assert_eq!(event.label, GraphNodeLabel::Policy);
        assert_eq!(event.subject, "lakecat:warehouse:local:policy:agent-read");
        assert!(event.table.is_none());
    }

    #[test]
    fn storage_profile_event_uses_stable_catalog_subject() {
        let warehouse = WarehouseName::new("local").unwrap();
        let event = GraphEvent::storage_profile(
            GraphAction::Upserted,
            warehouse,
            "s3-events",
            serde_json::json!({"kind": "test"}),
        );

        assert_eq!(event.label, GraphNodeLabel::StorageProfile);
        assert_eq!(
            event.subject,
            "lakecat:warehouse:local:storage-profile:s3-events"
        );
        assert!(event.table.is_none());
    }

    #[test]
    fn warehouse_event_uses_stable_catalog_subject() {
        let event = GraphEvent::warehouse(
            GraphAction::Upserted,
            WarehouseName::new("local").unwrap(),
            serde_json::json!({"kind": "test"}),
        );

        assert_eq!(event.label, GraphNodeLabel::Warehouse);
        assert_eq!(event.subject, "lakecat:warehouse:local");
        assert!(event.table.is_none());
    }

    #[test]
    fn view_event_uses_stable_catalog_subject() {
        let event = GraphEvent::view(
            GraphAction::Upserted,
            WarehouseName::new("local").unwrap(),
            "default.analytics".parse::<Namespace>().unwrap(),
            "events_view",
            serde_json::json!({"kind": "test"}),
        );

        assert_eq!(event.label, GraphNodeLabel::View);
        assert_eq!(
            event.subject,
            "lakecat:warehouse:local:namespace:default.analytics:view:events_view"
        );
        assert!(event.table.is_none());
    }

    #[test]
    fn scan_plan_event_uses_stable_catalog_subject() {
        let event = GraphEvent::scan_plan(
            GraphAction::PlannedScan,
            "evt-scan",
            serde_json::json!({"kind": "test"}),
        );

        assert_eq!(event.label, GraphNodeLabel::ScanPlan);
        assert_eq!(event.subject, "lakecat:scan-plan:evt-scan");
        assert!(event.table.is_none());
    }

    #[test]
    fn commit_event_uses_stable_catalog_subject() {
        let table = TableIdent::new(
            WarehouseName::new("local").unwrap(),
            "default".parse::<Namespace>().unwrap(),
            lakecat_core::TableName::new("events").unwrap(),
        );
        let event = GraphEvent::commit(
            GraphAction::Committed,
            &table,
            7,
            serde_json::json!({"kind": "test"}),
        );

        assert_eq!(event.label, GraphNodeLabel::Commit);
        assert_eq!(
            event.subject,
            "lakecat:commit:lakecat:table:local:default:events:7"
        );
        assert_eq!(event.table.as_ref(), Some(&table));
    }

    #[test]
    fn principal_event_uses_stable_catalog_subject() {
        let principal =
            Principal::new("did:example:agent", lakecat_core::PrincipalKind::Agent).unwrap();
        let event = GraphEvent::principal(
            GraphAction::Loaded,
            &principal,
            serde_json::json!({"kind": "test"}),
        );

        assert_eq!(event.label, GraphNodeLabel::Principal);
        assert_eq!(event.subject, "lakecat:principal:did:example:agent");
        assert!(event.table.is_none());
    }

    #[test]
    fn column_event_uses_stable_catalog_subject() {
        let table = TableIdent::new(
            WarehouseName::new("local").unwrap(),
            "default".parse::<Namespace>().unwrap(),
            lakecat_core::TableName::new("events").unwrap(),
        );
        let event = GraphEvent::column(
            GraphAction::Created,
            &table,
            "1",
            serde_json::json!({"kind": "test"}),
        );

        assert_eq!(event.label, GraphNodeLabel::Column);
        assert_eq!(
            event.subject,
            "lakecat:column:lakecat:table:local:default:events:1"
        );
        assert_eq!(event.table.as_ref(), Some(&table));
    }

    #[test]
    fn snapshot_event_uses_stable_catalog_subject() {
        let table = TableIdent::new(
            WarehouseName::new("local").unwrap(),
            "default".parse::<Namespace>().unwrap(),
            lakecat_core::TableName::new("events").unwrap(),
        );
        let event = GraphEvent::snapshot(
            GraphAction::Created,
            &table,
            "42",
            serde_json::json!({"kind": "test"}),
        );

        assert_eq!(event.label, GraphNodeLabel::Snapshot);
        assert_eq!(
            event.subject,
            "lakecat:snapshot:lakecat:table:local:default:events:42"
        );
        assert_eq!(event.table.as_ref(), Some(&table));
    }
}

#[cfg(feature = "grust-local")]
pub mod grust_integration {
    use std::sync::Arc;

    use async_trait::async_trait;
    use grust_graph::prelude::*;
    use lakecat_core::{LakeCatError, LakeCatResult};

    use crate::{CatalogGraphSink, GraphAction, GraphEvent, GraphNodeLabel};

    pub struct GrustCatalogGraphSink<S>
    where
        S: GraphStore,
    {
        store: Arc<S>,
    }

    impl<S> GrustCatalogGraphSink<S>
    where
        S: GraphStore,
    {
        pub fn new(store: Arc<S>) -> Arc<Self> {
            Arc::new(Self { store })
        }
    }

    #[async_trait]
    impl<S> CatalogGraphSink for GrustCatalogGraphSink<S>
    where
        S: GraphStore + 'static,
    {
        async fn emit(&self, event: GraphEvent) -> LakeCatResult<()> {
            let graph = graph_event_to_grust(&event);
            self.store.put_graph(&graph).await.map_err(|err| {
                LakeCatError::Internal(format!("Grust graph write failed: {err}"))
            })?;
            Ok(())
        }
    }

    pub fn graph_event_to_grust(event: &GraphEvent) -> Graph {
        grust_graph::lakecat_catalog_event_graph(&LakeCatCatalogEvent {
            event_id: event.event_id.clone(),
            subject: event.subject.clone(),
            label: graph_label_name(&event.label).to_string(),
            action: graph_action_name(&event.action).to_string(),
            emitted_at: event.emitted_at.to_rfc3339(),
            properties: event.properties.clone(),
            table: event.table.as_ref().map(|table| LakeCatTableRef {
                stable_id: table.stable_id(),
                warehouse: table.warehouse.as_str().to_string(),
                namespace: table.namespace.parts().to_vec(),
                name: table.name.as_str().to_string(),
            }),
        })
    }

    fn graph_label_name(label: &GraphNodeLabel) -> &'static str {
        match label {
            GraphNodeLabel::Server => "Server",
            GraphNodeLabel::Project => "Project",
            GraphNodeLabel::Warehouse => "Warehouse",
            GraphNodeLabel::Namespace => "Namespace",
            GraphNodeLabel::Table => "Table",
            GraphNodeLabel::View => "View",
            GraphNodeLabel::Column => "Column",
            GraphNodeLabel::Snapshot => "Snapshot",
            GraphNodeLabel::Manifest => "Manifest",
            GraphNodeLabel::DataFile => "DataFile",
            GraphNodeLabel::DeleteFile => "DeleteFile",
            GraphNodeLabel::Policy => "Policy",
            GraphNodeLabel::StorageProfile => "StorageProfile",
            GraphNodeLabel::Principal => "Principal",
            GraphNodeLabel::ScanPlan => "ScanPlan",
            GraphNodeLabel::Commit => "Commit",
            GraphNodeLabel::LineageRun => "LineageRun",
            GraphNodeLabel::QueryGraphModel => "QueryGraphModel",
        }
    }

    fn graph_action_name(action: &GraphAction) -> &'static str {
        match action {
            GraphAction::Created => "created",
            GraphAction::Upserted => "upserted",
            GraphAction::Loaded => "loaded",
            GraphAction::PlannedScan => "planned-scan",
            GraphAction::Committed => "committed",
            GraphAction::Deleted => "deleted",
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        #[cfg(feature = "grust-turso-local")]
        use grust_graph::{
            CypherMutationExecutor, GraphAdminStore, GraphMutationCardinality, GraphMutationPlan,
            GraphMutationPlanOp, Label, NodeId, Props, Traversal,
        };
        use grust_graph::{
            CypherMutationOptions, GraphIndex, GraphStore, MemoryGraphStore, Value,
            execute_cypher_mutation_returning_with_options_on_store,
        };
        use lakecat_core::{Namespace, TableIdent, TableName, WarehouseName};
        #[cfg(feature = "grust-turso-local")]
        use std::sync::Arc;

        #[test]
        fn converts_server_event_to_valid_grust_graph_event() {
            let event = GraphEvent::server(
                GraphAction::Upserted,
                "prod",
                serde_json::json!({"server-id":"prod"}),
            )
            .with_event_id("lakecat:outbox:server-1");
            let graph = graph_event_to_grust(&event);

            assert_eq!(graph.nodes.len(), 1);
            assert_eq!(graph.edges.len(), 0);
            assert_eq!(graph.nodes[0].label.as_str(), "CatalogEvent");
            assert_eq!(
                graph.nodes[0].props.get("label"),
                Some(&Value::String("Server".to_string()))
            );
            assert_eq!(
                graph.nodes[0].props.get("subject"),
                Some(&Value::String("lakecat:server:prod".to_string()))
            );
            GraphIndex::new(&graph).expect("server event graph should be valid");
        }

        #[test]
        fn converts_table_event_to_valid_grust_graph() {
            let table = TableIdent::new(
                WarehouseName::new("local").unwrap(),
                "default".parse::<Namespace>().unwrap(),
                TableName::new("events").unwrap(),
            );
            let event = GraphEvent::table(
                GraphAction::Created,
                table,
                serde_json::json!({"kind":"test"}),
            )
            .with_event_id("lakecat:outbox:evt-1");
            let graph = graph_event_to_grust(&event);
            assert_eq!(graph.nodes.len(), 4);
            assert_eq!(graph.edges.len(), 3);
            assert!(
                graph
                    .edges
                    .iter()
                    .any(|edge| edge.label.as_str() == "AFFECTS_TABLE")
            );
            GraphIndex::new(&graph).expect("event graph should be valid");
        }

        #[test]
        fn converts_policy_event_to_valid_grust_graph_event() {
            let event = GraphEvent::policy(
                GraphAction::Upserted,
                WarehouseName::new("local").unwrap(),
                "agent-read",
                serde_json::json!({"kind":"test"}),
            )
            .with_event_id("lakecat:outbox:policy-1");
            let graph = graph_event_to_grust(&event);

            assert_eq!(graph.nodes.len(), 1);
            assert_eq!(graph.edges.len(), 0);
            assert_eq!(graph.nodes[0].label.as_str(), "CatalogEvent");
            assert_eq!(
                graph.nodes[0].props.get("label"),
                Some(&Value::String("Policy".to_string()))
            );
            assert_eq!(
                graph.nodes[0].props.get("action"),
                Some(&Value::String("upserted".to_string()))
            );
            GraphIndex::new(&graph).expect("policy event graph should be valid");
        }

        #[test]
        fn converts_storage_profile_event_to_valid_grust_graph_event() {
            let event = GraphEvent::storage_profile(
                GraphAction::Upserted,
                WarehouseName::new("local").unwrap(),
                "s3-events",
                serde_json::json!({
                    "storage-profile": {
                        "profile-id": "s3-events",
                        "provider": "s3",
                        "secret-ref-present": true,
                        "secret-ref-provider": "vault"
                    }
                }),
            )
            .with_event_id("lakecat:outbox:storage-profile-1");
            let graph = graph_event_to_grust(&event);

            assert_eq!(graph.nodes.len(), 1);
            assert_eq!(graph.edges.len(), 0);
            assert_eq!(graph.nodes[0].label.as_str(), "CatalogEvent");
            assert_eq!(
                graph.nodes[0].props.get("label"),
                Some(&Value::String("StorageProfile".to_string()))
            );
            assert_eq!(
                graph.nodes[0].props.get("subject"),
                Some(&Value::String(
                    "lakecat:warehouse:local:storage-profile:s3-events".to_string()
                ))
            );
            GraphIndex::new(&graph).expect("storage profile event graph should be valid");
        }

        #[test]
        fn converts_warehouse_event_to_valid_grust_graph_event() {
            let event = GraphEvent::warehouse(
                GraphAction::Upserted,
                WarehouseName::new("local").unwrap(),
                serde_json::json!({"warehouse":"local"}),
            )
            .with_event_id("lakecat:outbox:warehouse");
            let graph = graph_event_to_grust(&event);

            assert_eq!(graph.nodes.len(), 1);
            assert_eq!(graph.edges.len(), 0);
            assert_eq!(graph.nodes[0].label.as_str(), "CatalogEvent");
            assert_eq!(
                graph.nodes[0].props.get("label"),
                Some(&Value::String("Warehouse".to_string()))
            );
            GraphIndex::new(&graph).expect("warehouse event graph should be valid");
        }

        #[test]
        fn converts_project_event_to_valid_grust_graph_event() {
            let event = GraphEvent::project(
                GraphAction::Upserted,
                "default",
                serde_json::json!({"project-id":"default"}),
            )
            .with_event_id("lakecat:outbox:project");
            let graph = graph_event_to_grust(&event);

            assert_eq!(graph.nodes.len(), 1);
            assert_eq!(graph.edges.len(), 0);
            assert_eq!(graph.nodes[0].label.as_str(), "CatalogEvent");
            assert_eq!(
                graph.nodes[0].props.get("label"),
                Some(&Value::String("Project".to_string()))
            );
            GraphIndex::new(&graph).expect("project event graph should be valid");
        }

        #[test]
        fn converts_view_event_to_valid_grust_graph_event() {
            let event = GraphEvent::view(
                GraphAction::Upserted,
                WarehouseName::new("local").unwrap(),
                "default".parse::<Namespace>().unwrap(),
                "events_view",
                serde_json::json!({"view":{"name":"events_view"}}),
            )
            .with_event_id("lakecat:outbox:view");
            let graph = graph_event_to_grust(&event);

            assert_eq!(graph.nodes.len(), 1);
            assert_eq!(graph.edges.len(), 0);
            assert_eq!(graph.nodes[0].label.as_str(), "CatalogEvent");
            assert_eq!(
                graph.nodes[0].props.get("label"),
                Some(&Value::String("View".to_string()))
            );
            GraphIndex::new(&graph).expect("view event graph should be valid");
        }

        #[test]
        fn converts_scan_plan_event_to_valid_grust_graph_event() {
            let event = GraphEvent::scan_plan(
                GraphAction::PlannedScan,
                "evt-scan",
                serde_json::json!({"kind":"test"}),
            )
            .with_event_id("lakecat:outbox:scan-1:scan-plan");
            let graph = graph_event_to_grust(&event);

            assert_eq!(graph.nodes.len(), 1);
            assert_eq!(graph.edges.len(), 0);
            assert_eq!(graph.nodes[0].label.as_str(), "CatalogEvent");
            assert_eq!(
                graph.nodes[0].props.get("label"),
                Some(&Value::String("ScanPlan".to_string()))
            );
            assert_eq!(
                graph.nodes[0].props.get("action"),
                Some(&Value::String("planned-scan".to_string()))
            );
            GraphIndex::new(&graph).expect("scan plan event graph should be valid");
        }

        #[test]
        fn converts_commit_event_to_valid_grust_graph_event() {
            let table = TableIdent::new(
                WarehouseName::new("local").unwrap(),
                "default".parse::<Namespace>().unwrap(),
                TableName::new("events").unwrap(),
            );
            let event = GraphEvent::commit(
                GraphAction::Committed,
                &table,
                7,
                serde_json::json!({"kind":"test"}),
            )
            .with_event_id("lakecat:outbox:commit-1:commit");
            let graph = graph_event_to_grust(&event);

            assert_eq!(graph.nodes.len(), 4);
            assert_eq!(graph.edges.len(), 3);
            assert!(
                graph
                    .edges
                    .iter()
                    .any(|edge| edge.label.as_str() == "AFFECTS_TABLE")
            );
            assert_eq!(
                graph.nodes[0].props.get("label"),
                Some(&Value::String("Commit".to_string()))
            );
            assert_eq!(
                graph.nodes[0].props.get("action"),
                Some(&Value::String("committed".to_string()))
            );
            GraphIndex::new(&graph).expect("commit event graph should be valid");
        }

        #[test]
        fn converts_principal_event_to_valid_grust_graph_event() {
            let principal = lakecat_core::Principal::new(
                "did:example:agent",
                lakecat_core::PrincipalKind::Agent,
            )
            .unwrap();
            let event = GraphEvent::principal(
                GraphAction::Loaded,
                &principal,
                serde_json::json!({"kind":"test"}),
            )
            .with_event_id("lakecat:outbox:evt-1:principal");
            let graph = graph_event_to_grust(&event);

            assert_eq!(graph.nodes.len(), 1);
            assert_eq!(graph.edges.len(), 0);
            assert_eq!(graph.nodes[0].label.as_str(), "CatalogEvent");
            assert_eq!(
                graph.nodes[0].props.get("label"),
                Some(&Value::String("Principal".to_string()))
            );
            GraphIndex::new(&graph).expect("principal event graph should be valid");
        }

        #[test]
        fn converts_column_event_to_valid_grust_graph_event() {
            let table = TableIdent::new(
                WarehouseName::new("local").unwrap(),
                "default".parse::<Namespace>().unwrap(),
                TableName::new("events").unwrap(),
            );
            let event = GraphEvent::column(
                GraphAction::Created,
                &table,
                "1",
                serde_json::json!({"field":{"id":1,"name":"event_id"}}),
            )
            .with_event_id("lakecat:outbox:evt-1:column:1");
            let graph = graph_event_to_grust(&event);

            assert!(graph.nodes.len() >= 2);
            assert!(!graph.edges.is_empty());
            assert_eq!(graph.nodes[0].label.as_str(), "CatalogEvent");
            assert_eq!(
                graph.nodes[0].props.get("label"),
                Some(&Value::String("Column".to_string()))
            );
            GraphIndex::new(&graph).expect("column event graph should be valid");
        }

        #[test]
        fn converts_snapshot_event_to_valid_grust_graph_event() {
            let table = TableIdent::new(
                WarehouseName::new("local").unwrap(),
                "default".parse::<Namespace>().unwrap(),
                TableName::new("events").unwrap(),
            );
            let event = GraphEvent::snapshot(
                GraphAction::Created,
                &table,
                "42",
                serde_json::json!({"snapshot":{"snapshot-id":42}}),
            )
            .with_event_id("lakecat:outbox:evt-1:snapshot:42");
            let graph = graph_event_to_grust(&event);

            assert!(graph.nodes.len() >= 2);
            assert!(!graph.edges.is_empty());
            assert_eq!(graph.nodes[0].label.as_str(), "CatalogEvent");
            assert_eq!(
                graph.nodes[0].props.get("label"),
                Some(&Value::String("Snapshot".to_string()))
            );
            GraphIndex::new(&graph).expect("snapshot event graph should be valid");
        }

        #[tokio::test]
        async fn grust_cypher_can_query_lakecat_catalog_projection_boundary() {
            let table = TableIdent::new(
                WarehouseName::new("local").unwrap(),
                "default".parse::<Namespace>().unwrap(),
                TableName::new("events").unwrap(),
            );
            let table_id = table.stable_id();
            let event = GraphEvent::table(
                GraphAction::Created,
                table,
                serde_json::json!({"kind":"test"}),
            )
            .with_event_id("lakecat:outbox:evt-1");
            let graph = graph_event_to_grust(&event);
            let store = MemoryGraphStore::new();
            store.put_graph(&graph).await.expect("catalog graph write");

            let result = execute_cypher_mutation_returning_with_options_on_store(
                &store,
                &format!(
                    "MATCH (t:Table {{id: '{table_id}'}}) SET t.querygraph_ready = true RETURN t.id AS id, t.querygraph_ready AS ready"
                ),
                CypherMutationOptions::default(),
            )
            .await
            .expect("Grust Cypher mutation over LakeCat graph");

            assert_eq!(result.table.columns, vec!["id", "ready"]);
            assert_eq!(
                result.table.rows,
                vec![vec![Value::String(table_id), Value::Bool(true)]]
            );
        }

        #[cfg(feature = "grust-turso-local")]
        #[tokio::test]
        async fn grust_turso_store_persists_lakecat_catalog_projection_boundary() {
            let table = TableIdent::new(
                WarehouseName::new("local").unwrap(),
                "default".parse::<Namespace>().unwrap(),
                TableName::new("events").unwrap(),
            );
            let table_id = table.stable_id();
            let event = GraphEvent::table(
                GraphAction::Created,
                table,
                serde_json::json!({"kind":"turso-test"}),
            )
            .with_event_id("lakecat:outbox:evt-turso");
            let graph = graph_event_to_grust(&event);
            let store = grust_graph::TursoGraphStore::in_memory()
                .await
                .expect("Grust Turso graph store");
            store.bootstrap().await.expect("Grust Turso bootstrap");

            store
                .put_graph(&graph)
                .await
                .expect("catalog graph write to Turso");
            let table_node = store
                .get_node(&NodeId::new(table_id.clone()))
                .await
                .expect("catalog table node read from Turso")
                .expect("table node persisted in Turso");

            assert_eq!(table_node.id.as_str(), table_id);
            assert_eq!(table_node.label.as_str(), "Table");
            assert_eq!(
                table_node.props.get("warehouse"),
                Some(&Value::String("local".to_string()))
            );
        }

        #[cfg(feature = "grust-turso-local")]
        #[tokio::test]
        async fn grust_turso_sink_emits_lakecat_catalog_projection_boundary() {
            let table = TableIdent::new(
                WarehouseName::new("local").unwrap(),
                "default".parse::<Namespace>().unwrap(),
                TableName::new("events").unwrap(),
            );
            let table_id = table.stable_id();
            let event = GraphEvent::table(
                GraphAction::Created,
                table,
                serde_json::json!({"kind":"turso-sink-test"}),
            )
            .with_event_id("lakecat:outbox:evt-turso-sink");
            let store = Arc::new(
                grust_graph::TursoGraphStore::in_memory()
                    .await
                    .expect("Grust Turso graph store"),
            );
            store.bootstrap().await.expect("Grust Turso bootstrap");
            let sink = GrustCatalogGraphSink::new(store.clone());

            crate::CatalogGraphSink::emit(sink.as_ref(), event)
                .await
                .expect("LakeCat graph sink should emit through Grust Turso");
            let table_node = store
                .get_node(&NodeId::new(table_id.clone()))
                .await
                .expect("catalog table node read from Turso")
                .expect("table node persisted by sink in Turso");

            assert_eq!(table_node.id.as_str(), table_id);
            assert_eq!(table_node.label.as_str(), "Table");
            assert_eq!(
                table_node.props.get("warehouse"),
                Some(&Value::String("local".to_string()))
            );
        }

        #[cfg(feature = "grust-turso-local")]
        #[tokio::test]
        async fn grust_turso_store_traverses_lakecat_catalog_projection_boundary() {
            let table = TableIdent::new(
                WarehouseName::new("local").unwrap(),
                "default".parse::<Namespace>().unwrap(),
                TableName::new("events").unwrap(),
            );
            let table_id = table.stable_id();
            let event = GraphEvent::table(
                GraphAction::Created,
                table,
                serde_json::json!({"kind":"turso-cypher-test"}),
            )
            .with_event_id("lakecat:outbox:evt-turso-cypher");
            let graph = graph_event_to_grust(&event);
            let store = grust_graph::TursoGraphStore::in_memory()
                .await
                .expect("Grust Turso graph store");
            store.bootstrap().await.expect("Grust Turso bootstrap");
            store
                .put_graph(&graph)
                .await
                .expect("catalog graph write to Turso");

            let affected_tables = store
                .traverse(
                    Traversal::from_node("lakecat:outbox:evt-turso-cypher")
                        .out("AFFECTS_TABLE")
                        .to("Table"),
                )
                .await
                .expect("Grust Turso traversal over LakeCat graph");

            assert_eq!(affected_tables.len(), 1);
            assert_eq!(affected_tables[0].id.as_str(), table_id);
            assert_eq!(affected_tables[0].label.as_str(), "Table");
            assert_eq!(
                affected_tables[0].props.get("warehouse"),
                Some(&Value::String("local".to_string()))
            );
        }

        #[cfg(feature = "grust-turso-local")]
        #[tokio::test]
        async fn grust_turso_store_runs_cypher_over_lakecat_catalog_projection_boundary() {
            let table = TableIdent::new(
                WarehouseName::new("local").unwrap(),
                "default".parse::<Namespace>().unwrap(),
                TableName::new("events").unwrap(),
            );
            let table_id = table.stable_id();
            let event = GraphEvent::table(
                GraphAction::Created,
                table,
                serde_json::json!({"kind":"turso-cypher-test"}),
            )
            .with_event_id("lakecat:outbox:evt-turso-cypher-query");
            let graph = graph_event_to_grust(&event);
            let store = grust_graph::TursoGraphStore::in_memory()
                .await
                .expect("Grust Turso graph store");
            store.bootstrap().await.expect("Grust Turso bootstrap");
            store
                .put_graph(&graph)
                .await
                .expect("catalog graph write to Turso");

            let result = execute_cypher_mutation_returning_with_options_on_store(
                &store,
                &format!(
                    "MATCH (t:Table {{id: '{table_id}'}}) SET t.querygraph_ready = true RETURN t.id AS id, t.querygraph_ready AS ready"
                ),
                CypherMutationOptions::default(),
            )
            .await
            .expect("Grust Cypher mutation over Turso-backed LakeCat graph");

            assert_eq!(result.table.columns, vec!["id", "ready"]);
            assert_eq!(
                result.table.rows,
                vec![vec![Value::String(table_id), Value::Bool(true)]]
            );
        }

        #[cfg(feature = "grust-turso-local")]
        #[tokio::test]
        async fn grust_turso_store_patches_lakecat_catalog_projection_nodes() {
            let table = TableIdent::new(
                WarehouseName::new("local").unwrap(),
                "default".parse::<Namespace>().unwrap(),
                TableName::new("events").unwrap(),
            );
            let table_id = table.stable_id();
            let event = GraphEvent::table(
                GraphAction::Created,
                table,
                serde_json::json!({"kind":"turso-matched-node-test"}),
            )
            .with_event_id("lakecat:outbox:evt-turso-matched-node");
            let graph = graph_event_to_grust(&event);
            let store = grust_graph::TursoGraphStore::in_memory()
                .await
                .expect("Grust Turso graph store");
            store.bootstrap().await.expect("Grust Turso bootstrap");
            store
                .put_graph(&graph)
                .await
                .expect("catalog graph write to Turso");

            let report = store
                .execute_cypher_mutation_plan(&GraphMutationPlan::new(vec![
                    GraphMutationPlanOp::PatchMatchingNodes {
                        label: Some(Label::new("Table")),
                        props: Props::from([("id".to_string(), Value::from(table_id.as_str()))]),
                        predicates: Vec::new(),
                        patch: Props::from([("querygraph_ready".to_string(), Value::from(true))]),
                        cardinality: GraphMutationCardinality::SingleIdentity,
                    },
                ]))
                .await
                .expect("Grust Turso matched-node patch over LakeCat graph");

            assert_eq!(report.matched_rows, 1);
            assert_eq!(report.node_patches, 1);
            assert_eq!(report.changed_nodes, 1);
            let table_node = store
                .get_node(&NodeId::new(table_id.clone()))
                .await
                .expect("catalog table node read from Turso")
                .expect("table node patched in Turso");
            assert_eq!(
                table_node.props.get("querygraph_ready"),
                Some(&Value::Bool(true))
            );
        }

        #[tokio::test]
        async fn grust_cypher_can_query_catalog_event_taxonomy_labels() {
            let table = TableIdent::new(
                WarehouseName::new("local").unwrap(),
                "default".parse::<Namespace>().unwrap(),
                TableName::new("events").unwrap(),
            );
            let principal = lakecat_core::Principal::new(
                "did:example:agent",
                lakecat_core::PrincipalKind::Agent,
            )
            .unwrap();
            let events = vec![
                GraphEvent::column(
                    GraphAction::Created,
                    &table,
                    "1",
                    serde_json::json!({"field":{"id":1,"name":"event_id"}}),
                )
                .with_event_id("lakecat:outbox:evt-1:column:1"),
                GraphEvent::snapshot(
                    GraphAction::Created,
                    &table,
                    "42",
                    serde_json::json!({"snapshot":{"snapshot-id":42}}),
                )
                .with_event_id("lakecat:outbox:evt-1:snapshot:42"),
                GraphEvent::commit(
                    GraphAction::Committed,
                    &table,
                    7,
                    serde_json::json!({"sequence-number":7}),
                )
                .with_event_id("lakecat:outbox:evt-1:commit"),
                GraphEvent::principal(
                    GraphAction::Loaded,
                    &principal,
                    serde_json::json!({"principal-kind":"agent"}),
                )
                .with_event_id("lakecat:outbox:evt-1:principal"),
                GraphEvent::scan_plan(
                    GraphAction::PlannedScan,
                    "evt-scan",
                    serde_json::json!({"read-restriction":{"allowed-columns":["event_id"]}}),
                )
                .with_event_id("lakecat:outbox:evt-1:scan-plan"),
            ];
            let store = MemoryGraphStore::new();
            for event in events {
                let graph = graph_event_to_grust(&event);
                store.put_graph(&graph).await.expect("catalog graph write");
            }

            let result = execute_cypher_mutation_returning_with_options_on_store(
                &store,
                "MATCH (e:CatalogEvent {label: 'Column'}) SET e.querygraph_seen = true RETURN e.subject AS subject, e.action AS action, e.querygraph_seen AS seen",
                CypherMutationOptions::default(),
            )
            .await
            .expect("Grust Cypher mutation over LakeCat column event");

            assert_eq!(result.table.columns, vec!["subject", "action", "seen"]);
            assert_eq!(
                result.table.rows,
                vec![vec![
                    Value::String(
                        "lakecat:column:lakecat:table:local:default:events:1".to_string(),
                    ),
                    Value::String("created".to_string()),
                    Value::Bool(true),
                ]]
            );

            let result = execute_cypher_mutation_returning_with_options_on_store(
                &store,
                "MATCH (e:CatalogEvent {label: 'Snapshot'}) SET e.querygraph_seen = true RETURN e.subject AS subject, e.action AS action, e.querygraph_seen AS seen",
                CypherMutationOptions::default(),
            )
            .await
            .expect("Grust Cypher query over LakeCat snapshot event");

            assert_eq!(result.table.columns, vec!["subject", "action", "seen"]);
            assert_eq!(
                result.table.rows,
                vec![vec![
                    Value::String(
                        "lakecat:snapshot:lakecat:table:local:default:events:42".to_string(),
                    ),
                    Value::String("created".to_string()),
                    Value::Bool(true),
                ]]
            );
        }
    }
}
