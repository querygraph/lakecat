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
    ListTablesResponse, ListViewVersionReceiptChainsResponse, ListViewVersionReceiptsResponse,
    ListViewsResponse, ListWarehousesResponse, LoadCredentialsResponse, LoadTableResponse,
    NamespaceResponse, PlanTableScanRequest, PlanTableScanResponse, PolicyBindingResponse,
    ProjectResponse, ServerResponse, StorageCredential, StorageProfileResponse,
    TableCommitRecordResponse, TableIdentifier, UpsertPolicyBindingRequest, UpsertProjectRequest,
    UpsertServerRequest, UpsertStorageProfileRequest, UpsertViewRequest, UpsertWarehouseRequest,
    ViewColumnResponse, ViewResponse, ViewVersionReceiptChainResponse, ViewVersionReceiptResponse,
    WarehouseResponse,
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

#[tokio::test]
async fn create_table_generates_metadata_from_standard_schema() {
    // Spec `createTable`: client sends name + schema (no metadata, no
    // location); the catalog generates the initial metadata and a location.
    let app = test_app();
    let create = Request::builder()
        .method(Method::POST)
        .uri("/catalog/v1/namespaces/default/tables")
        .header("content-type", "application/json")
        .body(Body::from(
            r#"{"name":"events","schema":{"type":"struct","schema-id":0,"fields":[{"id":1,"name":"id","required":true,"type":"long"},{"id":2,"name":"name","required":false,"type":"string"}]}}"#,
        ))
        .unwrap();
    let response = app.clone().oneshot(create).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
    // Server generated a metadata location and a valid empty table.
    assert!(
        payload["metadata-location"]
            .as_str()
            .unwrap()
            .contains("/metadata/")
    );
    assert_eq!(payload["metadata"]["format-version"], serde_json::json!(2));
    assert_eq!(
        payload["metadata"]["current-schema-id"],
        serde_json::json!(0)
    );
    assert_eq!(payload["metadata"]["last-column-id"], serde_json::json!(2));
    assert_eq!(payload["metadata"]["snapshots"], serde_json::json!([]));
    assert!(payload["metadata"]["table-uuid"].as_str().is_some());

    // The generated table loads and commits via the bare spec path.
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

#[test]
fn metadata_write_plan_requires_metadata_location() {
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
        new_metadata_location: None,
        new_metadata: serde_json::json!({"format-version": 3}),
        metadata_write_required: true,
        metadata_patch: serde_json::json!({}),
    };

    let err = validate_planned_metadata_location(&plan, None, &storage_profile).unwrap_err();
    assert!(matches!(err, LakeCatError::InvalidArgument(_)));
    assert!(
        err.to_string()
            .contains("metadata object commit requires a new metadata location")
    );
}

#[test]
fn metadata_write_plan_rejects_dot_segment_locations() {
    let table = TableRecord::new(
        table_ident("local", "default", "events").unwrap(),
        "file:///tmp/events".to_string(),
        Some("file:///tmp/events/metadata/00000.json".to_string()),
        serde_json::json!({"format-version": 3}),
        Principal::anonymous(),
    );
    let storage_profile = StorageProfile::inferred_for_table(&table);
    for location in [
        "file:///tmp/events/metadata/../00001.json",
        "file:///tmp/events/metadata/%2e%2e/00001.json",
    ] {
        let plan = lakecat_core::sail::CommitPlan {
            prepared_by: "test".to_string(),
            requirements: Vec::new(),
            updates: Vec::new(),
            new_metadata_location: Some(location.to_string()),
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
        assert!(message.contains("dot path segments"));
        assert!(message.contains("metadata-location-hash=sha256:"));
        assert!(!message.contains(location));
        assert!(!message.contains("00001.json"));
    }
}

#[test]
fn metadata_write_plan_rejects_query_or_fragment_locations() {
    let table = TableRecord::new(
        table_ident("local", "default", "events").unwrap(),
        "file:///tmp/events".to_string(),
        Some("file:///tmp/events/metadata/00000.json".to_string()),
        serde_json::json!({"format-version": 3}),
        Principal::anonymous(),
    );
    let storage_profile = StorageProfile::inferred_for_table(&table);
    for location in [
        "file:///tmp/events/metadata/00001.json?version=staged",
        "file:///tmp/events/metadata/00001.json#staged",
    ] {
        let plan = lakecat_core::sail::CommitPlan {
            prepared_by: "test".to_string(),
            requirements: Vec::new(),
            updates: Vec::new(),
            new_metadata_location: Some(location.to_string()),
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
        assert!(message.contains("query strings or fragments"));
        assert!(message.contains("metadata-location-hash=sha256:"));
        assert!(!message.contains(location));
        assert!(!message.contains("00001.json"));
    }
}

#[test]
fn metadata_write_plan_rejects_userinfo_locations() {
    let table = TableRecord::new(
        table_ident("local", "default", "events").unwrap(),
        "s3://lakecat-demo/events".to_string(),
        Some("s3://lakecat-demo/events/metadata/00000.json".to_string()),
        serde_json::json!({"format-version": 3}),
        Principal::anonymous(),
    );
    let storage_profile = StorageProfile::new(
        "s3-events",
        WarehouseName::new("local").unwrap(),
        "s3://lakecat-demo/events",
        StorageProvider::S3,
        CredentialIssuanceMode::ShortLivedSecretRef,
        Some("typesec://lakecat/local/s3-events".to_string()),
        BTreeMap::new(),
    )
    .unwrap();
    let location = "s3://access:secret@lakecat-demo/events/metadata/00001.json";
    let plan = lakecat_core::sail::CommitPlan {
        prepared_by: "test".to_string(),
        requirements: Vec::new(),
        updates: Vec::new(),
        new_metadata_location: Some(location.to_string()),
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
    assert!(message.contains("userinfo"));
    assert!(message.contains("metadata-location-hash=sha256:"));
    assert!(!message.contains(location));
    assert!(!message.contains("access"));
    assert!(!message.contains("secret"));
    assert!(!message.contains("00001.json"));
}

#[test]
fn metadata_cleanup_error_redacts_metadata_location() {
    let location = "file:///tmp/lakecat-secret/events/metadata/00001.json";
    let err = metadata_cleanup_error(
        location,
        "permission denied at /tmp/lakecat-secret/events/metadata/00001.json",
    );
    let message = err.to_string();

    assert!(matches!(err, LakeCatError::Internal(_)));
    assert!(message.contains("metadata-location-hash=sha256:"));
    assert!(message.contains("error-detail-hash=sha256:"));
    assert!(!message.contains("lakecat-secret"));
    assert!(!message.contains("00001.json"));
    assert!(!message.contains("permission denied"));
}

#[test]
fn metadata_write_error_redacts_backend_detail() {
    let location = "file:///tmp/lakecat-secret/events/metadata/00001.json";
    let err = metadata_object_write_error(
        location,
        "backend write failed for /tmp/lakecat-secret/events/metadata/00001.json",
    );
    let message = err.to_string();

    assert!(matches!(err, LakeCatError::Internal(_)));
    assert!(message.contains("metadata-location-hash=sha256:"));
    assert!(message.contains("error-detail-hash=sha256:"));
    assert!(!message.contains("lakecat-secret"));
    assert!(!message.contains("00001.json"));
    assert!(!message.contains("backend write failed"));
}

#[test]
fn metadata_object_store_redacts_invalid_location_parse_failures() {
    let location = "not a uri /tmp/lakecat-secret/events/metadata/00001.json";
    let err = metadata_object_store(location).unwrap_err();
    let message = err.to_string();

    assert!(matches!(err, LakeCatError::InvalidArgument(_)));
    assert!(message.contains("metadata-location-hash=sha256:"));
    assert!(message.contains("backend-error-hash=sha256:"));
    assert!(!message.contains("error-detail-hash=sha256:"));
    assert!(!message.contains(location));
    assert!(!message.contains("lakecat-secret"));
    assert!(!message.contains("00001.json"));
    assert!(!message.contains("relative URL"));
}

#[test]
fn metadata_object_store_redacts_unsupported_backend_setup_failures() {
    let location = "ftp://lakecat-secret/events/metadata/00001.json";
    let err = metadata_object_store(location).unwrap_err();
    let message = err.to_string();

    assert!(matches!(err, LakeCatError::InvalidArgument(_)));
    assert!(message.contains("metadata-location-hash=sha256:"));
    assert!(message.contains("backend-error-hash=sha256:"));
    assert!(!message.contains("error-detail-hash=sha256:"));
    assert!(!message.contains(location));
    assert!(!message.contains("lakecat-secret"));
    assert!(!message.contains("00001.json"));
    assert!(!message.contains("ftp"));
}

#[test]
fn metadata_cleanup_retry_delay_is_bounded_and_increasing() {
    assert_eq!(METADATA_CLEANUP_DELETE_ATTEMPTS, 3);
    assert_eq!(metadata_cleanup_retry_delay(0).as_millis(), 25);
    assert_eq!(metadata_cleanup_retry_delay(1).as_millis(), 50);
}

#[tokio::test]
async fn metadata_cleanup_skips_previous_metadata_pointer() {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("lakecat-current-cleanup-{unique}"));
    let metadata_dir = root.join("events").join("metadata");
    std::fs::create_dir_all(&metadata_dir).unwrap();
    let current_metadata = metadata_dir.join("00000.json");
    let sentinel = "{\n  \"sentinel\": \"committed metadata must survive cleanup\"\n}\n";
    std::fs::write(&current_metadata, sentinel).unwrap();
    let current_metadata_location = url::Url::from_file_path(&current_metadata)
        .unwrap()
        .to_string();

    cleanup_planned_metadata(
        Some(PlannedMetadataWrite {
            location: current_metadata_location.clone(),
        }),
        Some(&current_metadata_location),
    )
    .await
    .expect("cleanup should skip the previous committed metadata pointer");

    assert_eq!(
        std::fs::read_to_string(&current_metadata).unwrap(),
        sentinel
    );
    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test]
async fn delete_table_soft_deletes_from_catalog_reads() {
    let app = test_app();
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

    let delete = Request::builder()
        .method(Method::DELETE)
        .uri("/catalog/v1/namespaces/default/tables/events")
        .header("x-lakecat-principal", "operator@example.com")
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(delete).await.unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let load = Request::builder()
        .method(Method::GET)
        .uri("/catalog/v1/namespaces/default/tables/events")
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(load).await.unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let delete_again = Request::builder()
        .method(Method::DELETE)
        .uri("/catalog/v1/namespaces/default/tables/events")
        .header("x-lakecat-principal", "operator@example.com")
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(delete_again).await.unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn restore_table_reopens_soft_deleted_catalog_reads() {
    let app = test_app();
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

    let delete = Request::builder()
        .method(Method::DELETE)
        .uri("/catalog/v1/namespaces/default/tables/events")
        .header("x-lakecat-principal", "operator@example.com")
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(delete).await.unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let restore = Request::builder()
        .method(Method::POST)
        .uri("/management/v1/warehouses/local/namespaces/default/tables/events/restore")
        .header("x-lakecat-principal", "operator@example.com")
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(restore).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(body["identifier"]["name"], serde_json::json!("events"));

    let load = Request::builder()
        .method(Method::GET)
        .uri("/catalog/v1/namespaces/default/tables/events")
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(load).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

async fn create_named_table(app: &Router, namespace: &str, name: &str) {
    let create = Request::builder()
        .method(Method::POST)
        .uri(format!("/catalog/v1/namespaces/{namespace}/tables"))
        .header("content-type", "application/json")
        .body(Body::from(format!(
            r#"{{"name":"{name}","location":"file:///tmp/{name}","metadata-location":"file:///tmp/{name}/metadata/00000.json","metadata":{{"format-version":3,"current-schema-id":1,"schemas":[{{"schema-id":1,"fields":[{{"id":1,"name":"event_id","type":"string","required":true}}]}}]}}}}"#
        )))
        .unwrap();
    let response = app.clone().oneshot(create).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn list_tables_returns_identifiers_for_namespace() {
    // Iceberg REST `listTables`: GET on the `/tables` collection returns the
    // table identifiers in the requested namespace.
    let app = test_app();
    create_named_table(&app, "default", "events").await;
    create_named_table(&app, "default", "metrics").await;

    let list = Request::builder()
        .method(Method::GET)
        .uri("/catalog/v1/namespaces/default/tables")
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(list).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();

    // Assert the wire shape: `identifiers` is an array of
    // `{namespace:[...], name:"..."}` objects.
    let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let identifiers = value["identifiers"].as_array().unwrap();
    assert_eq!(identifiers.len(), 2);
    for identifier in identifiers {
        assert_eq!(identifier["namespace"], serde_json::json!(["default"]));
        assert!(identifier["name"].is_string());
    }

    // And it deserializes into the typed response.
    let parsed: ListTablesResponse = serde_json::from_slice(&body).unwrap();
    let mut names: Vec<String> = parsed
        .identifiers
        .iter()
        .map(|identifier| identifier.name.clone())
        .collect();
    names.sort();
    assert_eq!(names, vec!["events".to_string(), "metrics".to_string()]);
    assert!(
        parsed
            .identifiers
            .iter()
            .all(|identifier| identifier.namespace == vec!["default".to_string()])
    );
}

#[tokio::test]
async fn list_tables_excludes_other_namespaces() {
    // A table in a different namespace must not appear in the listing.
    let app = test_app();
    create_named_table(&app, "default", "events").await;
    create_named_table(&app, "other", "secrets").await;

    let list = Request::builder()
        .method(Method::GET)
        .uri("/catalog/v1/namespaces/default/tables")
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(list).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let parsed: ListTablesResponse = serde_json::from_slice(&body).unwrap();
    let names: Vec<String> = parsed
        .identifiers
        .iter()
        .map(|identifier| identifier.name.clone())
        .collect();
    assert_eq!(names, vec!["events".to_string()]);
    assert!(!names.contains(&"secrets".to_string()));
}
