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
    NamespaceCreated,
    NamespaceDropped,
    TableCreated,
    TableLoaded,
    TableScanPlanned,
    TableCommitted,
    TableDeleted,
    TableRestored,
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
        LineageEventType::NamespaceCreated | LineageEventType::NamespaceDropped => vec![json!({
            "namespace": "lakecat.namespace",
            "name": event
                .payload
                .get("namespace")
                .map(Value::to_string)
                .unwrap_or_else(|| "unknown".to_string()),
            "facets": {
                "lakecat_namespace": {
                    "_producer": "https://querygraph.ai/lakecat",
                    "_schemaURL": "https://querygraph.ai/schemas/openlineage/lakecat-namespace-facet/0.1.0.json",
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

fn lineage_event_type_name(event_type: &LineageEventType) -> &'static str {
    match event_type {
        LineageEventType::NamespaceCreated => "namespace-created",
        LineageEventType::NamespaceDropped => "namespace-dropped",
        LineageEventType::TableCreated => "table-created",
        LineageEventType::TableLoaded => "table-loaded",
        LineageEventType::TableScanPlanned => "table-scan-planned",
        LineageEventType::TableCommitted => "table-committed",
        LineageEventType::TableDeleted => "table-deleted",
        LineageEventType::TableRestored => "table-restored",
        LineageEventType::CredentialsVendAttempted => "credentials-vend-attempted",
        LineageEventType::QueryGraphBootstrap => "querygraph-bootstrap",
    }
}

#[cfg(test)]
mod tests {
    use lakecat_core::{Namespace, PrincipalKind, TableName, WarehouseName};

    use super::*;

    #[test]
    fn projects_table_scan_to_openlineage_input() {
        let table = TableIdent::new(
            WarehouseName::new("local").unwrap(),
            "default".parse::<Namespace>().unwrap(),
            TableName::new("events").unwrap(),
        );
        let event = LineageEvent::new(
            LineageEventType::TableScanPlanned,
            Principal {
                subject: "agent:reader".to_string(),
                kind: PrincipalKind::Agent,
            },
            Some(table),
            json!({
                "planned-by": "lakecat-sail",
                "storage-location": "file:///tmp/events",
                "read-restriction": {
                    "allowed-columns": ["event_id"],
                    "row-predicate": {
                        "type": "eq",
                        "term": "event_id",
                        "value": "evt-1"
                    }
                },
            }),
        );
        let projected = open_lineage_event(&event);
        assert_eq!(projected["eventType"], json!("COMPLETE"));
        assert_eq!(projected["job"]["name"], json!("table-scan-planned"));
        assert_eq!(projected["inputs"][0]["name"], json!("events"));
        assert_eq!(
            projected["inputs"][0]["facets"]["dataSource"]["uri"],
            json!("file:///tmp/events")
        );
        assert_eq!(
            projected["inputs"][0]["facets"]["lakecat_catalog"]["payload"]["read-restriction"]["allowed-columns"],
            json!(["event_id"])
        );
        assert_eq!(projected["outputs"], json!([]));
    }

    #[test]
    fn projects_table_restore_to_openlineage_output() {
        let table = TableIdent::new(
            WarehouseName::new("local").unwrap(),
            "default".parse::<Namespace>().unwrap(),
            TableName::new("events").unwrap(),
        );
        let event = LineageEvent::new(
            LineageEventType::TableRestored,
            Principal::anonymous(),
            Some(table),
            json!({
                "metadata-location": "file:///tmp/events/metadata/00000.json",
                "version": 0,
            }),
        );
        let projected = open_lineage_event(&event);
        assert_eq!(projected["job"]["name"], json!("table-restored"));
        assert_eq!(projected["inputs"], json!([]));
        assert_eq!(projected["outputs"][0]["name"], json!("events"));
        assert_eq!(
            projected["outputs"][0]["facets"]["dataSource"]["uri"],
            json!("file:///tmp/events/metadata/00000.json")
        );
    }

    #[test]
    fn projects_querygraph_bootstrap_to_openlineage_output() {
        let event = LineageEvent::new(
            LineageEventType::QueryGraphBootstrap,
            Principal::anonymous(),
            None,
            json!({
                "warehouse": "local",
                "table-count": 1,
                "policy-binding-count": 1,
                "bundle-hash": "sha256:bundle",
                "graph-hash": "sha256:graph",
                "open-lineage-hash": "sha256:openlineage",
                "querygraph-import-hash": "sha256:querygraph-import",
                "authorization-receipt": {
                    "request-identity": {
                        "attestation-state": "verified",
                        "typedid": "did:example:agent"
                    }
                }
            }),
        );
        let projected = open_lineage_event(&event);
        assert_eq!(projected["job"]["name"], json!("querygraph-bootstrap"));
        assert_eq!(projected["inputs"], json!([]));
        assert_eq!(
            projected["outputs"][0]["namespace"],
            json!("lakecat.querygraph")
        );
        assert_eq!(projected["outputs"][0]["name"], json!("bootstrap"));
        assert_eq!(
            projected["outputs"][0]["facets"]["queryGraph_bootstrap"]["tableCount"],
            json!(1)
        );
        assert_eq!(
            projected["outputs"][0]["facets"]["queryGraph_bootstrap"]["bundleHash"],
            json!("sha256:bundle")
        );
        assert_eq!(
            projected["outputs"][0]["facets"]["queryGraph_bootstrap"]["graphHash"],
            json!("sha256:graph")
        );
        assert_eq!(
            projected["outputs"][0]["facets"]["queryGraph_bootstrap"]["openLineageHash"],
            json!("sha256:openlineage")
        );
        assert_eq!(
            projected["outputs"][0]["facets"]["queryGraph_bootstrap"]["queryGraphImportHash"],
            json!("sha256:querygraph-import")
        );
        assert_eq!(
            projected["outputs"][0]["facets"]["queryGraph_bootstrap"]["payload"]["authorization-receipt"]
                ["request-identity"]["attestation-state"],
            json!("verified")
        );
    }

    #[test]
    fn projects_credential_vend_attempt_to_openlineage_run_facet() {
        let event = LineageEvent::new(
            LineageEventType::CredentialsVendAttempted,
            Principal {
                subject: "agent:reader".to_string(),
                kind: PrincipalKind::Agent,
            },
            None,
            json!({
                "credential-count": 0,
                "lakecat:raw-credential-exception": {
                    "allowed": false,
                    "reason": "fine-grained read restriction requires Sail-planned reads"
                },
            }),
        );
        let projected = open_lineage_event(&event);
        assert_eq!(
            projected["job"]["name"],
            json!("credentials-vend-attempted")
        );
        assert_eq!(projected["inputs"], json!([]));
        assert_eq!(projected["outputs"], json!([]));
        assert_eq!(
            projected["run"]["facets"]["lakecat_catalogEvent"]["payload"]["credential-count"],
            json!(0)
        );
        assert_eq!(
            projected["run"]["facets"]["lakecat_catalogEvent"]["payload"]["lakecat:raw-credential-exception"]
                ["allowed"],
            json!(false)
        );
    }

    #[tokio::test]
    async fn hash_sink_receipts_include_openlineage_hash() {
        let sink = HashOnlyLineageSink;
        let receipt = sink
            .emit(LineageEvent::new(
                LineageEventType::NamespaceCreated,
                Principal::anonymous(),
                None,
                json!({"namespace": ["default"]}),
            ))
            .await
            .unwrap();
        assert_eq!(receipt.sink, "lakecat-openlineage-hash");
        assert!(!receipt.event_hash.is_empty());
        assert!(!receipt.open_lineage_hash.is_empty());
    }
}
