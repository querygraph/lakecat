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

#[tokio::test]
async fn config_endpoint_reports_lakecat_capabilities() {
    let app = test_app();
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/catalog/v1/config")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_config_defaults_include(&payload["defaults"], "lakecat.format.v4", "extension-ready");
    assert_config_defaults_include(
        &payload["defaults"],
        "lakecat.format.v4.bridge",
        "json-passthrough",
    );
    assert_config_defaults_include(
        &payload["defaults"],
        "lakecat.format.v4.typed-sail",
        "unavailable",
    );
    assert_config_endpoints_include(
        &payload["endpoints"],
        "POST /catalog/v1/namespaces/{namespace}/tables/{table}/commit",
    );
    assert_config_endpoints_include(
        &payload["endpoints"],
        "POST /catalog/v1/namespaces/{namespace}/tables",
    );
    assert_config_endpoints_include(
        &payload["endpoints"],
        "POST /catalog/v1/{warehouse}/namespaces/{namespace}/tables",
    );
    assert_config_endpoints_include(
        &payload["endpoints"],
        "POST /catalog/v1/{warehouse}/namespaces/{namespace}/tables/{table}/commit",
    );
    assert_config_endpoints_include(
        &payload["endpoints"],
        "POST /catalog/v1/namespaces/{namespace}/tables/{table}/plan",
    );
    assert_config_endpoints_include(
        &payload["endpoints"],
        "POST /catalog/v1/namespaces/{namespace}/tables/{table}/fetch-scan-tasks",
    );
    assert_config_endpoints_include(
        &payload["endpoints"],
        "GET /catalog/v1/namespaces/{namespace}/tables/{table}/credentials",
    );
    assert_config_endpoints_include(&payload["endpoints"], "POST /management/v1/lineage/drain");
    assert_config_endpoints_include(&payload["endpoints"], "GET /querygraph/v1/bootstrap");
}

#[tokio::test]
async fn list_namespaces_does_not_fabricate_default_namespace() {
    let app = test_app();
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/catalog/v1/namespaces")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(payload["namespaces"], serde_json::json!([]));
}

#[tokio::test]
async fn namespaces_load_and_drop_through_catalog_routes() {
    let store = MemoryCatalogStore::new();
    store
        .upsert_project(
            ProjectRecord::new(
                "default",
                None,
                Some("Default Project".to_string()),
                std::collections::BTreeMap::new(),
                Principal::anonymous(),
            )
            .unwrap(),
        )
        .await
        .unwrap();
    store
        .upsert_warehouse(
            WarehouseRecord::new(
                WarehouseName::new("local").unwrap(),
                "default",
                Some("file:///tmp/lakecat".to_string()),
                std::collections::BTreeMap::new(),
                Principal::anonymous(),
            )
            .unwrap(),
        )
        .await
        .unwrap();
    let app = app(LakeCatState::new(
        WarehouseName::new("local").unwrap(),
        store,
    ));

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/catalog/v1/namespaces/empty")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/catalog/v1/namespaces")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"namespace":["empty"]}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/catalog/v1/namespaces/empty")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(payload["namespace"], serde_json::json!(["empty"]));

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::DELETE)
                .uri("/catalog/v1/namespaces/empty")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/catalog/v1/namespaces/empty")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/catalog/v1/local/namespaces")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"namespace":["prefixed"]}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/catalog/v1/local/namespaces/prefixed")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::DELETE)
                .uri("/catalog/v1/local/namespaces/prefixed")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/catalog/v1/namespaces/default/tables")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"name":"events","location":"file:///tmp/events","metadata":{"format-version":3}}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::DELETE)
                .uri("/catalog/v1/namespaces/default")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CONFLICT);
}

#[test]
fn location_dot_segment_detection_decodes_percent_encoded_segments() {
    assert!(location_has_dot_path_segment(
        "s3://lakecat/events/metadata/../00001.json"
    ));
    assert!(location_has_dot_path_segment(
        "s3://lakecat/events/metadata/%2E%2e/00001.json"
    ));
    assert!(!location_has_dot_path_segment(
        "s3://lakecat/events/metadata/v1.2/00001.json"
    ));
}

#[cfg(feature = "typesec-local")]
#[test]
fn environment_secret_resolver_parses_supported_secret_shapes() {
    use crate::typesec_credential_issuer::{
        SecretRefProvider, config_entries_from_secret_json, config_entries_from_vault_secret_json,
        env_secret_variable, secret_ref_provider, vault_secret_path,
    };

    assert_eq!(
        env_secret_variable("typesec://env/LAKECAT_S3_EVENTS").unwrap(),
        "LAKECAT_S3_EVENTS"
    );
    for secret_ref in [
        "typesec://env/lowercase",
        "typesec://vault/path",
        "vault://",
        "typesec://env/",
        "not a typesec uri with secret=abc",
    ] {
        let err = if secret_ref.starts_with("vault://") || secret_ref.starts_with("not a typesec") {
            vault_secret_path(secret_ref).unwrap_err()
        } else {
            env_secret_variable(secret_ref).unwrap_err()
        };
        let message = err.to_string();
        assert!(message.contains("secret-ref-hash=sha256:"));
        assert!(
            !message.contains(secret_ref),
            "resolver validation errors must not expose raw secret refs"
        );
    }
    let malformed_provider_ref = "not a credential ref token=abc";
    let err = secret_ref_provider(malformed_provider_ref).unwrap_err();
    let message = err.to_string();
    assert!(message.contains("secret-ref-hash=sha256:"));
    assert!(
        !message.contains(malformed_provider_ref),
        "provider validation errors must not expose raw secret refs"
    );
    let malformed_env_ref = "not a typesec env ref token=abc";
    let err = env_secret_variable(malformed_env_ref).unwrap_err();
    let message = err.to_string();
    assert!(message.contains("secret-ref-hash=sha256:"));
    assert!(
        !message.contains(malformed_env_ref),
        "environment resolver parse errors must not expose raw secret refs"
    );
    assert_eq!(
        secret_ref_provider("typesec://env/LAKECAT_S3_EVENTS").unwrap(),
        SecretRefProvider::TypeSecEnv
    );
    assert_eq!(
        secret_ref_provider("vault://secret/data/lakecat/s3-events").unwrap(),
        SecretRefProvider::Vault
    );
    assert_eq!(
        vault_secret_path("vault://secret/data/lakecat/s3-events").unwrap(),
        "v1/secret/data/lakecat/s3-events"
    );
    assert_eq!(
        secret_ref_provider("aws-sm://lakecat/s3-events").unwrap(),
        SecretRefProvider::AwsSecretsManager
    );
    assert_eq!(
        secret_ref_provider("gcp-sm://lakecat/s3-events").unwrap(),
        SecretRefProvider::GcpSecretManager
    );
    assert_eq!(
        secret_ref_provider("azure-kv://lakecat/s3-events").unwrap(),
        SecretRefProvider::AzureKeyVault
    );
    let unsupported_provider_ref = "file:///tmp/raw-secret";
    let err = secret_ref_provider(unsupported_provider_ref).unwrap_err();
    let message = err.to_string();
    assert!(message.contains("secret-ref-hash=sha256:"));
    for forbidden in [unsupported_provider_ref, "file", "raw-secret"] {
        assert!(
            !message.contains(forbidden),
            "unsupported provider errors must not expose {forbidden}"
        );
    }
    for (secret_ref, resolver) in [
        (
            "typesec://env/LAKECAT_S3_EVENTS?token=raw-secret",
            "environment",
        ),
        ("vault://token@secret/data/lakecat/s3-events", "vault"),
        ("aws-sm://lakecat/s3-events#raw-secret", "provider"),
    ] {
        let err = match resolver {
            "environment" => env_secret_variable(secret_ref).unwrap_err(),
            "vault" => vault_secret_path(secret_ref).unwrap_err(),
            "provider" => secret_ref_provider(secret_ref).unwrap_err(),
            _ => unreachable!(),
        };
        let message = err.to_string();
        assert!(message.contains("query strings, fragments, or userinfo"));
        assert!(message.contains("secret-ref-hash=sha256:"));
        for forbidden in [secret_ref, "raw-secret", "token@secret", "s3-events"] {
            assert!(
                !message.contains(forbidden),
                "decorated {resolver} secret-ref errors must not expose {forbidden}"
            );
        }
    }

    let object_entries = config_entries_from_secret_json(
        r#"{"aws.session-token":"temporary-token","aws.region":"us-west-2"}"#,
    )
    .unwrap();
    assert!(
        object_entries
            .iter()
            .any(|entry| entry.key == "aws.session-token" && entry.value == "temporary-token")
    );

    let array_entries = config_entries_from_secret_json(
        r#"[{"key":"lakecat.credential-kind","value":"typesec-env-short-lived"}]"#,
    )
    .unwrap();
    assert_eq!(
        array_entries,
        vec![ConfigEntry::new(
            "lakecat.credential-kind",
            "typesec-env-short-lived"
        )]
    );

    assert!(config_entries_from_secret_json(r#"{"aws.session-token":123}"#).is_err());
    assert!(config_entries_from_secret_json(r#"{" ":"temporary-token"}"#).is_err());
    assert!(config_entries_from_secret_json(r#"[{"key":" ","value":"temporary-token"}]"#).is_err());

    let vault_entries = config_entries_from_vault_secret_json(serde_json::json!({
        "data": {
            "data": {
                "aws.session-token": "temporary-token",
                "aws.region": "us-west-2"
            }
        }
    }))
    .unwrap();
    assert!(
        vault_entries
            .iter()
            .any(|entry| entry.key == "aws.session-token" && entry.value == "temporary-token")
    );
    assert!(
        config_entries_from_vault_secret_json(serde_json::json!({
            "data": {
                "data": {
                    "aws.session-token": 123
                }
            }
        }))
        .is_err()
    );
    assert!(
        config_entries_from_vault_secret_json(serde_json::json!({
            "data": {
                "data": {
                    " ": "temporary-token"
                }
            }
        }))
        .is_err()
    );
}
