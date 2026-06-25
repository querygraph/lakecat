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

#[test]
fn commit_history_graph_entries_require_commit_hashes() {
    let base_event = OutboxEvent {
        event_id: "evt-commit-history-helper".to_string(),
        sink: "lakecat.lineage-and-graph".to_string(),
        event_type: "table.commits-listed".to_string(),
        payload: json!({
            "payload": {
                "sequence-numbers": [1],
                "commit-hashes": [content_hash_json(&json!({"commit": 1})).unwrap()],
            },
        }),
        created_at: chrono::Utc::now(),
        delivered_at: None,
    };
    let entries = outbox_commit_history_entries(&base_event).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].0, 1);
    assert!(is_full_sha256_hash(&entries[0].1));

    let mut missing_hashes = base_event.clone();
    missing_hashes
        .payload
        .pointer_mut("/payload")
        .unwrap()
        .as_object_mut()
        .unwrap()
        .remove("commit-hashes");
    let err = outbox_commit_history_entries(&missing_hashes).unwrap_err();
    assert!(
        err.to_string()
            .contains("commit history payload is missing commit hashes"),
        "{err}"
    );

    let mut count_drift = base_event.clone();
    count_drift.payload["payload"]["commit-hashes"] = json!([]);
    let err = outbox_commit_history_entries(&count_drift).unwrap_err();
    assert!(
        err.to_string()
            .contains("commit hash count does not match sequence numbers"),
        "{err}"
    );

    let mut non_string_hash = base_event;
    non_string_hash.payload["payload"]["commit-hashes"] = json!([42]);
    let err = outbox_commit_history_entries(&non_string_hash).unwrap_err();
    assert!(
        err.to_string()
            .contains("commit history payload has a non-string commit hash"),
        "{err}"
    );
}

#[tokio::test]
async fn commit_table_accepts_bare_iceberg_rest_update_path() {
    // Iceberg REST `updateTable` is a bare POST on the table path, with no
    // `/commit` segment. A stock PyIceberg/Spark/Trino client commits there,
    // so LakeCat must accept it (the `/commit` route is a LakeCat alias).
    let app = test_app();
    let create = Request::builder()
        .method(Method::POST)
        .uri("/catalog/v1/namespaces/default/tables")
        .header("content-type", "application/json")
        .body(Body::from(
            r#"{"name":"events","location":"file:///tmp/events","metadata-location":"file:///tmp/events/metadata/00000.json","metadata":{"format-version":3,"table-uuid":"11111111-1111-1111-1111-111111111111","location":"file:///tmp/events","last-sequence-number":7,"last-updated-ms":1710000000000,"last-column-id":1,"schemas":[{"type":"struct","schema-id":1,"fields":[{"id":1,"name":"id","type":"string","required":true,"doc":"Event identifier."}]}],"current-schema-id":1,"partition-specs":[{"spec-id":0,"fields":[]}],"default-spec-id":0,"current-snapshot-id":42,"snapshots":[{"snapshot-id":42,"sequence-number":7,"timestamp-ms":1710000000000,"manifest-list":"file:///tmp/events/metadata/snap-42.avro","summary":{"operation":"append"},"schema-id":1}],"snapshot-log":[{"timestamp-ms":1710000000000,"snapshot-id":42}]}}"#,
        ))
        .unwrap();
    assert_eq!(
        app.clone().oneshot(create).await.unwrap().status(),
        StatusCode::OK
    );

    // Bare path (spec updateTable), not `.../events/commit`.
    let commit = Request::builder()
        .method(Method::POST)
        .uri("/catalog/v1/namespaces/default/tables/events")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"requirements":[],"updates":[]}"#))
        .unwrap();
    assert_eq!(
        app.clone().oneshot(commit).await.unwrap().status(),
        StatusCode::OK
    );
}

#[tokio::test]
async fn create_load_commit_and_plan_table_round_trips_through_integrations() {
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

    let load = Request::builder()
        .method(Method::GET)
        .uri("/catalog/v1/namespaces/default/tables/events")
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(load).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let commit = Request::builder()
        .method(Method::POST)
        .uri("/catalog/v1/namespaces/default/tables/events/commit")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"requirements":[],"updates":[]}"#))
        .unwrap();
    let response = app.clone().oneshot(commit).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let plan = Request::builder()
        .method(Method::POST)
        .uri("/catalog/v1/namespaces/default/tables/events/plan")
        .header("content-type", "application/json")
        .body(Body::from(
            r#"{"select":["id"],"filter":{"type":"always-true"},"case-sensitive":true,"limit":10}"#,
        ))
        .unwrap();
    let response = app.clone().oneshot(plan).await.unwrap();
    #[cfg(not(feature = "sail-local"))]
    assert_eq!(response.status(), StatusCode::NOT_IMPLEMENTED);
    #[cfg(feature = "sail-local")]
    {
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(payload["status"], serde_json::json!("completed"));
        let _: sail_catalog_iceberg::models::PlanTableScanRequest =
            serde_json::from_value(serde_json::json!({
                "select": ["id"],
                "filter": {"type": "always-true"},
                "case-sensitive": true
            }))
            .unwrap();
        assert_eq!(
            payload["residual-filter"]["lakecat:scan-request"]["case-sensitive"],
            serde_json::json!(true)
        );
        assert_eq!(
            payload["lakecat-plan-tasks"][0]["task-type"],
            serde_json::json!("manifest-list")
        );
        assert_eq!(
            payload["residual-filter"]["filters-accepted-by-sail"][0]["expression-type"],
            serde_json::json!("always-true")
        );
        let plan_task = payload["plan-tasks"][0]
            .as_str()
            .expect("plan task token")
            .to_string();

        let fetch = Request::builder()
            .method(Method::POST)
            .uri("/catalog/v1/namespaces/default/tables/events/fetch-scan-tasks")
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
        let _: sail_catalog_iceberg::models::FetchScanTasksResult =
            serde_json::from_value(payload.clone()).unwrap();
        assert_eq!(
            payload["residual-filter"]["lakecat:sail-target"],
            serde_json::json!("sail_iceberg::io::load_manifest_list")
        );
    }
}

#[tokio::test]
async fn commit_can_advance_metadata_location_extension() {
    let app = test_app();
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("lakecat-commit-metadata-{unique}"));
    let table_dir = root.join("events");
    let metadata_dir = table_dir.join("metadata");
    std::fs::create_dir_all(&metadata_dir).unwrap();
    let table_location = url::Url::from_directory_path(&table_dir)
        .expect("table dir URL")
        .to_string();
    let initial_metadata_location = url::Url::from_file_path(metadata_dir.join("00000.json"))
        .unwrap()
        .to_string();
    let committed_metadata_location = url::Url::from_file_path(metadata_dir.join("00001.json"))
        .unwrap()
        .to_string();
    let new_metadata = serde_json::json!({
        "format-version": 3,
        "table-uuid": "11111111-1111-1111-1111-111111111111",
        "location": table_location,
        "last-sequence-number": 8,
        "last-updated-ms": 1710000000100_i64,
        "last-column-id": 1,
        "schemas": [{
            "type": "struct",
            "schema-id": 1,
            "fields": [{
                "id": 1,
                "name": "id",
                "type": "string",
                "required": true,
                "doc": "Event identifier."
            }]
        }],
        "current-schema-id": 1,
        "partition-specs": [{"spec-id": 0, "fields": []}],
        "default-spec-id": 0,
        "current-snapshot-id": 43,
        "snapshots": [{
            "snapshot-id": 43,
            "sequence-number": 8,
            "timestamp-ms": 1710000000100_i64,
            "summary": {"operation": "append"},
            "schema-id": 1
        }],
        "snapshot-log": [{"timestamp-ms": 1710000000100_i64, "snapshot-id": 43}]
    });
    let create = Request::builder()
        .method(Method::POST)
        .uri("/catalog/v1/namespaces/default/tables")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::json!({
                "name": "events",
                "location": table_location,
                "metadata-location": initial_metadata_location,
                "metadata": {
                    "format-version": 3,
                    "table-uuid": "11111111-1111-1111-1111-111111111111",
                    "location": table_location,
                    "last-sequence-number": 7,
                    "last-updated-ms": 1710000000000_i64,
                    "last-column-id": 1,
                    "schemas": [{
                        "type": "struct",
                        "schema-id": 1,
                        "fields": [{
                            "id": 1,
                            "name": "id",
                            "type": "string",
                            "required": true,
                            "doc": "Event identifier."
                        }]
                    }],
                    "current-schema-id": 1,
                    "partition-specs": [{"spec-id": 0, "fields": []}],
                    "default-spec-id": 0,
                    "current-snapshot-id": 42,
                    "snapshots": [{
                        "snapshot-id": 42,
                        "sequence-number": 7,
                        "timestamp-ms": 1710000000000_i64,
                        "summary": {"operation": "append"},
                        "schema-id": 1
                    }],
                    "snapshot-log": [{"timestamp-ms": 1710000000000_i64, "snapshot-id": 42}]
                }
            })
            .to_string(),
        ))
        .unwrap();
    let response = app.clone().oneshot(create).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let commit = Request::builder()
        .method(Method::POST)
        .uri("/catalog/v1/namespaces/default/tables/events/commit")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::json!({
                "requirements": [],
                "updates": [],
                "metadata-location": committed_metadata_location,
                "metadata": new_metadata,
            })
            .to_string(),
        ))
        .unwrap();
    let response = app.clone().oneshot(commit).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(
        payload["metadata-location"],
        serde_json::json!(committed_metadata_location)
    );
    let written_metadata: serde_json::Value =
        serde_json::from_slice(&std::fs::read(metadata_dir.join("00001.json")).unwrap()).unwrap();
    assert_eq!(
        written_metadata["current-snapshot-id"],
        serde_json::json!(43)
    );

    let load = Request::builder()
        .method(Method::GET)
        .uri("/catalog/v1/namespaces/default/tables/events")
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(load).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(
        payload["metadata-location"],
        serde_json::json!(committed_metadata_location)
    );
    assert_eq!(
        payload["metadata"]["current-snapshot-id"],
        serde_json::json!(43)
    );
    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test]
async fn commit_replays_rest_idempotency_key() {
    let store = MemoryCatalogStore::new();
    let app = app(LakeCatState::new(
        WarehouseName::new("local").unwrap(),
        store.clone(),
    ));
    let create = Request::builder()
        .method(Method::POST)
        .uri("/catalog/v1/namespaces/default/tables")
        .header("content-type", "application/json")
        .body(Body::from(
            r#"{"name":"events","location":"file:///tmp/events","metadata-location":"file:///tmp/events/metadata/00000.json","metadata":{"format-version":3,"table-uuid":"11111111-1111-1111-1111-111111111111","location":"file:///tmp/events","last-sequence-number":7,"last-updated-ms":1710000000000,"last-column-id":1,"schemas":[{"type":"struct","schema-id":1,"fields":[{"id":1,"name":"id","type":"string","required":true}]}],"current-schema-id":1,"partition-specs":[{"spec-id":0,"fields":[]}],"default-spec-id":0,"current-snapshot-id":42,"snapshots":[{"snapshot-id":42,"sequence-number":7,"timestamp-ms":1710000000000,"summary":{"operation":"append"},"schema-id":1}],"snapshot-log":[{"timestamp-ms":1710000000000,"snapshot-id":42}]}}"#,
        ))
        .unwrap();
    let response = app.clone().oneshot(create).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    for _ in 0..2 {
        let commit = Request::builder()
            .method(Method::POST)
            .uri("/catalog/v1/namespaces/default/tables/events/commit")
            .header("content-type", "application/json")
            .header("x-lakecat-idempotency-key", "commit:events:0001")
            .body(Body::from(r#"{"requirements":[],"updates":[]}"#))
            .unwrap();
        let response = app.clone().oneshot(commit).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(
            payload["metadata-location"],
            serde_json::json!("file:///tmp/events/metadata/00000.json")
        );
    }

    let mismatched_commit = Request::builder()
        .method(Method::POST)
        .uri("/catalog/v1/namespaces/default/tables/events/commit")
        .header("content-type", "application/json")
        .header("x-lakecat-idempotency-key", "commit:events:0001")
        .body(Body::from(
            r#"{"requirements":[],"updates":[],"metadata-location":"file:///tmp/events/metadata/00001.json"}"#,
        ))
        .unwrap();
    let response = app.clone().oneshot(mismatched_commit).await.unwrap();
    assert_eq!(response.status(), StatusCode::CONFLICT);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let message = payload["error"]["message"].as_str().unwrap();
    assert!(message.contains("idempotency key reused with different commit request"));
    assert!(!message.contains("commit:events:0001"));
    assert!(!message.contains("00001.json"));
    assert!(!message.contains("file:///tmp/events/metadata/00001.json"));

    let ident = table_ident("local", "default".to_string(), "events".to_string()).unwrap();
    let records = store.table_commit_records(&ident, 0, None).await.unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].sequence_number, 1);
    assert_eq!(
        records[0].idempotency_key_sha256.as_deref(),
        Some(content_hash_bytes("commit:events:0001".as_bytes()).as_str())
    );
    let pending = store
        .pending_outbox_events(Some("lakecat.lineage-and-graph"), 10)
        .await
        .unwrap();
    let commit_outbox_count = pending
        .iter()
        .filter(|event| event.event_type == "table.commit")
        .count();
    assert_eq!(
        commit_outbox_count, 1,
        "idempotent replay and mismatch conflicts must not enqueue extra commit outbox events"
    );
    assert_eq!(store.load_table(&ident).await.unwrap().version, 1);
}

#[tokio::test]
async fn commit_replays_standard_rest_idempotency_key() {
    let store = MemoryCatalogStore::new();
    let app = app(LakeCatState::new(
        WarehouseName::new("local").unwrap(),
        store.clone(),
    ));
    let create = Request::builder()
        .method(Method::POST)
        .uri("/catalog/v1/namespaces/default/tables")
        .header("content-type", "application/json")
        .body(Body::from(
            r#"{"name":"events","location":"file:///tmp/events","metadata-location":"file:///tmp/events/metadata/00000.json","metadata":{"format-version":3,"table-uuid":"11111111-1111-1111-1111-111111111111","location":"file:///tmp/events","last-sequence-number":7,"last-updated-ms":1710000000000,"last-column-id":1,"schemas":[{"type":"struct","schema-id":1,"fields":[{"id":1,"name":"id","type":"string","required":true}]}],"current-schema-id":1,"partition-specs":[{"spec-id":0,"fields":[]}],"default-spec-id":0,"current-snapshot-id":42,"snapshots":[{"snapshot-id":42,"sequence-number":7,"timestamp-ms":1710000000000,"summary":{"operation":"append"},"schema-id":1}],"snapshot-log":[{"timestamp-ms":1710000000000,"snapshot-id":42}]}}"#,
        ))
        .unwrap();
    let response = app.clone().oneshot(create).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    for _ in 0..2 {
        let commit = Request::builder()
            .method(Method::POST)
            .uri("/catalog/v1/namespaces/default/tables/events/commit")
            .header("content-type", "application/json")
            .header("Idempotency-Key", "commit:events:standard")
            .body(Body::from(r#"{"requirements":[],"updates":[]}"#))
            .unwrap();
        let response = app.clone().oneshot(commit).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(
            payload["metadata-location"],
            serde_json::json!("file:///tmp/events/metadata/00000.json")
        );
    }

    let ident = table_ident("local", "default".to_string(), "events".to_string()).unwrap();
    let records = store.table_commit_records(&ident, 0, None).await.unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(
        records[0].idempotency_key_sha256.as_deref(),
        Some(content_hash_bytes("commit:events:standard".as_bytes()).as_str())
    );
    let pending = store
        .pending_outbox_events(Some("lakecat.lineage-and-graph"), 10)
        .await
        .unwrap();
    assert_eq!(
        pending
            .iter()
            .filter(|event| event.event_type == "table.commit")
            .count(),
        1,
        "standard Idempotency-Key replay must not enqueue extra commit outbox events"
    );
}

#[tokio::test]
async fn commit_accepts_matching_standard_and_lakecat_idempotency_headers() {
    let store = MemoryCatalogStore::new();
    let app = app(LakeCatState::new(
        WarehouseName::new("local").unwrap(),
        store.clone(),
    ));
    let create = Request::builder()
        .method(Method::POST)
        .uri("/catalog/v1/namespaces/default/tables")
        .header("content-type", "application/json")
        .body(Body::from(
            r#"{"name":"events","location":"file:///tmp/events","metadata-location":"file:///tmp/events/metadata/00000.json","metadata":{"format-version":3,"table-uuid":"11111111-1111-1111-1111-111111111111","location":"file:///tmp/events","last-sequence-number":7,"last-updated-ms":1710000000000,"last-column-id":1,"schemas":[{"type":"struct","schema-id":1,"fields":[{"id":1,"name":"id","type":"string","required":true}]}],"current-schema-id":1,"partition-specs":[{"spec-id":0,"fields":[]}],"default-spec-id":0,"current-snapshot-id":42,"snapshots":[{"snapshot-id":42,"sequence-number":7,"timestamp-ms":1710000000000,"summary":{"operation":"append"},"schema-id":1}],"snapshot-log":[{"timestamp-ms":1710000000000,"snapshot-id":42}]}}"#,
        ))
        .unwrap();
    let response = app.clone().oneshot(create).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let commit = Request::builder()
        .method(Method::POST)
        .uri("/catalog/v1/namespaces/default/tables/events/commit")
        .header("content-type", "application/json")
        .header("Idempotency-Key", "commit:events:dual")
        .header("x-lakecat-idempotency-key", "commit:events:dual")
        .body(Body::from(r#"{"requirements":[],"updates":[]}"#))
        .unwrap();
    let response = app.clone().oneshot(commit).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let ident = table_ident("local", "default".to_string(), "events".to_string()).unwrap();
    let records = store.table_commit_records(&ident, 0, None).await.unwrap();
    assert_eq!(
        records[0].idempotency_key_sha256.as_deref(),
        Some(content_hash_bytes("commit:events:dual".as_bytes()).as_str())
    );
}

#[tokio::test]
async fn commit_without_rest_idempotency_key_still_drains_replay_evidence() {
    let store = MemoryCatalogStore::new();
    let graph = Arc::new(RecordingGraph::default());
    let lineage = Arc::new(RecordingLineage::default());
    let app = app(
        LakeCatState::new(WarehouseName::new("local").unwrap(), store.clone()).with_integrations(
            default_sail_engine(),
            AllowAllGovernanceEngine::new(),
            graph.clone(),
            lineage.clone(),
        ),
    );
    let create = Request::builder()
        .method(Method::POST)
        .uri("/catalog/v1/namespaces/default/tables")
        .header("content-type", "application/json")
        .body(Body::from(
            r#"{"name":"events","location":"file:///tmp/events","metadata-location":"file:///tmp/events/metadata/00000.json","metadata":{"format-version":3,"table-uuid":"11111111-1111-1111-1111-111111111111","location":"file:///tmp/events","last-sequence-number":7,"last-updated-ms":1710000000000,"last-column-id":1,"schemas":[{"type":"struct","schema-id":1,"fields":[{"id":1,"name":"id","type":"string","required":true}]}],"current-schema-id":1,"partition-specs":[{"spec-id":0,"fields":[]}],"default-spec-id":0,"current-snapshot-id":42,"snapshots":[{"snapshot-id":42,"sequence-number":7,"timestamp-ms":1710000000000,"summary":{"operation":"append"},"schema-id":1}],"snapshot-log":[{"timestamp-ms":1710000000000,"snapshot-id":42}]}}"#,
        ))
        .unwrap();
    let response = app.clone().oneshot(create).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let commit = Request::builder()
        .method(Method::POST)
        .uri("/catalog/v1/namespaces/default/tables/events/commit")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"requirements":[],"updates":[]}"#))
        .unwrap();
    let response = app.clone().oneshot(commit).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let ident = table_ident("local", "default".to_string(), "events".to_string()).unwrap();
    let records = store.table_commit_records(&ident, 0, None).await.unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].idempotency_key_sha256, None);

    let drain = Request::builder()
        .method(Method::POST)
        .uri("/management/v1/lineage/drain")
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(drain).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(
        payload["event-types"]
            .as_array()
            .unwrap()
            .iter()
            .any(|event_type| event_type == "table.commit")
    );
    let summaries = payload["events"].as_array().unwrap();
    let commit_summary = summaries
        .iter()
        .find(|event| event["event-type"] == "table.commit")
        .expect("standard commit should be included in drained replay");
    assert_eq!(commit_summary["lineage-events"], serde_json::json!(1));
    assert!(
        graph.events.lock().await.iter().any(|event| {
            event.label == GraphNodeLabel::Commit && event.action == GraphAction::Committed
        }),
        "standard no-idempotency commits must still project commit graph evidence"
    );
    assert!(
        lineage
            .events
            .lock()
            .await
            .iter()
            .any(|event| event.event_type == LineageEventType::TableCommitted),
        "standard no-idempotency commits must still project OpenLineage evidence"
    );
}

#[tokio::test]
async fn commit_rejects_invalid_rest_idempotency_keys() {
    let sail = Arc::new(RecordingSailEngine::default());
    let governance = Arc::new(RecordingGovernance::default());
    let store = MemoryCatalogStore::new();
    let app = app(
        LakeCatState::new(WarehouseName::new("local").unwrap(), store.clone()).with_integrations(
            sail.clone(),
            governance.clone(),
            NoopCatalogGraphSink::new(),
            Arc::new(RecordingLineage::default()),
        ),
    );
    let cases = vec![
        (
            HeaderValue::from_static("commit events 0001"),
            "x-lakecat-idempotency-key may only contain",
        ),
        (
            HeaderValue::from_str("x".repeat(129).as_str()).unwrap(),
            "x-lakecat-idempotency-key must be 1..=128 ASCII characters",
        ),
        (
            HeaderValue::from_bytes("commit:é".as_bytes()).unwrap(),
            "x-lakecat-idempotency-key must be 1..=128 ASCII characters",
        ),
        (
            HeaderValue::from_bytes(b"commit:\xff").unwrap(),
            "x-lakecat-idempotency-key must be 1..=128 ASCII characters",
        ),
    ];

    for (key, expected_message) in cases {
        let commit = Request::builder()
            .method(Method::POST)
            .uri("/catalog/v1/namespaces/default/tables/events/commit")
            .header("content-type", "application/json")
            .header("x-lakecat-idempotency-key", key)
            .body(Body::from(r#"{"requirements":[],"updates":[]}"#))
            .unwrap();
        let response = app.clone().oneshot(commit).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let message = String::from_utf8_lossy(&body);
        assert!(message.contains(expected_message), "{message}");
    }

    let duplicate = Request::builder()
        .method(Method::POST)
        .uri("/catalog/v1/namespaces/default/tables/events/commit")
        .header("content-type", "application/json")
        .header("x-lakecat-idempotency-key", "commit:events:0001")
        .header("x-lakecat-idempotency-key", "commit:events:0002")
        .body(Body::from(r#"{"requirements":[],"updates":[]}"#))
        .unwrap();
    let response = app.clone().oneshot(duplicate).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let message = String::from_utf8_lossy(&body);
    assert!(
        message.contains("x-lakecat-idempotency-key must appear at most once"),
        "{message}"
    );
    assert!(!message.contains("commit:events:0001"));
    assert!(!message.contains("commit:events:0002"));

    let duplicate_standard = Request::builder()
        .method(Method::POST)
        .uri("/catalog/v1/namespaces/default/tables/events/commit")
        .header("content-type", "application/json")
        .header("Idempotency-Key", "commit:events:0001")
        .header("Idempotency-Key", "commit:events:0002")
        .body(Body::from(r#"{"requirements":[],"updates":[]}"#))
        .unwrap();
    let response = app.clone().oneshot(duplicate_standard).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let message = String::from_utf8_lossy(&body);
    assert!(
        message.contains("idempotency-key must appear at most once"),
        "{message}"
    );
    assert!(!message.contains("commit:events:0001"));
    assert!(!message.contains("commit:events:0002"));

    let conflicting_headers = Request::builder()
        .method(Method::POST)
        .uri("/catalog/v1/namespaces/default/tables/events/commit")
        .header("content-type", "application/json")
        .header("Idempotency-Key", "commit:events:0001")
        .header("x-lakecat-idempotency-key", "commit:events:0002")
        .body(Body::from(r#"{"requirements":[],"updates":[]}"#))
        .unwrap();
    let response = app.clone().oneshot(conflicting_headers).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let message = String::from_utf8_lossy(&body);
    assert!(
        message.contains(
            "Idempotency-Key and x-lakecat-idempotency-key must match when both are present"
        ),
        "{message}"
    );
    assert!(!message.contains("commit:events:0001"));
    assert!(!message.contains("commit:events:0002"));

    assert_eq!(*sail.commit_prepare_count.lock().await, 0);
    assert!(
        governance.principals.lock().await.is_empty(),
        "invalid idempotency keys must fail before authorization"
    );
    let pending = store
        .pending_outbox_events(Some("lakecat.lineage-and-graph"), 10)
        .await
        .unwrap();
    assert!(
        pending.is_empty(),
        "invalid idempotency keys must fail before durable outbox side effects"
    );
}

#[tokio::test]
async fn management_table_commits_lists_pointer_log_evidence() {
    let store = MemoryCatalogStore::new();
    let graph = Arc::new(RecordingGraph::default());
    let lineage = Arc::new(RecordingLineage::default());
    let app = app(
        LakeCatState::new(WarehouseName::new("local").unwrap(), store.clone()).with_integrations(
            default_sail_engine(),
            AllowAllGovernanceEngine::new(),
            graph.clone(),
            lineage,
        ),
    );
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("lakecat-commit-history-{unique}"));
    let table_dir = root.join("events");
    let metadata_dir = table_dir.join("metadata");
    std::fs::create_dir_all(&metadata_dir).unwrap();
    let table_location = url::Url::from_directory_path(&table_dir)
        .expect("table dir URL")
        .to_string();
    let initial_metadata_location = url::Url::from_file_path(metadata_dir.join("00000.json"))
        .unwrap()
        .to_string();
    let committed_metadata_location = url::Url::from_file_path(metadata_dir.join("00001.json"))
        .unwrap()
        .to_string();
    let base_metadata = serde_json::json!({
        "format-version": 3,
        "table-uuid": "11111111-1111-1111-1111-111111111111",
        "location": table_location,
        "last-sequence-number": 7,
        "last-updated-ms": 1710000000000_i64,
        "last-column-id": 1,
        "schemas": [{
            "type": "struct",
            "schema-id": 1,
            "fields": [{
                "id": 1,
                "name": "id",
                "type": "string",
                "required": true
            }]
        }],
        "current-schema-id": 1,
        "partition-specs": [{"spec-id": 0, "fields": []}],
        "default-spec-id": 0,
        "current-snapshot-id": 42,
        "snapshots": [{
            "snapshot-id": 42,
            "sequence-number": 7,
            "timestamp-ms": 1710000000000_i64,
            "summary": {"operation": "append"},
            "schema-id": 1
        }],
        "snapshot-log": [{"timestamp-ms": 1710000000000_i64, "snapshot-id": 42}]
    });
    let advanced_metadata = serde_json::json!({
        "format-version": 3,
        "table-uuid": "11111111-1111-1111-1111-111111111111",
        "location": table_location,
        "last-sequence-number": 8,
        "last-updated-ms": 1710000000100_i64,
        "last-column-id": 1,
        "schemas": [{
            "type": "struct",
            "schema-id": 1,
            "fields": [{
                "id": 1,
                "name": "id",
                "type": "string",
                "required": true
            }]
        }],
        "current-schema-id": 1,
        "partition-specs": [{"spec-id": 0, "fields": []}],
        "default-spec-id": 0,
        "current-snapshot-id": 43,
        "snapshots": [{
            "snapshot-id": 43,
            "sequence-number": 8,
            "timestamp-ms": 1710000000100_i64,
            "summary": {"operation": "append"},
            "schema-id": 1
        }],
        "snapshot-log": [{"timestamp-ms": 1710000000100_i64, "snapshot-id": 43}]
    });
    let create = Request::builder()
        .method(Method::POST)
        .uri("/catalog/v1/namespaces/default/tables")
        .header("content-type", "application/json")
        .header("x-lakecat-principal", "operator@example.com")
        .body(Body::from(
            serde_json::json!({
                "name": "events",
                "location": table_location,
                "metadata-location": initial_metadata_location,
                "metadata": base_metadata,
            })
            .to_string(),
        ))
        .unwrap();
    let response = app.clone().oneshot(create).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let commit = Request::builder()
        .method(Method::POST)
        .uri("/catalog/v1/namespaces/default/tables/events/commit")
        .header("content-type", "application/json")
        .header("x-lakecat-principal", "operator@example.com")
        .header("x-lakecat-idempotency-key", "commit:events:history")
        .body(Body::from(
            serde_json::json!({
                "requirements": [],
                "updates": [],
                "metadata-location": committed_metadata_location,
                "metadata": advanced_metadata,
            })
            .to_string(),
        ))
        .unwrap();
    let response = app.clone().oneshot(commit).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let list = Request::builder()
        .method(Method::GET)
        .uri("/management/v1/warehouses/local/namespaces/default/tables/events/commits")
        .header("x-lakecat-principal", "operator@example.com")
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(list).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let commits = body["commits"].as_array().unwrap();
    assert_eq!(commits.len(), 1);
    assert_eq!(commits[0]["warehouse"], serde_json::json!("local"));
    assert_eq!(commits[0]["namespace"], serde_json::json!(["default"]));
    assert_eq!(commits[0]["table"], serde_json::json!("events"));
    assert_eq!(commits[0]["sequence-number"], serde_json::json!(1));
    assert_eq!(commits[0]["format-version"], serde_json::json!(3));
    assert_eq!(commits[0]["snapshot-id"], serde_json::json!(43));
    assert!(
        commits[0]["request-hash"]
            .as_str()
            .is_some_and(is_full_sha256_hash)
    );
    assert!(
        commits[0]["response-hash"]
            .as_str()
            .is_some_and(is_full_sha256_hash)
    );
    assert!(
        commits[0]["commit-hash"]
            .as_str()
            .is_some_and(is_full_sha256_hash)
    );
    assert_eq!(
        commits[0]["idempotency-key-sha256"],
        serde_json::json!(content_hash_bytes("commit:events:history".as_bytes()))
    );
    assert!(
        commits[0]["idempotency-key-sha256"]
            .as_str()
            .is_some_and(is_full_sha256_hash)
    );
    assert_eq!(
        commits[0]["principal-subject"],
        serde_json::json!("operator@example.com")
    );

    let pending = store
        .pending_outbox_events(Some("lakecat.lineage-and-graph"), 10)
        .await
        .unwrap();
    let commit_read = pending
        .iter()
        .find(|event| event.event_type == "table.commits-listed")
        .expect("commit history read should enter the durable outbox");
    let commit_read_payload = &commit_read.payload["payload"];
    assert_eq!(commit_read_payload["commit-count"], serde_json::json!(1));
    assert_eq!(
        commit_read_payload["commit-hashes"][0],
        commits[0]["commit-hash"]
    );
    assert!(
        commit_read_payload["commit-hashes"][0]
            .as_str()
            .is_some_and(is_full_sha256_hash)
    );
    assert_eq!(
        commit_read_payload["sequence-numbers"],
        serde_json::json!([1])
    );
    assert_eq!(
        commit_read_payload["principal-subject"],
        serde_json::json!("operator@example.com")
    );
    assert_eq!(
        commit_read_payload["principal-kind"],
        serde_json::json!("human")
    );

    let drain = Request::builder()
        .method(Method::POST)
        .uri("/management/v1/lineage/drain")
        .header("x-lakecat-principal", "operator@example.com")
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(drain).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(
        body["event-types"]
            .as_array()
            .unwrap()
            .iter()
            .any(|event_type| event_type == "table.commits-listed")
    );
    let commit_read_summary = body["events"]
        .as_array()
        .unwrap()
        .iter()
        .find(|event| event["event-type"] == "table.commits-listed")
        .expect("drain should summarize the commit history read");
    assert_eq!(commit_read_summary["graph-events"], serde_json::json!(2));
    assert_eq!(commit_read_summary["lineage-events"], serde_json::json!(1));
    assert_eq!(
        commit_read_summary["table-commit-count"],
        serde_json::json!(1)
    );
    assert_eq!(
        commit_read_summary["table-commit-sequence-numbers"],
        serde_json::json!([1])
    );
    assert_eq!(
        commit_read_summary["table-commit-hashes"],
        serde_json::json!([commits[0]["commit-hash"].clone()])
    );
    assert!(
        commit_read_summary["table-commit-hashes"][0]
            .as_str()
            .is_some_and(is_full_sha256_hash)
    );
    let graph_events = graph.events.lock().await;
    assert!(
        graph_events.iter().any(|event| {
            event.label == GraphNodeLabel::Commit && event.action == GraphAction::Committed
        }),
        "drain should also project the original table.commit event"
    );
    let commit_graph_event = graph_events
        .iter()
        .find(|event| event.label == GraphNodeLabel::Commit && event.action == GraphAction::Loaded)
        .expect("commit history read should project a loaded Commit graph event");
    assert_eq!(commit_graph_event.action, GraphAction::Loaded);
    assert_eq!(
        commit_graph_event.subject,
        "lakecat:commit:lakecat:table:local:default:events:1"
    );
    assert_eq!(
        commit_graph_event.event_id.as_deref(),
        Some(format!("{}:commit-history:1", commit_read.event_id).as_str())
    );
    assert_eq!(
        commit_graph_event.properties["commit-hash"],
        commits[0]["commit-hash"]
    );
    assert!(
        commit_graph_event.properties["commit-hash"]
            .as_str()
            .is_some_and(is_full_sha256_hash)
    );
    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test]
async fn management_table_commits_empty_history_still_drains_zero_count_proof() {
    let store = MemoryCatalogStore::new();
    let graph = Arc::new(RecordingGraph::default());
    let lineage = Arc::new(RecordingLineage::default());
    let app = app(
        LakeCatState::new(WarehouseName::new("local").unwrap(), store.clone()).with_integrations(
            default_sail_engine(),
            AllowAllGovernanceEngine::new(),
            graph.clone(),
            lineage,
        ),
    );
    let create = Request::builder()
        .method(Method::POST)
        .uri("/catalog/v1/namespaces/default/tables")
        .header("content-type", "application/json")
        .header("x-lakecat-principal", "operator@example.com")
        .body(Body::from(
            r#"{"name":"events","location":"file:///tmp/events","metadata-location":"file:///tmp/events/metadata/00000.json","metadata":{"format-version":3,"table-uuid":"11111111-1111-1111-1111-111111111111","location":"file:///tmp/events","last-sequence-number":7,"last-updated-ms":1710000000000,"last-column-id":1,"schemas":[{"type":"struct","schema-id":1,"fields":[{"id":1,"name":"id","type":"string","required":true}]}],"current-schema-id":1,"partition-specs":[{"spec-id":0,"fields":[]}],"default-spec-id":0,"current-snapshot-id":42,"snapshots":[{"snapshot-id":42,"sequence-number":7,"timestamp-ms":1710000000000,"summary":{"operation":"append"},"schema-id":1}],"snapshot-log":[{"timestamp-ms":1710000000000,"snapshot-id":42}]}}"#,
        ))
        .unwrap();
    let response = app.clone().oneshot(create).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let list = Request::builder()
        .method(Method::GET)
        .uri("/management/v1/warehouses/local/namespaces/default/tables/events/commits")
        .header("x-lakecat-principal", "operator@example.com")
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(list).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(body["commits"], serde_json::json!([]));

    let pending = store
        .pending_outbox_events(Some("lakecat.lineage-and-graph"), 10)
        .await
        .unwrap();
    let commit_read = pending
        .iter()
        .find(|event| event.event_type == "table.commits-listed")
        .expect("empty commit history read should enter the durable outbox");
    let commit_read_payload = &commit_read.payload["payload"];
    assert_eq!(commit_read_payload["commit-count"], serde_json::json!(0));
    assert_eq!(commit_read_payload["commit-hashes"], serde_json::json!([]));
    assert_eq!(
        commit_read_payload["sequence-numbers"],
        serde_json::json!([])
    );
    assert_eq!(
        commit_read_payload["principal-subject"],
        serde_json::json!("operator@example.com")
    );
    assert_eq!(
        commit_read_payload["principal-kind"],
        serde_json::json!("human")
    );

    let drain = Request::builder()
        .method(Method::POST)
        .uri("/management/v1/lineage/drain")
        .header("x-lakecat-principal", "operator@example.com")
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(drain).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let commit_read_summary = body["events"]
        .as_array()
        .unwrap()
        .iter()
        .find(|event| event["event-type"] == "table.commits-listed")
        .expect("drain should summarize the empty commit history read");
    assert_eq!(commit_read_summary["graph-events"], serde_json::json!(1));
    assert_eq!(commit_read_summary["lineage-events"], serde_json::json!(1));
    assert_eq!(
        commit_read_summary["table-commit-count"],
        serde_json::json!(0)
    );
    assert_eq!(
        commit_read_summary["table-commit-sequence-numbers"],
        serde_json::json!([])
    );
    assert_eq!(
        commit_read_summary["table-commit-hashes"],
        serde_json::json!([])
    );
    assert!(
        graph.events.lock().await.iter().all(|event| {
            event.label != GraphNodeLabel::Commit || event.action != GraphAction::Loaded
        }),
        "empty commit history reads should not fabricate commit graph nodes"
    );
}

#[tokio::test]
async fn idempotent_commit_replay_does_not_rewrite_metadata_object() {
    let store = MemoryCatalogStore::new();
    let app = app(LakeCatState::new(
        WarehouseName::new("local").unwrap(),
        store.clone(),
    ));
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("lakecat-idempotent-object-{unique}"));
    let table_dir = root.join("events");
    let metadata_dir = table_dir.join("metadata");
    std::fs::create_dir_all(&metadata_dir).unwrap();
    let table_location = url::Url::from_directory_path(&table_dir)
        .expect("table dir URL")
        .to_string();
    let initial_metadata_location = url::Url::from_file_path(metadata_dir.join("00000.json"))
        .unwrap()
        .to_string();
    let committed_metadata_path = metadata_dir.join("00001.json");
    let committed_metadata_location = url::Url::from_file_path(&committed_metadata_path)
        .unwrap()
        .to_string();
    let base_metadata = serde_json::json!({
        "format-version": 3,
        "table-uuid": "11111111-1111-1111-1111-111111111111",
        "location": table_location,
        "last-sequence-number": 7,
        "last-updated-ms": 1710000000000_i64,
        "last-column-id": 1,
        "schemas": [{
            "type": "struct",
            "schema-id": 1,
            "fields": [{
                "id": 1,
                "name": "id",
                "type": "string",
                "required": true
            }]
        }],
        "current-schema-id": 1,
        "partition-specs": [{"spec-id": 0, "fields": []}],
        "default-spec-id": 0,
        "current-snapshot-id": 42,
        "snapshots": [{
            "snapshot-id": 42,
            "sequence-number": 7,
            "timestamp-ms": 1710000000000_i64,
            "summary": {"operation": "append"},
            "schema-id": 1
        }],
        "snapshot-log": [{"timestamp-ms": 1710000000000_i64, "snapshot-id": 42}]
    });
    let advanced_metadata = serde_json::json!({
        "format-version": 3,
        "table-uuid": "11111111-1111-1111-1111-111111111111",
        "location": table_location,
        "last-sequence-number": 8,
        "last-updated-ms": 1710000000100_i64,
        "last-column-id": 1,
        "schemas": [{
            "type": "struct",
            "schema-id": 1,
            "fields": [{
                "id": 1,
                "name": "id",
                "type": "string",
                "required": true
            }]
        }],
        "current-schema-id": 1,
        "partition-specs": [{"spec-id": 0, "fields": []}],
        "default-spec-id": 0,
        "current-snapshot-id": 43,
        "snapshots": [{
            "snapshot-id": 43,
            "sequence-number": 8,
            "timestamp-ms": 1710000000100_i64,
            "summary": {"operation": "append"},
            "schema-id": 1
        }],
        "snapshot-log": [{"timestamp-ms": 1710000000100_i64, "snapshot-id": 43}]
    });
    let create = Request::builder()
        .method(Method::POST)
        .uri("/catalog/v1/namespaces/default/tables")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::json!({
                "name": "events",
                "location": table_location,
                "metadata-location": initial_metadata_location,
                "metadata": base_metadata,
            })
            .to_string(),
        ))
        .unwrap();
    let response = app.clone().oneshot(create).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let commit_body = serde_json::json!({
        "requirements": [],
        "updates": [],
        "metadata-location": committed_metadata_location,
        "metadata": advanced_metadata,
    })
    .to_string();
    let commit = || {
        Request::builder()
            .method(Method::POST)
            .uri("/catalog/v1/namespaces/default/tables/events/commit")
            .header("content-type", "application/json")
            .header("x-lakecat-idempotency-key", "commit:events:metadata-object")
            .body(Body::from(commit_body.clone()))
            .unwrap()
    };
    let response = app.clone().oneshot(commit()).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let original_written = std::fs::read_to_string(&committed_metadata_path).unwrap();
    assert!(original_written.contains("\"current-snapshot-id\": 43"));

    let sentinel = "{\n  \"sentinel\": \"replay must not rewrite metadata\"\n}\n";
    std::fs::write(&committed_metadata_path, sentinel).unwrap();
    let response = app.clone().oneshot(commit()).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(
        payload["metadata-location"],
        serde_json::json!(committed_metadata_location)
    );
    assert_eq!(
        std::fs::read_to_string(&committed_metadata_path).unwrap(),
        sentinel
    );

    let ident = table_ident("local", "default".to_string(), "events".to_string()).unwrap();
    let records = store.table_commit_records(&ident, 0, None).await.unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(store.load_table(&ident).await.unwrap().version, 1);
    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test]
async fn commit_rejects_metadata_object_overwrite_of_current_pointer() {
    let app = test_app();
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("lakecat-current-metadata-guard-{unique}"));
    let table_dir = root.join("events");
    let metadata_dir = table_dir.join("metadata");
    std::fs::create_dir_all(&metadata_dir).unwrap();
    let table_location = url::Url::from_directory_path(&table_dir)
        .expect("table dir URL")
        .to_string();
    let initial_metadata_path = metadata_dir.join("00000.json");
    let initial_metadata_location = url::Url::from_file_path(&initial_metadata_path)
        .unwrap()
        .to_string();
    let base_metadata = serde_json::json!({
        "format-version": 3,
        "table-uuid": "11111111-1111-1111-1111-111111111111",
        "location": table_location,
        "last-sequence-number": 7,
        "last-updated-ms": 1710000000000_i64,
        "last-column-id": 1,
        "schemas": [{
            "type": "struct",
            "schema-id": 1,
            "fields": [{
                "id": 1,
                "name": "id",
                "type": "string",
                "required": true
            }]
        }],
        "current-schema-id": 1,
        "partition-specs": [{"spec-id": 0, "fields": []}],
        "default-spec-id": 0,
        "current-snapshot-id": 42,
        "snapshots": [{
            "snapshot-id": 42,
            "sequence-number": 7,
            "timestamp-ms": 1710000000000_i64,
            "summary": {"operation": "append"},
            "schema-id": 1
        }],
        "snapshot-log": [{"timestamp-ms": 1710000000000_i64, "snapshot-id": 42}]
    });
    let create = Request::builder()
        .method(Method::POST)
        .uri("/catalog/v1/namespaces/default/tables")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::json!({
                "name": "events",
                "location": table_location,
                "metadata-location": initial_metadata_location,
                "metadata": base_metadata,
            })
            .to_string(),
        ))
        .unwrap();
    let response = app.clone().oneshot(create).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let sentinel = "{\n  \"sentinel\": \"current metadata must not be overwritten\"\n}\n";
    std::fs::write(&initial_metadata_path, sentinel).unwrap();
    let overwrite_metadata = serde_json::json!({
        "format-version": 3,
        "table-uuid": "11111111-1111-1111-1111-111111111111",
        "location": table_location,
        "last-sequence-number": 8,
        "last-updated-ms": 1710000000100_i64,
        "last-column-id": 1,
        "schemas": [{
            "type": "struct",
            "schema-id": 1,
            "fields": [{
                "id": 1,
                "name": "id",
                "type": "string",
                "required": true
            }]
        }],
        "current-schema-id": 1,
        "partition-specs": [{"spec-id": 0, "fields": []}],
        "default-spec-id": 0,
        "current-snapshot-id": 43,
        "snapshots": [{
            "snapshot-id": 43,
            "sequence-number": 8,
            "timestamp-ms": 1710000000100_i64,
            "summary": {"operation": "append"},
            "schema-id": 1
        }],
        "snapshot-log": [{"timestamp-ms": 1710000000100_i64, "snapshot-id": 43}]
    });
    let commit = Request::builder()
        .method(Method::POST)
        .uri("/catalog/v1/namespaces/default/tables/events/commit")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::json!({
                "requirements": [],
                "updates": [],
                "metadata-location": initial_metadata_location,
                "metadata": overwrite_metadata,
            })
            .to_string(),
        ))
        .unwrap();
    let response = app.oneshot(commit).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let message = payload["error"]["message"].as_str().unwrap();
    assert!(message.contains("metadata-location-hash=sha256:"));
    assert!(!message.contains(&initial_metadata_location));
    assert!(!message.contains("00000.json"));
    assert_eq!(
        std::fs::read_to_string(&initial_metadata_path).unwrap(),
        sentinel
    );
    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test]
async fn commit_rejects_metadata_object_overwrite_of_existing_target() {
    let app = test_app();
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("lakecat-existing-metadata-guard-{unique}"));
    let table_dir = root.join("events");
    let metadata_dir = table_dir.join("metadata");
    std::fs::create_dir_all(&metadata_dir).unwrap();
    let table_location = url::Url::from_directory_path(&table_dir)
        .expect("table dir URL")
        .to_string();
    let initial_metadata_location = url::Url::from_file_path(metadata_dir.join("00000.json"))
        .unwrap()
        .to_string();
    let target_metadata_path = metadata_dir.join("00001.json");
    let target_metadata_location = url::Url::from_file_path(&target_metadata_path)
        .unwrap()
        .to_string();
    let base_metadata = serde_json::json!({
        "format-version": 3,
        "table-uuid": "11111111-1111-1111-1111-111111111111",
        "location": table_location,
        "last-sequence-number": 7,
        "last-updated-ms": 1710000000000_i64,
        "last-column-id": 1,
        "schemas": [{
            "type": "struct",
            "schema-id": 1,
            "fields": [{
                "id": 1,
                "name": "id",
                "type": "string",
                "required": true
            }]
        }],
        "current-schema-id": 1,
        "partition-specs": [{"spec-id": 0, "fields": []}],
        "default-spec-id": 0,
        "current-snapshot-id": 42,
        "snapshots": [{
            "snapshot-id": 42,
            "sequence-number": 7,
            "timestamp-ms": 1710000000000_i64,
            "summary": {"operation": "append"},
            "schema-id": 1
        }],
        "snapshot-log": [{"timestamp-ms": 1710000000000_i64, "snapshot-id": 42}]
    });
    let create = Request::builder()
        .method(Method::POST)
        .uri("/catalog/v1/namespaces/default/tables")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::json!({
                "name": "events",
                "location": table_location,
                "metadata-location": initial_metadata_location,
                "metadata": base_metadata,
            })
            .to_string(),
        ))
        .unwrap();
    let response = app.clone().oneshot(create).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let sentinel = "{\n  \"sentinel\": \"existing target must not be overwritten\"\n}\n";
    std::fs::write(&target_metadata_path, sentinel).unwrap();
    let commit = Request::builder()
        .method(Method::POST)
        .uri("/catalog/v1/namespaces/default/tables/events/commit")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::json!({
                "requirements": [],
                "updates": [],
                "metadata-location": target_metadata_location,
                "metadata": {
                    "format-version": 3,
                    "table-uuid": "11111111-1111-1111-1111-111111111111",
                    "location": table_location,
                    "last-sequence-number": 8,
                    "last-updated-ms": 1710000000100_i64,
                    "last-column-id": 1,
                    "schemas": [{
                        "type": "struct",
                        "schema-id": 1,
                        "fields": [{
                            "id": 1,
                            "name": "id",
                            "type": "string",
                            "required": true
                        }]
                    }],
                    "current-schema-id": 1,
                    "partition-specs": [{"spec-id": 0, "fields": []}],
                    "default-spec-id": 0,
                    "current-snapshot-id": 43,
                    "snapshots": [{
                        "snapshot-id": 43,
                        "sequence-number": 8,
                        "timestamp-ms": 1710000000100_i64,
                        "summary": {"operation": "append"},
                        "schema-id": 1
                    }],
                    "snapshot-log": [{"timestamp-ms": 1710000000100_i64, "snapshot-id": 43}]
                },
            })
            .to_string(),
        ))
        .unwrap();
    let response = app.oneshot(commit).await.unwrap();
    assert_eq!(response.status(), StatusCode::CONFLICT);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(
        payload["error"]["message"]
            .as_str()
            .unwrap()
            .contains("refusing to overwrite existing metadata")
    );
    let message = payload["error"]["message"].as_str().unwrap();
    assert!(message.contains("metadata-location-hash=sha256:"));
    assert!(!message.contains(&target_metadata_location));
    assert!(!message.contains("00001.json"));
    assert_eq!(
        std::fs::read_to_string(&target_metadata_path).unwrap(),
        sentinel
    );
    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test]
async fn commit_rejects_metadata_object_outside_storage_profile_prefix() {
    let app = test_app();
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("lakecat-metadata-prefix-guard-{unique}"));
    let table_dir = root.join("events");
    let metadata_dir = table_dir.join("metadata");
    let outside_dir = root.join("outside");
    std::fs::create_dir_all(&metadata_dir).unwrap();
    let table_location = url::Url::from_directory_path(&table_dir)
        .expect("table dir URL")
        .to_string();
    let initial_metadata_location = url::Url::from_file_path(metadata_dir.join("00000.json"))
        .unwrap()
        .to_string();
    let outside_metadata_path = outside_dir.join("00001.json");
    let outside_metadata_location = url::Url::from_file_path(&outside_metadata_path)
        .unwrap()
        .to_string();
    let base_metadata = serde_json::json!({
        "format-version": 3,
        "table-uuid": "11111111-1111-1111-1111-111111111111",
        "location": table_location,
        "last-sequence-number": 7,
        "last-updated-ms": 1710000000000_i64,
        "last-column-id": 1,
        "schemas": [{
            "type": "struct",
            "schema-id": 1,
            "fields": [{
                "id": 1,
                "name": "id",
                "type": "string",
                "required": true
            }]
        }],
        "current-schema-id": 1,
        "partition-specs": [{"spec-id": 0, "fields": []}],
        "default-spec-id": 0,
        "current-snapshot-id": 42,
        "snapshots": [{
            "snapshot-id": 42,
            "sequence-number": 7,
            "timestamp-ms": 1710000000000_i64,
            "summary": {"operation": "append"},
            "schema-id": 1
        }],
        "snapshot-log": [{"timestamp-ms": 1710000000000_i64, "snapshot-id": 42}]
    });
    let create = Request::builder()
        .method(Method::POST)
        .uri("/catalog/v1/namespaces/default/tables")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::json!({
                "name": "events",
                "location": table_location,
                "metadata-location": initial_metadata_location,
                "metadata": base_metadata,
            })
            .to_string(),
        ))
        .unwrap();
    let response = app.clone().oneshot(create).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let commit = Request::builder()
        .method(Method::POST)
        .uri("/catalog/v1/namespaces/default/tables/events/commit")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::json!({
                "requirements": [],
                "updates": [],
                "metadata-location": outside_metadata_location,
                "metadata": {
                    "format-version": 3,
                    "table-uuid": "11111111-1111-1111-1111-111111111111",
                    "location": table_location,
                    "last-sequence-number": 8,
                    "last-updated-ms": 1710000000100_i64,
                    "last-column-id": 1,
                    "schemas": [{
                        "type": "struct",
                        "schema-id": 1,
                        "fields": [{
                            "id": 1,
                            "name": "id",
                            "type": "string",
                            "required": true
                        }]
                    }],
                    "current-schema-id": 1,
                    "partition-specs": [{"spec-id": 0, "fields": []}],
                    "default-spec-id": 0,
                    "current-snapshot-id": 43,
                    "snapshots": [{
                        "snapshot-id": 43,
                        "sequence-number": 8,
                        "timestamp-ms": 1710000000100_i64,
                        "summary": {"operation": "append"},
                        "schema-id": 1
                    }],
                    "snapshot-log": [{"timestamp-ms": 1710000000100_i64, "snapshot-id": 43}]
                },
            })
            .to_string(),
        ))
        .unwrap();
    let response = app.oneshot(commit).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let message = payload["error"]["message"].as_str().unwrap();
    assert!(message.contains("metadata-location-hash=sha256:"));
    assert!(message.contains("storage-profile-prefix-hash=sha256:"));
    assert!(!message.contains(&outside_metadata_location));
    assert!(!message.contains(&table_location));
    assert!(!outside_metadata_path.exists());
    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test]
async fn commit_rejects_decorated_metadata_locations_without_leaking_details() {
    let app = test_app();
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("lakecat-decorated-metadata-{unique}"));
    let table_dir = root.join("events");
    let metadata_dir = table_dir.join("metadata");
    std::fs::create_dir_all(&metadata_dir).unwrap();
    let table_location = url::Url::from_directory_path(&table_dir)
        .expect("table dir URL")
        .to_string();
    let initial_metadata_location = url::Url::from_file_path(metadata_dir.join("00000.json"))
        .unwrap()
        .to_string();
    let decorated_metadata_path = metadata_dir.join("token=raw-secret.json");
    let decorated_metadata_location = url::Url::from_file_path(&decorated_metadata_path)
        .unwrap()
        .to_string();
    let base_metadata = serde_json::json!({
        "format-version": 3,
        "table-uuid": "11111111-1111-1111-1111-111111111111",
        "location": table_location,
        "last-sequence-number": 7,
        "last-updated-ms": 1710000000000_i64,
        "last-column-id": 1,
        "schemas": [{
            "type": "struct",
            "schema-id": 1,
            "fields": [{
                "id": 1,
                "name": "id",
                "type": "string",
                "required": true
            }]
        }],
        "current-schema-id": 1,
        "partition-specs": [{"spec-id": 0, "fields": []}],
        "default-spec-id": 0,
        "current-snapshot-id": 42,
        "snapshots": [{
            "snapshot-id": 42,
            "sequence-number": 7,
            "timestamp-ms": 1710000000000_i64,
            "summary": {"operation": "append"},
            "schema-id": 1
        }],
        "snapshot-log": [{"timestamp-ms": 1710000000000_i64, "snapshot-id": 42}]
    });
    let create = Request::builder()
        .method(Method::POST)
        .uri("/catalog/v1/namespaces/default/tables")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::json!({
                "name": "events",
                "location": table_location,
                "metadata-location": initial_metadata_location,
                "metadata": base_metadata,
            })
            .to_string(),
        ))
        .unwrap();
    let response = app.clone().oneshot(create).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let commit = Request::builder()
        .method(Method::POST)
        .uri("/catalog/v1/namespaces/default/tables/events/commit")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::json!({
                "requirements": [],
                "updates": [],
                "metadata-location": decorated_metadata_location,
                "metadata": {
                    "format-version": 3,
                    "table-uuid": "11111111-1111-1111-1111-111111111111",
                    "location": table_location,
                    "last-sequence-number": 8,
                    "last-updated-ms": 1710000000100_i64,
                    "last-column-id": 1,
                    "schemas": [{
                        "type": "struct",
                        "schema-id": 1,
                        "fields": [{
                            "id": 1,
                            "name": "id",
                            "type": "string",
                            "required": true
                        }]
                    }],
                    "current-schema-id": 1,
                    "partition-specs": [{"spec-id": 0, "fields": []}],
                    "default-spec-id": 0,
                    "current-snapshot-id": 43,
                    "snapshots": [{
                        "snapshot-id": 43,
                        "sequence-number": 8,
                        "timestamp-ms": 1710000000100_i64,
                        "summary": {"operation": "append"},
                        "schema-id": 1
                    }],
                    "snapshot-log": [{"timestamp-ms": 1710000000100_i64, "snapshot-id": 43}]
                },
            })
            .to_string(),
        ))
        .unwrap();
    let response = app.oneshot(commit).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let message = payload["error"]["message"].as_str().unwrap();
    assert!(message.contains("credential material"));
    assert!(message.contains("metadata-location-hash=sha256:"));
    assert!(!message.contains(&decorated_metadata_location));
    assert!(!message.contains("raw-secret"));
    assert!(!message.contains("token="));
    assert!(!message.contains("00001-secret.json"));
    assert!(!decorated_metadata_path.exists());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn metadata_cleanup_failure_preserves_commit_conflict() {
    let err = commit_error_with_cleanup_failure(
        LakeCatError::Conflict("metadata pointer changed".to_string()),
        LakeCatError::Internal(
            "failed to clean up /tmp/lakecat-secret/events/metadata/00001.json".to_string(),
        ),
    );

    let LakeCatError::Conflict(message) = err else {
        panic!("expected cleanup failure to preserve commit conflict");
    };
    assert!(message.contains("metadata pointer changed"));
    assert!(message.contains("metadata cleanup also failed"));
    assert!(message.contains("error-detail-hash=sha256:"));
    assert!(!message.contains("lakecat-secret"));
    assert!(!message.contains("00001.json"));
    assert!(!message.contains("failed to clean up"));
}

#[tokio::test]
async fn metadata_cleanup_after_commit_error_redacts_cleanup_setup_failure() {
    let cleanup_location = "not a uri /tmp/lakecat-secret/events/metadata/00001.json";
    let err = cleanup_planned_metadata_after_commit_error(
        Some(PlannedMetadataWrite {
            location: cleanup_location.to_string(),
        }),
        None,
        LakeCatError::Conflict("metadata pointer changed".to_string()),
    )
    .await;

    let LakeCatError::Conflict(message) = err else {
        panic!("expected cleanup setup failure to preserve commit conflict");
    };
    assert!(message.contains("metadata pointer changed"));
    assert!(message.contains("metadata cleanup also failed"));
    assert!(message.contains("error-detail-hash=sha256:"));
    assert!(!message.contains(cleanup_location));
    assert!(!message.contains("lakecat-secret"));
    assert!(!message.contains("00001.json"));
    assert!(!message.contains("relative URL"));
    assert!(!message.contains("invalid metadata location"));
}

#[tokio::test]
async fn metadata_cleanup_treats_missing_uncommitted_object_as_clean() {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("lakecat-missing-cleanup-{unique}"));
    std::fs::create_dir_all(&root).unwrap();
    let missing = root.join("metadata").join("00001.json");
    let missing_location = url::Url::from_file_path(&missing).unwrap().to_string();

    cleanup_planned_metadata(
        Some(PlannedMetadataWrite {
            location: missing_location,
        }),
        None,
    )
    .await
    .expect("missing uncommitted metadata object should already be clean");

    let _ = std::fs::remove_dir_all(root);
}

#[cfg(feature = "sail-local")]
#[tokio::test]
async fn idempotent_commit_replay_skips_stale_sail_revalidation() {
    let store = MemoryCatalogStore::new();
    let app = app(LakeCatState::new(
        WarehouseName::new("local").unwrap(),
        store.clone(),
    ));
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("lakecat-idempotent-replay-{unique}"));
    let table_dir = root.join("events");
    let metadata_dir = table_dir.join("metadata");
    std::fs::create_dir_all(&metadata_dir).unwrap();
    let table_location = url::Url::from_directory_path(&table_dir)
        .expect("table dir URL")
        .to_string();
    let initial_metadata_location = url::Url::from_file_path(metadata_dir.join("00000.json"))
        .unwrap()
        .to_string();
    let committed_metadata_location = url::Url::from_file_path(metadata_dir.join("00001.json"))
        .unwrap()
        .to_string();
    let base_metadata = serde_json::json!({
        "format-version": 3,
        "table-uuid": "11111111-1111-1111-1111-111111111111",
        "location": table_location,
        "last-sequence-number": 7,
        "last-updated-ms": 1710000000000_i64,
        "last-column-id": 1,
        "schemas": [{
            "type": "struct",
            "schema-id": 1,
            "fields": [{
                "id": 1,
                "name": "id",
                "type": "string",
                "required": true
            }]
        }],
        "current-schema-id": 1,
        "partition-specs": [{"spec-id": 0, "fields": []}],
        "default-spec-id": 0,
        "current-snapshot-id": 42,
        "snapshots": [{
            "snapshot-id": 42,
            "sequence-number": 7,
            "timestamp-ms": 1710000000000_i64,
            "summary": {"operation": "append"},
            "schema-id": 1
        }],
        "snapshot-log": [{"timestamp-ms": 1710000000000_i64, "snapshot-id": 42}]
    });
    let advanced_metadata = serde_json::json!({
        "format-version": 3,
        "table-uuid": "11111111-1111-1111-1111-111111111111",
        "location": table_location,
        "last-sequence-number": 8,
        "last-updated-ms": 1710000000100_i64,
        "last-column-id": 2,
        "schemas": [
            {
                "type": "struct",
                "schema-id": 1,
                "fields": [{
                    "id": 1,
                    "name": "id",
                    "type": "string",
                    "required": true
                }]
            },
            {
                "type": "struct",
                "schema-id": 2,
                "fields": [
                    {
                        "id": 1,
                        "name": "id",
                        "type": "string",
                        "required": true
                    },
                    {
                        "id": 2,
                        "name": "payload",
                        "type": "string",
                        "required": false
                    }
                ]
            }
        ],
        "current-schema-id": 2,
        "partition-specs": [{"spec-id": 0, "fields": []}],
        "default-spec-id": 0,
        "current-snapshot-id": 43,
        "snapshots": [{
            "snapshot-id": 43,
            "sequence-number": 8,
            "timestamp-ms": 1710000000100_i64,
            "summary": {"operation": "append"},
            "schema-id": 2
        }],
        "snapshot-log": [{"timestamp-ms": 1710000000100_i64, "snapshot-id": 43}]
    });

    let create = Request::builder()
        .method(Method::POST)
        .uri("/catalog/v1/namespaces/default/tables")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::json!({
                "name": "events",
                "location": table_location,
                "metadata-location": initial_metadata_location,
                "metadata": base_metadata,
            })
            .to_string(),
        ))
        .unwrap();
    let response = app.clone().oneshot(create).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let commit_body = serde_json::json!({
        "requirements": [{
            "type": "assert-current-schema-id",
            "current-schema-id": 1
        }],
        "updates": [],
        "metadata-location": committed_metadata_location,
        "metadata": advanced_metadata,
    })
    .to_string();
    for attempt in 0..2 {
        let commit = Request::builder()
            .method(Method::POST)
            .uri("/catalog/v1/namespaces/default/tables/events/commit")
            .header("content-type", "application/json")
            .header("x-lakecat-idempotency-key", "commit:events:schema-2")
            .body(Body::from(commit_body.clone()))
            .unwrap();
        let response = app.clone().oneshot(commit).await.unwrap();
        assert_eq!(
            response.status(),
            StatusCode::OK,
            "idempotent commit attempt {attempt} should replay before Sail validation"
        );
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(
            payload["metadata-location"],
            serde_json::json!(committed_metadata_location)
        );
        assert_eq!(
            payload["metadata"]["current-schema-id"],
            serde_json::json!(2)
        );
    }

    let ident = table_ident("local", "default".to_string(), "events".to_string()).unwrap();
    let records = store.table_commit_records(&ident, 0, None).await.unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(store.load_table(&ident).await.unwrap().version, 1);
    let _ = std::fs::remove_dir_all(root);
}

#[cfg(feature = "sail-local")]
#[tokio::test]
async fn stale_commit_requirement_returns_conflict_with_sail_local_engine() {
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

    let commit = Request::builder()
        .method(Method::POST)
        .uri("/catalog/v1/namespaces/default/tables/events/commit")
        .header("content-type", "application/json")
        .body(Body::from(
            r#"{"requirements":[{"type":"assert-current-schema-id","current-schema-id":9}],"updates":[]}"#,
        ))
        .unwrap();
    let response = app.oneshot(commit).await.unwrap();
    assert_eq!(response.status(), StatusCode::CONFLICT);
}

#[cfg(feature = "sail-local")]
#[tokio::test]
async fn stale_commit_cleans_up_uncommitted_metadata_file() {
    let app = test_app();
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("lakecat-orphan-cleanup-{unique}"));
    let table_dir = root.join("events");
    let metadata_dir = table_dir.join("metadata");
    std::fs::create_dir_all(&metadata_dir).unwrap();
    let table_location = url::Url::from_directory_path(&table_dir)
        .expect("table dir URL")
        .to_string();
    let initial_metadata_location = url::Url::from_file_path(metadata_dir.join("00000.json"))
        .unwrap()
        .to_string();
    let rejected_metadata_path = metadata_dir.join("00001.json");
    let rejected_metadata_location = url::Url::from_file_path(&rejected_metadata_path)
        .unwrap()
        .to_string();
    let base_metadata = serde_json::json!({
        "format-version": 3,
        "table-uuid": "11111111-1111-1111-1111-111111111111",
        "location": table_location,
        "last-sequence-number": 7,
        "last-updated-ms": 1710000000000_i64,
        "last-column-id": 1,
        "schemas": [{
            "type": "struct",
            "schema-id": 1,
            "fields": [{
                "id": 1,
                "name": "id",
                "type": "string",
                "required": true
            }]
        }],
        "current-schema-id": 1,
        "partition-specs": [{"spec-id": 0, "fields": []}],
        "default-spec-id": 0,
        "current-snapshot-id": 42,
        "snapshots": [{
            "snapshot-id": 42,
            "sequence-number": 7,
            "timestamp-ms": 1710000000000_i64,
            "summary": {"operation": "append"},
            "schema-id": 1
        }],
        "snapshot-log": [{"timestamp-ms": 1710000000000_i64, "snapshot-id": 42}]
    });
    let create = Request::builder()
        .method(Method::POST)
        .uri("/catalog/v1/namespaces/default/tables")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::json!({
                "name": "events",
                "location": table_location,
                "metadata-location": initial_metadata_location,
                "metadata": base_metadata,
            })
            .to_string(),
        ))
        .unwrap();
    let response = app.clone().oneshot(create).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let mut rejected_metadata = base_metadata;
    rejected_metadata["last-sequence-number"] = serde_json::json!(8);
    rejected_metadata["last-updated-ms"] = serde_json::json!(1710000000100_i64);
    let commit = Request::builder()
        .method(Method::POST)
        .uri("/catalog/v1/namespaces/default/tables/events/commit")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::json!({
                "requirements": [{
                    "type": "assert-current-schema-id",
                    "current-schema-id": 9
                }],
                "updates": [],
                "metadata-location": rejected_metadata_location,
                "metadata": rejected_metadata,
            })
            .to_string(),
        ))
        .unwrap();
    let response = app.oneshot(commit).await.unwrap();
    assert_eq!(response.status(), StatusCode::CONFLICT);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let message = payload["error"]["message"].as_str().unwrap();
    assert!(!message.contains(&initial_metadata_location));
    assert!(!message.contains(&rejected_metadata_location));
    assert!(!message.contains("00000.json"));
    assert!(!message.contains("00001.json"));
    assert!(!rejected_metadata_path.exists());
    let _ = std::fs::remove_dir_all(root);
}

#[cfg(feature = "sail-local")]
#[tokio::test]
async fn cas_race_cleans_up_uncommitted_metadata_file_with_redacted_conflict() {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("lakecat-cas-cleanup-{unique}"));
    let table_dir = root.join("events");
    let metadata_dir = table_dir.join("metadata");
    std::fs::create_dir_all(&metadata_dir).unwrap();
    let table_location = url::Url::from_directory_path(&table_dir)
        .expect("table dir URL")
        .to_string();
    let initial_metadata_location = url::Url::from_file_path(metadata_dir.join("00000.json"))
        .unwrap()
        .to_string();
    let racing_metadata_location = url::Url::from_file_path(metadata_dir.join("00001-race.json"))
        .unwrap()
        .to_string();
    let rejected_metadata_path = metadata_dir.join("00002-rejected.json");
    let rejected_metadata_location = url::Url::from_file_path(&rejected_metadata_path)
        .unwrap()
        .to_string();
    let store = CasRaceStore::new(MemoryCatalogStore::new(), racing_metadata_location.clone());
    let app = app(LakeCatState::new(
        WarehouseName::new("local").unwrap(),
        store.clone(),
    ));
    let base_metadata = serde_json::json!({
        "format-version": 3,
        "table-uuid": "11111111-1111-1111-1111-111111111111",
        "location": table_location,
        "last-sequence-number": 7,
        "last-updated-ms": 1710000000000_i64,
        "last-column-id": 1,
        "schemas": [{
            "type": "struct",
            "schema-id": 1,
            "fields": [{
                "id": 1,
                "name": "id",
                "type": "string",
                "required": true
            }]
        }],
        "current-schema-id": 1,
        "partition-specs": [{"spec-id": 0, "fields": []}],
        "default-spec-id": 0,
        "current-snapshot-id": 42,
        "snapshots": [{
            "snapshot-id": 42,
            "sequence-number": 7,
            "timestamp-ms": 1710000000000_i64,
            "summary": {"operation": "append"},
            "schema-id": 1
        }],
        "snapshot-log": [{"timestamp-ms": 1710000000000_i64, "snapshot-id": 42}]
    });
    let create = Request::builder()
        .method(Method::POST)
        .uri("/catalog/v1/namespaces/default/tables")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::json!({
                "name": "events",
                "location": table_location,
                "metadata-location": initial_metadata_location,
                "metadata": base_metadata,
            })
            .to_string(),
        ))
        .unwrap();
    let response = app.clone().oneshot(create).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let mut rejected_metadata = base_metadata;
    rejected_metadata["last-sequence-number"] = serde_json::json!(8);
    rejected_metadata["last-updated-ms"] = serde_json::json!(1710000000100_i64);
    let commit = Request::builder()
        .method(Method::POST)
        .uri("/catalog/v1/namespaces/default/tables/events/commit")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::json!({
                "requirements": [],
                "updates": [],
                "metadata-location": rejected_metadata_location,
                "metadata": rejected_metadata,
            })
            .to_string(),
        ))
        .unwrap();
    let response = app.oneshot(commit).await.unwrap();
    assert_eq!(response.status(), StatusCode::CONFLICT);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let message = payload["error"]["message"].as_str().unwrap();
    assert!(message.contains("metadata pointer changed"));
    assert!(message.contains("expected-metadata-location-hash=sha256:"));
    assert!(message.contains("actual-metadata-location-hash=sha256:"));
    assert!(!message.contains(&initial_metadata_location));
    assert!(!message.contains(&racing_metadata_location));
    assert!(!message.contains(&rejected_metadata_location));
    assert!(!message.contains("00000.json"));
    assert!(!message.contains("00001-race.json"));
    assert!(!message.contains("00002-rejected.json"));
    assert!(!rejected_metadata_path.exists());

    let ident = table_ident("local", "default".to_string(), "events".to_string()).unwrap();
    let table = store.load_table(&ident).await.unwrap();
    assert_eq!(
        table.metadata_location.as_deref(),
        Some(racing_metadata_location.as_str())
    );
    let _ = std::fs::remove_dir_all(root);
}
