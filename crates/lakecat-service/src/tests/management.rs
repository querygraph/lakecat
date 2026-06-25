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
fn management_root_event_redaction_keeps_generated_evidence_hash_only() {
    let endpoint_url = "https://lakecat.example.com";
    let storage_root = "file:///tmp/lakecat";

    let server_payload = redact_server_event_payload(json!({
        "event-type": "server.upserted",
        "server-id": "prod",
        "server-record": {
            "server-id": "prod",
            "display-name": "Production",
            "endpoint-url": endpoint_url,
            "properties": {"region": "global"}
        }
    }));
    assert!(
        server_payload["server-record"]
            .get("endpoint-url")
            .is_none(),
        "generated server evidence must not persist raw endpoint URLs"
    );
    assert_eq!(
        server_payload["server-record"]["endpoint-url-hash"],
        json!(content_hash_json(&json!({"endpoint-url": endpoint_url})).unwrap())
    );
    assert!(
        !serde_json::to_string(&server_payload)
            .unwrap()
            .contains(endpoint_url),
        "generated server evidence should be hash-only before audit/outbox recording"
    );

    let warehouse_payload = redact_warehouse_event_payload(json!({
        "event-type": "warehouse.upserted",
        "warehouse": "local",
        "warehouse-record": {
            "warehouse": "local",
            "project-id": "default",
            "storage-root": storage_root,
            "properties": {"region": "local"}
        }
    }));
    assert!(
        warehouse_payload["warehouse-record"]
            .get("storage-root")
            .is_none(),
        "generated warehouse evidence must not persist raw storage roots"
    );
    assert_eq!(
        warehouse_payload["warehouse-record"]["storage-root-hash"],
        json!(content_hash_json(&json!({"storage-root": storage_root})).unwrap())
    );
    assert!(
        !serde_json::to_string(&warehouse_payload)
            .unwrap()
            .contains(storage_root),
        "generated warehouse evidence should be hash-only before audit/outbox recording"
    );
}

#[tokio::test]
async fn prefixed_catalog_routes_target_requested_warehouse() {
    let app = test_app();
    let upsert_project = Request::builder()
        .method(Method::PUT)
        .uri("/management/v1/projects/default")
        .header("content-type", "application/json")
        .header("x-lakecat-principal", "operator@example.com")
        .body(Body::from(
            serde_json::json!({
                "display-name": "Default Project"
            })
            .to_string(),
        ))
        .unwrap();
    let response = app.clone().oneshot(upsert_project).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    for (warehouse, location, metadata_location, uuid) in [
        (
            "local",
            "file:///tmp/local-events",
            "file:///tmp/local-events/metadata/00000.json",
            "11111111-1111-1111-1111-111111111111",
        ),
        (
            "other",
            "file:///tmp/other-events",
            "file:///tmp/other-events/metadata/00000.json",
            "22222222-2222-2222-2222-222222222222",
        ),
    ] {
        let upsert_warehouse = Request::builder()
            .method(Method::PUT)
            .uri(format!("/management/v1/warehouses/{warehouse}"))
            .header("content-type", "application/json")
            .header("x-lakecat-principal", "operator@example.com")
            .body(Body::from(
                serde_json::json!({
                    "project-id": "default",
                    "storage-root": location,
                })
                .to_string(),
            ))
            .unwrap();
        let response = app.clone().oneshot(upsert_warehouse).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let create = Request::builder()
            .method(Method::POST)
            .uri(format!("/catalog/v1/{warehouse}/namespaces/default/tables"))
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::json!({
                    "name": "events",
                    "location": location,
                    "metadata-location": metadata_location,
                    "metadata": {
                        "format-version": 3,
                        "table-uuid": uuid,
                        "location": location,
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
                        "default-spec-id": 0
                    }
                })
                .to_string(),
            ))
            .unwrap();
        let response = app.clone().oneshot(create).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    let default_load = Request::builder()
        .method(Method::GET)
        .uri("/catalog/v1/namespaces/default/tables/events")
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(default_load).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(
        body["metadata-location"],
        serde_json::json!("file:///tmp/local-events/metadata/00000.json")
    );
    assert_eq!(
        body["metadata"]["location"],
        serde_json::json!("file:///tmp/local-events")
    );

    let prefixed_load = Request::builder()
        .method(Method::GET)
        .uri("/catalog/v1/other/namespaces/default/tables/events")
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(prefixed_load).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(
        body["metadata-location"],
        serde_json::json!("file:///tmp/other-events/metadata/00000.json")
    );
    assert_eq!(
        body["metadata"]["location"],
        serde_json::json!("file:///tmp/other-events")
    );

    let missing_warehouse = Request::builder()
        .method(Method::GET)
        .uri("/catalog/v1/missing/namespaces/default/tables/events")
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(missing_warehouse).await.unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[test]
fn metadata_write_plan_rejects_storage_profile_root_location() {
    let table = TableRecord::new(
        table_ident("local", "default", "events").unwrap(),
        "file:///tmp/events".to_string(),
        Some("file:///tmp/events/metadata/00000.json".to_string()),
        serde_json::json!({"format-version": 3}),
        Principal::anonymous(),
    );
    let storage_profile = StorageProfile::inferred_for_table(&table);
    let plan = lakecat_core::sail::CommitPlan {
        prepared_by: "test".to_string(),
        requirements: Vec::new(),
        updates: Vec::new(),
        new_metadata_location: Some("file:///tmp/events".to_string()),
        new_metadata: serde_json::json!({"format-version": 3}),
        metadata_write_required: true,
        metadata_patch: serde_json::json!({}),
    };

    let err = validate_planned_metadata_location(
        &plan,
        table.metadata_location.as_deref(),
        &storage_profile,
    )
    .unwrap_err();
    assert!(matches!(err, LakeCatError::InvalidArgument(_)));
    let message = err.to_string();
    assert!(message.contains("not a child object"));
    assert!(message.contains("metadata-location-hash=sha256:"));
    assert!(message.contains("storage-profile-prefix-hash=sha256:"));
    assert!(!message.contains("file:///tmp/events"));
}

#[test]
fn metadata_object_location_must_be_child_of_storage_profile_root() {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("lakecat-metadata-root-guard-{unique}"));
    std::fs::create_dir_all(&root).unwrap();
    let storage_root = url::Url::from_directory_path(&root)
        .expect("storage root URL")
        .to_string();
    let profile = StorageProfile::new(
        "local-files",
        WarehouseName::new("local").unwrap(),
        storage_root.clone(),
        StorageProvider::File,
        CredentialIssuanceMode::LocalFileNoSecret,
        None,
        BTreeMap::new(),
    )
    .unwrap();
    let plan = lakecat_core::sail::CommitPlan {
        prepared_by: "test".to_string(),
        requirements: Vec::new(),
        updates: Vec::new(),
        new_metadata_location: Some(storage_root.clone()),
        new_metadata: serde_json::json!({"format-version": 3}),
        metadata_write_required: true,
        metadata_patch: serde_json::json!({}),
    };

    let err = validate_planned_metadata_location(&plan, None, &profile).unwrap_err();

    let message = err.to_string();
    assert!(message.contains("metadata-location-hash=sha256:"));
    assert!(message.contains("storage-profile-prefix-hash=sha256:"));
    assert!(!message.contains(&storage_root));
    assert!(!message.contains(root.to_string_lossy().as_ref()));
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn metadata_prefix_rejection_redacts_storage_profile_id() {
    let storage_root = "s3://lakecat-secret-bucket/events";
    let profile = StorageProfile::new(
        "tenant-secret-prod-profile",
        WarehouseName::new("local").unwrap(),
        storage_root,
        StorageProvider::S3,
        CredentialIssuanceMode::ShortLivedSecretRef,
        Some("typesec://lakecat/local/tenant-secret-prod-profile".to_string()),
        BTreeMap::new(),
    )
    .unwrap();
    let outside_location = "s3://other-secret-bucket/events/metadata/00001.json";
    let plan = lakecat_core::sail::CommitPlan {
        prepared_by: "test".to_string(),
        requirements: Vec::new(),
        updates: Vec::new(),
        new_metadata_location: Some(outside_location.to_string()),
        new_metadata: serde_json::json!({"format-version": 3}),
        metadata_write_required: true,
        metadata_patch: serde_json::json!({}),
    };

    let err = validate_planned_metadata_location(&plan, None, &profile).unwrap_err();

    let message = err.to_string();
    assert!(message.contains("metadata-location-hash=sha256:"));
    assert!(message.contains("storage-profile-prefix-hash=sha256:"));
    assert!(!message.contains("tenant-secret-prod-profile"));
    assert!(!message.contains("lakecat-secret-bucket"));
    assert!(!message.contains("other-secret-bucket"));
    assert!(!message.contains("00001.json"));
}

#[tokio::test]
async fn management_servers_are_durable_management_entities() {
    let app = test_app();
    let upsert = Request::builder()
        .method(Method::PUT)
        .uri("/management/v1/servers/lakecat-local")
        .header("content-type", "application/json")
        .header("x-lakecat-principal", "operator@example.com")
        .body(Body::from(
            serde_json::json!({
                "display-name": "Local LakeCat",
                "endpoint-url": "http://127.0.0.1:8181",
                "properties": {
                    "deployment": "local"
                }
            })
            .to_string(),
        ))
        .unwrap();
    let response = app.clone().oneshot(upsert).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(body["server-id"], serde_json::json!("lakecat-local"));
    assert_eq!(body["display-name"], serde_json::json!("Local LakeCat"));
    assert_eq!(
        body["endpoint-url"],
        serde_json::json!("http://127.0.0.1:8181")
    );
    assert_eq!(body["properties"]["deployment"], serde_json::json!("local"));

    let list = Request::builder()
        .method(Method::GET)
        .uri("/management/v1/servers")
        .header("x-lakecat-principal", "operator@example.com")
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(list).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(body["servers"].as_array().unwrap().len(), 1);
    assert_eq!(
        body["servers"][0]["server-id"],
        serde_json::json!("lakecat-local")
    );
}

#[tokio::test]
async fn management_server_rejects_decorated_endpoint_urls() {
    let app = test_app();
    let endpoint_url = "https://lakecat.example.com?token=raw-secret";
    let upsert = Request::builder()
        .method(Method::PUT)
        .uri("/management/v1/servers/prod")
        .header("content-type", "application/json")
        .header("x-lakecat-principal", "operator@example.com")
        .body(Body::from(
            serde_json::json!({
                "display-name": "Production",
                "endpoint-url": endpoint_url,
                "properties": {
                    "deployment": "prod"
                }
            })
            .to_string(),
        ))
        .unwrap();
    let response = app.oneshot(upsert).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let message = body["error"]["message"].as_str().unwrap();
    assert!(message.contains("query strings, fragments, or userinfo"));
    assert!(message.contains("server-endpoint-url-hash=sha256:"));
    assert!(
        !message.contains(endpoint_url),
        "server endpoint validation must not expose raw endpoint URLs"
    );
    assert!(!message.contains("raw-secret"));
}

#[tokio::test]
async fn management_server_rejects_invalid_endpoint_urls() {
    let app = test_app();
    let endpoint_url = "s3://lakecat-demo/catalog";
    let upsert = Request::builder()
        .method(Method::PUT)
        .uri("/management/v1/servers/prod")
        .header("content-type", "application/json")
        .header("x-lakecat-principal", "operator@example.com")
        .body(Body::from(
            serde_json::json!({
                "display-name": "Production",
                "endpoint-url": endpoint_url,
                "properties": {
                    "deployment": "prod"
                }
            })
            .to_string(),
        ))
        .unwrap();
    let response = app.oneshot(upsert).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let message = body["error"]["message"].as_str().unwrap();
    assert!(message.contains("http or https scheme"));
    assert!(message.contains("server-endpoint-url-hash=sha256:"));
    assert!(
        !message.contains(endpoint_url),
        "server endpoint validation must not expose raw endpoint URLs"
    );
}

#[tokio::test]
async fn management_warehouses_are_durable_management_entities() {
    let app = test_app();
    let upsert_project = Request::builder()
        .method(Method::PUT)
        .uri("/management/v1/projects/default")
        .header("content-type", "application/json")
        .header("x-lakecat-principal", "operator@example.com")
        .body(Body::from(
            serde_json::json!({
                "display-name": "Default Project"
            })
            .to_string(),
        ))
        .unwrap();
    let response = app.clone().oneshot(upsert_project).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let upsert_scoped = Request::builder()
        .method(Method::PUT)
        .uri("/management/v1/projects/default/warehouses/project_local")
        .header("content-type", "application/json")
        .header("x-lakecat-principal", "operator@example.com")
        .body(Body::from(
            serde_json::json!({
                "storage-root": "file:///tmp/lakecat-project-local",
                "properties": {
                    "region": "project-scoped"
                }
            })
            .to_string(),
        ))
        .unwrap();
    let response = app.clone().oneshot(upsert_scoped).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(body["warehouse"], serde_json::json!("project_local"));
    assert_eq!(body["project-id"], serde_json::json!("default"));

    let scoped_list = Request::builder()
        .method(Method::GET)
        .uri("/management/v1/projects/default/warehouses")
        .header("x-lakecat-principal", "operator@example.com")
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(scoped_list).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(body["warehouses"].as_array().unwrap().len(), 1);
    assert_eq!(
        body["warehouses"][0]["warehouse"],
        serde_json::json!("project_local")
    );

    let mismatched_project = Request::builder()
        .method(Method::PUT)
        .uri("/management/v1/projects/default/warehouses/mismatch")
        .header("content-type", "application/json")
        .header("x-lakecat-principal", "operator@example.com")
        .body(Body::from(
            serde_json::json!({
                "project-id": "other",
                "storage-root": "file:///tmp/mismatch"
            })
            .to_string(),
        ))
        .unwrap();
    let response = app.clone().oneshot(mismatched_project).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let upsert = Request::builder()
        .method(Method::PUT)
        .uri("/management/v1/warehouses/local")
        .header("content-type", "application/json")
        .header("x-lakecat-principal", "operator@example.com")
        .body(Body::from(
            serde_json::json!({
                "project-id": "default",
                "storage-root": "file:///tmp/lakecat",
                "properties": {
                    "region": "local"
                }
            })
            .to_string(),
        ))
        .unwrap();
    let response = app.clone().oneshot(upsert).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(body["warehouse"], serde_json::json!("local"));
    assert_eq!(body["project-id"], serde_json::json!("default"));
    assert_eq!(
        body["storage-root"],
        serde_json::json!("file:///tmp/lakecat")
    );
    assert_eq!(body["properties"]["region"], serde_json::json!("local"));

    let list = Request::builder()
        .method(Method::GET)
        .uri("/management/v1/warehouses")
        .header("x-lakecat-principal", "operator@example.com")
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(list).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(body["warehouses"].as_array().unwrap().len(), 2);
    assert!(
        body["warehouses"]
            .as_array()
            .unwrap()
            .iter()
            .any(|warehouse| { warehouse["warehouse"] == serde_json::json!("local") })
    );

    let other_warehouse = Request::builder()
        .method(Method::PUT)
        .uri("/management/v1/warehouses/other")
        .header("content-type", "application/json")
        .header("x-lakecat-principal", "operator@example.com")
        .body(Body::from(
            serde_json::json!({
                "project-id": "default",
                "storage-root": "file:///tmp/lakecat-other",
                "properties": {
                    "region": "other"
                }
            })
            .to_string(),
        ))
        .unwrap();
    let response = app.clone().oneshot(other_warehouse).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(body["warehouse"], serde_json::json!("other"));
    assert_eq!(
        body["storage-root"],
        serde_json::json!("file:///tmp/lakecat-other")
    );

    let list = Request::builder()
        .method(Method::GET)
        .uri("/management/v1/warehouses")
        .header("x-lakecat-principal", "operator@example.com")
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(list).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(body["warehouses"].as_array().unwrap().len(), 3);
    assert!(
        body["warehouses"]
            .as_array()
            .unwrap()
            .iter()
            .any(|warehouse| { warehouse["warehouse"] == serde_json::json!("other") })
    );

    let missing_project = Request::builder()
        .method(Method::PUT)
        .uri("/management/v1/warehouses/orphaned")
        .header("content-type", "application/json")
        .header("x-lakecat-principal", "operator@example.com")
        .body(Body::from(
            serde_json::json!({
                "project-id": "missing-project",
                "storage-root": "file:///tmp/orphaned"
            })
            .to_string(),
        ))
        .unwrap();
    let response = app.oneshot(missing_project).await.unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn management_warehouse_rejects_decorated_storage_roots() {
    let app = test_app();
    let storage_root = "file:///tmp/lakecat?token=raw-secret";
    let upsert = Request::builder()
        .method(Method::PUT)
        .uri("/management/v1/warehouses/decorated_root")
        .header("content-type", "application/json")
        .header("x-lakecat-principal", "operator@example.com")
        .body(Body::from(
            serde_json::json!({
                "project-id": "default",
                "storage-root": storage_root
            })
            .to_string(),
        ))
        .unwrap();
    let response = app.oneshot(upsert).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let message = body["error"]["message"].as_str().unwrap();
    assert!(message.contains("query strings, fragments, or userinfo"));
    assert!(message.contains("warehouse-storage-root-hash=sha256:"));
    assert!(!message.contains(storage_root));
    assert!(!message.contains("raw-secret"));
}

#[tokio::test]
async fn management_warehouse_rejects_dot_segment_storage_roots() {
    let app = test_app();
    let storage_root = "file:///tmp/lakecat/../private";
    let upsert = Request::builder()
        .method(Method::PUT)
        .uri("/management/v1/warehouses/dot_root")
        .header("content-type", "application/json")
        .header("x-lakecat-principal", "operator@example.com")
        .body(Body::from(
            serde_json::json!({
                "project-id": "default",
                "storage-root": storage_root
            })
            .to_string(),
        ))
        .unwrap();
    let response = app.oneshot(upsert).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let message = body["error"]["message"].as_str().unwrap();
    assert!(message.contains("dot path segments"));
    assert!(message.contains("warehouse-storage-root-hash=sha256:"));
    assert!(!message.contains(storage_root));
    assert!(!message.contains("../private"));
}

#[tokio::test]
async fn management_projects_are_durable_management_entities() {
    let app = test_app();
    let upsert_server = Request::builder()
        .method(Method::PUT)
        .uri("/management/v1/servers/lakecat-local")
        .header("content-type", "application/json")
        .header("x-lakecat-principal", "operator@example.com")
        .body(Body::from(
            serde_json::json!({
                "display-name": "Local LakeCat"
            })
            .to_string(),
        ))
        .unwrap();
    let response = app.clone().oneshot(upsert_server).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let upsert = Request::builder()
        .method(Method::PUT)
        .uri("/management/v1/projects/default")
        .header("content-type", "application/json")
        .header("x-lakecat-principal", "operator@example.com")
        .body(Body::from(
            serde_json::json!({
                "server-id": "lakecat-local",
                "display-name": "Default Project",
                "properties": {
                    "owner": "querygraph"
                }
            })
            .to_string(),
        ))
        .unwrap();
    let response = app.clone().oneshot(upsert).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(body["project-id"], serde_json::json!("default"));
    assert_eq!(body["server-id"], serde_json::json!("lakecat-local"));
    assert_eq!(body["display-name"], serde_json::json!("Default Project"));
    assert_eq!(body["properties"]["owner"], serde_json::json!("querygraph"));

    let list = Request::builder()
        .method(Method::GET)
        .uri("/management/v1/projects")
        .header("x-lakecat-principal", "operator@example.com")
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(list).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(body["projects"].as_array().unwrap().len(), 1);
    assert_eq!(
        body["projects"][0]["project-id"],
        serde_json::json!("default")
    );
    assert_eq!(
        body["projects"][0]["server-id"],
        serde_json::json!("lakecat-local")
    );
    assert_eq!(
        body["projects"][0]["display-name"],
        serde_json::json!("Default Project")
    );

    let missing_server = Request::builder()
        .method(Method::PUT)
        .uri("/management/v1/projects/orphaned")
        .header("content-type", "application/json")
        .header("x-lakecat-principal", "operator@example.com")
        .body(Body::from(
            serde_json::json!({
                "server-id": "missing-server",
                "display-name": "Orphaned Project"
            })
            .to_string(),
        ))
        .unwrap();
    let response = app.oneshot(missing_server).await.unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn management_views_are_durable_management_entities() {
    let app = test_app();
    let upsert = Request::builder()
        .method(Method::PUT)
        .uri("/management/v1/warehouses/local/namespaces/default/views/active_customers")
        .header("content-type", "application/json")
        .header("x-lakecat-principal", "operator@example.com")
        .body(Body::from(
            serde_json::json!({
                "sql": "select id, email from customers where active",
                "dialect": "sql",
                "schema-version": 1,
                "columns": [
                    {
                        "name": "id",
                        "data-type": "int",
                        "nullable": false,
                        "comment": "Customer identifier"
                    },
                    {
                        "name": "email",
                        "data-type": "string",
                        "nullable": true
                    }
                ],
                "properties": {
                    "semantic-domain": "customer"
                }
            })
            .to_string(),
        ))
        .unwrap();
    let response = app.clone().oneshot(upsert).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(body["warehouse"], serde_json::json!("local"));
    assert_eq!(body["namespace"], serde_json::json!(["default"]));
    assert_eq!(body["name"], serde_json::json!("active_customers"));
    assert_eq!(body["view-version"], serde_json::json!(1));
    assert_eq!(
        body["properties"]["semantic-domain"],
        serde_json::json!("customer")
    );
    assert_eq!(body["columns"].as_array().unwrap().len(), 2);
    assert_eq!(body["columns"][0]["name"], serde_json::json!("id"));
    assert_eq!(body["columns"][0]["data-type"], serde_json::json!("int"));
    assert_eq!(body["columns"][0]["nullable"], serde_json::json!(false));

    let list = Request::builder()
        .method(Method::GET)
        .uri("/management/v1/warehouses/local/namespaces/default/views")
        .header("x-lakecat-principal", "operator@example.com")
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(list).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(body["views"].as_array().unwrap().len(), 1);
    assert_eq!(
        body["views"][0]["name"],
        serde_json::json!("active_customers")
    );
    assert_eq!(body["views"][0]["schema-version"], serde_json::json!(1));
    assert_eq!(body["views"][0]["view-version"], serde_json::json!(1));
    assert_eq!(
        body["views"][0]["columns"][0]["comment"],
        serde_json::json!("Customer identifier")
    );

    let catalog_list = Request::builder()
        .method(Method::GET)
        .uri("/catalog/v1/local/namespaces/default/views")
        .header("x-lakecat-principal", "operator@example.com")
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(catalog_list).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(body["views"].as_array().unwrap().len(), 1);
    assert_eq!(
        body["views"][0]["name"],
        serde_json::json!("active_customers")
    );
    assert_eq!(body["views"][0]["view-version"], serde_json::json!(1));

    let catalog_load = Request::builder()
        .method(Method::GET)
        .uri("/catalog/v1/local/namespaces/default/views/active_customers")
        .header("x-lakecat-principal", "operator@example.com")
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(catalog_load).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(body["name"], serde_json::json!("active_customers"));
    assert_eq!(body["view-version"], serde_json::json!(1));
    assert_eq!(body["schema-version"], serde_json::json!(1));
    assert_eq!(
        body["properties"]["semantic-domain"],
        serde_json::json!("customer")
    );

    let update = Request::builder()
        .method(Method::PUT)
        .uri("/management/v1/warehouses/local/namespaces/default/views/active_customers")
        .header("content-type", "application/json")
        .header("x-lakecat-principal", "operator@example.com")
        .body(Body::from(
            serde_json::json!({
                "sql": "select id from customers where active",
                "dialect": "sql",
                "schema-version": 2,
                "expected-view-version": 1,
                "properties": {
                    "semantic-domain": "customer"
                }
            })
            .to_string(),
        ))
        .unwrap();
    let response = app.clone().oneshot(update).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(body["view-version"], serde_json::json!(2));

    let stale_update = Request::builder()
        .method(Method::PUT)
        .uri("/management/v1/warehouses/local/namespaces/default/views/active_customers")
        .header("content-type", "application/json")
        .header("x-lakecat-principal", "operator@example.com")
        .body(Body::from(
            serde_json::json!({
                "sql": "select email from customers where active",
                "dialect": "sql",
                "schema-version": 3,
                "expected-view-version": 1
            })
            .to_string(),
        ))
        .unwrap();
    let response = app.clone().oneshot(stale_update).await.unwrap();
    assert_eq!(response.status(), StatusCode::CONFLICT);

    let receipts = Request::builder()
        .method(Method::GET)
        .uri(
            "/management/v1/warehouses/local/namespaces/default/views/active_customers/version-receipts",
        )
        .header("x-lakecat-principal", "operator@example.com")
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(receipts).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let receipts = body["receipts"].as_array().unwrap();
    assert_eq!(receipts.len(), 2);
    assert_eq!(
        receipts[0]["stable-id"],
        serde_json::json!("lakecat:view:local:default:active_customers")
    );
    assert_eq!(receipts[0]["view-version"], serde_json::json!(1));
    assert!(receipts[0]["previous-view-version"].is_null());
    assert!(receipts[0].get("previous-receipt-hash").is_none());
    assert_eq!(receipts[0]["operation"], serde_json::json!("upsert"));
    assert!(
        receipts[0]["receipt-hash"]
            .as_str()
            .is_some_and(is_full_sha256_hash)
    );
    assert!(
        receipts[0]["view-hash"]
            .as_str()
            .is_some_and(is_full_sha256_hash)
    );
    assert_eq!(receipts[1]["view-version"], serde_json::json!(2));
    assert_eq!(receipts[1]["previous-view-version"], serde_json::json!(1));
    assert_eq!(
        receipts[1]["previous-receipt-hash"],
        receipts[0]["receipt-hash"]
    );
    assert!(
        receipts[1]["receipt-hash"]
            .as_str()
            .is_some_and(is_full_sha256_hash)
    );
    assert!(
        receipts[1]["view-hash"]
            .as_str()
            .is_some_and(is_full_sha256_hash)
    );
    assert_ne!(receipts[0]["view-hash"], receipts[1]["view-hash"]);

    let catalog_upsert = Request::builder()
        .method(Method::PUT)
        .uri("/catalog/v1/local/namespaces/default/views/catalog_customers")
        .header("content-type", "application/json")
        .header("x-lakecat-principal", "operator@example.com")
        .body(Body::from(
            serde_json::json!({
                "sql": "select id from customers where active",
                "dialect": "sql",
                "schema-version": 2,
                "properties": {
                    "semantic-domain": "catalog-customer"
                }
            })
            .to_string(),
        ))
        .unwrap();
    let response = app.clone().oneshot(catalog_upsert).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let catalog_load = Request::builder()
        .method(Method::GET)
        .uri("/catalog/v1/local/namespaces/default/views/catalog_customers")
        .header("x-lakecat-principal", "operator@example.com")
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(catalog_load).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(body["name"], serde_json::json!("catalog_customers"));
    assert_eq!(body["view-version"], serde_json::json!(1));
    assert_eq!(body["schema-version"], serde_json::json!(2));

    let catalog_drop = Request::builder()
        .method(Method::DELETE)
        .uri("/catalog/v1/local/namespaces/default/views/catalog_customers?expected-view-version=1")
        .header("x-lakecat-principal", "operator@example.com")
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(catalog_drop).await.unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let dropped_catalog_load = Request::builder()
        .method(Method::GET)
        .uri("/catalog/v1/local/namespaces/default/views/catalog_customers")
        .header("x-lakecat-principal", "operator@example.com")
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(dropped_catalog_load).await.unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let stale_management_drop = Request::builder()
        .method(Method::DELETE)
        .uri("/management/v1/warehouses/local/namespaces/default/views/active_customers?expected-view-version=1")
        .header("x-lakecat-principal", "operator@example.com")
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(stale_management_drop).await.unwrap();
    assert_eq!(response.status(), StatusCode::CONFLICT);

    let receipts_before_drop = Request::builder()
        .method(Method::GET)
        .uri(
            "/management/v1/warehouses/local/namespaces/default/views/active_customers/version-receipts",
        )
        .header("x-lakecat-principal", "operator@example.com")
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(receipts_before_drop).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(body["receipts"].as_array().unwrap().len(), 2);

    let management_drop = Request::builder()
        .method(Method::DELETE)
        .uri("/management/v1/warehouses/local/namespaces/default/views/active_customers?expected-view-version=2")
        .header("x-lakecat-principal", "operator@example.com")
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(management_drop).await.unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let receipts_after_drop = Request::builder()
        .method(Method::GET)
        .uri(
            "/management/v1/warehouses/local/namespaces/default/views/active_customers/version-receipts",
        )
        .header("x-lakecat-principal", "operator@example.com")
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(receipts_after_drop).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let receipts = body["receipts"].as_array().unwrap();
    assert_eq!(receipts.len(), 3);
    assert_eq!(receipts[2]["view-version"], serde_json::json!(2));
    assert_eq!(receipts[2]["previous-view-version"], serde_json::json!(2));
    assert_eq!(
        receipts[2]["previous-receipt-hash"],
        receipts[1]["receipt-hash"]
    );
    assert_eq!(receipts[2]["operation"], serde_json::json!("drop"));
    assert_eq!(receipts[2]["view-hash"], receipts[1]["view-hash"]);
    assert!(
        receipts[2]["receipt-hash"]
            .as_str()
            .is_some_and(is_full_sha256_hash)
    );
    assert!(
        receipts[2]["view-hash"]
            .as_str()
            .is_some_and(is_full_sha256_hash)
    );

    let chains_after_drop = Request::builder()
        .method(Method::GET)
        .uri("/management/v1/warehouses/local/namespaces/default/view-version-receipt-chains")
        .header("x-lakecat-principal", "operator@example.com")
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(chains_after_drop).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let chains = body["chains"].as_array().unwrap();
    assert_eq!(chains.len(), 2);
    let active_chain = chains
        .iter()
        .find(|chain| chain["name"] == serde_json::json!("catalog_customers"))
        .unwrap();
    assert_eq!(active_chain["tombstoned"], serde_json::json!(true));
    assert_eq!(active_chain["latest-operation"], serde_json::json!("drop"));
    assert_eq!(active_chain["chain-verified"], serde_json::json!(true));
    assert!(
        active_chain["chain-hash"]
            .as_str()
            .is_some_and(is_full_sha256_hash)
    );
    let dropped_chain = chains
        .iter()
        .find(|chain| chain["name"] == serde_json::json!("active_customers"))
        .unwrap();
    assert_eq!(dropped_chain["tombstoned"], serde_json::json!(true));
    assert_eq!(dropped_chain["latest-view-version"], serde_json::json!(2));
    assert_eq!(dropped_chain["latest-operation"], serde_json::json!("drop"));
    assert_eq!(dropped_chain["receipt-count"], serde_json::json!(3));
    assert_eq!(dropped_chain["chain-verified"], serde_json::json!(true));
    assert!(
        dropped_chain["chain-hash"]
            .as_str()
            .is_some_and(is_full_sha256_hash)
    );
    assert_ne!(active_chain["chain-hash"], dropped_chain["chain-hash"]);

    let list = Request::builder()
        .method(Method::GET)
        .uri("/management/v1/warehouses/local/namespaces/default/views")
        .header("x-lakecat-principal", "operator@example.com")
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(list).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(body["views"].as_array().unwrap().len(), 0);

    let repeated_drop = Request::builder()
        .method(Method::DELETE)
        .uri("/management/v1/warehouses/local/namespaces/default/views/active_customers")
        .header("x-lakecat-principal", "operator@example.com")
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(repeated_drop).await.unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let recreate = Request::builder()
        .method(Method::PUT)
        .uri("/management/v1/warehouses/local/namespaces/default/views/active_customers")
        .header("content-type", "application/json")
        .header("x-lakecat-principal", "operator@example.com")
        .body(Body::from(
            serde_json::json!({
                "sql": "select id from customers where active",
                "dialect": "sql",
                "schema-version": 3,
                "properties": {
                    "semantic-domain": "customer"
                }
            })
            .to_string(),
        ))
        .unwrap();
    let response = app.clone().oneshot(recreate).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(body["view-version"], serde_json::json!(3));

    let receipts_after_recreate = Request::builder()
        .method(Method::GET)
        .uri(
            "/management/v1/warehouses/local/namespaces/default/views/active_customers/version-receipts",
        )
        .header("x-lakecat-principal", "operator@example.com")
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(receipts_after_recreate).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let receipts = body["receipts"].as_array().unwrap();
    assert_eq!(receipts.len(), 4);
    assert_eq!(receipts[3]["view-version"], serde_json::json!(3));
    assert_eq!(receipts[3]["previous-view-version"], serde_json::json!(2));
    assert_eq!(
        receipts[3]["previous-receipt-hash"],
        receipts[2]["receipt-hash"]
    );
    assert_eq!(receipts[3]["operation"], serde_json::json!("upsert"));
    assert_ne!(receipts[3]["view-hash"], receipts[2]["view-hash"]);
    assert!(
        receipts[3]["receipt-hash"]
            .as_str()
            .is_some_and(is_full_sha256_hash)
    );

    let chains_after_recreate = Request::builder()
        .method(Method::GET)
        .uri("/management/v1/warehouses/local/namespaces/default/view-version-receipt-chains")
        .header("x-lakecat-principal", "operator@example.com")
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(chains_after_recreate).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let chains = body["chains"].as_array().unwrap();
    let recreated_chain = chains
        .iter()
        .find(|chain| chain["name"] == serde_json::json!("active_customers"))
        .unwrap();
    assert_eq!(recreated_chain["tombstoned"], serde_json::json!(false));
    assert_eq!(recreated_chain["latest-view-version"], serde_json::json!(3));
    assert_eq!(
        recreated_chain["latest-operation"],
        serde_json::json!("upsert")
    );
    assert_eq!(recreated_chain["receipt-count"], serde_json::json!(4));
    assert_eq!(recreated_chain["chain-verified"], serde_json::json!(true));
    assert!(
        recreated_chain["chain-hash"]
            .as_str()
            .is_some_and(is_full_sha256_hash)
    );

    let missing = Request::builder()
        .method(Method::GET)
        .uri("/catalog/v1/local/namespaces/default/views/missing_view")
        .header("x-lakecat-principal", "operator@example.com")
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(missing).await.unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn management_storage_profile_rejects_provider_prefix_mismatch() {
    let app = test_app();
    let upsert = Request::builder()
        .method(Method::PUT)
        .uri("/management/v1/warehouses/local/storage-profiles/wrong-provider")
        .header("content-type", "application/json")
        .header("x-lakecat-principal", "operator@example.com")
        .body(Body::from(
            serde_json::json!({
                "location-prefix": "s3://lakecat-demo/events",
                "provider": "file",
                "issuance-mode": "local-file-no-secret"
            })
            .to_string(),
        ))
        .unwrap();
    let response = app.oneshot(upsert).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let message = body["error"]["message"].as_str().unwrap();
    assert!(message.contains("storage-profile-prefix-hash=sha256:"));
    assert!(!message.contains("s3://lakecat-demo/events"));
    assert!(!message.contains("lakecat-demo"));
}

#[tokio::test]
async fn management_storage_profile_rejects_decorated_location_prefixes() {
    let app = test_app();
    let upsert = Request::builder()
        .method(Method::PUT)
        .uri("/management/v1/warehouses/local/storage-profiles/decorated-prefix")
        .header("content-type", "application/json")
        .header("x-lakecat-principal", "operator@example.com")
        .body(Body::from(
            serde_json::json!({
                "location-prefix": "s3://lakecat-demo/events?token=raw-secret",
                "provider": "s3",
                "issuance-mode": "governed-read-required"
            })
            .to_string(),
        ))
        .unwrap();
    let response = app.oneshot(upsert).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let message = body["error"]["message"].as_str().unwrap();
    assert!(message.contains("query strings, fragments, or userinfo"));
    assert!(message.contains("storage-profile-prefix-hash=sha256:"));
    assert!(!message.contains("s3://lakecat-demo/events"));
    assert!(!message.contains("lakecat-demo"));
    assert!(!message.contains("token=raw-secret"));
    assert!(!message.contains("raw-secret"));
}

#[tokio::test]
async fn management_storage_profile_rejects_remote_local_no_secret_mode() {
    let app = test_app();
    let upsert = Request::builder()
        .method(Method::PUT)
        .uri("/management/v1/warehouses/local/storage-profiles/remote-no-secret")
        .header("content-type", "application/json")
        .header("x-lakecat-principal", "operator@example.com")
        .body(Body::from(
            serde_json::json!({
                "location-prefix": "s3://lakecat-demo/events",
                "provider": "s3",
                "issuance-mode": "local-file-no-secret"
            })
            .to_string(),
        ))
        .unwrap();
    let response = app.oneshot(upsert).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let message = body["error"]["message"].as_str().unwrap();
    assert!(message.contains("local-file-no-secret issuance mode requires file provider"));
    assert!(message.contains("storage-profile-prefix-hash=sha256:"));
    assert!(!message.contains("s3://lakecat-demo/events"));
    assert!(!message.contains("lakecat-demo"));
    assert!(!message.contains("raw-secret"));
}

#[tokio::test]
async fn management_storage_profile_rejects_public_secret_values() {
    let app = test_app();
    let upsert = Request::builder()
        .method(Method::PUT)
        .uri("/management/v1/warehouses/local/storage-profiles/public-secret")
        .header("content-type", "application/json")
        .header("x-lakecat-principal", "operator@example.com")
        .body(Body::from(
            serde_json::json!({
                "location-prefix": "s3://lakecat-demo/events",
                "provider": "s3",
                "issuance-mode": "short-lived-secret-ref",
                "secret-ref": "typesec://lakecat/local/s3-events",
                "public-config": {
                    "lakecat.endpoint": "https://storage.example.invalid?token=raw-secret"
                }
            })
            .to_string(),
        ))
        .unwrap();
    let response = app.oneshot(upsert).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn management_storage_profile_rejects_reserved_public_config_keys() {
    let app = test_app();
    let upsert = Request::builder()
        .method(Method::PUT)
        .uri("/management/v1/warehouses/local/storage-profiles/reserved-public-config")
        .header("content-type", "application/json")
        .header("x-lakecat-principal", "operator@example.com")
        .body(Body::from(
            serde_json::json!({
                "location-prefix": "file:///tmp/events",
                "provider": "file",
                "issuance-mode": "local-file-no-secret",
                "public-config": {
                    "lakecat.storage-profile-id": "shadow-profile"
                }
            })
            .to_string(),
        ))
        .unwrap();
    let response = app.oneshot(upsert).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let message = body["error"]["message"].as_str().unwrap();
    assert!(message.contains("reserved for LakeCat credential evidence"));
    assert!(message.contains("public-config-key-hash=sha256:"));
    assert!(!message.contains("lakecat.storage-profile-id"));
}

#[tokio::test]
async fn policy_bindings_are_governed_and_attached_to_table_authorization_context() {
    let governance = Arc::new(RecordingGovernance::default());
    let store = MemoryCatalogStore::new();
    let app = app(
        LakeCatState::new(WarehouseName::new("local").unwrap(), store.clone()).with_integrations(
            default_sail_engine(),
            governance.clone(),
            NoopCatalogGraphSink::new(),
            HashOnlyLineageSink::new(),
        ),
    );

    let upsert = Request::builder()
        .method(Method::PUT)
        .uri("/management/v1/warehouses/local/policies/agent-read")
        .header("content-type", "application/json")
        .header("x-lakecat-principal", "operator@example.com")
        .body(Body::from(
            serde_json::json!({
                "namespace": ["default"],
                "table": "events",
                "enforced": true,
                "odrl": {
                    "uid": "policy:agent-read",
                    "permission": [{
                        "action": "read",
                        "constraint": [{
                            "leftOperand": "purpose",
                            "operator": "eq",
                            "rightOperand": "resilience-demo"
                        }]
                    }]
                }
            })
            .to_string(),
        ))
        .unwrap();
    let response = app.clone().oneshot(upsert).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let policy_outbox = store
        .pending_outbox_events(Some("lakecat.lineage-and-graph"), 10)
        .await
        .unwrap()
        .into_iter()
        .find(|event| event.event_type == "policy-binding.upserted")
        .expect("policy upsert should record outbox evidence");
    let policy_payload = &policy_outbox.payload["payload"]["policy"];
    assert_eq!(
        policy_payload["odrl-hash"],
        serde_json::json!(content_hash_json(&policy_payload["odrl"]).unwrap())
    );

    let list = Request::builder()
        .method(Method::GET)
        .uri("/management/v1/warehouses/local/policies")
        .header("x-lakecat-principal", "operator@example.com")
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(list).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(body["policies"].as_array().unwrap().len(), 1);

    let create = Request::builder()
        .method(Method::POST)
        .uri("/catalog/v1/namespaces/default/tables")
        .header("content-type", "application/json")
        .body(Body::from(
            r#"{"name":"events","location":"file:///tmp/events","metadata-location":"file:///tmp/events/metadata/00000.json","metadata":{"format-version":3,"current-schema-id":1,"schemas":[{"schema-id":1,"fields":[{"id":1,"name":"event_id","type":"string","required":true}]}]}}"#,
        ))
        .unwrap();
    let response = app.clone().oneshot(create).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let load = Request::builder()
        .method(Method::GET)
        .uri("/catalog/v1/namespaces/default/tables/events")
        .header("x-lakecat-agent-did", "did:example:agent")
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(load).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let contexts = governance.contexts.lock().await;
    let load_context = contexts
        .iter()
        .find(|context| {
            context["policy-bindings"]
                .as_array()
                .is_some_and(|bindings| !bindings.is_empty())
        })
        .expect("table authorization should include active policy bindings");
    assert_eq!(
        load_context["policy-bindings"][0]["policy-id"],
        serde_json::json!("agent-read")
    );
    assert_eq!(
        load_context["policy-bindings"][0]["odrl"]["uid"],
        serde_json::json!("policy:agent-read")
    );
}

#[test]
fn projection_receipt_evidence_rejects_malformed_lineage_hashes() {
    let event = OutboxEvent {
        event_id: "evt-malformed-projection-receipt".to_string(),
        sink: "lakecat.lineage-and-graph".to_string(),
        event_type: "table.scan-planned".to_string(),
        payload: json!({
            "audit-event-id": "audit-malformed-projection-receipt",
            "event-type": "table.scan-planned",
            "payload": {}
        }),
        created_at: chrono::Utc::now(),
        delivered_at: None,
    };
    let receipt = OutboxProjectionReceipt {
        graph_events: 1,
        lineage_events: 1,
        lineage_event_hashes: vec!["sha256:short".to_string()],
        open_lineage_hashes: vec![content_hash_bytes(b"openlineage")],
    };

    let err = validate_projection_receipt_evidence(&event, &receipt)
        .expect_err("projection receipt hashes must be full SHA-256 evidence");

    let message = err.to_string();
    assert!(message.contains("table.scan-planned"));
    assert!(
        message.contains("projection receipt replay event hashes must contain full SHA-256 hashes")
    );
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains("evt-malformed-projection-receipt"));
}

#[test]
fn projection_receipt_evidence_rejects_malformed_openlineage_hashes() {
    let event = OutboxEvent {
        event_id: "evt-malformed-openlineage-projection-receipt".to_string(),
        sink: "lakecat.lineage-and-graph".to_string(),
        event_type: "credentials.vend-attempted".to_string(),
        payload: json!({
            "audit-event-id": "audit-malformed-openlineage-projection-receipt",
            "event-type": "credentials.vend-attempted",
            "payload": {}
        }),
        created_at: chrono::Utc::now(),
        delivered_at: None,
    };
    let receipt = OutboxProjectionReceipt {
        graph_events: 1,
        lineage_events: 1,
        lineage_event_hashes: vec![content_hash_bytes(b"credential-replay")],
        open_lineage_hashes: vec!["sha256:short".to_string()],
    };

    let err = validate_projection_receipt_evidence(&event, &receipt)
        .expect_err("projection OpenLineage hashes must be full SHA-256 evidence");

    let message = err.to_string();
    assert!(message.contains("credentials.vend-attempted"));
    assert!(
        message.contains("projection receipt OpenLineage hashes must contain full SHA-256 hashes")
    );
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains("evt-malformed-openlineage-projection-receipt"));
}

#[test]
fn projection_receipt_evidence_rejects_duplicate_openlineage_hashes() {
    let event = OutboxEvent {
        event_id: "evt-duplicate-projection-receipt".to_string(),
        sink: "lakecat.lineage-and-graph".to_string(),
        event_type: "table.scan-tasks-fetched".to_string(),
        payload: json!({
            "audit-event-id": "audit-duplicate-projection-receipt",
            "event-type": "table.scan-tasks-fetched",
            "payload": {}
        }),
        created_at: chrono::Utc::now(),
        delivered_at: None,
    };
    let duplicate_openlineage_hash = content_hash_bytes(b"duplicate-openlineage");
    let receipt = OutboxProjectionReceipt {
        graph_events: 1,
        lineage_events: 2,
        lineage_event_hashes: vec![
            content_hash_bytes(b"lineage-a"),
            content_hash_bytes(b"lineage-b"),
        ],
        open_lineage_hashes: vec![
            duplicate_openlineage_hash.clone(),
            duplicate_openlineage_hash,
        ],
    };

    let err = validate_projection_receipt_evidence(&event, &receipt)
        .expect_err("projection receipt hashes must be duplicate-free");

    let message = err.to_string();
    assert!(message.contains("table.scan-tasks-fetched"));
    assert!(message.contains("projection receipt OpenLineage hashes must be duplicate-free"));
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains("evt-duplicate-projection-receipt"));
}

#[test]
fn projection_receipt_evidence_rejects_duplicate_replay_event_hashes() {
    let event = OutboxEvent {
        event_id: "evt-duplicate-replay-projection-receipt".to_string(),
        sink: "lakecat.lineage-and-graph".to_string(),
        event_type: "table.commits-listed".to_string(),
        payload: json!({
            "audit-event-id": "audit-duplicate-replay-projection-receipt",
            "event-type": "table.commits-listed",
            "payload": {}
        }),
        created_at: chrono::Utc::now(),
        delivered_at: None,
    };
    let duplicate_replay_hash = content_hash_bytes(b"duplicate-replay-event");
    let receipt = OutboxProjectionReceipt {
        graph_events: 1,
        lineage_events: 2,
        lineage_event_hashes: vec![duplicate_replay_hash.clone(), duplicate_replay_hash],
        open_lineage_hashes: vec![
            content_hash_bytes(b"openlineage-a"),
            content_hash_bytes(b"openlineage-b"),
        ],
    };

    let err = validate_projection_receipt_evidence(&event, &receipt)
        .expect_err("projection replay event hashes must be duplicate-free");

    let message = err.to_string();
    assert!(message.contains("table.commits-listed"));
    assert!(message.contains("projection receipt replay event hashes must be duplicate-free"));
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains("evt-duplicate-replay-projection-receipt"));
}

#[test]
fn projection_receipt_evidence_rejects_hash_count_drift() {
    let event = OutboxEvent {
        event_id: "evt-count-drift-projection-receipt".to_string(),
        sink: "lakecat.lineage-and-graph".to_string(),
        event_type: "policy-binding.upserted".to_string(),
        payload: json!({
            "audit-event-id": "audit-count-drift-projection-receipt",
            "event-type": "policy-binding.upserted",
            "payload": {}
        }),
        created_at: chrono::Utc::now(),
        delivered_at: None,
    };
    let receipt = OutboxProjectionReceipt {
        graph_events: 1,
        lineage_events: 2,
        lineage_event_hashes: vec![content_hash_bytes(b"lineage-a")],
        open_lineage_hashes: vec![content_hash_bytes(b"openlineage-a")],
    };

    let err = validate_projection_receipt_evidence(&event, &receipt)
        .expect_err("projection receipt hash counts must match lineage count");

    let message = err.to_string();
    assert!(message.contains("policy-binding.upserted"));
    assert!(message.contains("projection receipt hash counts must match lineage event count"));
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains("evt-count-drift-projection-receipt"));
}

#[test]
fn effective_projection_cannot_widen_policy_columns() {
    let restriction = ReadRestriction {
        allowed_columns: Some(vec!["event_id".to_string()]),
        ..ReadRestriction::unrestricted()
    };
    assert_eq!(
        restriction.effective_projection(&[]).unwrap(),
        vec!["event_id".to_string()]
    );
    assert_eq!(
        restriction
            .effective_projection(&["event_id".to_string(), "payload".to_string()])
            .unwrap(),
        vec!["event_id".to_string()]
    );
    assert!(
        restriction
            .effective_projection(&["payload".to_string()])
            .is_err()
    );
}
