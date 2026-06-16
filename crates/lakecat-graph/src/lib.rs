use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use lakecat_core::{LakeCatResult, TableIdent};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[async_trait]
pub trait CatalogGraphSink: Send + Sync + 'static {
    async fn emit(&self, event: GraphEvent) -> LakeCatResult<()>;
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GraphEvent {
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
            subject: table.stable_id(),
            label: GraphNodeLabel::Table,
            action,
            table: Some(table),
            properties,
            emitted_at: Utc::now(),
        }
    }
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
        let mut builder = Graph::builder();
        let event_id = format!("lakecat:event:{}", event.subject);
        let _ = builder
            .node("CatalogEvent", event_id.clone())
            .prop("subject", event.subject.clone())
            .prop("label", graph_label_name(&event.label))
            .prop("action", graph_action_name(&event.action))
            .prop("emitted_at", event.emitted_at.to_rfc3339())
            .prop("properties", event.properties.clone())
            .finish();

        if let Some(table) = &event.table {
            let table_id = table.stable_id();
            let _ = builder
                .node("Table", table_id.clone())
                .prop("warehouse", table.warehouse.as_str())
                .prop("namespace", table.namespace.path())
                .prop("name", table.name.as_str())
                .finish();
            let _ = builder
                .edge("EMITTED", event_id, table_id)
                .prop("action", graph_action_name(&event.action))
                .finish();
        }

        builder.build()
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
            GraphAction::Loaded => "loaded",
            GraphAction::PlannedScan => "planned-scan",
            GraphAction::Committed => "committed",
            GraphAction::Deleted => "deleted",
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use grust_graph::GraphIndex;
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
            );
            let graph = graph_event_to_grust(&event);
            assert_eq!(graph.nodes.len(), 2);
            assert_eq!(graph.edges.len(), 1);
            GraphIndex::new(&graph).expect("event graph should be valid");
        }
    }
}
