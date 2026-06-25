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
fn request_identity_hashes_typedid_envelope_material() {
    let mut headers = HeaderMap::new();
    headers.insert("x-lakecat-agent-did", "did:example:agent".parse().unwrap());
    headers.insert(
        "x-lakecat-typedid-envelope",
        r#"{"protected":"typedid-envelope"}"#.parse().unwrap(),
    );
    headers.insert("x-lakecat-typedid-proof", "signed-proof".parse().unwrap());
    headers.insert(
        "x-lakecat-agent-delegation",
        "delegation-token".parse().unwrap(),
    );
    headers.insert(
        "x-lakecat-agent-summary-signature",
        "summary-secret".parse().unwrap(),
    );

    let identity = request_identity(&headers).expect("identity should parse");

    assert_eq!(identity.principal.subject, "did:example:agent");
    assert_eq!(identity.principal.kind, PrincipalKind::Agent);
    assert_eq!(
        identity.envelope["typedid-proof-sha256"],
        serde_json::json!(content_hash_bytes("signed-proof".as_bytes()))
    );
    assert_eq!(
        identity.envelope["typedid-envelope-sha256"],
        serde_json::json!(content_hash_bytes(
            r#"{"protected":"typedid-envelope"}"#.as_bytes()
        ))
    );
    assert_eq!(
        identity.envelope["agent-delegation-sha256"],
        serde_json::json!(content_hash_bytes("delegation-token".as_bytes()))
    );
    assert_eq!(
        identity.envelope["agent-summary-signature-sha256"],
        serde_json::json!(content_hash_bytes("summary-secret".as_bytes()))
    );
    assert_eq!(
        identity.envelope["raw-secret-material"],
        serde_json::json!(false)
    );
    let envelope = identity.envelope.to_string();
    assert!(!envelope.contains("signed-proof"));
    assert!(!envelope.contains("protected"));
    assert!(!envelope.contains("delegation-token"));
    assert!(!envelope.contains("summary-secret"));
}

#[test]
fn request_identity_rejects_duplicate_identity_headers() {
    let mut headers = HeaderMap::new();
    headers.append("x-lakecat-principal", "alice@example.com".parse().unwrap());
    headers.append("x-lakecat-principal", "bob@example.com".parse().unwrap());

    let err = request_identity(&headers).expect_err("duplicate principal should fail closed");
    let LakeCatHttpError(inner) = err;
    let message = inner.to_string();

    assert!(message.contains("x-lakecat-principal header must appear at most once"));
    assert!(!message.contains("alice@example.com"));
    assert!(!message.contains("bob@example.com"));
}

#[tokio::test]
async fn config_endpoint_rejects_duplicate_identity_headers_before_governance() {
    let governance = Arc::new(RecordingGovernance::default());
    let app = app(LakeCatState::new(
        WarehouseName::new("local").unwrap(),
        MemoryCatalogStore::new(),
    )
    .with_integrations(
        default_sail_engine(),
        governance.clone(),
        NoopCatalogGraphSink::new(),
        HashOnlyLineageSink::new(),
    ));

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/catalog/v1/config")
                .header("x-lakecat-agent-did", "did:example:agent-a")
                .header("x-lakecat-agent-did", "did:example:agent-b")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let message = String::from_utf8(body.to_vec()).unwrap();
    assert!(message.contains("x-lakecat-agent-did header must appear at most once"));
    assert!(!message.contains("did:example:agent-a"));
    assert!(!message.contains("did:example:agent-b"));
    assert!(governance.principals.lock().await.is_empty());
    assert!(governance.contexts.lock().await.is_empty());
}

#[test]
fn request_identity_rejects_empty_bearer_token() {
    let mut headers = HeaderMap::new();
    headers.insert("authorization", "Bearer    ".parse().unwrap());

    let err = request_identity(&headers).expect_err("empty bearer token should fail closed");
    let LakeCatHttpError(inner) = err;
    let message = inner.to_string();

    assert!(message.contains("Authorization Bearer token must not be empty"));
    assert!(!message.contains("bearer:"));
    assert!(!message.contains(content_hash_bytes("".as_bytes()).as_str()));
}

#[tokio::test]
async fn config_endpoint_rejects_empty_bearer_token_before_governance() {
    let governance = Arc::new(RecordingGovernance::default());
    let app = app(LakeCatState::new(
        WarehouseName::new("local").unwrap(),
        MemoryCatalogStore::new(),
    )
    .with_integrations(
        default_sail_engine(),
        governance.clone(),
        NoopCatalogGraphSink::new(),
        HashOnlyLineageSink::new(),
    ));

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/catalog/v1/config")
                .header("authorization", "Bearer    ")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let message = String::from_utf8(body.to_vec()).unwrap();
    assert!(message.contains("Authorization Bearer token must not be empty"));
    assert!(!message.contains("bearer:"));
    assert!(governance.principals.lock().await.is_empty());
    assert!(governance.contexts.lock().await.is_empty());
}

#[test]
fn request_identity_rejects_whitespace_in_bearer_token() {
    let mut headers = HeaderMap::new();
    headers.insert("authorization", "Bearer service token".parse().unwrap());

    let err = request_identity(&headers).expect_err("whitespace-bearing token should fail closed");
    let LakeCatHttpError(inner) = err;
    let message = inner.to_string();

    assert!(message.contains("Authorization Bearer token must not contain whitespace"));
    assert!(!message.contains("service token"));
    assert!(!message.contains("bearer:"));
}

#[tokio::test]
async fn config_endpoint_rejects_whitespace_bearer_token_before_governance() {
    let governance = Arc::new(RecordingGovernance::default());
    let app = app(LakeCatState::new(
        WarehouseName::new("local").unwrap(),
        MemoryCatalogStore::new(),
    )
    .with_integrations(
        default_sail_engine(),
        governance.clone(),
        NoopCatalogGraphSink::new(),
        HashOnlyLineageSink::new(),
    ));

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/catalog/v1/config")
                .header("authorization", "Bearer service-token ")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let message = String::from_utf8(body.to_vec()).unwrap();
    assert!(message.contains("Authorization Bearer token must not contain whitespace"));
    assert!(!message.contains("service-token"));
    assert!(!message.contains("bearer:"));
    assert!(governance.principals.lock().await.is_empty());
    assert!(governance.contexts.lock().await.is_empty());
}

#[test]
fn request_identity_rejects_authorization_with_explicit_identity() {
    let mut headers = HeaderMap::new();
    headers.insert("x-lakecat-principal", "alice@example.com".parse().unwrap());
    headers.insert("x-lakecat-principal-kind", "human".parse().unwrap());
    headers.insert("authorization", "Bearer service-token".parse().unwrap());

    let err = request_identity(&headers).expect_err("mixed identity should fail closed");
    let LakeCatHttpError(inner) = err;
    let message = inner.to_string();

    assert!(message.contains(
        "Authorization cannot be combined with x-lakecat-principal, x-lakecat-agent-did, or x-lakecat-typedid"
    ));
    assert!(!message.contains("alice@example.com"));
    assert!(!message.contains("service-token"));
    assert!(!message.contains("bearer:"));
}

#[tokio::test]
async fn config_endpoint_rejects_authorization_with_agent_identity_before_governance() {
    let governance = Arc::new(RecordingGovernance::default());
    let app = app(LakeCatState::new(
        WarehouseName::new("local").unwrap(),
        MemoryCatalogStore::new(),
    )
    .with_integrations(
        default_sail_engine(),
        governance.clone(),
        NoopCatalogGraphSink::new(),
        HashOnlyLineageSink::new(),
    ));

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/catalog/v1/config")
                .header("x-lakecat-agent-did", "did:example:agent")
                .header("authorization", "Bearer service-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let message = String::from_utf8(body.to_vec()).unwrap();
    assert!(message.contains(
        "Authorization cannot be combined with x-lakecat-principal, x-lakecat-agent-did, or x-lakecat-typedid"
    ));
    assert!(!message.contains("did:example:agent"));
    assert!(!message.contains("service-token"));
    assert!(!message.contains("bearer:"));
    assert!(governance.principals.lock().await.is_empty());
    assert!(governance.contexts.lock().await.is_empty());
}

#[test]
fn request_identity_rejects_explicit_anonymous_principal_kind() {
    let mut headers = HeaderMap::new();
    headers.insert("x-lakecat-principal", "alice@example.com".parse().unwrap());
    headers.insert("x-lakecat-principal-kind", "anonymous".parse().unwrap());

    let err = request_identity(&headers).expect_err("explicit anonymous kind should fail");
    let LakeCatHttpError(inner) = err;
    let message = inner.to_string();

    assert!(message.contains(
        "x-lakecat-principal-kind cannot be anonymous; omit identity headers for anonymous access"
    ));
    assert!(!message.contains("alice@example.com"));
}

#[tokio::test]
async fn config_endpoint_rejects_explicit_anonymous_principal_kind_before_governance() {
    let governance = Arc::new(RecordingGovernance::default());
    let app = app(LakeCatState::new(
        WarehouseName::new("local").unwrap(),
        MemoryCatalogStore::new(),
    )
    .with_integrations(
        default_sail_engine(),
        governance.clone(),
        NoopCatalogGraphSink::new(),
        HashOnlyLineageSink::new(),
    ));

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/catalog/v1/config")
                .header("x-lakecat-principal", "alice@example.com")
                .header("x-lakecat-principal-kind", "anonymous")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let message = String::from_utf8(body.to_vec()).unwrap();
    assert!(message.contains(
        "x-lakecat-principal-kind cannot be anonymous; omit identity headers for anonymous access"
    ));
    assert!(!message.contains("alice@example.com"));
    assert!(governance.principals.lock().await.is_empty());
    assert!(governance.contexts.lock().await.is_empty());
}

#[test]
fn request_identity_rejects_orphan_principal_kind() {
    let mut headers = HeaderMap::new();
    headers.insert("x-lakecat-principal-kind", "service".parse().unwrap());
    headers.insert("authorization", "Bearer service-token".parse().unwrap());

    let err = request_identity(&headers).expect_err("orphan principal kind should fail closed");
    let LakeCatHttpError(inner) = err;
    let message = inner.to_string();

    assert!(message.contains("x-lakecat-principal-kind requires x-lakecat-principal"));
    assert!(!message.contains("service-token"));
    assert!(!message.contains("bearer:"));
}

#[tokio::test]
async fn config_endpoint_rejects_orphan_principal_kind_before_governance() {
    let governance = Arc::new(RecordingGovernance::default());
    let app = app(LakeCatState::new(
        WarehouseName::new("local").unwrap(),
        MemoryCatalogStore::new(),
    )
    .with_integrations(
        default_sail_engine(),
        governance.clone(),
        NoopCatalogGraphSink::new(),
        HashOnlyLineageSink::new(),
    ));

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/catalog/v1/config")
                .header("x-lakecat-principal-kind", "agent")
                .header("x-lakecat-agent-did", "did:example:agent")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let message = String::from_utf8(body.to_vec()).unwrap();
    assert!(message.contains("x-lakecat-principal-kind requires x-lakecat-principal"));
    assert!(!message.contains("did:example:agent"));
    assert!(governance.principals.lock().await.is_empty());
    assert!(governance.contexts.lock().await.is_empty());
}

#[test]
fn request_identity_rejects_unpaired_typedid_proof() {
    let mut headers = HeaderMap::new();
    headers.insert("x-lakecat-agent-did", "did:example:agent".parse().unwrap());
    headers.insert(
        "x-lakecat-typedid-proof",
        "raw-proof-secret".parse().unwrap(),
    );

    let err = request_identity(&headers).expect_err("unpaired proof should fail closed");
    let LakeCatHttpError(inner) = err;
    let message = inner.to_string();
    assert!(message.contains("x-lakecat-typedid-proof requires x-lakecat-typedid-envelope"));
    assert!(message.contains("typedid-proof-hash=sha256:"));
    assert!(!message.contains("raw-proof-secret"));
}

#[tokio::test]
async fn config_endpoint_rejects_unpaired_typedid_proof_before_governance() {
    let governance = Arc::new(RecordingGovernance::default());
    let app = app(LakeCatState::new(
        WarehouseName::new("local").unwrap(),
        MemoryCatalogStore::new(),
    )
    .with_integrations(
        default_sail_engine(),
        governance.clone(),
        NoopCatalogGraphSink::new(),
        HashOnlyLineageSink::new(),
    ));

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/catalog/v1/config")
                .header("x-lakecat-agent-did", "did:example:agent")
                .header("x-lakecat-typedid-proof", "raw-proof-secret")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let message = String::from_utf8(body.to_vec()).unwrap();
    assert!(message.contains("x-lakecat-typedid-proof requires x-lakecat-typedid-envelope"));
    assert!(message.contains("typedid-proof-hash=sha256:"));
    assert!(!message.contains("raw-proof-secret"));
    assert!(governance.principals.lock().await.is_empty());
    assert!(governance.contexts.lock().await.is_empty());
}

#[tokio::test]
async fn config_endpoint_redacts_typedid_subject_mismatch_before_governance() {
    let governance = Arc::new(RecordingGovernance::default());
    let verifier = Arc::new(StaticTypeDidVerifier {
        verification: TypeDidVerification {
            principal: Principal::new("did:example:verified-secret", PrincipalKind::Agent).unwrap(),
            attestation: serde_json::json!({
                "subject": "did:example:verified-secret",
                "resource": "lakecat:catalog:config"
            }),
        },
    });
    let app = app(LakeCatState::new(
        WarehouseName::new("local").unwrap(),
        MemoryCatalogStore::new(),
    )
    .with_integrations(
        default_sail_engine(),
        governance.clone(),
        NoopCatalogGraphSink::new(),
        HashOnlyLineageSink::new(),
    )
    .with_typedid_verifier(verifier));

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/catalog/v1/config")
                .header("x-lakecat-agent-did", "did:example:supplied-secret")
                .header(
                    "x-lakecat-typedid-envelope",
                    r#"{"protected":"typedid-envelope","payload":"secret"}"#,
                )
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CONFLICT);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let message = String::from_utf8(body.to_vec()).unwrap();
    assert!(message.contains("TypeDID verified subject does not match supplied principal"));
    assert!(message.contains("verified-principal-hash=sha256:"));
    assert!(message.contains("supplied-principal-hash=sha256:"));
    assert!(!message.contains("did:example:verified-secret"));
    assert!(!message.contains("did:example:supplied-secret"));
    assert!(!message.contains("typedid-envelope"));
    assert!(governance.principals.lock().await.is_empty());
    assert!(governance.contexts.lock().await.is_empty());
}

#[tokio::test]
async fn config_endpoint_redacts_custom_typedid_verifier_errors_before_governance() {
    let governance = Arc::new(RecordingGovernance::default());
    let verifier = Arc::new(LeakingTypeDidVerifier {
        err: LakeCatError::Conflict(
            "gateway rejected did:example:agent with raw envelope payload secret=abc".to_string(),
        ),
    });
    let app = app(LakeCatState::new(
        WarehouseName::new("local").unwrap(),
        MemoryCatalogStore::new(),
    )
    .with_integrations(
        default_sail_engine(),
        governance.clone(),
        NoopCatalogGraphSink::new(),
        HashOnlyLineageSink::new(),
    )
    .with_typedid_verifier(verifier));

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/catalog/v1/config")
                .header("x-lakecat-agent-did", "did:example:agent")
                .header(
                    "x-lakecat-typedid-envelope",
                    r#"{"protected":"typedid-envelope","payload":"secret=abc"}"#,
                )
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CONFLICT);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let message = String::from_utf8(body.to_vec()).unwrap();
    assert!(message.contains("TypeDID envelope verification failed"));
    assert!(message.contains("typedid-envelope-hash=sha256:"));
    assert!(message.contains("error-detail-hash=sha256:"));
    for forbidden in [
        "did:example:agent",
        "raw envelope payload",
        "secret=abc",
        r#""protected":"typedid-envelope""#,
    ] {
        assert!(
            !message.contains(forbidden),
            "TypeDID verifier errors must not expose {forbidden}"
        );
    }
    assert!(governance.principals.lock().await.is_empty());
    assert!(governance.contexts.lock().await.is_empty());
}

#[test]
fn request_identity_rejects_agent_proof_headers_without_agent_identity() {
    let mut headers = HeaderMap::new();
    headers.insert("x-lakecat-principal", "alice@example.com".parse().unwrap());
    headers.insert("x-lakecat-principal-kind", "human".parse().unwrap());
    headers.insert(
        "x-lakecat-agent-delegation",
        "raw-delegation-secret".parse().unwrap(),
    );
    headers.insert(
        "x-lakecat-agent-summary-signature",
        "raw-summary-secret".parse().unwrap(),
    );

    let err = request_identity(&headers).expect_err("agent proof should require an agent");
    let LakeCatHttpError(inner) = err;
    let message = inner.to_string();
    assert!(message.contains("x-lakecat-agent-delegation requires an agent identity"));
    assert!(message.contains("agent-delegation-hash=sha256:"));
    assert!(!message.contains("raw-delegation-secret"));
    assert!(!message.contains("raw-summary-secret"));
}

#[tokio::test]
async fn config_endpoint_rejects_agent_summary_without_agent_before_governance() {
    let governance = Arc::new(RecordingGovernance::default());
    let app = app(LakeCatState::new(
        WarehouseName::new("local").unwrap(),
        MemoryCatalogStore::new(),
    )
    .with_integrations(
        default_sail_engine(),
        governance.clone(),
        NoopCatalogGraphSink::new(),
        HashOnlyLineageSink::new(),
    ));

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/catalog/v1/config")
                .header("x-lakecat-principal", "alice@example.com")
                .header("x-lakecat-principal-kind", "human")
                .header("x-lakecat-agent-summary-signature", "raw-summary-secret")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let message = String::from_utf8(body.to_vec()).unwrap();
    assert!(message.contains("x-lakecat-agent-summary-signature requires an agent identity"));
    assert!(message.contains("agent-summary-signature-hash=sha256:"));
    assert!(!message.contains("raw-summary-secret"));
    assert!(governance.principals.lock().await.is_empty());
    assert!(governance.contexts.lock().await.is_empty());
}

#[cfg(feature = "typesec-local")]
#[tokio::test]
async fn typesec_typedid_envelope_verification_updates_authorization_context() {
    use typesec::integrations::{
        DidMessageBody, StaticDidResolver, TypeDidConversation, TypeDidMode, TypeDidProfile,
    };
    use typesec::{Did, DidEnvelope, Ed25519DidKey, Ed25519DidKeyStore, TypeDidGateway};

    let agent_key = Ed25519DidKey::from_seed(b"lakecat-agent-ed25519");
    let lakecat_key = Ed25519DidKey::from_seed(b"lakecat-service-ed25519");
    let agent = Did::key(agent_key.signing_public());
    let lakecat = Did::key(lakecat_key.signing_public());
    let resolver = StaticDidResolver::new()
        .with_document(agent_key.document(agent.clone()))
        .with_document(lakecat_key.document(lakecat.clone()));
    let keys = Ed25519DidKeyStore::new()
        .with_key(agent.clone(), agent_key)
        .with_key(lakecat.clone(), lakecat_key);
    let envelope = DidEnvelope::typedid(
        "lakecat-typedid-1",
        agent.clone(),
        lakecat.clone(),
        DidMessageBody::agent_message("lakecat:catalog:config", "internal"),
        TypeDidConversation::new(
            "lakecat-config",
            TypeDidMode::RequestReply,
            TypeDidProfile::ed25519_x25519_chacha20().id,
            "https",
        ),
        b"secret agent payload",
        &resolver,
        &keys,
    )
    .expect("typedid envelope");
    let envelope_json = serde_json::to_string(&envelope).expect("typedid envelope json");
    let envelope_signature = envelope.signature.clone();
    let gateway = Arc::new(TypeDidGateway::new(
        Arc::new(resolver),
        Arc::new(keys),
        lakecat,
    ));
    let governance = Arc::new(RecordingGovernance::default());
    let app = app(LakeCatState::new(
        WarehouseName::new("local").unwrap(),
        MemoryCatalogStore::new(),
    )
    .with_integrations(
        default_sail_engine(),
        governance.clone(),
        NoopCatalogGraphSink::new(),
        HashOnlyLineageSink::new(),
    )
    .with_typedid_verifier(crate::typesec_typedid::TypeSecTypeDidVerifier::new(gateway)));

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/catalog/v1/config")
                .header("x-lakecat-typedid-envelope", envelope_json.clone())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let principals = governance.principals.lock().await;
    assert_eq!(principals[0].subject, agent.to_string());
    assert_eq!(principals[0].kind, PrincipalKind::Agent);
    drop(principals);

    let contexts = governance.contexts.lock().await;
    let request_identity = &contexts[0]["request-identity"];
    assert_eq!(
        request_identity["source"],
        serde_json::json!("x-lakecat-typedid-envelope")
    );
    assert_eq!(
        request_identity["typedid"],
        serde_json::json!(agent.to_string())
    );
    assert_eq!(
        request_identity["attestation-state"],
        serde_json::json!("verified")
    );
    assert_eq!(
        request_identity["typedid-envelope-sha256"],
        serde_json::json!(content_hash_bytes(envelope_json.as_bytes()))
    );
    assert_eq!(
        request_identity["typedid-attestation"]["subject"],
        serde_json::json!(agent.to_string())
    );
    assert_eq!(
        request_identity["typedid-attestation"]["envelope_id"],
        serde_json::json!("lakecat-typedid-1")
    );
    assert_eq!(
        request_identity["typedid-attestation"]["resource"],
        serde_json::json!("lakecat:catalog:config")
    );
    let rendered = request_identity.to_string();
    assert!(!rendered.contains("secret agent payload"));
    assert!(!rendered.contains(&envelope_signature));
}

#[test]
fn request_identity_typedid_header_alone_selects_agent_principal() {
    let mut headers = HeaderMap::new();
    headers.insert(
        "x-lakecat-typedid",
        "did:example:typedid-only".parse().unwrap(),
    );
    let identity = request_identity(&headers).expect("identity should parse");
    assert_eq!(identity.principal.subject, "did:example:typedid-only");
    assert_eq!(identity.principal.kind, PrincipalKind::Agent);
    assert_eq!(
        identity.envelope["source"],
        serde_json::json!("x-lakecat-typedid")
    );
    assert_eq!(
        identity.envelope["typedid"],
        serde_json::json!("did:example:typedid-only")
    );
}

#[test]
fn request_identity_agent_did_takes_precedence_over_typedid() {
    let mut headers = HeaderMap::new();
    headers.insert("x-lakecat-agent-did", "did:example:agent".parse().unwrap());
    headers.insert("x-lakecat-typedid", "did:example:typedid".parse().unwrap());
    let identity = request_identity(&headers).expect("identity should parse");
    assert_eq!(identity.principal.subject, "did:example:agent");
    assert_eq!(identity.principal.kind, PrincipalKind::Agent);
    assert_eq!(
        identity.envelope["source"],
        serde_json::json!("x-lakecat-agent-did")
    );
}

#[tokio::test]
async fn authorization_headers_resolve_typed_principal() {
    let governance = Arc::new(RecordingGovernance::default());
    let app = app(LakeCatState::new(
        WarehouseName::new("local").unwrap(),
        MemoryCatalogStore::new(),
    )
    .with_integrations(
        default_sail_engine(),
        governance.clone(),
        NoopCatalogGraphSink::new(),
        HashOnlyLineageSink::new(),
    ));
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/catalog/v1/config")
                .header("x-lakecat-principal", "alice@example.com")
                .header("x-lakecat-principal-kind", "human")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let principals = governance.principals.lock().await;
    assert_eq!(principals[0].subject, "alice@example.com");
    assert_eq!(principals[0].kind, PrincipalKind::Human);
    drop(principals);
    let contexts = governance.contexts.lock().await;
    assert_eq!(
        contexts[0]["request-identity"]["source"],
        serde_json::json!("x-lakecat-principal")
    );
    assert_eq!(
        contexts[0]["request-identity"]["principal"]["subject"],
        serde_json::json!("alice@example.com")
    );
}

#[tokio::test]
async fn outbox_drain_rejects_mismatched_table_commit_identity_evidence() {
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let other_table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("shadow_events").unwrap(),
    );
    let principal = Principal::new("agent:writer", PrincipalKind::Agent).unwrap();
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-mismatched-commit-table".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "table.commit".to_string(),
            payload: json!({
                "audit-event-id": "audit-mismatched-commit-table",
                "event-type": "table.commit",
                "table": table,
                "commit": {
                    "table": other_table,
                    "previous_metadata_location": "file:///tmp/events/metadata/00000.json",
                    "new_metadata_location": "file:///tmp/events/metadata/00001.json",
                    "sequence_number": 7,
                    "principal": principal,
                    "format_version": 3,
                    "snapshot_id": 42,
                    "policy_hash": null,
                    "request_hash": content_hash_json(&json!({"request": "commit"})).unwrap(),
                    "response_hash": content_hash_json(&json!({"response": "commit"})).unwrap(),
                    "idempotency_key_sha256": content_hash_bytes("commit:events:0001".as_bytes()),
                    "committed_at": chrono::Utc::now(),
                },
                "authorization-receipt": {
                    "principal": principal,
                    "action": "table-commit",
                    "allowed": true,
                    "engine": "test",
                    "policy_hash": null,
                    "checked_at": chrono::Utc::now(),
                },
            }),
            created_at: chrono::Utc::now(),
            delivered_at: None,
        }]),
        delivered: Mutex::default(),
    });
    let graph = Arc::new(RecordingGraph::default());
    let lineage = Arc::new(RecordingLineage::default());
    let state = LakeCatState::new(WarehouseName::new("local").unwrap(), store.clone())
        .with_integrations(
            default_sail_engine(),
            AllowAllGovernanceEngine::new(),
            graph.clone(),
            lineage.clone(),
        );

    let err = drain_outbox_once(&state, 10)
        .await
        .expect_err("mismatched table commit identity should fail before delivery");

    let message = err.to_string();
    assert!(message.contains("table.commit"));
    assert!(message.contains("table commit table does not match table identity"));
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains("evt-mismatched-commit-table"));
    assert!(
        store.delivered.lock().await.is_empty(),
        "mismatched commit identity must fail before acknowledgement"
    );
    assert!(
        graph.events.lock().await.is_empty(),
        "mismatched commit identity must fail before graph projection"
    );
    assert!(
        lineage.events.lock().await.is_empty(),
        "mismatched commit identity must fail before lineage projection"
    );
}

#[tokio::test]
async fn outbox_drain_rejects_mismatched_table_commit_history_identity() {
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let principal = Principal {
        subject: "agent:writer".to_string(),
        kind: PrincipalKind::Agent,
    };
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-mismatched-commit-history-identity".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "table.commits-listed".to_string(),
            payload: json!({
                "audit-event-id": "audit-mismatched-commit-history-identity",
                "event-type": "table.commits-listed",
                "table": table,
                "payload": {
                    "authorization-receipt": {
                        "principal": principal,
                        "action": "table-load",
                        "allowed": true,
                        "engine": "test",
                        "policy_hash": null,
                        "checked_at": chrono::Utc::now(),
                    },
                    "warehouse": "local",
                    "namespace": ["other"],
                    "table": "events",
                    "commit-count": 1,
                    "commit-hashes": [
                        content_hash_json(&json!({"commit": 1})).unwrap()
                    ],
                    "sequence-numbers": [1],
                },
            }),
            created_at: chrono::Utc::now(),
            delivered_at: None,
        }]),
        delivered: Mutex::default(),
    });
    let graph = Arc::new(RecordingGraph::default());
    let lineage = Arc::new(RecordingLineage::default());
    let state = LakeCatState::new(WarehouseName::new("local").unwrap(), store.clone())
        .with_integrations(
            default_sail_engine(),
            AllowAllGovernanceEngine::new(),
            graph.clone(),
            lineage.clone(),
        );

    let err = drain_outbox_once(&state, 10)
        .await
        .expect_err("mismatched commit-history table identity should fail before delivery");

    let message = err.to_string();
    assert!(message.contains("table.commits-listed"));
    assert!(
        message.contains("table commit-history namespace does not match table identity"),
        "{message}"
    );
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains("evt-mismatched-commit-history-identity"));
    assert!(
        store.delivered.lock().await.is_empty(),
        "mismatched commit-history table identity must fail before acknowledgement"
    );
    assert!(
        graph.events.lock().await.is_empty(),
        "mismatched commit-history table identity must fail before graph projection"
    );
    assert!(
        lineage.events.lock().await.is_empty(),
        "mismatched commit-history table identity must fail before lineage projection"
    );
}

#[tokio::test]
async fn outbox_drain_rejects_mismatched_credential_vend_table_identity() {
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let forged_table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("other_events").unwrap(),
    );
    let principal = Principal {
        subject: "human:operator".to_string(),
        kind: PrincipalKind::Human,
    };
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-credential-vend-table-drift".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "credentials.vend-attempted".to_string(),
            payload: json!({
                "audit-event-id": "audit-credential-vend-table-drift",
                "event-type": "credentials.vend-attempted",
                "table": table,
                "payload": {
                    "authorization-receipt": {
                        "principal": principal,
                        "action": "credentials-vend",
                        "allowed": true,
                        "engine": "test",
                        "policy_hash": null,
                        "checked_at": chrono::Utc::now(),
                    },
                    "table": forged_table,
                    "credential-count": 0,
                    "credential-response-evidence": [],
                    "storage-profile-id": "events-local",
                    "storage-profile": {
                        "profile-id": "events-local",
                        "warehouse": "local",
                        "provider": "file",
                        "issuance-mode": "local-file-no-secret",
                        "secret-ref-present": false,
                        "location-prefix-hash": "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                    },
                },
            }),
            created_at: chrono::Utc::now(),
            delivered_at: None,
        }]),
        delivered: Mutex::default(),
    });
    let graph = Arc::new(RecordingGraph::default());
    let lineage = Arc::new(RecordingLineage::default());
    let state = LakeCatState::new(WarehouseName::new("local").unwrap(), store.clone())
        .with_integrations(
            default_sail_engine(),
            AllowAllGovernanceEngine::new(),
            graph.clone(),
            lineage.clone(),
        );

    let err = drain_outbox_once(&state, 10)
        .await
        .expect_err("mismatched credential-vend table identity should fail before delivery");

    let message = err.to_string();
    assert!(message.contains("credentials.vend-attempted"));
    assert!(
        message.contains("credential-vend payload table does not match table identity"),
        "{message}"
    );
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains("evt-credential-vend-table-drift"));
    assert!(
        store.delivered.lock().await.is_empty(),
        "mismatched credential-vend table identity must fail before acknowledgement"
    );
    assert!(
        graph.events.lock().await.is_empty(),
        "mismatched credential-vend table identity must fail before graph projection"
    );
    assert!(
        lineage.events.lock().await.is_empty(),
        "mismatched credential-vend table identity must fail before lineage projection"
    );
}

#[tokio::test]
async fn outbox_drain_rejects_extra_querygraph_bootstrap_request_identity_fields() {
    let principal = Principal::new("agent:reader", PrincipalKind::Agent).unwrap();
    let mut payload = valid_querygraph_bootstrap_payload(principal.clone());
    payload["payload"]["authorization-receipt"]["context"] = json!({
        "request-identity": {
            "type": "lakecat.request-identity.v1",
            "principal": principal,
            "source": "x-lakecat-typedid-envelope",
            "agent-did": null,
            "typedid": null,
            "typedid-envelope-sha256": content_hash_json(&json!({"typedid": "envelope"})).unwrap(),
            "typedid-proof-sha256": content_hash_json(&json!({"typedid": "proof"})).unwrap(),
            "agent-delegation-sha256": null,
            "agent-summary-signature-sha256": null,
            "bearer-token-sha256": null,
            "attestation-state": "unverified",
            "raw-secret-material": false,
            "unverified-identity-claim": "agent-safe"
        }
    });
    let event_id = "evt-bootstrap-extra-request-identity";
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: event_id.to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "querygraph.bootstrap".to_string(),
            payload,
            created_at: chrono::Utc::now(),
            delivered_at: None,
        }]),
        delivered: Mutex::default(),
    });
    let graph = Arc::new(RecordingGraph::default());
    let lineage = Arc::new(RecordingLineage::default());
    let state = LakeCatState::new(WarehouseName::new("local").unwrap(), store.clone())
        .with_integrations(
            default_sail_engine(),
            AllowAllGovernanceEngine::new(),
            graph.clone(),
            lineage.clone(),
        );

    let err = drain_outbox_once(&state, 10)
        .await
        .expect_err("extra QueryGraph bootstrap request-identity fields should fail");

    let message = err.to_string();
    assert!(message.contains("querygraph.bootstrap"));
    assert!(
        message.contains(
            "querygraph bootstrap request-identity contains unexpected field unverified-identity-claim"
        ),
        "extra QueryGraph bootstrap request identity field should be rejected: {message}"
    );
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains(event_id));
    assert!(
        store.delivered.lock().await.is_empty(),
        "extra request identity evidence must fail before acknowledgement"
    );
    assert!(
        graph.events.lock().await.is_empty(),
        "extra request identity evidence must fail before graph projection"
    );
    assert!(
        lineage.events.lock().await.is_empty(),
        "extra request identity evidence must fail before lineage projection"
    );
}

#[tokio::test]
async fn outbox_drain_rejects_missing_table_lifecycle_identity() {
    let principal = Principal::new("agent:writer", PrincipalKind::Agent).unwrap();
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-secret-table-token".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "table.created".to_string(),
            payload: json!({
                "audit-event-id": "audit-corrupt-table",
                "event-type": "table.created",
                "payload": {
                    "authorization-receipt": {
                        "principal": principal,
                        "action": "table-create",
                        "allowed": true,
                        "engine": "test",
                        "policy_hash": null,
                        "checked_at": chrono::Utc::now(),
                    },
                    "metadata-location": "file:///tmp/events/metadata/00000.json",
                }
            }),
            created_at: chrono::Utc::now(),
            delivered_at: None,
        }]),
        delivered: Mutex::default(),
    });
    let graph = Arc::new(RecordingGraph::default());
    let lineage = Arc::new(RecordingLineage::default());
    let state = LakeCatState::new(WarehouseName::new("local").unwrap(), store.clone())
        .with_integrations(
            default_sail_engine(),
            AllowAllGovernanceEngine::new(),
            graph.clone(),
            lineage.clone(),
        );

    let err = drain_outbox_once(&state, 10)
        .await
        .expect_err("missing table lifecycle identity should fail");

    let message = err.to_string();
    assert!(message.contains("outbox event table.created (lakecat.lineage-and-graph) has invalid"));
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(message.contains("table lifecycle evidence must contain table identity"));
    assert!(!message.contains("evt-secret-table-token"));
    assert!(
        store.delivered.lock().await.is_empty(),
        "malformed table lifecycle evidence must fail before acknowledgement"
    );
    assert!(
        graph.events.lock().await.is_empty(),
        "malformed table lifecycle evidence must fail before graph projection"
    );
    assert!(
        lineage.events.lock().await.is_empty(),
        "malformed table lifecycle evidence must fail before lineage projection"
    );
}

#[tokio::test]
async fn outbox_drain_rejects_extra_table_lifecycle_identity_fields() {
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let principal = Principal::new("agent:writer", PrincipalKind::Agent).unwrap();
    let checked_at = chrono::Utc::now();
    let receipt = |action: &str| {
        json!({
            "principal": &principal,
            "action": action,
            "allowed": true,
            "engine": "test",
            "policy_hash": null,
            "checked_at": checked_at,
        })
    };
    let mut table_with_extra = serde_json::to_value(&table).unwrap();
    table_with_extra["unverified-table-claim"] = json!(true);
    let mut soft_delete_with_extra = json!({
        "table": &table,
        "metadata-location": "file:///tmp/events/metadata/00000.json",
        "version": 1,
        "format-version": 3,
        "principal": &principal,
        "authorization-receipt": null,
        "deleted-at": checked_at,
    });
    soft_delete_with_extra["unverified-soft-delete-claim"] = json!(true);
    let mut metadata_graph_with_extra = json!({
        "current-schema-id": 1,
        "fields": [{
            "id": 1,
            "name": "event_id",
            "type": "string",
            "required": true,
        }],
        "current-snapshot-id": null,
        "current-snapshot": null,
    });
    metadata_graph_with_extra["unverified-metadata-graph-claim"] = json!(true);
    let cases = vec![
        (
            "evt-created-extra-table-identity",
            "table.created",
            "table lifecycle table identity contains unexpected field unverified-table-claim",
            json!({
                "audit-event-id": "audit-created-extra-table-identity",
                "event-type": "table.created",
                "table": table_with_extra,
                "payload": {
                    "authorization-receipt": receipt("table-create"),
                    "metadata-location": "file:///tmp/events/metadata/00000.json",
                    "format-version": 3,
                    "version": 1,
                }
            }),
        ),
        (
            "evt-deleted-extra-soft-delete",
            "table.deleted",
            "table lifecycle soft-delete contains unexpected field unverified-soft-delete-claim",
            json!({
                "audit-event-id": "audit-deleted-extra-soft-delete",
                "event-type": "table.deleted",
                "table": &table,
                "soft-delete": soft_delete_with_extra,
                "authorization-receipt": receipt("table-drop"),
            }),
        ),
        (
            "evt-created-extra-metadata-graph",
            "table.created",
            "table lifecycle metadata-graph contains unexpected field unverified-metadata-graph-claim",
            json!({
                "audit-event-id": "audit-created-extra-metadata-graph",
                "event-type": "table.created",
                "table": &table,
                "payload": {
                    "authorization-receipt": receipt("table-create"),
                    "metadata-location": "file:///tmp/events/metadata/00000.json",
                    "format-version": 3,
                    "metadata-graph": metadata_graph_with_extra,
                    "version": 1,
                }
            }),
        ),
    ];

    for (event_id, event_type, expected_message, payload) in cases {
        let store = Arc::new(RecordingOutboxStore {
            events: Mutex::new(vec![OutboxEvent {
                event_id: event_id.to_string(),
                sink: "lakecat.lineage-and-graph".to_string(),
                event_type: event_type.to_string(),
                payload,
                created_at: chrono::Utc::now(),
                delivered_at: None,
            }]),
            delivered: Mutex::default(),
        });
        let graph = Arc::new(RecordingGraph::default());
        let lineage = Arc::new(RecordingLineage::default());
        let state = LakeCatState::new(WarehouseName::new("local").unwrap(), store.clone())
            .with_integrations(
                default_sail_engine(),
                AllowAllGovernanceEngine::new(),
                graph.clone(),
                lineage.clone(),
            );

        let err = drain_outbox_once(&state, 10)
            .await
            .expect_err("extra table lifecycle fields should fail");

        let message = err.to_string();
        assert!(message.contains(event_type), "{message}");
        assert!(message.contains(expected_message), "{event_id}: {message}");
        assert!(message.contains("event-id-hash=sha256:"), "{message}");
        assert!(!message.contains(event_id), "{message}");
        assert!(
            store.delivered.lock().await.is_empty(),
            "{event_type} must fail before acknowledgement"
        );
        assert!(
            graph.events.lock().await.is_empty(),
            "{event_type} must fail before graph projection"
        );
        assert!(
            lineage.events.lock().await.is_empty(),
            "{event_type} must fail before lineage projection"
        );
    }
}

#[test]
fn lineage_drain_summary_rejects_malformed_request_identity_hashes() {
    let receipt = OutboxProjectionReceipt::default();
    let cases = [
        (
            "typedid-envelope-sha256",
            json!("sha256:not-full"),
            "typedid-envelope-sha256 must contain full SHA-256 digest evidence",
        ),
        (
            "typedid-proof-sha256",
            json!(42),
            "typedid-proof-sha256 must contain full SHA-256 digest evidence",
        ),
        (
            "agent-delegation-sha256",
            json!("sha256:not-full"),
            "agent-delegation-sha256 must contain full SHA-256 digest evidence",
        ),
        (
            "agent-summary-signature-sha256",
            json!("sha256:not-full"),
            "agent-summary-signature-sha256 must contain full SHA-256 digest evidence",
        ),
    ];

    for (field, value, expected_message) in cases {
        let mut event =
            valid_lineage_summary_querygraph_bootstrap_event(&format!("evt-bad-summary-{field}"));
        event.payload["payload"]["authorization-receipt"]["context"] = json!({
            "request-identity": {
                field: value
            }
        });
        let err = lineage_drain_event_summary(&event, &receipt)
            .unwrap_err()
            .to_string();
        assert!(err.contains(expected_message), "{err}");
    }
}

#[test]
fn lineage_drain_summary_rejects_malformed_authorization_identity_strings() {
    let receipt = OutboxProjectionReceipt::default();
    let cases = [
        (
            "evt-bad-summary-principal-subject",
            json!({
                "authorization-receipt": {
                    "principal": {
                        "subject": 42,
                        "kind": "agent"
                    }
                }
            }),
            "querygraph bootstrap authorization receipt principal must be a valid principal",
        ),
        (
            "evt-blank-summary-principal-kind",
            json!({
                "authorization-receipt": {
                    "principal": {
                        "subject": "agent:writer",
                        "kind": " "
                    }
                }
            }),
            "querygraph bootstrap authorization receipt principal must be a valid principal",
        ),
        (
            "evt-bad-summary-receipt-action",
            json!({
                "authorization-receipt": {
                    "action": 42
                }
            }),
            "querygraph bootstrap evidence must contain authorization receipt action",
        ),
        (
            "evt-blank-summary-request-identity-state",
            json!({
                "authorization-receipt": {
                    "context": {
                        "request-identity": {
                            "attestation-state": " "
                        }
                    }
                }
            }),
            "attestation-state must not be blank",
        ),
        (
            "evt-bad-summary-request-identity-source",
            json!({
                "authorization-receipt": {
                    "context": {
                        "request-identity": {
                            "source": 42
                        }
                    }
                }
            }),
            "source must be a string when present",
        ),
    ];

    for (event_id, payload, expected_message) in cases {
        let mut event = valid_lineage_summary_querygraph_bootstrap_event(event_id);
        merge_json_object(&mut event.payload["payload"], payload);
        let err = lineage_drain_event_summary(&event, &receipt)
            .unwrap_err()
            .to_string();
        assert!(err.contains(expected_message), "{err}");
    }
}
