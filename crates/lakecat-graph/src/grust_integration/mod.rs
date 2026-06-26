use std::sync::Arc;

use async_trait::async_trait;
use grust_graph::prelude::*;
use lakecat_core::{LakeCatError, LakeCatResult};

#[cfg(test)]
use crate::GraphNodeLabel;
use crate::{CatalogGraphSink, GraphAction, GraphEvent};

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
        event.validate()?;
        let graph = graph_event_to_grust(&event);
        self.store
            .put_graph(&graph)
            .await
            .map_err(|err| LakeCatError::Internal(format!("Grust graph write failed: {err}")))?;
        Ok(())
    }
}

pub fn graph_event_to_grust(event: &GraphEvent) -> Graph {
    grust_graph::lakecat_catalog_event_graph(&LakeCatCatalogEvent {
        event_id: event.event_id.clone(),
        subject: event.subject.clone(),
        label: event.label.as_str().to_string(),
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
mod tests;
