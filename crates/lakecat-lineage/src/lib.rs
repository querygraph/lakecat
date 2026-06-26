use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use lakecat_core::{LakeCatResult, Principal, TableIdent, content_hash_json};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[async_trait]
pub trait LineageSink: Send + Sync + 'static {
    async fn emit(&self, event: LineageEvent) -> LakeCatResult<LineageReceipt>;
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LineageEvent {
    pub event_type: LineageEventType,
    pub principal: Principal,
    pub table: Option<TableIdent>,
    pub payload: Value,
    pub emitted_at: DateTime<Utc>,
}

impl LineageEvent {
    pub fn new(
        event_type: LineageEventType,
        principal: Principal,
        table: Option<TableIdent>,
        payload: Value,
    ) -> Self {
        Self {
            event_type,
            principal,
            table,
            payload,
            emitted_at: Utc::now(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum LineageEventType {
    CatalogConfigRead,
    NamespaceCreated,
    NamespaceDropped,
    NamespaceListed,
    NamespaceLoaded,
    PolicyBindingListed,
    PolicyBindingUpserted,
    ProjectListed,
    ProjectUpserted,
    ServerListed,
    ServerUpserted,
    StorageProfileListed,
    StorageProfileUpserted,
    TableCreated,
    TableLoaded,
    TableCommitRecordsListed,
    TableScanPlanned,
    TableCommitted,
    TableDeleted,
    TableRestored,
    ViewDropped,
    ViewLoaded,
    ViewListed,
    ViewVersionReceiptChainsListed,
    ViewVersionReceiptsListed,
    ViewUpserted,
    WarehouseListed,
    WarehouseUpserted,
    CredentialsVendAttempted,
    QueryGraphBootstrap,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LineageReceipt {
    pub event_hash: String,
    pub open_lineage_hash: String,
    pub sink: String,
}

pub fn open_lineage_event(event: &LineageEvent) -> Value {
    let event_type = lineage_event_type_name(&event.event_type);
    let run_id = content_hash_json(&json!({
        "event-type": event_type,
        "principal": event.principal,
        "table": event.table,
        "payload": event.payload,
        "emitted-at": event.emitted_at,
    }))
    .unwrap_or_else(|_| "unhashable-lineage-event".to_string());
    json!({
        "eventType": "COMPLETE",
        "eventTime": event.emitted_at,
        "run": {
            "runId": format!("lakecat-{run_id}"),
            "facets": {
                "lakecat_catalogEvent": {
                    "_producer": "https://querygraph.ai/lakecat",
                    "_schemaURL": "https://querygraph.ai/schemas/openlineage/lakecat-catalog-event-facet/0.1.0.json",
                    "eventType": event_type,
                    "principal": event.principal,
                    "payload": event.payload,
                }
            }
        },
        "job": {
            "namespace": "lakecat.catalog",
            "name": event_type,
        },
        "inputs": lineage_inputs(event),
        "outputs": lineage_outputs(event),
        "producer": "https://querygraph.ai/lakecat",
        "schemaURL": "https://openlineage.io/spec/2-0-2/OpenLineage.json",
    })
}

#[derive(Debug, Default)]
pub struct HashOnlyLineageSink;

impl HashOnlyLineageSink {
    pub fn new() -> Arc<Self> {
        Arc::new(Self)
    }
}

#[async_trait]
impl LineageSink for HashOnlyLineageSink {
    async fn emit(&self, event: LineageEvent) -> LakeCatResult<LineageReceipt> {
        let event_hash = content_hash_json(&serde_json::to_value(&event).map_err(|err| {
            lakecat_core::LakeCatError::Internal(format!("failed to encode lineage event: {err}"))
        })?)?;
        let open_lineage = open_lineage_event(&event);
        let open_lineage_hash = content_hash_json(&open_lineage)?;
        Ok(LineageReceipt {
            event_hash,
            open_lineage_hash,
            sink: "lakecat-openlineage-hash".to_string(),
        })
    }
}

fn lineage_inputs(event: &LineageEvent) -> Vec<Value> {
    match event.event_type {
        LineageEventType::TableLoaded
        | LineageEventType::TableScanPlanned
        | LineageEventType::TableDeleted => event
            .table
            .as_ref()
            .map(|table| vec![open_lineage_dataset(table, &event.payload)])
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

fn lineage_outputs(event: &LineageEvent) -> Vec<Value> {
    match event.event_type {
        LineageEventType::TableCreated
        | LineageEventType::TableCommitted
        | LineageEventType::TableRestored => event
            .table
            .as_ref()
            .map(|table| vec![open_lineage_dataset(table, &event.payload)])
            .unwrap_or_default(),
        LineageEventType::CatalogConfigRead => vec![json!({
            "namespace": "lakecat.catalog-config",
            "name": event
                .payload
                .get("warehouse")
                .and_then(Value::as_str)
                .unwrap_or("unknown"),
            "facets": {
                "lakecat_catalogConfig": {
                    "_producer": "https://querygraph.ai/lakecat",
                    "_schemaURL": "https://querygraph.ai/schemas/openlineage/lakecat-catalog-config-facet/0.1.0.json",
                    "payload": event.payload,
                }
            }
        })],
        LineageEventType::NamespaceCreated
        | LineageEventType::NamespaceDropped
        | LineageEventType::NamespaceLoaded => vec![json!({
            "namespace": "lakecat.namespace",
            "name": namespace_lineage_output_name(event),
            "facets": {
                "lakecat_namespace": {
                    "_producer": "https://querygraph.ai/lakecat",
                    "_schemaURL": "https://querygraph.ai/schemas/openlineage/lakecat-namespace-facet/0.1.0.json",
                    "payload": event.payload,
                }
            }
        })],
        LineageEventType::NamespaceListed => vec![json!({
            "namespace": "lakecat.namespace-list",
            "name": event
                .payload
                .get("warehouse")
                .and_then(Value::as_str)
                .unwrap_or("unknown"),
            "facets": {
                "lakecat_namespace": {
                    "_producer": "https://querygraph.ai/lakecat",
                    "_schemaURL": "https://querygraph.ai/schemas/openlineage/lakecat-namespace-facet/0.1.0.json",
                    "payload": event.payload,
                }
            }
        })],
        LineageEventType::PolicyBindingListed => vec![json!({
            "namespace": "lakecat.policy-list",
            "name": event
                .payload
                .get("warehouse")
                .and_then(Value::as_str)
                .unwrap_or("unknown"),
            "facets": {
                "lakecat_policyBinding": {
                    "_producer": "https://querygraph.ai/lakecat",
                    "_schemaURL": "https://querygraph.ai/schemas/openlineage/lakecat-policy-binding-facet/0.1.0.json",
                    "warehouse": event.payload.get("warehouse"),
                    "payload": event.payload,
                }
            }
        })],
        LineageEventType::PolicyBindingUpserted => vec![json!({
            "namespace": "lakecat.policy",
            "name": event
                .payload
                .pointer("/policy/policy-id")
                .and_then(Value::as_str)
                .unwrap_or("unknown"),
            "facets": {
                "lakecat_policyBinding": {
                    "_producer": "https://querygraph.ai/lakecat",
                    "_schemaURL": "https://querygraph.ai/schemas/openlineage/lakecat-policy-binding-facet/0.1.0.json",
                    "warehouse": event.payload.get("warehouse"),
                    "payload": event.payload,
                }
            }
        })],
        LineageEventType::ProjectListed => vec![json!({
            "namespace": "lakecat.project-list",
            "name": "projects",
            "facets": {
                "lakecat_project": {
                    "_producer": "https://querygraph.ai/lakecat",
                    "_schemaURL": "https://querygraph.ai/schemas/openlineage/lakecat-project-facet/0.1.0.json",
                    "payload": event.payload,
                }
            }
        })],
        LineageEventType::ProjectUpserted => vec![json!({
            "namespace": "lakecat.project",
            "name": event
                .payload
                .get("project-id")
                .and_then(Value::as_str)
                .unwrap_or("unknown"),
            "facets": {
                "lakecat_project": {
                    "_producer": "https://querygraph.ai/lakecat",
                    "_schemaURL": "https://querygraph.ai/schemas/openlineage/lakecat-project-facet/0.1.0.json",
                    "payload": event.payload,
                }
            }
        })],
        LineageEventType::ServerListed => vec![json!({
            "namespace": "lakecat.server-list",
            "name": "servers",
            "facets": {
                "lakecat_server": {
                    "_producer": "https://querygraph.ai/lakecat",
                    "_schemaURL": "https://querygraph.ai/schemas/openlineage/lakecat-server-facet/0.1.0.json",
                    "payload": event.payload,
                }
            }
        })],
        LineageEventType::ServerUpserted => vec![json!({
            "namespace": "lakecat.server",
            "name": event
                .payload
                .get("server-id")
                .and_then(Value::as_str)
                .unwrap_or("unknown"),
            "facets": {
                "lakecat_server": {
                    "_producer": "https://querygraph.ai/lakecat",
                    "_schemaURL": "https://querygraph.ai/schemas/openlineage/lakecat-server-facet/0.1.0.json",
                    "payload": event.payload,
                }
            }
        })],
        LineageEventType::StorageProfileListed => vec![json!({
            "namespace": "lakecat.storage-profile-list",
            "name": event
                .payload
                .get("warehouse")
                .and_then(Value::as_str)
                .unwrap_or("unknown"),
            "facets": {
                "lakecat_storageProfile": {
                    "_producer": "https://querygraph.ai/lakecat",
                    "_schemaURL": "https://querygraph.ai/schemas/openlineage/lakecat-storage-profile-facet/0.1.0.json",
                    "warehouse": event.payload.get("warehouse"),
                    "payload": event.payload,
                }
            }
        })],
        LineageEventType::StorageProfileUpserted => vec![json!({
            "namespace": "lakecat.storage-profile",
            "name": event
                .payload
                .pointer("/storage-profile/profile-id")
                .and_then(Value::as_str)
                .unwrap_or("unknown"),
            "facets": {
                "lakecat_storageProfile": {
                    "_producer": "https://querygraph.ai/lakecat",
                    "_schemaURL": "https://querygraph.ai/schemas/openlineage/lakecat-storage-profile-facet/0.1.0.json",
                    "warehouse": event.payload.get("warehouse"),
                    "payload": event.payload,
                }
            }
        })],
        LineageEventType::ViewDropped
        | LineageEventType::ViewLoaded
        | LineageEventType::ViewListed
        | LineageEventType::ViewVersionReceiptChainsListed
        | LineageEventType::ViewVersionReceiptsListed
        | LineageEventType::ViewUpserted => vec![json!({
            "namespace": if matches!(
                event.event_type,
                LineageEventType::ViewListed
                    | LineageEventType::ViewVersionReceiptChainsListed
                    | LineageEventType::ViewVersionReceiptsListed
            ) {
                "lakecat.view-list"
            } else {
                "lakecat.view"
            },
            "name": view_lineage_output_name(event),
            "facets": {
                "lakecat_view": {
                    "_producer": "https://querygraph.ai/lakecat",
                    "_schemaURL": "https://querygraph.ai/schemas/openlineage/lakecat-view-facet/0.1.0.json",
                    "warehouse": event.payload.get("warehouse"),
                    "namespace": event.payload.get("namespace"),
                    "payload": event.payload,
                }
            }
        })],
        LineageEventType::WarehouseListed => vec![json!({
            "namespace": "lakecat.warehouse-list",
            "name": event
                .payload
                .get("project-id")
                .and_then(Value::as_str)
                .unwrap_or("warehouses"),
            "facets": {
                "lakecat_warehouse": {
                    "_producer": "https://querygraph.ai/lakecat",
                    "_schemaURL": "https://querygraph.ai/schemas/openlineage/lakecat-warehouse-facet/0.1.0.json",
                    "payload": event.payload,
                }
            }
        })],
        LineageEventType::WarehouseUpserted => vec![json!({
            "namespace": "lakecat.warehouse",
            "name": event
                .payload
                .get("warehouse")
                .and_then(Value::as_str)
                .unwrap_or("unknown"),
            "facets": {
                "lakecat_warehouse": {
                    "_producer": "https://querygraph.ai/lakecat",
                    "_schemaURL": "https://querygraph.ai/schemas/openlineage/lakecat-warehouse-facet/0.1.0.json",
                    "payload": event.payload,
                }
            }
        })],
        LineageEventType::QueryGraphBootstrap => vec![json!({
            "namespace": "lakecat.querygraph",
            "name": "bootstrap",
            "facets": {
                "queryGraph_bootstrap": {
                    "_producer": "https://querygraph.ai/lakecat",
                    "_schemaURL": "https://querygraph.ai/schemas/openlineage/querygraph-bootstrap-facet/0.1.0.json",
                    "warehouse": event.payload.get("warehouse"),
                    "tableCount": event.payload.get("table-count"),
                    "policyBindingCount": event.payload.get("policy-binding-count"),
                    "bundleHash": event.payload.get("bundle-hash"),
                    "graphHash": event.payload.get("graph-hash"),
                    "openLineageHash": event.payload.get("open-lineage-hash"),
                    "queryGraphImportHash": event.payload.get("querygraph-import-hash"),
                    "payload": event.payload,
                }
            }
        })],
        _ => Vec::new(),
    }
}

fn open_lineage_dataset(table: &TableIdent, payload: &Value) -> Value {
    let uri = payload
        .get("storage-location")
        .or_else(|| payload.get("location"))
        .or_else(|| payload.get("metadata-location"))
        .and_then(Value::as_str)
        .map(ToString::to_string)
        .unwrap_or_else(|| table.stable_id());
    json!({
        "namespace": format!("lakecat.{}.{}", table.warehouse.as_str(), table.namespace.path()),
        "name": table.name.as_str(),
        "facets": {
            "dataSource": {
                "_producer": "https://querygraph.ai/lakecat",
                "_schemaURL": "https://openlineage.io/spec/facets/1-0-0/DatasourceDatasetFacet.json",
                "name": "LakeCat",
                "uri": uri,
            },
            "lakecat_catalog": {
                "_producer": "https://querygraph.ai/lakecat",
                "_schemaURL": "https://querygraph.ai/schemas/openlineage/lakecat-catalog-facet/0.1.0.json",
                "stableId": table.stable_id(),
                "payload": payload,
            }
        }
    })
}

fn namespace_lineage_output_name(event: &LineageEvent) -> String {
    match event.payload.get("namespace") {
        Some(Value::Array(parts)) => parts
            .iter()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>()
            .join("."),
        Some(Value::String(path)) => path.clone(),
        _ => "unknown".to_string(),
    }
}

fn view_lineage_output_name(event: &LineageEvent) -> String {
    if let Some(name) = event.payload.pointer("/view/name").and_then(Value::as_str) {
        return name.to_string();
    }
    if let Some(name) = event.payload.get("view").and_then(Value::as_str) {
        return name.to_string();
    }
    match event.payload.get("namespace") {
        Some(Value::Array(parts)) => parts
            .iter()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>()
            .join("."),
        Some(Value::String(path)) => path.clone(),
        _ => "unknown".to_string(),
    }
}

fn lineage_event_type_name(event_type: &LineageEventType) -> &'static str {
    match event_type {
        LineageEventType::CatalogConfigRead => "catalog-config-read",
        LineageEventType::NamespaceCreated => "namespace-created",
        LineageEventType::NamespaceDropped => "namespace-dropped",
        LineageEventType::NamespaceListed => "namespace-listed",
        LineageEventType::NamespaceLoaded => "namespace-loaded",
        LineageEventType::PolicyBindingListed => "policy-binding-listed",
        LineageEventType::PolicyBindingUpserted => "policy-binding-upserted",
        LineageEventType::ProjectListed => "project-listed",
        LineageEventType::ProjectUpserted => "project-upserted",
        LineageEventType::ServerListed => "server-listed",
        LineageEventType::ServerUpserted => "server-upserted",
        LineageEventType::StorageProfileListed => "storage-profile-listed",
        LineageEventType::StorageProfileUpserted => "storage-profile-upserted",
        LineageEventType::TableCreated => "table-created",
        LineageEventType::TableLoaded => "table-loaded",
        LineageEventType::TableCommitRecordsListed => "table-commit-records-listed",
        LineageEventType::TableScanPlanned => "table-scan-planned",
        LineageEventType::TableCommitted => "table-committed",
        LineageEventType::TableDeleted => "table-deleted",
        LineageEventType::TableRestored => "table-restored",
        LineageEventType::ViewDropped => "view-dropped",
        LineageEventType::ViewLoaded => "view-loaded",
        LineageEventType::ViewListed => "view-listed",
        LineageEventType::ViewVersionReceiptChainsListed => "view-version-receipt-chains-listed",
        LineageEventType::ViewVersionReceiptsListed => "view-version-receipts-listed",
        LineageEventType::ViewUpserted => "view-upserted",
        LineageEventType::WarehouseListed => "warehouse-listed",
        LineageEventType::WarehouseUpserted => "warehouse-upserted",
        LineageEventType::CredentialsVendAttempted => "credentials-vend-attempted",
        LineageEventType::QueryGraphBootstrap => "querygraph-bootstrap",
    }
}

#[cfg(test)]
mod tests;
