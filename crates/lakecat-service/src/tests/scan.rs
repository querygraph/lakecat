#![allow(unused_imports)]
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use async_trait::async_trait;
use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use http::{HeaderValue, Method, Request};
use lakecat_api::{
    CatalogConfigResponse, CommitTableRequest, CommitTableResponse, ConfigEntry,
    CreateNamespaceRequest, CreateTableRequest, FetchScanTasksRequest as ApiFetchScanTasksRequest,
    FetchScanTasksResponse, LAKECAT_COMPATIBILITY_KEY, LAKECAT_COMPATIBILITY_VALUE,
    LAKECAT_FORMAT_BASELINE_KEY, LAKECAT_FORMAT_BASELINE_VALUE, LAKECAT_FORMAT_V4_BRIDGE_KEY,
    LAKECAT_FORMAT_V4_BRIDGE_VALUE, LAKECAT_FORMAT_V4_KEY, LAKECAT_FORMAT_V4_TYPED_SAIL_KEY,
    LAKECAT_FORMAT_V4_TYPED_SAIL_VALUE, LAKECAT_FORMAT_V4_VALUE, LineageDrainEventSummary,
    LineageDrainResponse, ListNamespacesResponse, ListPolicyBindingsResponse, ListProjectsResponse,
    ListServersResponse, ListStorageProfilesResponse, ListTableCommitRecordsResponse,
    ListViewVersionReceiptChainsResponse, ListViewVersionReceiptsResponse, ListViewsResponse,
    ListWarehousesResponse, LoadCredentialsResponse, LoadTableResponse, NamespaceResponse,
    PlanTableScanRequest, PlanTableScanResponse, PolicyBindingResponse, ProjectResponse,
    ServerResponse, StorageCredential, StorageProfileResponse, TableCommitRecordResponse,
    TableIdentifier, UpsertPolicyBindingRequest, UpsertProjectRequest, UpsertServerRequest,
    UpsertStorageProfileRequest, UpsertViewRequest, UpsertWarehouseRequest, ViewColumnResponse,
    ViewResponse, ViewVersionReceiptChainResponse, ViewVersionReceiptResponse, WarehouseResponse,
};
#[cfg(not(feature = "sail-local"))]
use lakecat_core::sail::DeferredSailCatalogEngine;
#[cfg(not(feature = "sail-local"))]
use lakecat_core::sail::FetchScanTasksRequest as SailFetchScanTasksRequest;
#[cfg(not(feature = "sail-local"))]
use lakecat_core::sail::ScanPlanningRequest;
use lakecat_core::sail::{CommitPreparationRequest, SailCatalogEngine};
use lakecat_core::{
    LakeCatError, LakeCatResult, Namespace, Principal, PrincipalKind, TableIdent, TableName,
    WarehouseName, content_hash_bytes, content_hash_json,
};
use lakecat_graph::GraphNodeLabel;
use lakecat_graph::{CatalogGraphSink, GraphAction, GraphEvent, NoopCatalogGraphSink};
use lakecat_lineage::{
    HashOnlyLineageSink, LineageEvent, LineageEventType, LineageReceipt, LineageSink,
};
use lakecat_querygraph::{
    QueryGraphBootstrap, QueryGraphTenantProjection, QueryGraphViewReceiptEvidence,
};
#[cfg(feature = "sail-local")]
use lakecat_sail::catalog_provider::{
    LakeCatCatalogProvider, ProviderFetchScanTasksRequest, ProviderScanPlanningRequest,
};
use lakecat_security::{
    AllowAllGovernanceEngine, AuthorizationReceipt, AuthorizationRequest, CatalogAction,
    CatalogConfigCapability, CredentialsVendCapability, GovernanceEngine, GraphReadCapability,
    LineageReadCapability, NamespaceCreateCapability, NamespaceDropCapability,
    NamespaceListCapability, NamespaceLoadCapability, PolicyManageCapability,
    ProjectManageCapability, ReadRestriction, ServerManageCapability,
    StorageProfileManageCapability, TableCommitCapability, TableCreateCapability,
    TableDropCapability, TableLoadCapability, TableRestoreCapability, TableScanCapability,
    ViewDropCapability, ViewLoadCapability, ViewManageCapability, WarehouseManageCapability,
};
use lakecat_store::MemoryCatalogStore;
use lakecat_store::{
    CatalogAuditEvent, CatalogStore, CredentialIssuanceMode, OutboxEvent, PolicyBinding,
    ProjectRecord, ServerRecord, StorageProfile, StorageProvider, TableCommit, TableCommitRecord,
    TableRecord, ViewColumnRecord, ViewRecord, ViewVersionOperation, ViewVersionReceipt,
    WarehouseRecord, table_ident,
};
use object_store::path::Path as ObjectPath;
use object_store::{ObjectStore, ObjectStoreExt, PutMode, PutPayload};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::sync::Mutex;
use tower::ServiceExt;
use url::Url;

use super::common::*;
use crate::*;

#[cfg(feature = "sail-local")]
#[tokio::test]
async fn sail_local_scan_uses_one_http_authorization_receipt_per_request() {
    let store = MemoryCatalogStore::new();
    let warehouse = WarehouseName::new("local").unwrap();
    let ident = table_ident("local", "default".to_string(), "events".to_string()).unwrap();
    store
        .create_table(TableRecord::new(
            ident,
            "file:///tmp/events".to_string(),
            Some("file:///tmp/events/metadata/00000.json".to_string()),
            serde_json::json!({"format-version": 3}),
            Principal::anonymous(),
        ))
        .await
        .unwrap();
    let sail = Arc::new(CapturingSailEngine::default());
    let governance = Arc::new(RecordingGovernance::default());
    let app = app(LakeCatState::new(warehouse, store).with_integrations(
        sail.clone(),
        governance.clone(),
        NoopCatalogGraphSink::new(),
        HashOnlyLineageSink::new(),
    ));

    let plan = Request::builder()
        .method(Method::POST)
        .uri("/catalog/v1/namespaces/default/tables/events/plan")
        .header("content-type", "application/json")
        .header("x-lakecat-principal", "analyst@example.com")
        .body(Body::from(
            serde_json::json!({"select": ["event_id"]}).to_string(),
        ))
        .unwrap();
    let response = app.clone().oneshot(plan).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let fetch = Request::builder()
        .method(Method::POST)
        .uri("/catalog/v1/namespaces/default/tables/events/fetch-scan-tasks")
        .header("content-type", "application/json")
        .header("x-lakecat-principal", "analyst@example.com")
        .body(Body::from(
            serde_json::json!({"plan-task": "lakecat:plan:captured"}).to_string(),
        ))
        .unwrap();
    let response = app.oneshot(fetch).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let contexts = governance.contexts.lock().await;
    assert_eq!(
        contexts.len(),
        2,
        "plan and fetch should each have one HTTP authorization decision"
    );
    assert!(contexts.iter().all(|context| {
        context["request-identity"]["principal"]["subject"]
            == serde_json::json!("analyst@example.com")
            && context.get("lakecat:sail-provider").is_none()
    }));
    drop(contexts);
    assert_eq!(
        *governance.actions.lock().await,
        vec![CatalogAction::TablePlanScan, CatalogAction::TablePlanScan]
    );
    let planned = sail.last_scan.lock().await.clone().unwrap();
    assert_eq!(planned.principal.subject, "analyst@example.com");
    let fetched = sail.last_fetch.lock().await.clone().unwrap();
    assert_eq!(fetched.principal.subject, "analyst@example.com");
}

#[cfg(feature = "sail-local")]
#[tokio::test]
async fn fetch_scan_tasks_exposes_iceberg_rest_plan_task_tokens() {
    let fixture = local_manifest_fixture();
    let app = test_app();
    let upsert_policy = Request::builder()
        .method(Method::PUT)
        .uri("/management/v1/warehouses/local/policies/agent-id-read")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::json!({
                "namespace": ["default"],
                "table": "events",
                "enforced": true,
                "odrl": {
                    "uid": "policy:agent-id-read",
                    "lakecat:read-restriction": {
                        "allowed-columns": ["id"],
                        "row-predicate": {"type": "always-true"}
                    }
                }
            })
            .to_string(),
        ))
        .unwrap();
    let response = app.clone().oneshot(upsert_policy).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let create_payload = serde_json::json!({
        "name": "events",
        "location": fixture.table_location,
        "metadata-location": fixture.metadata_location,
        "metadata": fixture.metadata,
    });
    let create = Request::builder()
        .method(Method::POST)
        .uri("/catalog/v1/namespaces/default/tables")
        .header("content-type", "application/json")
        .body(Body::from(create_payload.to_string()))
        .unwrap();
    let response = app.clone().oneshot(create).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let plan = Request::builder()
        .method(Method::POST)
        .uri("/catalog/v1/namespaces/default/tables/events/plan")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::json!({
                "select": ["id"],
                "filter": {"type": "always-true"},
                "case-sensitive": true,
                "stats-fields": ["id"]
            })
            .to_string(),
        ))
        .unwrap();
    let response = app.clone().oneshot(plan).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let _: sail_catalog_iceberg::models::CompletedPlanningWithIdResult =
        serde_json::from_value(payload.clone()).unwrap();
    assert_eq!(payload["status"], serde_json::json!("completed"));
    assert_eq!(
        payload["residual-filter"]["lakecat:scan-request"]["stats-fields"][0],
        serde_json::json!("id")
    );
    assert_eq!(
        payload["residual-filter"]["lakecat:scan-request"]["read-restriction"],
        serde_json::json!({
            "allowed-columns": ["id"],
            "row-predicate": {"type": "always-true"},
            "policy-hashes": [
                lakecat_core::content_hash_json(&serde_json::json!({
                    "uid": "policy:agent-id-read",
                    "lakecat:read-restriction": {
                        "allowed-columns": ["id"],
                        "row-predicate": {"type": "always-true"}
                    }
                })).unwrap()
            ]
        })
    );
    assert_eq!(
        payload["residual-filter"]["filters-accepted-by-sail"][0]["filter"],
        serde_json::json!({"type": "always-true"})
    );
    let plan_task = payload["plan-tasks"][0]
        .as_str()
        .expect("plan task token")
        .to_string();

    let fetch = Request::builder()
        .method(Method::POST)
        .uri("/catalog/v1/namespaces/default/tables/events/tasks")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::json!({ "plan-task": plan_task }).to_string(),
        ))
        .unwrap();
    let response = app.oneshot(fetch).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let _: sail_catalog_iceberg::models::FileScanTask =
        serde_json::from_value(payload["file-scan-tasks"][0].clone()).unwrap();
    let _: sail_catalog_iceberg::models::PositionDeleteFile =
        serde_json::from_value(payload["delete-files"][0].clone()).unwrap();

    assert!(payload["plan-tasks"][0].as_str().is_some());
    assert_eq!(
        payload["lakecat-plan-tasks"][0]["task-type"],
        serde_json::json!("manifest")
    );
    assert_eq!(
        payload["file-scan-tasks"][0]["delete-file-references"][0],
        serde_json::json!(0)
    );
    assert_eq!(
        payload["delete-files"][0]["file-path"],
        serde_json::json!(fixture.delete_file_path)
    );
    assert_eq!(
        payload["residual-filter"]["lakecat:fetch-scan-tasks"]["read-restriction"],
        serde_json::json!({
            "allowed-columns": ["id"],
            "row-predicate": {"type": "always-true"},
            "policy-hashes": [
                lakecat_core::content_hash_json(&serde_json::json!({
                    "uid": "policy:agent-id-read",
                    "lakecat:read-restriction": {
                        "allowed-columns": ["id"],
                        "row-predicate": {"type": "always-true"}
                    }
                })).unwrap()
            ]
        })
    );

    let _ = std::fs::remove_dir_all(fixture.root);
}

#[tokio::test]
async fn plan_rejects_invalid_incremental_scan_modes() {
    let app = test_app();
    let create = Request::builder()
        .method(Method::POST)
        .uri("/catalog/v1/namespaces/default/tables")
        .header("content-type", "application/json")
        .body(Body::from(
            r#"{"name":"events","location":"file:///tmp/events","metadata-location":"file:///tmp/events/metadata/00000.json","metadata":{"format-version":3,"table-uuid":"11111111-1111-1111-1111-111111111111","location":"file:///tmp/events","last-sequence-number":7,"last-updated-ms":1710000000000,"last-column-id":1,"schemas":[{"type":"struct","schema-id":1,"fields":[{"id":1,"name":"id","type":"string","required":true,"doc":"Event identifier."}]}],"current-schema-id":1,"partition-specs":[{"spec-id":0,"fields":[]}],"default-spec-id":0,"current-snapshot-id":42,"snapshots":[{"snapshot-id":42,"sequence-number":7,"timestamp-ms":1710000000000,"manifest-list":"file:///tmp/events/metadata/snap-42.avro","summary":{"operation":"append"},"schema-id":1}],"snapshot-log":[{"timestamp-ms":1710000000000,"snapshot-id":42}]}}"#,
        ))
        .unwrap();
    let response = app.clone().oneshot(create).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let mixed = Request::builder()
        .method(Method::POST)
        .uri("/catalog/v1/namespaces/default/tables/events/plan")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::json!({
                "snapshot-id": 42,
                "start-snapshot-id": 1,
                "end-snapshot-id": 42
            })
            .to_string(),
        ))
        .unwrap();
    let response = app.clone().oneshot(mixed).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let missing_end = Request::builder()
        .method(Method::POST)
        .uri("/catalog/v1/namespaces/default/tables/events/plan")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::json!({"start-snapshot-id": 1}).to_string(),
        ))
        .unwrap();
    let response = app.clone().oneshot(missing_end).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let missing_start = Request::builder()
        .method(Method::POST)
        .uri("/catalog/v1/namespaces/default/tables/events/plan")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::json!({"end-snapshot-id": 42}).to_string(),
        ))
        .unwrap();
    let response = app.clone().oneshot(missing_start).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    #[cfg(feature = "sail-local")]
    {
        let invalid_range = Request::builder()
            .method(Method::POST)
            .uri("/catalog/v1/namespaces/default/tables/events/plan")
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::json!({
                    "start-snapshot-id": 1,
                    "end-snapshot-id": 42
                })
                .to_string(),
            ))
            .unwrap();
        let response = app.clone().oneshot(invalid_range).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    let valid_empty_delta = Request::builder()
        .method(Method::POST)
        .uri("/catalog/v1/namespaces/default/tables/events/plan")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::json!({
                "start-snapshot-id": 42,
                "end-snapshot-id": 42
            })
            .to_string(),
        ))
        .unwrap();
    let response = app.oneshot(valid_empty_delta).await.unwrap();
    #[cfg(not(feature = "sail-local"))]
    assert_eq!(response.status(), StatusCode::NOT_IMPLEMENTED);
    #[cfg(feature = "sail-local")]
    {
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["snapshot-id"], serde_json::json!(42));
        assert_eq!(body["plan-tasks"], serde_json::json!([]));
        assert_eq!(
            body["residual-filter"]["lakecat:scan-request"]["start-snapshot-id"],
            serde_json::json!(42)
        );
    }
}

#[tokio::test]
async fn table_scan_authorization_carries_policy_read_restriction() {
    let store = MemoryCatalogStore::new();
    let governance = Arc::new(RecordingGovernance::default());
    let state = LakeCatState::new(WarehouseName::new("local").unwrap(), store.clone())
        .with_integrations(
            default_sail_engine(),
            governance.clone(),
            NoopCatalogGraphSink::new(),
            HashOnlyLineageSink::new(),
        );
    let ident = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        Namespace::new(vec!["default".to_string()]).unwrap(),
        TableName::new("events").unwrap(),
    );
    store
        .upsert_policy_binding(
            PolicyBinding::new(
                "agent-columns",
                WarehouseName::new("local").unwrap(),
                Some(ident.namespace.clone()),
                Some(ident.name.clone()),
                true,
                serde_json::json!({
                    "uid": "policy:agent-columns",
                    "lakecat:read-restriction": {
                        "allowed-columns": ["event_id"],
                        "row-predicate": {
                            "type": "eq",
                            "term": "event_id",
                            "value": "evt-1"
                        }
                    },
                    "permission": [{
                        "action": "read",
                        "constraint": [{
                            "leftOperand": "purpose",
                            "operator": "eq",
                            "rightOperand": "resilience-demo"
                        }]
                    }]
                }),
            )
            .unwrap(),
        )
        .await
        .unwrap();

    let capability = authorize_table_scan(
        &state,
        RequestIdentity {
            principal: Principal::new("did:example:agent", PrincipalKind::Agent).unwrap(),
            envelope: serde_json::json!({"type": "test"}),
            typedid_envelope: None,
        },
        ident,
    )
    .await
    .unwrap();
    let restriction = capability.read_restriction().unwrap();
    assert_eq!(
        restriction.allowed_columns.as_deref(),
        Some(&["event_id".to_string()][..])
    );
    assert_eq!(restriction.purpose.as_deref(), Some("resilience-demo"));
    assert_eq!(
        restriction.row_predicate,
        Some(serde_json::json!({
            "type": "eq",
            "term": "event_id",
            "value": "evt-1"
        }))
    );
    assert_eq!(restriction.policy_hashes.len(), 1);
    assert!(
        capability.receipt().policy_hash.is_some(),
        "governed scan receipt should summarize enforced policy hashes"
    );

    let contexts = governance.contexts.lock().await;
    assert_eq!(
        contexts[0]["read-restriction"]["allowed-columns"][0],
        serde_json::json!("event_id")
    );
    assert_eq!(
        contexts[0]["read-restriction"]["row-predicate"],
        serde_json::json!({
            "type": "eq",
            "term": "event_id",
            "value": "evt-1"
        })
    );
}

#[test]
fn scan_planned_audit_payload_surfaces_policy_context() {
    let ident = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let table = TableRecord::new(
        ident.clone(),
        "file:///tmp/events".to_string(),
        Some("file:///tmp/events/metadata/00000.json".to_string()),
        serde_json::json!({ "format-version": 3 }),
        Principal::anonymous(),
    );
    let receipt = AuthorizationReceipt {
        principal: Principal::new("did:example:agent", PrincipalKind::Agent).unwrap(),
        action: CatalogAction::TablePlanScan,
        table: Some(ident.clone()),
        allowed: true,
        engine: "test".to_string(),
        policy_hash: Some("policy-hash".to_string()),
        context: serde_json::json!({
            "read-restriction": {
                "allowed-columns": ["event_id"],
                "row-predicate": {
                    "type": "eq",
                    "term": "event_id",
                    "value": "evt-1"
                }
            }
        }),
        checked_at: chrono::Utc::now(),
    };
    let scan = lakecat_core::sail::ScanPlan {
        planned_by: "lakecat-sail".to_string(),
        snapshot_id: Some(42),
        scan_tasks: vec![serde_json::json!({"task": 1})],
        residual_filter: None,
    };

    let scan_request_extensions = serde_json::json!({
        "requested-projection": ["event_id", "payload"],
        "effective-projection": ["event_id"],
        "requested-stats-fields": ["event_id", "payload"],
        "effective-stats-fields": ["event_id"]
    });
    let payload =
        table_scan_planned_audit_payload(&ident, &table, &receipt, &scan, &scan_request_extensions);
    assert_eq!(
        payload["storage-location"],
        serde_json::json!("file:///tmp/events")
    );
    assert_eq!(
        payload["metadata-location"],
        serde_json::json!("file:///tmp/events/metadata/00000.json")
    );
    assert_eq!(
        payload["read-restriction"]["allowed-columns"],
        serde_json::json!(["event_id"])
    );
    assert_eq!(
        payload["authorization-receipt"]["context"]["read-restriction"],
        payload["read-restriction"]
    );
    assert_eq!(payload["scan-task-count"], serde_json::json!(1));
    assert_eq!(
        payload["requested-projection"],
        serde_json::json!(["event_id", "payload"])
    );
    assert_eq!(
        payload["effective-projection"],
        serde_json::json!(["event_id"])
    );
    assert_eq!(
        payload["requested-stats-fields"],
        serde_json::json!(["event_id", "payload"])
    );
    assert_eq!(
        payload["effective-stats-fields"],
        serde_json::json!(["event_id"])
    );
}

#[test]
fn scan_planned_drain_summary_preserves_projection_evidence() {
    let principal = Principal::new("did:example:agent", PrincipalKind::Agent).unwrap();
    let policy_hash =
        content_hash_json(&json!({"policy-id": "agent-read", "scope": "default.events"})).unwrap();
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let read_restriction = json!({
        "allowed-columns": ["event_id"],
        "row-predicate": {
            "type": "eq",
            "term": "event_id",
            "value": "evt-1"
        },
        "purpose": "qglake-agent-demo",
        "max-credential-ttl-seconds": 300,
        "policy-hashes": [policy_hash]
    });
    let event = OutboxEvent {
        event_id: "evt-plan".to_string(),
        sink: "lakecat.lineage-and-graph".to_string(),
        event_type: "table.scan-planned".to_string(),
        payload: json!({
            "audit-event-id": "audit-plan",
            "event-type": "table.scan-planned",
            "table": table,
            "payload": {
                "event-type": "table.scan-planned",
                "table": table,
                "authorization-receipt": {
                    "principal": principal,
                    "action": "table-plan-scan",
                    "allowed": true,
                    "engine": "test",
                    "policy_hash": null,
                    "context": {
                        "read-restriction": read_restriction
                    },
                    "checked_at": chrono::Utc::now(),
                },
                "planned-by": "test",
                "snapshot-id": 1,
                "storage-location": "s3://bucket/events",
                "metadata-location": "s3://bucket/events/metadata/v1.json",
                "read-restriction": read_restriction,
                "requested-projection": ["event_id", "payload"],
                "effective-projection": ["event_id"],
                "requested-stats-fields": ["event_id", "payload"],
                "effective-stats-fields": ["event_id"],
                "required-filters": [{
                    "type": "eq",
                    "term": "event_id",
                    "value": "evt-1"
                }],
                "scan-task-count": 1
            }
        }),
        created_at: chrono::Utc::now(),
        delivered_at: None,
    };
    let receipt = OutboxProjectionReceipt {
        graph_events: 3,
        lineage_events: 1,
        lineage_event_hashes: vec![content_hash_bytes(b"recorded")],
        open_lineage_hashes: vec![content_hash_bytes(b"recorded-openlineage")],
    };

    let summary = lineage_drain_event_summary(&event, &receipt).unwrap();

    assert_eq!(summary.event_type, "table.scan-planned");
    assert_eq!(summary.scan_task_count, Some(1));
    assert_eq!(summary.graph_events, 3);
    assert_eq!(summary.lineage_events, 1);
    assert_eq!(
        summary.read_restriction.as_ref().unwrap()["allowed-columns"],
        serde_json::json!(["event_id"])
    );
    assert_eq!(
        summary.requested_projection,
        vec!["event_id".to_string(), "payload".to_string()]
    );
    assert_eq!(summary.effective_projection, vec!["event_id".to_string()]);
    assert_eq!(
        summary.requested_stats_fields,
        vec!["event_id".to_string(), "payload".to_string()]
    );
    assert_eq!(summary.effective_stats_fields, vec!["event_id".to_string()]);
    assert!(
        summary
            .replay_event_hashes
            .iter()
            .all(|hash| is_full_sha256_hash(hash))
    );
    assert_eq!(
        summary.replay_open_lineage_hashes.len(),
        summary.lineage_events
    );
    assert!(
        summary
            .replay_open_lineage_hashes
            .iter()
            .all(|hash| is_full_sha256_hash(hash))
    );
}

#[test]
fn scan_tasks_fetched_audit_payload_surfaces_policy_context() {
    let ident = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let table = TableRecord::new(
        ident.clone(),
        "file:///tmp/events".to_string(),
        Some("file:///tmp/events/metadata/00000.json".to_string()),
        serde_json::json!({ "format-version": 3 }),
        Principal::anonymous(),
    );
    let receipt = AuthorizationReceipt {
        principal: Principal::new("did:example:agent", PrincipalKind::Agent).unwrap(),
        action: CatalogAction::TablePlanScan,
        table: Some(ident.clone()),
        allowed: true,
        engine: "test".to_string(),
        policy_hash: Some("policy-hash".to_string()),
        context: serde_json::json!({
            "read-restriction": {
                "allowed-columns": ["event_id"],
                "row-predicate": {
                    "type": "eq",
                    "term": "event_id",
                    "value": "evt-1"
                }
            }
        }),
        checked_at: chrono::Utc::now(),
    };
    let fetched = lakecat_core::sail::FetchScanTasksPlan {
        planned_by: "lakecat-sail".to_string(),
        plan_task: "lakecat:plan:abc".to_string(),
        snapshot_id: Some(42),
        file_scan_tasks: vec![serde_json::json!({"file": "events.parquet"})],
        delete_files: vec![serde_json::json!({"file": "events-delete.parquet"})],
        plan_tasks: vec![serde_json::json!({"task": 2})],
        residual_filter: None,
    };

    let fetch_extensions = serde_json::json!({
        "requested-stats-fields": ["event_id"],
        "effective-stats-fields": ["event_id"],
        "stats-fields": ["event_id"],
    });
    let payload = table_scan_tasks_fetched_audit_payload(
        &ident,
        &table,
        &receipt,
        &fetched,
        &fetch_extensions,
    );
    assert_eq!(
        payload["storage-location"],
        serde_json::json!("file:///tmp/events")
    );
    assert_eq!(
        payload["metadata-location"],
        serde_json::json!("file:///tmp/events/metadata/00000.json")
    );
    assert_eq!(
        payload["read-restriction"]["allowed-columns"],
        serde_json::json!(["event_id"])
    );
    assert_eq!(
        payload["authorization-receipt"]["context"]["read-restriction"],
        payload["read-restriction"]
    );
    assert_eq!(
        payload["required-projection"],
        serde_json::json!(["event_id"])
    );
    assert_eq!(
        payload["effective-projection"],
        serde_json::json!(["event_id"])
    );
    assert_eq!(
        payload["required-filters"][0],
        serde_json::json!({
            "type": "eq",
            "term": "event_id",
            "value": "evt-1"
        })
    );
    assert_eq!(
        payload["requested-stats-fields"],
        serde_json::json!(["event_id"])
    );
    assert_eq!(
        payload["effective-stats-fields"],
        serde_json::json!(["event_id"])
    );
    assert_eq!(payload["stats-fields"], serde_json::json!(["event_id"]));
    assert_eq!(payload["file-scan-task-count"], serde_json::json!(1));
    assert_eq!(payload["delete-file-count"], serde_json::json!(1));
    assert_eq!(payload["child-plan-task-count"], serde_json::json!(1));
}

#[cfg(not(feature = "sail-local"))]
#[tokio::test]
async fn scan_planning_route_sends_effective_policy_scope_to_sail() {
    let store = MemoryCatalogStore::new();
    let sail = Arc::new(CapturingSailEngine::default());
    let app = app(
        LakeCatState::new(WarehouseName::new("local").unwrap(), store.clone()).with_integrations(
            sail.clone(),
            AllowAllGovernanceEngine::new(),
            NoopCatalogGraphSink::new(),
            HashOnlyLineageSink::new(),
        ),
    );

    let upsert = Request::builder()
        .method(Method::PUT)
        .uri("/management/v1/warehouses/local/policies/agent-columns")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::json!({
                "namespace": ["default"],
                "table": "events",
                "enforced": true,
                "odrl": {
                    "uid": "policy:agent-columns",
                    "lakecat:read-restriction": {
                        "allowed-columns": ["event_id"],
                        "row-predicate": {
                            "type": "eq",
                            "term": "event_id",
                            "value": "evt-1"
                        }
                    }
                }
            })
            .to_string(),
        ))
        .unwrap();
    let response = app.clone().oneshot(upsert).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let create = Request::builder()
        .method(Method::POST)
        .uri("/catalog/v1/namespaces/default/tables")
        .header("content-type", "application/json")
        .body(Body::from(
            r#"{"name":"events","location":"file:///tmp/events","metadata-location":"file:///tmp/events/metadata/00000.json","metadata":{"format-version":3,"current-schema-id":1,"schemas":[{"schema-id":1,"fields":[{"id":1,"name":"event_id","type":"string","required":true},{"id":2,"name":"payload","type":"string","required":false}]}]}}"#,
        ))
        .unwrap();
    let response = app.clone().oneshot(create).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let plan = Request::builder()
        .method(Method::POST)
        .uri("/catalog/v1/namespaces/default/tables/events/plan")
        .header("content-type", "application/json")
        .header("x-lakecat-agent-did", "did:example:agent")
        .body(Body::from(
            serde_json::json!({
                "select": ["event_id", "payload"],
                "stats-fields": ["event_id", "payload"],
                "case-sensitive": true
            })
            .to_string(),
        ))
        .unwrap();
    let response = app.clone().oneshot(plan).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(
        body["residual-filter"]["projection"],
        serde_json::json!(["event_id"])
    );
    assert_eq!(
        body["residual-filter"]["filters"][0],
        serde_json::json!({
            "type": "eq",
            "term": "event_id",
            "value": "evt-1"
        })
    );
    assert_eq!(
        body["residual-filter"]["lakecat:scan-request"]["requested-projection"],
        serde_json::json!(["event_id", "payload"])
    );
    assert_eq!(
        body["residual-filter"]["lakecat:scan-request"]["effective-projection"],
        serde_json::json!(["event_id"])
    );
    assert_eq!(
        body["residual-filter"]["lakecat:scan-request"]["requested-stats-fields"],
        serde_json::json!(["event_id", "payload"])
    );
    assert_eq!(
        body["residual-filter"]["lakecat:scan-request"]["effective-stats-fields"],
        serde_json::json!(["event_id"])
    );

    let captured = sail
        .last_scan
        .lock()
        .await
        .clone()
        .expect("scan should reach Sail");
    assert_eq!(captured.projection, vec!["event_id".to_string()]);
    assert_eq!(
        captured.filters,
        vec![serde_json::json!({
            "type": "eq",
            "term": "event_id",
            "value": "evt-1"
        })]
    );

    let ident = table_ident("local", "default", "events").unwrap();
    let outbox = store
        .pending_outbox_events(Some("lakecat.lineage-and-graph"), 10)
        .await
        .unwrap();
    let event = outbox
        .iter()
        .find(|event| event.event_type == "table.scan-planned")
        .expect("scan planning should be audited for replay");
    assert_eq!(event.payload["payload"]["table"], serde_json::json!(ident));
    assert_eq!(
        event.payload["payload"]["requested-stats-fields"],
        serde_json::json!(["event_id", "payload"])
    );
    assert_eq!(
        event.payload["payload"]["effective-stats-fields"],
        serde_json::json!(["event_id"])
    );
    assert_eq!(
        event.payload["payload"]["requested-projection"],
        serde_json::json!(["event_id", "payload"])
    );
    assert_eq!(
        event.payload["payload"]["effective-projection"],
        serde_json::json!(["event_id"])
    );
    assert_eq!(
        event.payload["payload"]["read-restriction"]["allowed-columns"],
        serde_json::json!(["event_id"])
    );
}

#[cfg(not(feature = "sail-local"))]
#[tokio::test]
async fn scan_planning_rejects_malformed_odrl_before_sail() {
    let store = MemoryCatalogStore::new();
    let sail = Arc::new(CapturingSailEngine::default());
    let app = app(
        LakeCatState::new(WarehouseName::new("local").unwrap(), store.clone()).with_integrations(
            sail.clone(),
            AllowAllGovernanceEngine::new(),
            NoopCatalogGraphSink::new(),
            HashOnlyLineageSink::new(),
        ),
    );

    let upsert = Request::builder()
        .method(Method::PUT)
        .uri("/management/v1/warehouses/local/policies/agent-columns")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::json!({
                "namespace": ["default"],
                "table": "events",
                "enforced": true,
                "odrl": {
                    "uid": "policy:agent-columns",
                    "permission": [{
                        "action": "read",
                        "constraint": [{
                            "leftOperand": "allowed-columns",
                            "operator": "eq"
                        }]
                    }]
                }
            })
            .to_string(),
        ))
        .unwrap();
    let response = app.clone().oneshot(upsert).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let create = Request::builder()
        .method(Method::POST)
        .uri("/catalog/v1/namespaces/default/tables")
        .header("content-type", "application/json")
        .body(Body::from(
            r#"{"name":"events","location":"file:///tmp/events","metadata-location":"file:///tmp/events/metadata/00000.json","metadata":{"format-version":3,"current-schema-id":1,"schemas":[{"schema-id":1,"fields":[{"id":1,"name":"event_id","type":"string","required":true},{"id":2,"name":"payload","type":"string","required":false}]}]}}"#,
        ))
        .unwrap();
    let response = app.clone().oneshot(create).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let plan = Request::builder()
        .method(Method::POST)
        .uri("/catalog/v1/namespaces/default/tables/events/plan")
        .header("content-type", "application/json")
        .header("x-lakecat-agent-did", "did:example:agent")
        .body(Body::from(
            serde_json::json!({
                "select": ["event_id", "payload"],
                "case-sensitive": true
            })
            .to_string(),
        ))
        .unwrap();
    let response = app.oneshot(plan).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let message = body["error"]["message"].as_str().unwrap();
    assert!(message.contains("ODRL allowed columns constraint must include a right operand"));
    assert!(
        sail.last_scan.lock().await.is_none(),
        "malformed active ODRL must fail before Sail planning"
    );
    let outbox = store
        .pending_outbox_events(Some("lakecat.lineage-and-graph"), 10)
        .await
        .unwrap();
    assert!(
        outbox
            .iter()
            .all(|event| event.event_type != "table.scan-planned"),
        "malformed active ODRL must not emit scan-planned replay evidence"
    );
}

#[cfg(not(feature = "sail-local"))]
#[tokio::test]
async fn scan_planning_rejects_malformed_jsonld_odrl_before_sail() {
    let store = MemoryCatalogStore::new();
    let sail = Arc::new(CapturingSailEngine::default());
    let app = app(
        LakeCatState::new(WarehouseName::new("local").unwrap(), store.clone()).with_integrations(
            sail.clone(),
            AllowAllGovernanceEngine::new(),
            NoopCatalogGraphSink::new(),
            HashOnlyLineageSink::new(),
        ),
    );

    let upsert = Request::builder()
        .method(Method::PUT)
        .uri("/management/v1/warehouses/local/policies/agent-columns")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::json!({
                "namespace": ["default"],
                "table": "events",
                "enforced": true,
                "odrl": {
                    "uid": "policy:agent-columns",
                    "permission": [{
                        "action": "read",
                        "constraint": [{
                            "leftOperand": { "@id": "lakecat:allowed-columns" },
                            "operator": { "@id": "odrl:isAnyOf" },
                            "rightOperand": {
                                "@list": [
                                    { "@value": "event_id" },
                                    { "@id": "lakecat:not-a-column-value" }
                                ]
                            }
                        }]
                    }]
                }
            })
            .to_string(),
        ))
        .unwrap();
    let response = app.clone().oneshot(upsert).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let create = Request::builder()
        .method(Method::POST)
        .uri("/catalog/v1/namespaces/default/tables")
        .header("content-type", "application/json")
        .body(Body::from(
            r#"{"name":"events","location":"file:///tmp/events","metadata-location":"file:///tmp/events/metadata/00000.json","metadata":{"format-version":3,"current-schema-id":1,"schemas":[{"schema-id":1,"fields":[{"id":1,"name":"event_id","type":"string","required":true},{"id":2,"name":"payload","type":"string","required":false}]}]}}"#,
        ))
        .unwrap();
    let response = app.clone().oneshot(create).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let plan = Request::builder()
        .method(Method::POST)
        .uri("/catalog/v1/namespaces/default/tables/events/plan")
        .header("content-type", "application/json")
        .header("x-lakecat-agent-did", "did:example:agent")
        .body(Body::from(
            serde_json::json!({
                "select": ["event_id", "payload"],
                "case-sensitive": true
            })
            .to_string(),
        ))
        .unwrap();
    let response = app.oneshot(plan).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let message = body["error"]["message"].as_str().unwrap();
    assert!(message.contains("ODRL allowed columns must be strings"));
    assert!(
        sail.last_scan.lock().await.is_none(),
        "malformed JSON-LD active ODRL must fail before Sail planning"
    );
    let outbox = store
        .pending_outbox_events(Some("lakecat.lineage-and-graph"), 10)
        .await
        .unwrap();
    assert!(
        outbox
            .iter()
            .all(|event| event.event_type != "table.scan-planned"),
        "malformed JSON-LD active ODRL must not emit scan-planned replay evidence"
    );
}

#[cfg(not(feature = "sail-local"))]
#[tokio::test]
async fn fetch_scan_tasks_route_sends_required_policy_scope_to_sail() {
    let store = MemoryCatalogStore::new();
    let sail = Arc::new(CapturingSailEngine::default());
    let app = app(
        LakeCatState::new(WarehouseName::new("local").unwrap(), store.clone()).with_integrations(
            sail.clone(),
            AllowAllGovernanceEngine::new(),
            NoopCatalogGraphSink::new(),
            HashOnlyLineageSink::new(),
        ),
    );

    let upsert = Request::builder()
        .method(Method::PUT)
        .uri("/management/v1/warehouses/local/policies/agent-columns")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::json!({
                "namespace": ["default"],
                "table": "events",
                "enforced": true,
                "odrl": {
                    "uid": "policy:agent-columns",
                    "lakecat:read-restriction": {
                        "allowed-columns": ["event_id"],
                        "row-predicate": {
                            "type": "eq",
                            "term": "event_id",
                            "value": "evt-1"
                        }
                    }
                }
            })
            .to_string(),
        ))
        .unwrap();
    let response = app.clone().oneshot(upsert).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let create = Request::builder()
        .method(Method::POST)
        .uri("/catalog/v1/namespaces/default/tables")
        .header("content-type", "application/json")
        .body(Body::from(
            r#"{"name":"events","location":"file:///tmp/events","metadata-location":"file:///tmp/events/metadata/00000.json","metadata":{"format-version":3,"current-schema-id":1,"schemas":[{"schema-id":1,"fields":[{"id":1,"name":"event_id","type":"string","required":true},{"id":2,"name":"payload","type":"string","required":false}]}]}}"#,
        ))
        .unwrap();
    let response = app.clone().oneshot(create).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let fetch = Request::builder()
        .method(Method::POST)
        .uri("/catalog/v1/namespaces/default/tables/events/tasks")
        .header("content-type", "application/json")
        .header("x-lakecat-agent-did", "did:example:agent")
        .body(Body::from(
            serde_json::json!({"plan-task": "lakecat:plan:captured"}).to_string(),
        ))
        .unwrap();
    let response = app.clone().oneshot(fetch).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(
        body["residual-filter"]["required-projection"],
        serde_json::json!(["event_id"])
    );
    assert_eq!(
        body["residual-filter"]["required-filters"][0],
        serde_json::json!({
            "type": "eq",
            "term": "event_id",
            "value": "evt-1"
        })
    );
    assert_eq!(
        body["residual-filter"]["lakecat:fetch-scan-tasks"]["required-projection"],
        serde_json::json!(["event_id"])
    );
    assert_eq!(
        body["residual-filter"]["lakecat:fetch-scan-tasks"]["effective-projection"],
        serde_json::json!(["event_id"])
    );
    assert_eq!(
        body["residual-filter"]["lakecat:fetch-scan-tasks"]["required-filters"][0],
        serde_json::json!({
            "type": "eq",
            "term": "event_id",
            "value": "evt-1"
        })
    );

    let captured = sail
        .last_fetch
        .lock()
        .await
        .clone()
        .expect("fetch should reach Sail");
    assert_eq!(captured.required_projection, vec!["event_id".to_string()]);
    assert_eq!(
        captured.required_filters,
        vec![serde_json::json!({
            "type": "eq",
            "term": "event_id",
            "value": "evt-1"
        })]
    );

    let ident = table_ident("local", "default", "events").unwrap();
    let outbox = store
        .pending_outbox_events(Some("lakecat.lineage-and-graph"), 10)
        .await
        .unwrap();
    let event = outbox
        .iter()
        .find(|event| event.event_type == "table.scan-tasks-fetched")
        .expect("scan-task fetch should be audited for replay");
    assert_eq!(event.payload["payload"]["table"], serde_json::json!(ident));
    assert_eq!(
        event.payload["payload"]["required-projection"],
        serde_json::json!(["event_id"])
    );
    assert_eq!(
        event.payload["payload"]["effective-projection"],
        serde_json::json!(["event_id"])
    );
    assert_eq!(
        event.payload["payload"]["required-filters"][0],
        serde_json::json!({
            "type": "eq",
            "term": "event_id",
            "value": "evt-1"
        })
    );
    assert_eq!(
        event.payload["payload"]["read-restriction"]["allowed-columns"],
        serde_json::json!(["event_id"])
    );
}

#[cfg(not(feature = "sail-local"))]
#[tokio::test]
async fn fetch_scan_tasks_rejects_malformed_odrl_before_sail() {
    let store = MemoryCatalogStore::new();
    let sail = Arc::new(CapturingSailEngine::default());
    let app = app(
        LakeCatState::new(WarehouseName::new("local").unwrap(), store.clone()).with_integrations(
            sail.clone(),
            AllowAllGovernanceEngine::new(),
            NoopCatalogGraphSink::new(),
            HashOnlyLineageSink::new(),
        ),
    );

    let upsert = Request::builder()
        .method(Method::PUT)
        .uri("/management/v1/warehouses/local/policies/agent-columns")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::json!({
                "namespace": ["default"],
                "table": "events",
                "enforced": true,
                "odrl": {
                    "uid": "policy:agent-columns",
                    "permission": [{
                        "action": "read",
                        "constraint": [{
                            "leftOperand": "allowed-columns",
                            "operator": "eq"
                        }]
                    }]
                }
            })
            .to_string(),
        ))
        .unwrap();
    let response = app.clone().oneshot(upsert).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let create = Request::builder()
        .method(Method::POST)
        .uri("/catalog/v1/namespaces/default/tables")
        .header("content-type", "application/json")
        .body(Body::from(
            r#"{"name":"events","location":"file:///tmp/events","metadata-location":"file:///tmp/events/metadata/00000.json","metadata":{"format-version":3,"current-schema-id":1,"schemas":[{"schema-id":1,"fields":[{"id":1,"name":"event_id","type":"string","required":true},{"id":2,"name":"payload","type":"string","required":false}]}]}}"#,
        ))
        .unwrap();
    let response = app.clone().oneshot(create).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let fetch = Request::builder()
        .method(Method::POST)
        .uri("/catalog/v1/namespaces/default/tables/events/tasks")
        .header("content-type", "application/json")
        .header("x-lakecat-agent-did", "did:example:agent")
        .body(Body::from(
            serde_json::json!({"plan-task": "lakecat:plan:captured"}).to_string(),
        ))
        .unwrap();
    let response = app.oneshot(fetch).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let message = body["error"]["message"].as_str().unwrap();
    assert!(message.contains("ODRL allowed columns constraint must include a right operand"));
    assert!(
        sail.last_fetch.lock().await.is_none(),
        "malformed active ODRL must fail before Sail fetch"
    );
    let outbox = store
        .pending_outbox_events(Some("lakecat.lineage-and-graph"), 10)
        .await
        .unwrap();
    assert!(
        outbox
            .iter()
            .all(|event| event.event_type != "table.scan-tasks-fetched"),
        "malformed active ODRL must not emit scan-task fetch replay evidence"
    );
}

#[cfg(not(feature = "sail-local"))]
#[tokio::test]
async fn fetch_scan_tasks_rejects_malformed_jsonld_odrl_before_sail() {
    let store = MemoryCatalogStore::new();
    let sail = Arc::new(CapturingSailEngine::default());
    let app = app(
        LakeCatState::new(WarehouseName::new("local").unwrap(), store.clone()).with_integrations(
            sail.clone(),
            AllowAllGovernanceEngine::new(),
            NoopCatalogGraphSink::new(),
            HashOnlyLineageSink::new(),
        ),
    );

    let upsert = Request::builder()
        .method(Method::PUT)
        .uri("/management/v1/warehouses/local/policies/agent-columns")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::json!({
                "namespace": ["default"],
                "table": "events",
                "enforced": true,
                "odrl": {
                    "uid": "policy:agent-columns",
                    "permission": [{
                        "action": "read",
                        "constraint": [{
                            "leftOperand": { "@id": "lakecat:allowed-columns" },
                            "operator": { "@id": "odrl:isAnyOf" },
                            "rightOperand": {
                                "@list": [
                                    { "@value": "event_id" },
                                    { "@id": "lakecat:not-a-column-value" }
                                ]
                            }
                        }]
                    }]
                }
            })
            .to_string(),
        ))
        .unwrap();
    let response = app.clone().oneshot(upsert).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let create = Request::builder()
        .method(Method::POST)
        .uri("/catalog/v1/namespaces/default/tables")
        .header("content-type", "application/json")
        .body(Body::from(
            r#"{"name":"events","location":"file:///tmp/events","metadata-location":"file:///tmp/events/metadata/00000.json","metadata":{"format-version":3,"current-schema-id":1,"schemas":[{"schema-id":1,"fields":[{"id":1,"name":"event_id","type":"string","required":true},{"id":2,"name":"payload","type":"string","required":false}]}]}}"#,
        ))
        .unwrap();
    let response = app.clone().oneshot(create).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let fetch = Request::builder()
        .method(Method::POST)
        .uri("/catalog/v1/namespaces/default/tables/events/tasks")
        .header("content-type", "application/json")
        .header("x-lakecat-agent-did", "did:example:agent")
        .body(Body::from(
            serde_json::json!({"plan-task": "lakecat:plan:captured"}).to_string(),
        ))
        .unwrap();
    let response = app.oneshot(fetch).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let message = body["error"]["message"].as_str().unwrap();
    assert!(message.contains("ODRL allowed columns must be strings"));
    assert!(
        sail.last_fetch.lock().await.is_none(),
        "malformed JSON-LD active ODRL must fail before Sail fetch"
    );
    let outbox = store
        .pending_outbox_events(Some("lakecat.lineage-and-graph"), 10)
        .await
        .unwrap();
    assert!(
        outbox
            .iter()
            .all(|event| event.event_type != "table.scan-tasks-fetched"),
        "malformed JSON-LD active ODRL must not emit scan-task fetch replay evidence"
    );
}

#[cfg(feature = "sail-local")]
#[tokio::test]
async fn scan_planning_applies_policy_column_restriction_before_sail() {
    let app = test_app();
    let upsert = Request::builder()
        .method(Method::PUT)
        .uri("/management/v1/warehouses/local/policies/agent-columns")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::json!({
                "namespace": ["default"],
                "table": "events",
                "enforced": true,
                "odrl": {
                    "uid": "policy:agent-columns",
                    "lakecat:read-restriction": {
                        "allowed-columns": ["event_id"],
                        "row-predicate": {
                            "type": "eq",
                            "term": "event_id",
                            "value": "evt-1"
                        }
                    }
                }
            })
            .to_string(),
        ))
        .unwrap();
    let response = app.clone().oneshot(upsert).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let create = Request::builder()
        .method(Method::POST)
        .uri("/catalog/v1/namespaces/default/tables")
        .header("content-type", "application/json")
        .body(Body::from(
            r#"{"name":"events","location":"file:///tmp/events","metadata-location":"file:///tmp/events/metadata/00000.json","metadata":{"format-version":3,"table-uuid":"11111111-1111-1111-1111-111111111111","location":"file:///tmp/events","last-sequence-number":7,"last-updated-ms":1710000000000,"last-column-id":2,"schemas":[{"type":"struct","schema-id":1,"fields":[{"id":1,"name":"event_id","type":"string","required":true},{"id":2,"name":"payload","type":"string","required":false}]}],"current-schema-id":1,"partition-specs":[{"spec-id":0,"fields":[]}],"default-spec-id":0,"current-snapshot-id":42,"snapshots":[{"snapshot-id":42,"sequence-number":7,"timestamp-ms":1710000000000,"manifest-list":"file:///tmp/events/metadata/snap-42.avro","summary":{"operation":"append"},"schema-id":1}],"snapshot-log":[{"timestamp-ms":1710000000000,"snapshot-id":42}]}}"#,
        ))
        .unwrap();
    let response = app.clone().oneshot(create).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let plan = Request::builder()
        .method(Method::POST)
        .uri("/catalog/v1/namespaces/default/tables/events/plan")
        .header("content-type", "application/json")
        .header("x-lakecat-agent-did", "did:example:agent")
        .body(Body::from(
            serde_json::json!({
                "select": ["event_id", "payload"],
                "stats-fields": ["event_id", "payload"],
                "case-sensitive": true
            })
            .to_string(),
        ))
        .unwrap();
    let response = app.clone().oneshot(plan).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(
        body["residual-filter"]["select"],
        serde_json::json!(["event_id"])
    );
    assert_eq!(
        body["residual-filter"]["lakecat:scan-request"]["requested-projection"],
        serde_json::json!(["event_id", "payload"])
    );
    assert_eq!(
        body["residual-filter"]["lakecat:scan-request"]["effective-projection"],
        serde_json::json!(["event_id"])
    );
    assert_eq!(
        body["residual-filter"]["lakecat:scan-request"]["requested-stats-fields"],
        serde_json::json!(["event_id", "payload"])
    );
    assert_eq!(
        body["residual-filter"]["lakecat:scan-request"]["effective-stats-fields"],
        serde_json::json!(["event_id"])
    );
    assert_eq!(
        body["residual-filter"]["lakecat:scan-request"]["stats-fields"],
        serde_json::json!(["event_id"])
    );
    assert_eq!(
        body["residual-filter"]["lakecat:scan-request"]["read-restriction"]["allowed-columns"],
        serde_json::json!(["event_id"])
    );
    assert_eq!(
        body["residual-filter"]["lakecat:scan-request"]["read-restriction"]["row-predicate"],
        serde_json::json!({
            "type": "eq",
            "term": "event_id",
            "value": "evt-1"
        })
    );
    assert_eq!(
        body["residual-filter"]["filters-accepted-by-sail"][0]["filter"],
        serde_json::json!({
            "type": "eq",
            "term": "event_id",
            "value": "evt-1"
        })
    );
    let plan_task = body["plan-tasks"][0].as_str().unwrap().to_string();

    let fetch = Request::builder()
        .method(Method::POST)
        .uri("/catalog/v1/namespaces/default/tables/events/tasks")
        .header("content-type", "application/json")
        .header("x-lakecat-agent-did", "did:example:agent")
        .body(Body::from(
            serde_json::json!({ "plan-task": plan_task }).to_string(),
        ))
        .unwrap();
    let response = app.oneshot(fetch).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(
        body["residual-filter"]["projection"],
        serde_json::json!(["event_id"])
    );
    assert_eq!(
        body["residual-filter"]["filters"][0],
        serde_json::json!({
            "type": "eq",
            "term": "event_id",
            "value": "evt-1"
        })
    );
    assert_eq!(
        body["residual-filter"]["lakecat:fetch-scan-tasks"]["required-projection"],
        serde_json::json!(["event_id"])
    );
    assert_eq!(
        body["residual-filter"]["lakecat:fetch-scan-tasks"]["required-filters"][0],
        serde_json::json!({
            "type": "eq",
            "term": "event_id",
            "value": "evt-1"
        })
    );
    assert_eq!(
        body["residual-filter"]["lakecat:fetch-scan-tasks"]["read-restriction"]["allowed-columns"],
        serde_json::json!(["event_id"])
    );
    assert_eq!(
        body["residual-filter"]["lakecat:fetch-scan-tasks"]["requested-stats-fields"],
        serde_json::json!(["event_id"])
    );
    assert_eq!(
        body["residual-filter"]["lakecat:fetch-scan-tasks"]["effective-stats-fields"],
        serde_json::json!(["event_id"])
    );
    assert_eq!(
        body["residual-filter"]["lakecat:fetch-scan-tasks"]["stats-fields"],
        serde_json::json!(["event_id"])
    );
}
