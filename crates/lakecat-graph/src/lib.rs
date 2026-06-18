use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use lakecat_core::{LakeCatResult, Namespace, TableIdent, WarehouseName};
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

    pub fn with_event_id(mut self, event_id: impl Into<String>) -> Self {
        self.event_id = Some(event_id.into());
        self
    }
}

pub fn namespace_stable_id(warehouse: &WarehouseName, namespace: &Namespace) -> String {
    format!(
        "lakecat:warehouse:{}:namespace:{}",
        warehouse.as_str(),
        namespace.path()
    )
}

pub fn policy_stable_id(warehouse: &WarehouseName, policy_id: &str) -> String {
    format!(
        "lakecat:warehouse:{}:policy:{}",
        warehouse.as_str(),
        policy_id
    )
}

pub fn scan_plan_stable_id(plan_id: &str) -> String {
    format!("lakecat:scan-plan:{plan_id}")
}

pub fn commit_stable_id(table: &TableIdent, sequence_number: u64) -> String {
    format!("lakecat:commit:{}:{sequence_number}", table.stable_id())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum GraphNodeLabel {
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
        use grust_graph::{
            CypherMutationOptions, GraphIndex, GraphStore, MemoryGraphStore, Value,
            execute_cypher_mutation_returning_with_options_on_store,
        };
        use lakecat_core::{Namespace, TableIdent, TableName, WarehouseName};

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
    }
}
