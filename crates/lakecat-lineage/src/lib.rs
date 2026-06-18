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
    PolicyBindingUpserted,
    ProjectUpserted,
    ServerUpserted,
    StorageProfileUpserted,
    TableCreated,
    TableLoaded,
    TableScanPlanned,
    TableCommitted,
    TableDeleted,
    TableRestored,
    ViewDropped,
    ViewUpserted,
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
        LineageEventType::ViewDropped | LineageEventType::ViewUpserted => vec![json!({
            "namespace": "lakecat.view",
            "name": event
                .payload
                .pointer("/view/name")
                .and_then(Value::as_str)
                .unwrap_or("unknown"),
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

fn lineage_event_type_name(event_type: &LineageEventType) -> &'static str {
    match event_type {
        LineageEventType::NamespaceCreated => "namespace-created",
        LineageEventType::NamespaceDropped => "namespace-dropped",
        LineageEventType::PolicyBindingUpserted => "policy-binding-upserted",
        LineageEventType::ProjectUpserted => "project-upserted",
        LineageEventType::ServerUpserted => "server-upserted",
        LineageEventType::StorageProfileUpserted => "storage-profile-upserted",
        LineageEventType::TableCreated => "table-created",
        LineageEventType::TableLoaded => "table-loaded",
        LineageEventType::TableScanPlanned => "table-scan-planned",
        LineageEventType::TableCommitted => "table-committed",
        LineageEventType::TableDeleted => "table-deleted",
        LineageEventType::TableRestored => "table-restored",
        LineageEventType::ViewDropped => "view-dropped",
        LineageEventType::ViewUpserted => "view-upserted",
        LineageEventType::WarehouseUpserted => "warehouse-upserted",
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
    fn projects_control_plane_upserts_to_openlineage_outputs() {
        let policy = open_lineage_event(&LineageEvent::new(
            LineageEventType::PolicyBindingUpserted,
            Principal::anonymous(),
            None,
            json!({
                "warehouse": "local",
                "policy": {
                    "policy-id": "agent-read",
                    "odrl": {
                        "uid": "policy:agent-read"
                    }
                }
            }),
        ));
        assert_eq!(policy["job"]["name"], json!("policy-binding-upserted"));
        assert_eq!(policy["outputs"][0]["namespace"], json!("lakecat.policy"));
        assert_eq!(policy["outputs"][0]["name"], json!("agent-read"));
        assert_eq!(
            policy["outputs"][0]["facets"]["lakecat_policyBinding"]["payload"]["policy"]["odrl"]["uid"],
            json!("policy:agent-read")
        );

        let project = open_lineage_event(&LineageEvent::new(
            LineageEventType::ProjectUpserted,
            Principal::anonymous(),
            None,
            json!({
                "project-id": "default",
                "project-record": {
                    "display-name": "Default Project"
                }
            }),
        ));
        assert_eq!(project["job"]["name"], json!("project-upserted"));
        assert_eq!(project["outputs"][0]["namespace"], json!("lakecat.project"));
        assert_eq!(project["outputs"][0]["name"], json!("default"));
        assert_eq!(
            project["outputs"][0]["facets"]["lakecat_project"]["payload"]["project-record"]["display-name"],
            json!("Default Project")
        );

        let server = open_lineage_event(&LineageEvent::new(
            LineageEventType::ServerUpserted,
            Principal::anonymous(),
            None,
            json!({
                "server-id": "prod",
                "server-record": {
                    "display-name": "Production"
                }
            }),
        ));
        assert_eq!(server["job"]["name"], json!("server-upserted"));
        assert_eq!(server["outputs"][0]["namespace"], json!("lakecat.server"));
        assert_eq!(server["outputs"][0]["name"], json!("prod"));
        assert_eq!(
            server["outputs"][0]["facets"]["lakecat_server"]["payload"]["server-record"]["display-name"],
            json!("Production")
        );

        let storage_profile = open_lineage_event(&LineageEvent::new(
            LineageEventType::StorageProfileUpserted,
            Principal::anonymous(),
            None,
            json!({
                "warehouse": "local",
                "storage-profile": {
                    "profile-id": "s3-events",
                    "location-prefix": "s3://lakecat/events",
                    "provider": "s3",
                    "issuance-mode": "secret-ref"
                }
            }),
        ));
        assert_eq!(
            storage_profile["job"]["name"],
            json!("storage-profile-upserted")
        );
        assert_eq!(
            storage_profile["outputs"][0]["namespace"],
            json!("lakecat.storage-profile")
        );
        assert_eq!(storage_profile["outputs"][0]["name"], json!("s3-events"));
        assert_eq!(
            storage_profile["outputs"][0]["facets"]["lakecat_storageProfile"]["warehouse"],
            json!("local")
        );
        assert_eq!(
            storage_profile["outputs"][0]["facets"]["lakecat_storageProfile"]["payload"]["storage-profile"]
                ["provider"],
            json!("s3")
        );

        let view = open_lineage_event(&LineageEvent::new(
            LineageEventType::ViewUpserted,
            Principal::anonymous(),
            None,
            json!({
                "warehouse": "local",
                "namespace": ["default"],
                "view": {
                    "name": "events_view",
                    "dialect": "sql",
                    "schema-version": 3
                }
            }),
        ));
        assert_eq!(view["job"]["name"], json!("view-upserted"));
        assert_eq!(view["outputs"][0]["namespace"], json!("lakecat.view"));
        assert_eq!(view["outputs"][0]["name"], json!("events_view"));
        assert_eq!(
            view["outputs"][0]["facets"]["lakecat_view"]["warehouse"],
            json!("local")
        );
        assert_eq!(
            view["outputs"][0]["facets"]["lakecat_view"]["namespace"],
            json!(["default"])
        );

        let warehouse = open_lineage_event(&LineageEvent::new(
            LineageEventType::WarehouseUpserted,
            Principal::anonymous(),
            None,
            json!({
                "warehouse": "local",
                "warehouse-record": {
                    "storage-root": "file:///tmp/lakecat"
                }
            }),
        ));
        assert_eq!(warehouse["job"]["name"], json!("warehouse-upserted"));
        assert_eq!(
            warehouse["outputs"][0]["namespace"],
            json!("lakecat.warehouse")
        );
        assert_eq!(warehouse["outputs"][0]["name"], json!("local"));
        assert_eq!(
            warehouse["outputs"][0]["facets"]["lakecat_warehouse"]["payload"]["warehouse-record"]["storage-root"],
            json!("file:///tmp/lakecat")
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
