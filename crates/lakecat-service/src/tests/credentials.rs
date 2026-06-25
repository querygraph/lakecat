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
fn credential_vend_rejects_extra_read_restriction_fields() {
    let event = OutboxEvent {
        event_id: "evt-credential-extra-read-restriction".to_string(),
        sink: "lakecat.lineage-and-graph".to_string(),
        event_type: "credentials.vend-attempted".to_string(),
        payload: json!({}),
        created_at: chrono::Utc::now(),
        delivered_at: None,
    };
    let read_restriction = json!({
        "allowed-columns": ["event_id"],
        "row-predicate": {
            "type": "always-true"
        },
        "purpose": "qglake-agent-demo",
        "max-credential-ttl-seconds": 300,
        "policy-hashes": [
            content_hash_json(&json!({"policy-id": "agent-read", "scope": "default.events"}))
                .unwrap()
        ],
        "unverified-restriction-claim": "credential-safe"
    });
    let payload = json!({
        "authorization-receipt": {
            "context": {
                "read-restriction": read_restriction
            }
        },
        "read-restriction": read_restriction
    });

    let err = validate_credential_vend_event_evidence(&event, &payload)
        .expect_err("credential read-restriction evidence should reject extra fields");

    let message = err.to_string();
    assert!(message.contains("credentials.vend-attempted"));
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(message.contains(
        "credential-vend read-restriction contains unexpected field unverified-restriction-claim"
    ));
    assert!(!message.contains("evt-credential-extra-read-restriction"));
}

#[test]
fn storage_profile_event_payload_redacts_secret_ref_and_location_prefix() {
    let profile = StorageProfile::new(
        "s3-events",
        WarehouseName::new("local").unwrap(),
        "s3://lakecat/events",
        StorageProvider::S3,
        CredentialIssuanceMode::ShortLivedSecretRef,
        Some("typesec://env/LAKECAT_S3_EVENTS".to_string()),
        BTreeMap::from([("lakecat.region".to_string(), "us-west-2".to_string())]),
    )
    .unwrap();

    let payload = storage_profile_event_payload(&profile);
    assert_eq!(payload["profile-id"], serde_json::json!("s3-events"));
    assert_eq!(
        payload["location-prefix-hash"],
        serde_json::json!(
            content_hash_json(&json!({"location-prefix": "s3://lakecat/events"})).unwrap()
        )
    );
    assert!(
        payload["location-prefix-hash"]
            .as_str()
            .is_some_and(is_full_sha256_hash)
    );
    assert_eq!(payload["secret-ref-present"], serde_json::json!(true));
    assert_eq!(payload["secret-ref-provider"], serde_json::json!("typesec"));
    assert_eq!(
        payload["secret-ref-hash"],
        serde_json::json!(content_hash_bytes(
            "typesec://env/LAKECAT_S3_EVENTS".as_bytes()
        ))
    );
    assert!(payload.get("location-prefix").is_none());
    assert!(payload.get("secret-ref").is_none());
}

#[test]
fn metadata_write_plan_rejects_credential_markers_in_location_paths() {
    let table = TableRecord::new(
        table_ident("local", "default", "events").unwrap(),
        "file:///tmp/events".to_string(),
        Some("file:///tmp/events/metadata/00000.json".to_string()),
        serde_json::json!({"format-version": 3}),
        Principal::anonymous(),
    );
    let storage_profile = StorageProfile::inferred_for_table(&table);
    for location in [
        "file:///tmp/events/metadata/token=raw-secret.json",
        "file:///tmp/events/metadata/token%3Draw-secret.json",
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
        assert!(message.contains("credential material"));
        assert!(message.contains("metadata-location-hash=sha256:"));
        assert!(!message.contains(location));
        assert!(!message.contains("raw-secret"));
    }
}

#[tokio::test]
async fn load_credentials_returns_scoped_local_file_profile_without_raw_secrets() {
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

    let credentials = Request::builder()
        .method(Method::GET)
        .uri("/catalog/v1/namespaces/default/tables/events/credentials")
        .header("x-lakecat-agent-did", "did:example:agent")
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(credentials).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let credentials = body["storage-credentials"].as_array().unwrap();
    assert_eq!(credentials.len(), 1);
    assert_eq!(
        credentials[0]["prefix"],
        serde_json::json!("file:///tmp/events")
    );
    let config = credentials[0]["config"].as_array().unwrap();
    assert!(config.iter().any(|entry| {
        entry["key"] == "lakecat.credential-mode" && entry["value"] == "local-file-no-secret"
    }));
    assert!(!config.iter().any(|entry| {
        entry["key"]
            .as_str()
            .is_some_and(|key| key.contains("secret") || key.contains("token"))
    }));
}

#[tokio::test]
async fn load_credentials_returns_empty_for_remote_profile_until_issuance_exists() {
    let app = test_app();
    let create = Request::builder()
        .method(Method::POST)
        .uri("/catalog/v1/namespaces/default/tables")
        .header("content-type", "application/json")
        .body(Body::from(
            r#"{"name":"events","location":"s3://lakecat-demo/events","metadata-location":"s3://lakecat-demo/events/metadata/00000.json","metadata":{"format-version":3,"current-schema-id":1,"schemas":[{"schema-id":1,"fields":[{"id":1,"name":"event_id","type":"string","required":true}]}]}}"#,
        ))
        .unwrap();
    let response = app.clone().oneshot(create).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let credentials = Request::builder()
        .method(Method::GET)
        .uri("/catalog/v1/namespaces/default/tables/events/credentials")
        .header("x-lakecat-agent-did", "did:example:agent")
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(credentials).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(body["storage-credentials"], serde_json::json!([]));
}

#[tokio::test]
async fn management_storage_profile_overrides_inferred_credentials_by_prefix() {
    let app = test_app();
    let upsert = Request::builder()
        .method(Method::PUT)
        .uri("/management/v1/warehouses/local/storage-profiles/local-events")
        .header("content-type", "application/json")
        .header("x-lakecat-principal", "operator@example.com")
        .body(Body::from(
            serde_json::json!({
                "location-prefix": "file:///tmp/events",
                "provider": "file",
                "issuance-mode": "local-file-no-secret",
                "public-config": {
                    "lakecat.endpoint": "local"
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
    assert_eq!(body["profile-id"], serde_json::json!("local-events"));

    let list = Request::builder()
        .method(Method::GET)
        .uri("/management/v1/warehouses/local/storage-profiles")
        .header("x-lakecat-principal", "operator@example.com")
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(list).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(body["storage-profiles"].as_array().unwrap().len(), 1);

    let create = Request::builder()
        .method(Method::POST)
        .uri("/catalog/v1/namespaces/default/tables")
        .header("content-type", "application/json")
        .body(Body::from(
            r#"{"name":"events","location":"file:///tmp/events/tenant-a","metadata-location":"file:///tmp/events/tenant-a/metadata/00000.json","metadata":{"format-version":3,"current-schema-id":1,"schemas":[{"schema-id":1,"fields":[{"id":1,"name":"event_id","type":"string","required":true}]}]}}"#,
        ))
        .unwrap();
    let response = app.clone().oneshot(create).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let credentials = Request::builder()
        .method(Method::GET)
        .uri("/catalog/v1/namespaces/default/tables/events/credentials")
        .header("x-lakecat-agent-did", "did:example:agent")
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(credentials).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let credentials = body["storage-credentials"].as_array().unwrap();
    assert_eq!(credentials.len(), 1);
    assert_eq!(
        credentials[0]["prefix"],
        serde_json::json!("file:///tmp/events")
    );
    let config = credentials[0]["config"].as_array().unwrap();
    assert!(config.iter().any(|entry| {
        entry["key"] == "lakecat.storage-profile-id" && entry["value"] == "local-events"
    }));
    assert!(
        config
            .iter()
            .any(|entry| { entry["key"] == "lakecat.endpoint" && entry["value"] == "local" })
    );
}

#[tokio::test]
async fn management_storage_profile_rejects_local_secret_ref_mode() {
    let app = test_app();
    let secret_ref = "typesec://lakecat/local/raw-secret-root";
    let upsert = Request::builder()
        .method(Method::PUT)
        .uri("/management/v1/warehouses/local/storage-profiles/local-secret-ref")
        .header("content-type", "application/json")
        .header("x-lakecat-principal", "operator@example.com")
        .body(Body::from(
            serde_json::json!({
                "location-prefix": "file:///tmp/events",
                "provider": "file",
                "issuance-mode": "short-lived-secret-ref",
                "secret-ref": secret_ref
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
    assert!(message.contains("short-lived-secret-ref issuance mode requires s3, gcs, or azure"));
    assert!(message.contains("storage-profile-prefix-hash=sha256:"));
    assert!(!message.contains("file:///tmp/events"));
    assert!(!message.contains(secret_ref));
    assert!(!message.contains("raw-secret-root"));
}

#[tokio::test]
async fn remote_storage_profile_accepts_secret_ref_without_vending_raw_secrets() {
    let app = test_app();
    let upsert = Request::builder()
        .method(Method::PUT)
        .uri("/management/v1/warehouses/local/storage-profiles/s3-events")
        .header("content-type", "application/json")
        .header("x-lakecat-principal", "operator@example.com")
        .body(Body::from(
            serde_json::json!({
                "location-prefix": "s3://lakecat-demo/events",
                "provider": "s3",
                "issuance-mode": "short-lived-secret-ref",
                "secret-ref": "typesec://lakecat/local/s3-events",
                "public-config": {
                    "lakecat.region": "us-west-2"
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
    let body_text = String::from_utf8(body.to_vec()).unwrap();
    assert!(
        !body_text.contains("typesec://lakecat/local/s3-events"),
        "management storage-profile response must not expose raw secret-ref"
    );
    let body: serde_json::Value = serde_json::from_str(&body_text).unwrap();
    assert!(body.get("secret-ref").is_none());
    assert_eq!(body["secret-ref-present"], serde_json::json!(true));
    assert_eq!(body["secret-ref-provider"], serde_json::json!("typesec"));
    assert!(
        body["secret-ref-hash"]
            .as_str()
            .is_some_and(|hash| hash.starts_with("sha256:"))
    );
    let upsert_secret_ref_hash = body["secret-ref-hash"].clone();

    let list = Request::builder()
        .method(Method::GET)
        .uri("/management/v1/warehouses/local/storage-profiles")
        .header("x-lakecat-principal", "operator@example.com")
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(list).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body_text = String::from_utf8(body.to_vec()).unwrap();
    assert!(
        !body_text.contains("typesec://lakecat/local/s3-events"),
        "management storage-profile list response must not expose raw secret-ref"
    );
    let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let listed = &body["storage-profiles"][0];
    assert!(listed.get("secret-ref").is_none());
    assert_eq!(listed["secret-ref-present"], serde_json::json!(true));
    assert_eq!(listed["secret-ref-provider"], serde_json::json!("typesec"));
    assert_eq!(listed["secret-ref-hash"], upsert_secret_ref_hash);

    let create = Request::builder()
        .method(Method::POST)
        .uri("/catalog/v1/namespaces/default/tables")
        .header("content-type", "application/json")
        .body(Body::from(
            r#"{"name":"events","location":"s3://lakecat-demo/events/tenant-a","metadata-location":"s3://lakecat-demo/events/tenant-a/metadata/00000.json","metadata":{"format-version":3,"current-schema-id":1,"schemas":[{"schema-id":1,"fields":[{"id":1,"name":"event_id","type":"string","required":true}]}]}}"#,
        ))
        .unwrap();
    let response = app.clone().oneshot(create).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let credentials = Request::builder()
        .method(Method::GET)
        .uri("/catalog/v1/namespaces/default/tables/events/credentials")
        .header("x-lakecat-agent-did", "did:example:agent")
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(credentials).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(body["storage-credentials"], serde_json::json!([]));
}

#[tokio::test]
async fn credential_issuer_vends_short_lived_credentials_for_secret_ref_profile() {
    let issuer = Arc::new(RecordingCredentialIssuer::default());
    let app = app(LakeCatState::new(
        WarehouseName::new("local").unwrap(),
        MemoryCatalogStore::new(),
    )
    .with_credential_issuer(issuer.clone()));
    let upsert = Request::builder()
        .method(Method::PUT)
        .uri("/management/v1/warehouses/local/storage-profiles/s3-events")
        .header("content-type", "application/json")
        .header("x-lakecat-principal", "operator@example.com")
        .body(Body::from(
            serde_json::json!({
                "location-prefix": "s3://lakecat-demo/events",
                "provider": "s3",
                "issuance-mode": "short-lived-secret-ref",
                "secret-ref": "typesec://lakecat/local/s3-events"
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
            r#"{"name":"events","location":"s3://lakecat-demo/events/tenant-a","metadata-location":"s3://lakecat-demo/events/tenant-a/metadata/00000.json","metadata":{"format-version":3,"current-schema-id":1,"schemas":[{"schema-id":1,"fields":[{"id":1,"name":"event_id","type":"string","required":true}]}]}}"#,
        ))
        .unwrap();
    let response = app.clone().oneshot(create).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let credentials = Request::builder()
        .method(Method::GET)
        .uri("/catalog/v1/namespaces/default/tables/events/credentials")
        .header("x-lakecat-agent-did", "did:example:agent")
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(credentials).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let credentials = body["storage-credentials"].as_array().unwrap();
    assert_eq!(credentials.len(), 1);
    assert_eq!(
        credentials[0]["prefix"],
        serde_json::json!("s3://lakecat-demo/events")
    );
    let config = credentials[0]["config"].as_array().unwrap();
    assert!(config.iter().any(|entry| {
        entry["key"] == "lakecat.credential-kind" && entry["value"] == "mock-short-lived"
    }));
    assert!(
        !config
            .iter()
            .any(|entry| { entry["value"] == "typesec://lakecat/local/s3-events" })
    );

    let requests = issuer.requests.lock().await;
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests[0].profile.secret_ref.as_deref(),
        Some("typesec://lakecat/local/s3-events")
    );
    assert_eq!(
        requests[0].authorization_receipt.principal.subject,
        "did:example:agent"
    );
}

#[cfg(feature = "typesec-local")]
#[tokio::test]
async fn typesec_credential_issuer_gates_secret_ref_resolution() {
    use crate::typesec_credential_issuer::{
        EnvironmentSecretRefCredentialResolver, TypeSecCredentialIssuer,
    };

    let issuer = TypeSecCredentialIssuer::new(
        Arc::new(AllowCredentialIssuePolicy {
            subject: "did:example:agent".to_string(),
            resource: "typesec://env/LAKECAT_S3_EVENTS_CREDENTIALS".to_string(),
        }),
        EnvironmentSecretRefCredentialResolver::with_reader(|name| {
            if name == "LAKECAT_S3_EVENTS_CREDENTIALS" {
                Ok(serde_json::json!({
                    "lakecat.credential-kind": "typesec-env-short-lived",
                    "aws.session-token": "temporary-typesec-token"
                })
                .to_string())
            } else {
                Err(std::env::VarError::NotPresent)
            }
        }),
    );
    let app = app(LakeCatState::new(
        WarehouseName::new("local").unwrap(),
        MemoryCatalogStore::new(),
    )
    .with_credential_issuer(issuer));
    let upsert = Request::builder()
        .method(Method::PUT)
        .uri("/management/v1/warehouses/local/storage-profiles/s3-events")
        .header("content-type", "application/json")
        .header("x-lakecat-principal", "operator@example.com")
        .body(Body::from(
            serde_json::json!({
                "location-prefix": "s3://lakecat-demo/events",
                "provider": "s3",
                "issuance-mode": "short-lived-secret-ref",
                "secret-ref": "typesec://env/LAKECAT_S3_EVENTS_CREDENTIALS"
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
            r#"{"name":"events","location":"s3://lakecat-demo/events/tenant-a","metadata-location":"s3://lakecat-demo/events/tenant-a/metadata/00000.json","metadata":{"format-version":3,"current-schema-id":1,"schemas":[{"schema-id":1,"fields":[{"id":1,"name":"event_id","type":"string","required":true}]}]}}"#,
        ))
        .unwrap();
    let response = app.clone().oneshot(create).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let credentials = Request::builder()
        .method(Method::GET)
        .uri("/catalog/v1/namespaces/default/tables/events/credentials")
        .header("x-lakecat-agent-did", "did:example:agent")
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(credentials).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let credentials = body["storage-credentials"].as_array().unwrap();
    assert_eq!(credentials.len(), 1);
    let config = credentials[0]["config"].as_array().unwrap();
    assert!(config.iter().any(|entry| {
        entry["key"] == "lakecat.credential-kind" && entry["value"] == "typesec-env-short-lived"
    }));

    let denied = Request::builder()
        .method(Method::GET)
        .uri("/catalog/v1/namespaces/default/tables/events/credentials")
        .header("x-lakecat-agent-did", "did:example:other")
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(denied).await.unwrap();
    assert_eq!(response.status(), StatusCode::CONFLICT);
}

#[cfg(feature = "typesec-local")]
#[tokio::test]
async fn typesec_credential_issuer_gates_production_secret_refs_before_dispatch() {
    use crate::typesec_credential_issuer::{
        ExternalSecretRefCredentialResolver, SecretRefProvider, TypeSecCredentialIssuer,
        secret_ref_provider,
    };

    let principal = Principal::new("did:example:agent", PrincipalKind::Agent).unwrap();
    let table = TableRecord::new(
        table_ident("local", "default", "events").unwrap(),
        "s3://lakecat-demo/events/tenant-a".to_string(),
        Some("s3://lakecat-demo/events/tenant-a/metadata/00000.json".to_string()),
        serde_json::json!({"format-version":3}),
        principal.clone(),
    );
    let profile = StorageProfile::new(
        "s3-events",
        WarehouseName::new("local").unwrap(),
        "s3://lakecat-demo/events",
        StorageProvider::S3,
        CredentialIssuanceMode::ShortLivedSecretRef,
        Some("vault://secret/data/lakecat/s3-events".to_string()),
        Default::default(),
    )
    .unwrap();
    let request = CredentialIssuanceRequest {
        table,
        profile,
        authorization_receipt: AuthorizationReceipt {
            principal,
            action: CatalogAction::CredentialsVend,
            table: Some(table_ident("local", "default", "events").unwrap()),
            allowed: true,
            engine: "test".to_string(),
            policy_hash: None,
            context: serde_json::json!({}),
            checked_at: chrono::Utc::now(),
        },
        max_credential_ttl_seconds: None,
    };

    for (secret_ref, provider_label) in [
        (
            "vault://secret/data/lakecat/s3-events",
            SecretRefProvider::Vault.as_str(),
        ),
        (
            "aws-sm://lakecat/s3-events",
            SecretRefProvider::AwsSecretsManager.as_str(),
        ),
        (
            "gcp-sm://lakecat/s3-events",
            SecretRefProvider::GcpSecretManager.as_str(),
        ),
        (
            "azure-kv://lakecat/s3-events",
            SecretRefProvider::AzureKeyVault.as_str(),
        ),
    ] {
        let mut request = request.clone();
        request.profile.secret_ref = Some(secret_ref.to_string());
        assert_eq!(
            secret_ref_provider(secret_ref).unwrap().as_str(),
            provider_label
        );

        let issuer = TypeSecCredentialIssuer::new(
            Arc::new(AllowCredentialIssuePolicy {
                subject: "did:example:agent".to_string(),
                resource: secret_ref.to_string(),
            }),
            ExternalSecretRefCredentialResolver::with_env_reader(|_| {
                Err(std::env::VarError::NotPresent)
            }),
        );
        let err = issuer.issue(request.clone()).await.unwrap_err();
        assert!(matches!(err, LakeCatError::InvalidArgument(_)));
        assert!(err.to_string().contains(&format!(
            "credential secret resolver for {provider_label} is not configured"
        )));
        assert!(err.to_string().contains("secret-ref-hash=sha256:"));
        assert!(
            !err.to_string().contains(secret_ref),
            "not-configured resolver errors must not expose the raw secret-ref URI"
        );

        let denied = TypeSecCredentialIssuer::new(
            Arc::new(AllowCredentialIssuePolicy {
                subject: "did:example:other".to_string(),
                resource: secret_ref.to_string(),
            }),
            ExternalSecretRefCredentialResolver::with_env_reader(|_| {
                Err(std::env::VarError::NotPresent)
            }),
        );
        let err = denied.issue(request).await.unwrap_err();
        assert!(matches!(err, LakeCatError::Conflict(_)));
        assert!(
            err.to_string()
                .contains("TypeSec denied credential issuance")
        );
    }
}

#[cfg(feature = "typesec-local")]
#[tokio::test]
async fn typesec_credential_issuer_dispatches_configured_production_secret_backends_after_authorization()
 {
    use crate::typesec_credential_issuer::{
        ExternalSecretRefCredentialResolver, SecretRefCredentialResolver, SecretRefProvider,
        TypeSecCredentialIssuer,
    };

    let principal = Principal::new("did:example:agent", PrincipalKind::Agent).unwrap();
    let table = TableRecord::new(
        table_ident("local", "default", "events").unwrap(),
        "s3://lakecat-demo/events/tenant-a".to_string(),
        Some("s3://lakecat-demo/events/tenant-a/metadata/00000.json".to_string()),
        serde_json::json!({"format-version":3}),
        principal.clone(),
    );
    let profile = StorageProfile::new(
        "s3-events",
        WarehouseName::new("local").unwrap(),
        "s3://lakecat-demo/events",
        StorageProvider::S3,
        CredentialIssuanceMode::ShortLivedSecretRef,
        Some("aws-sm://lakecat/s3-events".to_string()),
        Default::default(),
    )
    .unwrap();
    let request = CredentialIssuanceRequest {
        table,
        profile,
        authorization_receipt: AuthorizationReceipt {
            principal,
            action: CatalogAction::CredentialsVend,
            table: Some(table_ident("local", "default", "events").unwrap()),
            allowed: true,
            engine: "test".to_string(),
            policy_hash: None,
            context: serde_json::json!({}),
            checked_at: chrono::Utc::now(),
        },
        max_credential_ttl_seconds: Some(300),
    };

    for (provider, provider_label, secret_ref) in [
        (
            SecretRefProvider::AwsSecretsManager,
            "aws-secrets-manager",
            "aws-sm://lakecat/s3-events",
        ),
        (
            SecretRefProvider::GcpSecretManager,
            "gcp-secret-manager",
            "gcp-sm://lakecat/s3-events",
        ),
        (
            SecretRefProvider::AzureKeyVault,
            "azure-key-vault",
            "azure-kv://lakecat/s3-events",
        ),
    ] {
        let backend = Arc::new(MockProductionSecretRefResolver {
            provider_label,
            credential_prefix: None,
            requests: Mutex::new(Vec::new()),
        });
        let mut backends: BTreeMap<SecretRefProvider, Arc<dyn SecretRefCredentialResolver>> =
            BTreeMap::new();
        backends.insert(provider, backend.clone());
        let issuer = TypeSecCredentialIssuer::new(
            Arc::new(AllowCredentialIssuePolicy {
                subject: "did:example:agent".to_string(),
                resource: secret_ref.to_string(),
            }),
            ExternalSecretRefCredentialResolver::with_provider_backends(backends),
        );

        let mut allowed_request = request.clone();
        allowed_request.profile.secret_ref = Some(secret_ref.to_string());
        let credentials = issuer.issue(allowed_request.clone()).await.unwrap();
        assert_eq!(credentials.len(), 1);
        assert_eq!(credentials[0].prefix, "s3://lakecat-demo/events");
        assert!(credentials[0].config.iter().any(|entry| {
            entry.key == "lakecat.credential-kind"
                && entry.value == format!("{provider_label}-short-lived")
        }));
        assert!(credentials[0].config.iter().any(|entry| {
            entry.key == "lakecat.secret-ref-provider"
                && entry.value == secret_ref.split_once("://").unwrap().0
        }));
        assert!(credentials[0].config.iter().any(|entry| {
            entry.key == "lakecat.max-credential-ttl-seconds" && entry.value == "300"
        }));
        assert_eq!(
            *backend.requests.lock().await,
            vec![(secret_ref.to_string(), Some(300))]
        );

        let denied = TypeSecCredentialIssuer::new(
            Arc::new(AllowCredentialIssuePolicy {
                subject: "did:example:other".to_string(),
                resource: secret_ref.to_string(),
            }),
            ExternalSecretRefCredentialResolver::with_provider_backends(BTreeMap::from([(
                provider,
                backend.clone() as Arc<dyn SecretRefCredentialResolver>,
            )])),
        );
        let err = denied.issue(allowed_request).await.unwrap_err();
        assert!(matches!(err, LakeCatError::Conflict(_)));
        assert!(
            err.to_string()
                .contains("TypeSec denied credential issuance")
        );
        assert_eq!(
            *backend.requests.lock().await,
            vec![(secret_ref.to_string(), Some(300))],
            "denied TypeSec decisions must not dispatch to the production backend"
        );
    }
}

#[cfg(feature = "typesec-local")]
#[tokio::test]
async fn typesec_credential_issuer_replaces_backend_secret_ref_provider_evidence() {
    use crate::typesec_credential_issuer::{
        ExternalSecretRefCredentialResolver, SecretRefCredentialResolver, SecretRefProvider,
        StaticSecretRefCredentialResolver, TypeSecCredentialIssuer,
    };

    let secret_ref = "aws-sm://lakecat/s3-events";
    let issuer = TypeSecCredentialIssuer::new(
        Arc::new(AllowCredentialIssuePolicy {
            subject: "did:example:agent".to_string(),
            resource: secret_ref.to_string(),
        }),
        ExternalSecretRefCredentialResolver::with_provider_backends(BTreeMap::from([(
            SecretRefProvider::AwsSecretsManager,
            StaticSecretRefCredentialResolver::new(BTreeMap::from([(
                secret_ref.to_string(),
                vec![
                    ConfigEntry::new("lakecat.secret-ref-provider", "backend-shadow"),
                    ConfigEntry::new("aws.session-token", "temporary-token"),
                ],
            )])) as Arc<dyn SecretRefCredentialResolver>,
        )])),
    );

    let credentials = issuer
        .issue(production_secret_credential_request(secret_ref))
        .await
        .unwrap();
    assert_eq!(credentials.len(), 1);
    assert_single_config_value(
        &credentials[0].config,
        "lakecat.secret-ref-provider",
        "aws-sm",
    );
    assert!(
        credentials[0]
            .config
            .iter()
            .any(|entry| { entry.key == "aws.session-token" && entry.value == "temporary-token" })
    );
}

#[cfg(feature = "typesec-local")]
#[tokio::test]
async fn typesec_credential_issuer_resolves_file_backed_secret_refs_after_authorization() {
    use crate::typesec_credential_issuer::{
        ExternalSecretRefCredentialResolver, SecretRefProvider, TypeSecCredentialIssuer,
        secret_ref_hash_file_name,
    };

    let secret_ref = "gcp-sm://lakecat/events";
    let root = std::env::temp_dir().join(format!(
        "lakecat-file-secret-{}",
        chrono::Utc::now()
            .timestamp_nanos_opt()
            .expect("timestamp should fit")
    ));
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(
        root.join(secret_ref_hash_file_name(secret_ref)),
        serde_json::json!({
            "lakecat.credential-kind": "gcp-file-backed",
            "gcp.access-token": "temporary-gcp-token"
        })
        .to_string(),
    )
    .unwrap();

    let issuer = TypeSecCredentialIssuer::new(
        Arc::new(AllowCredentialIssuePolicy {
            subject: "did:example:agent".to_string(),
            resource: secret_ref.to_string(),
        }),
        ExternalSecretRefCredentialResolver::with_file_provider_roots(BTreeMap::from([(
            SecretRefProvider::GcpSecretManager,
            root.clone(),
        )])),
    );

    let credentials = issuer
        .issue(production_secret_credential_request(secret_ref))
        .await
        .unwrap();

    assert_eq!(credentials.len(), 1);
    assert_eq!(credentials[0].prefix, "s3://lakecat-demo/events");
    assert_single_config_value(
        &credentials[0].config,
        "lakecat.secret-ref-provider",
        "gcp-sm",
    );
    assert_single_config_value(
        &credentials[0].config,
        "lakecat.secret-ref-hash",
        &lakecat_core::content_hash_bytes(secret_ref.as_bytes()),
    );
    assert!(
        credentials[0].config.iter().any(|entry| {
            entry.key == "gcp.access-token" && entry.value == "temporary-gcp-token"
        })
    );

    std::fs::remove_dir_all(root).ok();
}

#[cfg(feature = "typesec-local")]
#[tokio::test]
async fn typesec_credential_issuer_does_not_read_file_backed_secret_refs_when_denied() {
    use crate::typesec_credential_issuer::{
        ExternalSecretRefCredentialResolver, SecretRefProvider, TypeSecCredentialIssuer,
    };

    let secret_ref = "azure-kv://lakecat/events";
    let root = std::env::temp_dir().join(format!(
        "lakecat-file-secret-denied-{}",
        chrono::Utc::now()
            .timestamp_nanos_opt()
            .expect("timestamp should fit")
    ));
    std::fs::create_dir_all(&root).unwrap();
    let issuer = TypeSecCredentialIssuer::new(
        Arc::new(AllowCredentialIssuePolicy {
            subject: "did:example:other".to_string(),
            resource: secret_ref.to_string(),
        }),
        ExternalSecretRefCredentialResolver::with_file_provider_roots(BTreeMap::from([(
            SecretRefProvider::AzureKeyVault,
            root.clone(),
        )])),
    );

    let err = issuer
        .issue(production_secret_credential_request(secret_ref))
        .await
        .unwrap_err();

    assert!(matches!(err, LakeCatError::Conflict(_)));
    assert!(
        err.to_string()
            .contains("TypeSec denied credential issuance")
    );
    assert!(!err.to_string().contains("failed to read file-backed"));
    assert!(!err.to_string().contains(secret_ref));
    std::fs::remove_dir_all(root).ok();
}

#[cfg(feature = "typesec-local")]
#[tokio::test]
async fn typesec_credential_issuer_redacts_file_backed_secret_parse_failures() {
    use crate::typesec_credential_issuer::{
        ExternalSecretRefCredentialResolver, SecretRefProvider, TypeSecCredentialIssuer,
        secret_ref_hash_file_name,
    };

    let secret_ref = "aws-sm://lakecat/s3-events";
    let root = std::env::temp_dir().join(format!(
        "lakecat-file-secret-invalid-{}",
        chrono::Utc::now()
            .timestamp_nanos_opt()
            .expect("timestamp should fit")
    ));
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(
        root.join(secret_ref_hash_file_name(secret_ref)),
        r#"{"aws.session-token": 123, "raw-token": "temporary-secret"}"#,
    )
    .unwrap();
    let issuer = TypeSecCredentialIssuer::new(
        Arc::new(AllowCredentialIssuePolicy {
            subject: "did:example:agent".to_string(),
            resource: secret_ref.to_string(),
        }),
        ExternalSecretRefCredentialResolver::with_file_provider_roots(BTreeMap::from([(
            SecretRefProvider::AwsSecretsManager,
            root.clone(),
        )])),
    );

    let err = issuer
        .issue(production_secret_credential_request(secret_ref))
        .await
        .unwrap_err();
    let message = err.to_string();
    assert!(message.contains("failed to resolve aws-secrets-manager credential secret"));
    assert!(message.contains("secret-ref-hash=sha256:"));
    assert!(message.contains("error-detail-hash=sha256:"));
    for forbidden in [
        secret_ref,
        "temporary-secret",
        "raw-token",
        "aws.session-token",
        root.to_string_lossy().as_ref(),
    ] {
        assert!(
            !message.contains(forbidden),
            "file-backed resolver failures must not expose {forbidden}"
        );
    }

    std::fs::remove_dir_all(root).ok();
}

#[cfg(feature = "typesec-local")]
#[tokio::test]
async fn typesec_credential_issuer_rejects_backend_credentials_outside_profile_scope() {
    use crate::typesec_credential_issuer::{
        ExternalSecretRefCredentialResolver, SecretRefCredentialResolver, SecretRefProvider,
        TypeSecCredentialIssuer,
    };

    let principal = Principal::new("did:example:agent", PrincipalKind::Agent).unwrap();
    let table = TableRecord::new(
        table_ident("local", "default", "events").unwrap(),
        "s3://lakecat-demo/events/tenant-a".to_string(),
        Some("s3://lakecat-demo/events/tenant-a/metadata/00000.json".to_string()),
        serde_json::json!({"format-version":3}),
        principal.clone(),
    );
    let profile = StorageProfile::new(
        "s3-events",
        WarehouseName::new("local").unwrap(),
        "s3://lakecat-demo/events",
        StorageProvider::S3,
        CredentialIssuanceMode::ShortLivedSecretRef,
        Some("aws-sm://lakecat/s3-events".to_string()),
        Default::default(),
    )
    .unwrap();
    let request = CredentialIssuanceRequest {
        table,
        profile,
        authorization_receipt: AuthorizationReceipt {
            principal,
            action: CatalogAction::CredentialsVend,
            table: Some(table_ident("local", "default", "events").unwrap()),
            allowed: true,
            engine: "test".to_string(),
            policy_hash: None,
            context: serde_json::json!({}),
            checked_at: chrono::Utc::now(),
        },
        max_credential_ttl_seconds: Some(300),
    };
    let backend = Arc::new(MockProductionSecretRefResolver {
        provider_label: "aws-secrets-manager",
        credential_prefix: Some("s3://lakecat-demo"),
        requests: Mutex::new(Vec::new()),
    });
    let issuer = TypeSecCredentialIssuer::new(
        Arc::new(AllowCredentialIssuePolicy {
            subject: "did:example:agent".to_string(),
            resource: "aws-sm://lakecat/s3-events".to_string(),
        }),
        ExternalSecretRefCredentialResolver::with_provider_backends(BTreeMap::from([(
            SecretRefProvider::AwsSecretsManager,
            backend.clone() as Arc<dyn SecretRefCredentialResolver>,
        )])),
    );

    let err = issuer.issue(request).await.unwrap_err();
    assert!(matches!(err, LakeCatError::InvalidArgument(_)));
    let message = err.to_string();
    assert!(message.contains("issued credential prefix is outside storage profile scope"));
    assert!(message.contains("credential-prefix-hash=sha256:"));
    assert!(message.contains("storage-profile-prefix-hash=sha256:"));
    assert!(!message.contains("s3://lakecat-demo"));
    assert_eq!(
        *backend.requests.lock().await,
        vec![("aws-sm://lakecat/s3-events".to_string(), Some(300))],
        "authorized backend dispatch is allowed, but returned credentials must stay scoped"
    );
}

#[cfg(feature = "typesec-local")]
#[tokio::test]
async fn typesec_credential_issuer_redacts_configured_provider_backend_failures() {
    use crate::typesec_credential_issuer::{
        ExternalSecretRefCredentialResolver, SecretRefCredentialResolver, SecretRefProvider,
        TypeSecCredentialIssuer,
    };

    let secret_ref = "aws-sm://lakecat/s3-events";
    let backend = Arc::new(FailingProductionSecretRefResolver {
        error: "aws request failed for aws-sm://lakecat/s3-events with token raw-token-123 and arn arn:aws:secretsmanager:us-west-2:123456789012:secret:lakecat/s3-events",
        requests: Mutex::new(Vec::new()),
    });
    let issuer = TypeSecCredentialIssuer::new(
        Arc::new(AllowCredentialIssuePolicy {
            subject: "did:example:agent".to_string(),
            resource: secret_ref.to_string(),
        }),
        ExternalSecretRefCredentialResolver::with_provider_backends(BTreeMap::from([(
            SecretRefProvider::AwsSecretsManager,
            backend.clone() as Arc<dyn SecretRefCredentialResolver>,
        )])),
    );

    let err = issuer
        .issue(production_secret_credential_request(secret_ref))
        .await
        .unwrap_err();
    let message = err.to_string();
    assert!(message.contains("failed to resolve aws-secrets-manager credential secret"));
    assert!(message.contains("secret-ref-hash=sha256:"));
    assert!(message.contains("error-detail-hash=sha256:"));
    for forbidden in [
        secret_ref,
        "raw-token-123",
        "arn:aws:secretsmanager",
        "123456789012",
        "s3-events",
        "aws request failed",
    ] {
        assert!(
            !message.contains(forbidden),
            "configured provider failures must not expose {forbidden}"
        );
    }
    assert_eq!(
        *backend.requests.lock().await,
        vec![(secret_ref.to_string(), None)],
        "allowed TypeSec decisions may dispatch, but backend failures must be redacted"
    );
}

#[cfg(feature = "typesec-local")]
#[tokio::test]
async fn typesec_credential_issuer_resolves_vault_secret_refs_after_authorization() {
    use crate::typesec_credential_issuer::{
        ExternalSecretRefCredentialResolver, TypeSecCredentialIssuer,
        VaultSecretRefCredentialResolver,
    };

    let principal = Principal::new("did:example:agent", PrincipalKind::Agent).unwrap();
    let table = TableRecord::new(
        table_ident("local", "default", "events").unwrap(),
        "s3://lakecat-demo/events/tenant-a".to_string(),
        Some("s3://lakecat-demo/events/tenant-a/metadata/00000.json".to_string()),
        serde_json::json!({"format-version":3}),
        principal.clone(),
    );
    let profile = StorageProfile::new(
        "s3-events",
        WarehouseName::new("local").unwrap(),
        "s3://lakecat-demo/events",
        StorageProvider::S3,
        CredentialIssuanceMode::ShortLivedSecretRef,
        Some("vault://secret/data/lakecat/s3-events".to_string()),
        Default::default(),
    )
    .unwrap();
    let request = CredentialIssuanceRequest {
        table,
        profile,
        authorization_receipt: AuthorizationReceipt {
            principal,
            action: CatalogAction::CredentialsVend,
            table: Some(table_ident("local", "default", "events").unwrap()),
            allowed: true,
            engine: "test".to_string(),
            policy_hash: None,
            context: serde_json::json!({}),
            checked_at: chrono::Utc::now(),
        },
        max_credential_ttl_seconds: None,
    };
    let vault_client = Arc::new(MockVaultSecretClient::default());
    *vault_client.response.lock().await = Some(serde_json::json!({
        "data": {
            "data": {
                "lakecat.credential-kind": "vault-short-lived",
                "aws.session-token": "temporary-vault-token"
            },
            "metadata": {
                "version": 7
            }
        }
    }));
    let vault = VaultSecretRefCredentialResolver::new(
        "https://vault.example.test/",
        "vault-token",
        Some("lakecat/admin".to_string()),
        vault_client.clone(),
    )
    .unwrap();
    let issuer = TypeSecCredentialIssuer::new(
        Arc::new(AllowCredentialIssuePolicy {
            subject: "did:example:agent".to_string(),
            resource: "vault://secret/data/lakecat/s3-events".to_string(),
        }),
        ExternalSecretRefCredentialResolver::with_vault(vault),
    );

    let credentials = issuer.issue(request).await.unwrap();
    assert_eq!(credentials.len(), 1);
    assert_eq!(credentials[0].prefix, "s3://lakecat-demo/events");
    assert!(credentials[0].config.iter().any(|entry| {
        entry.key == "lakecat.credential-kind" && entry.value == "vault-short-lived"
    }));
    assert!(credentials[0].config.iter().any(|entry| {
        entry.key == "aws.session-token" && entry.value == "temporary-vault-token"
    }));

    let requests = vault_client.requests.lock().await;
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests[0].0,
        "https://vault.example.test/v1/secret/data/lakecat/s3-events"
    );
    assert_eq!(requests[0].1, "vault-token");
    assert_eq!(requests[0].2.as_deref(), Some("lakecat/admin"));
}

#[cfg(feature = "typesec-local")]
#[tokio::test]
async fn typesec_credential_issuer_redacts_vault_backend_failures() {
    use crate::typesec_credential_issuer::{
        ExternalSecretRefCredentialResolver, TypeSecCredentialIssuer,
        VaultSecretRefCredentialResolver,
    };

    let secret_ref = "vault://secret/data/lakecat/s3-events";
    let vault_client = Arc::new(MockVaultSecretClient::default());
    *vault_client.error.lock().await = Some(
        "backend token vault-token failed for vault://secret/data/lakecat/s3-events".to_string(),
    );
    let vault = VaultSecretRefCredentialResolver::new(
        "https://vault.example.test/",
        "vault-token",
        Some("lakecat/admin".to_string()),
        vault_client.clone(),
    )
    .unwrap();
    let issuer = TypeSecCredentialIssuer::new(
        Arc::new(AllowCredentialIssuePolicy {
            subject: "did:example:agent".to_string(),
            resource: secret_ref.to_string(),
        }),
        ExternalSecretRefCredentialResolver::with_vault(vault),
    );

    let err = issuer
        .issue(production_secret_credential_request(secret_ref))
        .await
        .unwrap_err();
    let message = err.to_string();
    assert!(message.contains("failed to resolve Vault credential secret"));
    assert!(message.contains("secret-ref-hash=sha256:"));
    assert!(message.contains("error-detail-hash=sha256:"));
    for forbidden in [
        secret_ref,
        "vault-token",
        "lakecat/admin",
        "backend token",
        "s3-events",
    ] {
        assert!(
            !message.contains(forbidden),
            "Vault backend failures must not expose {forbidden}"
        );
    }
    let requests = vault_client.requests.lock().await;
    assert_eq!(requests.len(), 1);
}

#[cfg(feature = "typesec-local")]
#[tokio::test]
async fn typesec_credential_issuer_redacts_environment_backend_failures() {
    use crate::typesec_credential_issuer::{
        EnvironmentSecretRefCredentialResolver, TypeSecCredentialIssuer,
    };

    let secret_ref = "typesec://env/LAKECAT_S3_EVENTS_CREDENTIALS";
    let issuer = TypeSecCredentialIssuer::new(
        Arc::new(AllowCredentialIssuePolicy {
            subject: "did:example:agent".to_string(),
            resource: secret_ref.to_string(),
        }),
        EnvironmentSecretRefCredentialResolver::with_reader(|_| {
            Err(std::env::VarError::NotPresent)
        }),
    );

    let err = issuer
        .issue(production_secret_credential_request(secret_ref))
        .await
        .unwrap_err();
    let message = err.to_string();
    assert!(message.contains("failed to resolve environment credential secret"));
    assert!(message.contains("secret-ref-hash=sha256:"));
    assert!(message.contains("error-detail-hash=sha256:"));
    for forbidden in [
        secret_ref,
        "LAKECAT_S3_EVENTS_CREDENTIALS",
        "environment variable not found",
        "NotPresent",
    ] {
        assert!(
            !message.contains(forbidden),
            "environment backend failures must not expose {forbidden}"
        );
    }
}

#[cfg(feature = "typesec-local")]
#[tokio::test]
async fn typesec_credential_issuer_rejects_blank_environment_secret_config_keys() {
    use crate::typesec_credential_issuer::{
        EnvironmentSecretRefCredentialResolver, TypeSecCredentialIssuer,
    };

    let secret_ref = "typesec://env/LAKECAT_S3_EVENTS_CREDENTIALS";
    let issuer = TypeSecCredentialIssuer::new(
        Arc::new(AllowCredentialIssuePolicy {
            subject: "did:example:agent".to_string(),
            resource: secret_ref.to_string(),
        }),
        EnvironmentSecretRefCredentialResolver::with_reader(|_| {
            Ok(serde_json::json!({
                " ": "temporary-token"
            })
            .to_string())
        }),
    );

    let err = issuer
        .issue(production_secret_credential_request(secret_ref))
        .await
        .unwrap_err();
    let message = err.to_string();
    assert!(message.contains("failed to parse environment credential secret"));
    assert!(message.contains("secret-ref-hash=sha256:"));
    assert!(message.contains("error-detail-hash=sha256:"));
    for forbidden in [
        secret_ref,
        "LAKECAT_S3_EVENTS_CREDENTIALS",
        "temporary-token",
        "credential config keys",
    ] {
        assert!(
            !message.contains(forbidden),
            "environment secret parse failures must not expose {forbidden}"
        );
    }
}

#[cfg(feature = "typesec-local")]
#[tokio::test]
async fn typesec_credential_issuer_rejects_blank_vault_secret_config_keys() {
    use crate::typesec_credential_issuer::{
        ExternalSecretRefCredentialResolver, TypeSecCredentialIssuer,
        VaultSecretRefCredentialResolver,
    };

    let secret_ref = "vault://secret/data/lakecat/s3-events";
    let vault_client = Arc::new(MockVaultSecretClient::default());
    *vault_client.response.lock().await = Some(serde_json::json!({
        "data": {
            "data": {
                " ": "temporary-vault-token"
            }
        }
    }));
    let vault = VaultSecretRefCredentialResolver::new(
        "https://vault.example.test/",
        "vault-token",
        Some("lakecat/admin".to_string()),
        vault_client.clone(),
    )
    .unwrap();
    let issuer = TypeSecCredentialIssuer::new(
        Arc::new(AllowCredentialIssuePolicy {
            subject: "did:example:agent".to_string(),
            resource: secret_ref.to_string(),
        }),
        ExternalSecretRefCredentialResolver::with_vault(vault),
    );

    let err = issuer
        .issue(production_secret_credential_request(secret_ref))
        .await
        .unwrap_err();
    let message = err.to_string();
    assert!(message.contains("failed to parse Vault credential secret"));
    assert!(message.contains("secret-ref-hash=sha256:"));
    assert!(message.contains("error-detail-hash=sha256:"));
    for forbidden in [
        secret_ref,
        "vault-token",
        "lakecat/admin",
        "temporary-vault-token",
        "credential config keys",
    ] {
        assert!(
            !message.contains(forbidden),
            "Vault secret parse failures must not expose {forbidden}"
        );
    }
    let requests = vault_client.requests.lock().await;
    assert_eq!(requests.len(), 1);
}

#[cfg(feature = "typesec-local")]
#[tokio::test]
async fn typesec_credential_issuer_rejects_blank_file_backed_secret_config_keys() {
    use crate::typesec_credential_issuer::{
        ExternalSecretRefCredentialResolver, SecretRefProvider, TypeSecCredentialIssuer,
        secret_ref_hash_file_name,
    };

    let secret_ref = "aws-sm://lakecat/s3-events";
    let root = std::env::temp_dir().join(format!(
        "lakecat-file-secret-blank-key-{}",
        chrono::Utc::now()
            .timestamp_nanos_opt()
            .expect("timestamp should fit")
    ));
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(
        root.join(secret_ref_hash_file_name(secret_ref)),
        serde_json::json!({
            " ": "temporary-file-token",
            "aws.session-token": "also-secret"
        })
        .to_string(),
    )
    .unwrap();
    let issuer = TypeSecCredentialIssuer::new(
        Arc::new(AllowCredentialIssuePolicy {
            subject: "did:example:agent".to_string(),
            resource: secret_ref.to_string(),
        }),
        ExternalSecretRefCredentialResolver::with_file_provider_roots(BTreeMap::from([(
            SecretRefProvider::AwsSecretsManager,
            root.clone(),
        )])),
    );

    let err = issuer
        .issue(production_secret_credential_request(secret_ref))
        .await
        .unwrap_err();
    let message = err.to_string();
    assert!(message.contains("failed to resolve aws-secrets-manager credential secret"));
    assert!(message.contains("secret-ref-hash=sha256:"));
    assert!(message.contains("error-detail-hash=sha256:"));
    for forbidden in [
        secret_ref,
        "temporary-file-token",
        "also-secret",
        "credential config keys",
        root.to_string_lossy().as_ref(),
    ] {
        assert!(
            !message.contains(forbidden),
            "file-backed secret parse failures must not expose {forbidden}"
        );
    }

    std::fs::remove_dir_all(root).ok();
}

#[tokio::test]
async fn credential_vend_blocks_raw_credentials_for_fine_grained_restriction() {
    let store = MemoryCatalogStore::new();
    let issuer = Arc::new(RecordingCredentialIssuer::default());
    let state = LakeCatState::new(WarehouseName::new("local").unwrap(), store.clone())
        .with_credential_issuer(issuer.clone());
    let create = TableRecord::new(
        TableIdent::new(
            WarehouseName::new("local").unwrap(),
            "default".parse::<Namespace>().unwrap(),
            TableName::new("events").unwrap(),
        ),
        "file:///tmp/events".to_string(),
        Some("file:///tmp/events/metadata/00000.json".to_string()),
        serde_json::json!({
            "format-version": 3,
            "current-schema-id": 1,
            "schemas": [{
                "schema-id": 1,
                "fields": [
                    {"id": 1, "name": "event_id", "type": "string", "required": true},
                    {"id": 2, "name": "payload", "type": "string", "required": false}
                ]
            }]
        }),
        Principal::anonymous(),
    );
    let ident = create.ident.clone();
    store.create_table(create).await.unwrap();
    store
        .upsert_policy_binding(
            PolicyBinding::new(
                "agent-credential-columns",
                WarehouseName::new("local").unwrap(),
                Some(ident.namespace.clone()),
                Some(ident.name.clone()),
                true,
                serde_json::json!({
                    "uid": "policy:agent-credential-columns",
                    "lakecat:read-restriction": {
                        "allowed-columns": ["event_id"],
                        "row-predicate": {
                            "type": "eq",
                            "term": "event_id",
                            "value": "evt-1"
                        },
                        "max-credential-ttl-seconds": 300
                    }
                }),
            )
            .unwrap(),
        )
        .await
        .unwrap();

    let mut headers = HeaderMap::new();
    headers.insert(
        "x-lakecat-agent-did",
        axum::http::HeaderValue::from_static("did:example:agent"),
    );
    let response = load_credentials(
        State(state),
        headers,
        Path(("default".to_string(), "events".to_string())),
    )
    .await
    .unwrap();
    assert_eq!(response.0.storage_credentials.len(), 0);

    let requests = issuer.requests.lock().await;
    assert!(requests.is_empty());
    drop(requests);

    let outbox = store
        .pending_outbox_events(Some("lakecat.lineage-and-graph"), 10)
        .await
        .unwrap();
    let event = outbox
        .iter()
        .find(|event| event.event_type == "credentials.vend-attempted")
        .expect("credentials vend audit event");
    let receipt = &event.payload["payload"]["authorization-receipt"];
    assert!(
        receipt["policy_hash"].as_str().is_some(),
        "governed credential receipt should summarize enforced policy hashes"
    );
    assert_eq!(
        receipt["action"],
        serde_json::json!(CatalogAction::CredentialsVend)
    );
    assert_eq!(
        receipt["context"]["lakecat:raw-credential-exception"]["allowed"],
        serde_json::json!(false)
    );
    assert_eq!(
        receipt["context"]["read-restriction"]["allowed-columns"],
        serde_json::json!(["event_id"])
    );
    assert_eq!(
        receipt["context"]["read-restriction"]["row-predicate"],
        serde_json::json!({
            "type": "eq",
            "term": "event_id",
            "value": "evt-1"
        })
    );
    assert_eq!(
        receipt["context"]["read-restriction"]["max-credential-ttl-seconds"],
        serde_json::json!(300)
    );
    assert_eq!(
        event.payload["payload"]["credential-count"],
        serde_json::json!(0)
    );
    assert_eq!(
        event.payload["payload"]["storage-profile"]["profile-id"],
        serde_json::json!("local:file")
    );
    assert_eq!(
        event.payload["payload"]["storage-profile"]["secret-ref-present"],
        serde_json::json!(false)
    );
    assert_eq!(
        event.payload["payload"]["credential-response-evidence"],
        serde_json::json!([])
    );
    assert_eq!(
        event.payload["payload"]["lakecat:credential-block-reason"],
        serde_json::json!("fine-grained read restriction requires Sail-planned reads")
    );
}

#[tokio::test]
async fn credential_vend_rejects_malformed_odrl_before_issuer() {
    let store = MemoryCatalogStore::new();
    let issuer = Arc::new(RecordingCredentialIssuer::default());
    let app = app(
        LakeCatState::new(WarehouseName::new("local").unwrap(), store.clone())
            .with_credential_issuer(issuer.clone()),
    );
    let table = TableRecord::new(
        TableIdent::new(
            WarehouseName::new("local").unwrap(),
            "default".parse::<Namespace>().unwrap(),
            TableName::new("events").unwrap(),
        ),
        "file:///tmp/events".to_string(),
        Some("file:///tmp/events/metadata/00000.json".to_string()),
        serde_json::json!({
            "format-version": 3,
            "current-schema-id": 1,
            "schemas": [{
                "schema-id": 1,
                "fields": [
                    {"id": 1, "name": "event_id", "type": "string", "required": true},
                    {"id": 2, "name": "payload", "type": "string", "required": false}
                ]
            }]
        }),
        Principal::anonymous(),
    );
    let ident = table.ident.clone();
    store.create_table(table).await.unwrap();
    store
        .upsert_policy_binding(
            PolicyBinding::new(
                "agent-credential-columns",
                WarehouseName::new("local").unwrap(),
                Some(ident.namespace.clone()),
                Some(ident.name.clone()),
                true,
                serde_json::json!({
                    "uid": "policy:agent-credential-columns",
                    "permission": [{
                        "action": "read",
                        "constraint": [{
                            "leftOperand": "allowed-columns",
                            "operator": "eq"
                        }]
                    }]
                }),
            )
            .unwrap(),
        )
        .await
        .unwrap();

    let credentials = Request::builder()
        .method(Method::GET)
        .uri("/catalog/v1/namespaces/default/tables/events/credentials")
        .header("x-lakecat-agent-did", "did:example:agent")
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(credentials).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let message = body["error"]["message"].as_str().unwrap();
    assert!(message.contains("ODRL allowed columns constraint must include a right operand"));
    assert!(
        issuer.requests.lock().await.is_empty(),
        "malformed active ODRL must fail before credential issuer dispatch"
    );
    let outbox = store
        .pending_outbox_events(Some("lakecat.lineage-and-graph"), 10)
        .await
        .unwrap();
    assert!(
        outbox
            .iter()
            .all(|event| event.event_type != "credentials.vend-attempted"),
        "malformed active ODRL must not emit credential-vend replay evidence"
    );
}

#[tokio::test]
async fn credential_vend_rejects_malformed_jsonld_odrl_before_issuer() {
    let store = MemoryCatalogStore::new();
    let issuer = Arc::new(RecordingCredentialIssuer::default());
    let app = app(
        LakeCatState::new(WarehouseName::new("local").unwrap(), store.clone())
            .with_credential_issuer(issuer.clone()),
    );
    let table = TableRecord::new(
        TableIdent::new(
            WarehouseName::new("local").unwrap(),
            "default".parse::<Namespace>().unwrap(),
            TableName::new("events").unwrap(),
        ),
        "file:///tmp/events".to_string(),
        Some("file:///tmp/events/metadata/00000.json".to_string()),
        serde_json::json!({
            "format-version": 3,
            "current-schema-id": 1,
            "schemas": [{
                "schema-id": 1,
                "fields": [
                    {"id": 1, "name": "event_id", "type": "string", "required": true},
                    {"id": 2, "name": "payload", "type": "string", "required": false}
                ]
            }]
        }),
        Principal::anonymous(),
    );
    let ident = table.ident.clone();
    store.create_table(table).await.unwrap();
    store
        .upsert_policy_binding(
            PolicyBinding::new(
                "agent-credential-columns",
                WarehouseName::new("local").unwrap(),
                Some(ident.namespace.clone()),
                Some(ident.name.clone()),
                true,
                serde_json::json!({
                    "uid": "policy:agent-credential-columns",
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
                }),
            )
            .unwrap(),
        )
        .await
        .unwrap();

    let credentials = Request::builder()
        .method(Method::GET)
        .uri("/catalog/v1/namespaces/default/tables/events/credentials")
        .header("x-lakecat-agent-did", "did:example:agent")
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(credentials).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let message = body["error"]["message"].as_str().unwrap();
    assert!(message.contains("ODRL allowed columns must be strings"));
    assert!(
        issuer.requests.lock().await.is_empty(),
        "malformed JSON-LD active ODRL must fail before credential issuer dispatch"
    );
    let outbox = store
        .pending_outbox_events(Some("lakecat.lineage-and-graph"), 10)
        .await
        .unwrap();
    assert!(
        outbox
            .iter()
            .all(|event| event.event_type != "credentials.vend-attempted"),
        "malformed JSON-LD active ODRL must not emit credential-vend replay evidence"
    );
}

#[tokio::test]
async fn credential_vend_rejects_issuer_credentials_outside_profile_scope() {
    let store = MemoryCatalogStore::new();
    let issuer = Arc::new(BroadCredentialIssuer::default());
    let state = LakeCatState::new(WarehouseName::new("local").unwrap(), store.clone())
        .with_credential_issuer(issuer.clone());
    let create = TableRecord::new(
        TableIdent::new(
            WarehouseName::new("local").unwrap(),
            "default".parse::<Namespace>().unwrap(),
            TableName::new("events").unwrap(),
        ),
        "s3://lakecat-demo/events/tenant-a".to_string(),
        Some("s3://lakecat-demo/events/tenant-a/metadata/00000.json".to_string()),
        serde_json::json!({"format-version": 3}),
        Principal::anonymous(),
    );
    store.create_table(create).await.unwrap();

    let mut headers = HeaderMap::new();
    headers.insert(
        "x-lakecat-principal",
        axum::http::HeaderValue::from_static("human:operator"),
    );
    let err = load_credentials(
        State(state),
        headers,
        Path(("default".to_string(), "events".to_string())),
    )
    .await
    .expect_err("broad issuer credentials must be rejected by LakeCat");
    let message = err.0.to_string();
    assert!(matches!(err.0, LakeCatError::InvalidArgument(_)));
    assert!(message.contains("issued credential prefix is outside storage profile scope"));
    assert!(message.contains("credential-prefix-hash=sha256:"));
    assert!(message.contains("storage-profile-prefix-hash=sha256:"));
    assert!(!message.contains("s3://lakecat-demo"));
    assert!(!message.contains("local:file"));

    let requests = issuer.requests.lock().await;
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests[0].profile.location_prefix,
        "s3://lakecat-demo/events/tenant-a"
    );
    drop(requests);

    let outbox = store
        .pending_outbox_events(Some("lakecat.lineage-and-graph"), 10)
        .await
        .unwrap();
    assert!(
        outbox
            .iter()
            .all(|event| event.event_type != "credentials.vend-attempted"),
        "out-of-scope issuer credentials must fail before credential-vend replay evidence"
    );
}

#[tokio::test]
async fn credential_vend_allows_trusted_human_raw_exception_for_restricted_table() {
    let store = MemoryCatalogStore::new();
    let issuer = Arc::new(RecordingCredentialIssuer::default());
    let state = LakeCatState::new(WarehouseName::new("local").unwrap(), store.clone())
        .with_credential_issuer(issuer.clone());
    let create = TableRecord::new(
        TableIdent::new(
            WarehouseName::new("local").unwrap(),
            "default".parse::<Namespace>().unwrap(),
            TableName::new("events").unwrap(),
        ),
        "file:///tmp/events".to_string(),
        Some("file:///tmp/events/metadata/00000.json".to_string()),
        serde_json::json!({
            "format-version": 3,
            "current-schema-id": 1,
            "schemas": [{
                "schema-id": 1,
                "fields": [
                    {"id": 1, "name": "event_id", "type": "string", "required": true},
                    {"id": 2, "name": "payload", "type": "string", "required": false}
                ]
            }]
        }),
        Principal::anonymous(),
    );
    let ident = create.ident.clone();
    store.create_table(create).await.unwrap();
    store
        .upsert_policy_binding(
            PolicyBinding::new(
                "agent-credential-columns",
                WarehouseName::new("local").unwrap(),
                Some(ident.namespace.clone()),
                Some(ident.name.clone()),
                true,
                serde_json::json!({
                    "uid": "policy:agent-credential-columns",
                    "lakecat:read-restriction": {
                        "allowed-columns": ["event_id"],
                        "row-predicate": {
                            "type": "eq",
                            "term": "event_id",
                            "value": "evt-1"
                        },
                        "max-credential-ttl-seconds": 300
                    }
                }),
            )
            .unwrap(),
        )
        .await
        .unwrap();

    let mut headers = HeaderMap::new();
    headers.insert(
        "x-lakecat-principal",
        axum::http::HeaderValue::from_static("human:operator"),
    );
    let response = load_credentials(
        State(state),
        headers,
        Path(("default".to_string(), "events".to_string())),
    )
    .await
    .unwrap();
    assert_eq!(response.0.storage_credentials.len(), 1);
    assert_eq!(
        response.0.storage_credentials[0].prefix,
        "file:///tmp/events"
    );
    assert!(
        response.0.storage_credentials[0]
            .config
            .iter()
            .any(|entry| {
                entry.key == "lakecat.max-credential-ttl-seconds" && entry.value == "300"
            })
    );

    let requests = issuer.requests.lock().await;
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests[0].authorization_receipt.principal.kind,
        PrincipalKind::Human
    );
    assert_eq!(requests[0].max_credential_ttl_seconds, Some(300));
    drop(requests);

    let outbox = store
        .pending_outbox_events(Some("lakecat.lineage-and-graph"), 10)
        .await
        .unwrap();
    let event = outbox
        .iter()
        .find(|event| event.event_type == "credentials.vend-attempted")
        .expect("credentials vend audit event");
    let receipt = &event.payload["payload"]["authorization-receipt"];
    assert_eq!(
        receipt["context"]["lakecat:raw-credential-exception"]["allowed"],
        serde_json::json!(true)
    );
    assert_eq!(
        receipt["context"]["lakecat:raw-credential-exception"]["reason"],
        serde_json::json!("trusted human principal may use audited raw credential vending")
    );
    assert_eq!(
        event.payload["payload"]["credential-count"],
        serde_json::json!(1)
    );
    assert_eq!(
        event.payload["payload"]["storage-profile"]["profile-id"],
        serde_json::json!("local:file")
    );
    assert_eq!(
        event.payload["payload"]["storage-profile"]["secret-ref-present"],
        serde_json::json!(false)
    );
    let response_evidence = event.payload["payload"]["credential-response-evidence"]
        .as_array()
        .expect("credential response evidence should be recorded in outbox");
    assert_eq!(response_evidence.len(), 1);
    assert_eq!(
        response_evidence[0]["storage-profile-id"],
        serde_json::json!("local:file")
    );
    assert_eq!(
        response_evidence[0]["storage-provider"],
        serde_json::json!("file")
    );
    assert_eq!(
        response_evidence[0]["credential-mode"],
        serde_json::json!("local-file-no-secret")
    );
    assert_eq!(
        response_evidence[0]["authorization-principal"],
        serde_json::json!("human:operator")
    );
    assert_eq!(
        response_evidence[0]["governed-read-required"],
        serde_json::json!("true")
    );
    assert_eq!(
        response_evidence[0]["max-credential-ttl-seconds"],
        serde_json::json!("300")
    );
    assert!(
        response_evidence[0]["prefix-hash"]
            .as_str()
            .is_some_and(|hash| hash.starts_with("sha256:"))
    );
    assert!(
        response_evidence[0]["issuer-config-hash"]
            .as_str()
            .is_some_and(|hash| hash.starts_with("sha256:"))
    );
    let evidence_text = serde_json::to_string(&response_evidence).unwrap();
    assert!(!evidence_text.contains("file:///tmp/events"));
    assert!(
        event.payload["payload"]
            .get("lakecat:credential-block-reason")
            .is_none()
    );
}

#[tokio::test]
async fn credential_vend_response_normalizes_duplicate_ttl_entries() {
    let store = MemoryCatalogStore::new();
    let issuer = Arc::new(DuplicateTtlCredentialIssuer::default());
    let state = LakeCatState::new(WarehouseName::new("local").unwrap(), store.clone())
        .with_credential_issuer(issuer.clone());
    let table = TableRecord::new(
        TableIdent::new(
            WarehouseName::new("local").unwrap(),
            "default".parse::<Namespace>().unwrap(),
            TableName::new("events").unwrap(),
        ),
        "file:///tmp/events".to_string(),
        Some("file:///tmp/events/metadata/00000.json".to_string()),
        serde_json::json!({
            "format-version": 3,
            "current-schema-id": 1,
            "schemas": [{
                "schema-id": 1,
                "fields": [
                    {"id": 1, "name": "event_id", "type": "string", "required": true}
                ]
            }]
        }),
        Principal::anonymous(),
    );
    let ident = table.ident.clone();
    store.create_table(table).await.unwrap();
    store
        .upsert_policy_binding(
            PolicyBinding::new(
                "trusted-human-ttl-cap",
                WarehouseName::new("local").unwrap(),
                Some(ident.namespace.clone()),
                Some(ident.name.clone()),
                true,
                serde_json::json!({
                    "uid": "policy:trusted-human-ttl-cap",
                    "lakecat:read-restriction": {
                        "allowed-columns": ["event_id"],
                        "max-credential-ttl-seconds": 300
                    }
                }),
            )
            .unwrap(),
        )
        .await
        .unwrap();

    let mut headers = HeaderMap::new();
    headers.insert(
        "x-lakecat-principal",
        axum::http::HeaderValue::from_static("human:operator"),
    );
    let response = load_credentials(
        State(state),
        headers,
        Path(("default".to_string(), "events".to_string())),
    )
    .await
    .unwrap();

    let credentials = response.0.storage_credentials;
    assert_eq!(credentials.len(), 1);
    let ttl_entries = credentials[0]
        .config
        .iter()
        .filter(|entry| entry.key == "lakecat.max-credential-ttl-seconds")
        .collect::<Vec<_>>();
    assert_eq!(ttl_entries.len(), 1);
    assert_eq!(ttl_entries[0].value, "120");
    assert!(credentials[0].config.iter().any(|entry| {
        entry.key == "aws.session-token" && entry.value == "temporary-test-token"
    }));

    let requests = issuer.requests.lock().await;
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].max_credential_ttl_seconds, Some(300));
}

#[tokio::test]
async fn credential_vend_response_replaces_shadowed_lakecat_evidence() {
    let store = MemoryCatalogStore::new();
    let issuer = Arc::new(ShadowingCredentialEvidenceIssuer::default());
    let state = LakeCatState::new(WarehouseName::new("local").unwrap(), store.clone())
        .with_credential_issuer(issuer.clone());
    let table = TableRecord::new(
        TableIdent::new(
            WarehouseName::new("local").unwrap(),
            "default".parse::<Namespace>().unwrap(),
            TableName::new("events").unwrap(),
        ),
        "file:///tmp/events".to_string(),
        Some("file:///tmp/events/metadata/00000.json".to_string()),
        serde_json::json!({
            "format-version": 3,
            "current-schema-id": 1,
            "schemas": [{
                "schema-id": 1,
                "fields": [
                    {"id": 1, "name": "event_id", "type": "string", "required": true}
                ]
            }]
        }),
        Principal::anonymous(),
    );
    let ident = table.ident.clone();
    store.create_table(table).await.unwrap();
    store
        .upsert_policy_binding(
            PolicyBinding::new(
                "trusted-human-shadowed-evidence",
                WarehouseName::new("local").unwrap(),
                Some(ident.namespace.clone()),
                Some(ident.name.clone()),
                true,
                serde_json::json!({
                    "uid": "policy:trusted-human-shadowed-evidence",
                    "lakecat:read-restriction": {
                        "allowed-columns": ["event_id"],
                        "max-credential-ttl-seconds": 300
                    }
                }),
            )
            .unwrap(),
        )
        .await
        .unwrap();

    let mut headers = HeaderMap::new();
    headers.insert(
        "x-lakecat-principal",
        axum::http::HeaderValue::from_static("human:operator"),
    );
    let response = load_credentials(
        State(state),
        headers,
        Path(("default".to_string(), "events".to_string())),
    )
    .await
    .unwrap();

    let credentials = response.0.storage_credentials;
    assert_eq!(credentials.len(), 1);
    let config = &credentials[0].config;
    assert_single_config_value(config, "lakecat.storage-profile-id", "local:file");
    assert_single_config_value(config, "lakecat.storage-provider", "file");
    assert_single_config_value(config, "lakecat.credential-mode", "local-file-no-secret");
    assert_single_config_value(config, "lakecat.authorization-principal", "human:operator");
    assert_single_config_value(config, "lakecat.governed-read-required", "true");
    assert_single_config_value(config, "lakecat.max-credential-ttl-seconds", "120");
    assert!(
        config.iter().any(|entry| {
            entry.key == "lakecat.credential-kind" && entry.value == "shadow-test"
        })
    );
    assert!(config.iter().any(|entry| {
        entry.key == "aws.session-token" && entry.value == "temporary-test-token"
    }));

    let requests = issuer.requests.lock().await;
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].max_credential_ttl_seconds, Some(300));
}

#[test]
fn credentials_vend_audit_payload_surfaces_policy_context() {
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
    let profile = StorageProfile::inferred_for_table(&table);
    let receipt = AuthorizationReceipt {
        principal: Principal::new("did:example:agent", PrincipalKind::Agent).unwrap(),
        action: CatalogAction::CredentialsVend,
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
                },
                "max-credential-ttl-seconds": 300
            },
            "lakecat:raw-credential-exception": {
                "requested": true,
                "allowed": false,
                "reason": "fine-grained read restriction requires Sail-planned reads"
            }
        }),
        checked_at: chrono::Utc::now(),
    };

    let credentials = canonicalize_credential_response_evidence(
        vec![StorageCredential {
            prefix: "file:///tmp/events".to_string(),
            config: vec![
                ConfigEntry::new("lakecat.storage-profile-id", "shadow"),
                ConfigEntry::new("aws.session-token", "temporary-test-token"),
                ConfigEntry::new("lakecat.max-credential-ttl-seconds", "120"),
            ],
        }],
        &profile,
        &receipt,
        true,
        Some(300),
    );

    let payload =
        credentials_vend_audit_payload(&ident, &table, &profile, &credentials, &receipt).unwrap();
    assert_eq!(
        payload["lakecat:raw-credential-exception"]["allowed"],
        serde_json::json!(false)
    );
    assert_eq!(
        payload["read-restriction"]["allowed-columns"],
        serde_json::json!(["event_id"])
    );
    assert_eq!(
        payload["read-restriction"]["row-predicate"],
        serde_json::json!({
            "type": "eq",
            "term": "event_id",
            "value": "evt-1"
        })
    );
    assert_eq!(
        payload["read-restriction"]["max-credential-ttl-seconds"],
        serde_json::json!(300)
    );
    assert_eq!(
        payload["authorization-receipt"]["context"]["read-restriction"],
        payload["read-restriction"]
    );
    assert_eq!(
        payload["storage-profile"]["location-prefix-hash"],
        serde_json::json!(
            content_hash_json(&json!({"location-prefix": "file:///tmp/events"})).unwrap()
        )
    );
    assert!(
        payload["storage-profile"]["location-prefix-hash"]
            .as_str()
            .is_some_and(is_full_sha256_hash)
    );
    assert!(
        payload["storage-profile"].get("location-prefix").is_none(),
        "credential-vend audit payload must not expose raw storage-profile location prefixes"
    );
    let response_evidence = payload["credential-response-evidence"]
        .as_array()
        .expect("credential response evidence should be an array");
    assert_eq!(response_evidence.len(), 1);
    assert_eq!(
        response_evidence[0]["storage-profile-id"],
        serde_json::json!("local:file")
    );
    assert_eq!(
        response_evidence[0]["storage-provider"],
        serde_json::json!("file")
    );
    assert_eq!(
        response_evidence[0]["credential-mode"],
        serde_json::json!("local-file-no-secret")
    );
    assert_eq!(
        response_evidence[0]["authorization-principal"],
        serde_json::json!("did:example:agent")
    );
    assert_eq!(
        response_evidence[0]["governed-read-required"],
        serde_json::json!("true")
    );
    assert_eq!(
        response_evidence[0]["secret-ref-provider"],
        serde_json::Value::Null
    );
    assert_eq!(
        response_evidence[0]["max-credential-ttl-seconds"],
        serde_json::json!("120")
    );
    assert!(
        response_evidence[0]["prefix-hash"]
            .as_str()
            .is_some_and(|hash| hash.starts_with("sha256:"))
    );
    assert!(
        response_evidence[0]["issuer-config-hash"]
            .as_str()
            .is_some_and(|hash| hash.starts_with("sha256:"))
    );
    let evidence_text = serde_json::to_string(&response_evidence).unwrap();
    assert!(!evidence_text.contains("temporary-test-token"));
    assert!(!evidence_text.contains("file:///tmp/events"));
}

#[test]
fn credentials_vend_audit_payload_records_secret_ref_provider_response_evidence() {
    let ident = table_ident("local", "default", "events").unwrap();
    let table = TableRecord::new(
        ident.clone(),
        "s3://lakecat-demo/events".to_string(),
        Some("s3://lakecat-demo/events/metadata/00000.json".to_string()),
        serde_json::json!({ "format-version": 3 }),
        Principal::anonymous(),
    );
    let profile = StorageProfile::new(
        "events-prod",
        WarehouseName::new("local").unwrap(),
        "s3://lakecat-demo/events",
        StorageProvider::S3,
        CredentialIssuanceMode::ShortLivedSecretRef,
        Some("typesec://lakecat/local/events-prod".to_string()),
        BTreeMap::new(),
    )
    .unwrap();
    let receipt = AuthorizationReceipt {
        principal: Principal::new("human:operator", PrincipalKind::Human).unwrap(),
        action: CatalogAction::CredentialsVend,
        table: Some(ident.clone()),
        allowed: true,
        engine: "test".to_string(),
        policy_hash: None,
        context: serde_json::json!({}),
        checked_at: chrono::Utc::now(),
    };
    let credentials = canonicalize_credential_response_evidence(
        vec![StorageCredential {
            prefix: "s3://lakecat-demo/events".to_string(),
            config: vec![
                ConfigEntry::new("lakecat.secret-ref-provider", "backend-shadow"),
                ConfigEntry::new("lakecat.secret-ref-hash", "sha256:shadow"),
                ConfigEntry::new("aws.session-token", "temporary-test-token"),
            ],
        }],
        &profile,
        &receipt,
        false,
        None,
    );

    let payload =
        credentials_vend_audit_payload(&ident, &table, &profile, &credentials, &receipt).unwrap();
    let response_evidence = payload["credential-response-evidence"]
        .as_array()
        .expect("credential response evidence should be an array");

    assert_eq!(
        response_evidence[0]["secret-ref-provider"],
        serde_json::json!("typesec")
    );
    assert_eq!(
        response_evidence[0]["secret-ref-hash"],
        serde_json::json!(content_hash_bytes(
            "typesec://lakecat/local/events-prod".as_bytes()
        ))
    );
    assert_eq!(
        payload["storage-profile"]["secret-ref-provider"],
        serde_json::json!("typesec")
    );
    assert_eq!(
        payload["storage-profile"]["secret-ref-hash"],
        serde_json::json!(content_hash_bytes(
            "typesec://lakecat/local/events-prod".as_bytes()
        ))
    );
    let evidence_text = serde_json::to_string(&response_evidence).unwrap();
    assert!(!evidence_text.contains("backend-shadow"));
    assert!(!evidence_text.contains("sha256:shadow"));
    assert!(!evidence_text.contains("temporary-test-token"));
    assert!(!evidence_text.contains("typesec://lakecat/local/events-prod"));
}

#[test]
fn credential_ttl_cap_preserves_stricter_issuer_ttl() {
    let credentials = apply_credential_ttl_cap(
        vec![StorageCredential {
            prefix: "s3://lakecat-demo/events".to_string(),
            config: vec![
                ConfigEntry::new("lakecat.credential-kind", "issuer-short-lived"),
                ConfigEntry::new("lakecat.max-credential-ttl-seconds", "60"),
            ],
        }],
        Some(300),
    );

    let ttl_entries = credentials[0]
        .config
        .iter()
        .filter(|entry| entry.key == "lakecat.max-credential-ttl-seconds")
        .collect::<Vec<_>>();
    assert_eq!(ttl_entries.len(), 1);
    assert_eq!(
        ttl_entries[0].value, "60",
        "issuer TTLs stricter than policy maximum must not be widened"
    );
}

#[test]
fn credential_ttl_cap_collapses_duplicate_issuer_ttl_entries() {
    let credentials = apply_credential_ttl_cap(
        vec![StorageCredential {
            prefix: "s3://lakecat-demo/events".to_string(),
            config: vec![
                ConfigEntry::new("lakecat.max-credential-ttl-seconds", "600"),
                ConfigEntry::new("aws.session-token", "temporary"),
                ConfigEntry::new("lakecat.max-credential-ttl-seconds", "120"),
                ConfigEntry::new("lakecat.max-credential-ttl-seconds", "not-a-number"),
            ],
        }],
        Some(300),
    );

    let ttl_entries = credentials[0]
        .config
        .iter()
        .filter(|entry| entry.key == "lakecat.max-credential-ttl-seconds")
        .collect::<Vec<_>>();
    assert_eq!(ttl_entries.len(), 1);
    assert_eq!(ttl_entries[0].value, "120");
    assert!(
        credentials[0]
            .config
            .iter()
            .any(|entry| { entry.key == "aws.session-token" && entry.value == "temporary" })
    );
}
