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
fn view_receipt_chain_verifier_requires_version_transitions() {
    let receipts = vec![
        test_view_receipt(1, None, None, "upsert", "sha256:r1"),
        test_view_receipt(2, Some(1), Some("sha256:r1"), "upsert", "sha256:r2"),
        test_view_receipt(2, Some(2), Some("sha256:r2"), "drop", "sha256:r3"),
    ];
    assert!(view_version_receipt_chain_verified(&receipts));

    let zero_version = vec![test_view_receipt(0, None, None, "upsert", "sha256:r0")];
    assert!(!view_version_receipt_chain_verified(&zero_version));

    let first_receipt_drop = vec![test_view_receipt(1, None, None, "drop", "sha256:r1")];
    assert!(!view_version_receipt_chain_verified(&first_receipt_drop));

    let first_receipt_with_previous_link = vec![test_view_receipt(
        1,
        Some(1),
        Some("sha256:previous"),
        "upsert",
        "sha256:r1",
    )];
    assert!(!view_version_receipt_chain_verified(
        &first_receipt_with_previous_link
    ));

    let skipped_version = vec![
        test_view_receipt(1, None, None, "upsert", "sha256:r1"),
        test_view_receipt(3, Some(1), Some("sha256:r1"), "upsert", "sha256:r3"),
    ];
    assert!(!view_version_receipt_chain_verified(&skipped_version));

    let tombstone_advanced_version = vec![
        test_view_receipt(1, None, None, "upsert", "sha256:r1"),
        test_view_receipt(2, Some(1), Some("sha256:r1"), "drop", "sha256:r2"),
    ];
    assert!(!view_version_receipt_chain_verified(
        &tombstone_advanced_version
    ));

    let wrong_previous_version = vec![
        test_view_receipt(1, None, None, "upsert", "sha256:r1"),
        test_view_receipt(2, Some(2), Some("sha256:r1"), "upsert", "sha256:r2"),
    ];
    assert!(!view_version_receipt_chain_verified(
        &wrong_previous_version
    ));

    let wrong_previous_receipt_hash = vec![
        test_view_receipt(1, None, None, "upsert", "sha256:r1"),
        test_view_receipt(2, Some(1), Some("sha256:other"), "upsert", "sha256:r2"),
    ];
    assert!(!view_version_receipt_chain_verified(
        &wrong_previous_receipt_hash
    ));

    let unsupported_operation = vec![
        test_view_receipt(1, None, None, "upsert", "sha256:r1"),
        test_view_receipt(2, Some(1), Some("sha256:r1"), "replace", "sha256:r2"),
    ];
    assert!(!view_version_receipt_chain_verified(&unsupported_operation));
}

#[tokio::test]
async fn querygraph_bootstrap_projects_catalog_tables() {
    let app = test_app();
    let server = Request::builder()
        .method(Method::PUT)
        .uri("/management/v1/servers/prod-server")
        .header("content-type", "application/json")
        .header("x-lakecat-principal", "operator@example.com")
        .body(Body::from(
            serde_json::json!({
                "display-name": "Production LakeCat",
                "endpoint-url": "https://lakecat.example.com"
            })
            .to_string(),
        ))
        .unwrap();
    let response = app.clone().oneshot(server).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let project = Request::builder()
        .method(Method::PUT)
        .uri("/management/v1/projects/analytics")
        .header("content-type", "application/json")
        .header("x-lakecat-principal", "operator@example.com")
        .body(Body::from(
            serde_json::json!({
                "server-id": "prod-server",
                "display-name": "Analytics"
            })
            .to_string(),
        ))
        .unwrap();
    let response = app.clone().oneshot(project).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let warehouse = Request::builder()
        .method(Method::PUT)
        .uri("/management/v1/projects/analytics/warehouses/local")
        .header("content-type", "application/json")
        .header("x-lakecat-principal", "operator@example.com")
        .body(Body::from(
            serde_json::json!({
                "storage-root": "file:///tmp/lakecat-analytics"
            })
            .to_string(),
        ))
        .unwrap();
    let response = app.clone().oneshot(warehouse).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let create = Request::builder()
        .method(Method::POST)
        .uri("/catalog/v1/namespaces/default/tables")
        .header("content-type", "application/json")
        .body(Body::from(
            r#"{"name":"events","location":"file:///tmp/events","metadata-location":"file:///tmp/events/metadata/00000.json","metadata":{"format-version":3,"current-schema-id":1,"schemas":[{"schema-id":1,"fields":[{"id":1,"name":"event_id","type":"string","required":true,"doc":"Event identifier.","semantic-type":"https://schema.org/identifier"}]}]}}"#,
        ))
        .unwrap();
    let response = app.clone().oneshot(create).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let policy = Request::builder()
        .method(Method::PUT)
        .uri("/management/v1/warehouses/local/policies/agent-read")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::json!({
                "namespace": ["default"],
                "table": "events",
                "enforced": true,
                "odrl": {
                    "uid": "policy:agent-read",
                    "lakecat:read-restriction": {
                        "allowed-columns": ["event_id"]
                    }
                }
            })
            .to_string(),
        ))
        .unwrap();
    let response = app.clone().oneshot(policy).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let bootstrap = Request::builder()
        .method(Method::GET)
        .uri("/querygraph/v1/bootstrap")
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(bootstrap).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(
        body["bundle-hash"]
            .as_str()
            .is_some_and(|value| !value.is_empty())
    );
    assert!(
        body["manifest"]["graph-hash"]
            .as_str()
            .is_some_and(|value| !value.is_empty())
    );
    assert!(
        body["manifest"]["open-lineage-hash"]
            .as_str()
            .is_some_and(|value| !value.is_empty())
    );
    let graph_nodes = body["graph"]["nodes"].as_array().unwrap();
    let server_node = graph_nodes
        .iter()
        .find(|node| node["id"] == serde_json::json!("lakecat:server:prod-server"))
        .expect("bootstrap graph should include durable server node");
    assert_eq!(server_node["label"], serde_json::json!("Server"));
    assert_eq!(
        server_node["properties"]["displayName"],
        serde_json::json!("Production LakeCat")
    );
    assert_eq!(
        server_node["properties"]["source"],
        serde_json::json!("lakecat-management-records")
    );
    assert_eq!(
        server_node["properties"]["endpointUrl"],
        serde_json::Value::Null
    );
    assert!(
        server_node["properties"]["endpointUrlHash"]
            .as_str()
            .is_some_and(is_full_sha256_hash)
    );
    let project_node = graph_nodes
        .iter()
        .find(|node| node["id"] == serde_json::json!("lakecat:project:analytics"))
        .expect("bootstrap graph should include durable project node");
    assert_eq!(project_node["label"], serde_json::json!("Project"));
    assert_eq!(
        project_node["properties"]["serverId"],
        serde_json::json!("prod-server")
    );
    let warehouse_node = graph_nodes
        .iter()
        .find(|node| node["id"] == serde_json::json!("lakecat:warehouse:local"))
        .expect("bootstrap graph should include durable warehouse node");
    assert_eq!(warehouse_node["label"], serde_json::json!("Warehouse"));
    assert_eq!(
        warehouse_node["properties"]["projectId"],
        serde_json::json!("analytics")
    );
    assert_eq!(
        warehouse_node["properties"]["storageRoot"],
        serde_json::Value::Null
    );
    assert!(
        warehouse_node["properties"]["storageRootHash"]
            .as_str()
            .is_some_and(is_full_sha256_hash)
    );
    let graph_json = serde_json::to_string(&body["graph"]).unwrap();
    assert!(
        !graph_json.contains("https://lakecat.example.com"),
        "bootstrap graph must not expose raw server endpoint URLs"
    );
    assert!(
        !graph_json.contains("file:///tmp/lakecat-analytics"),
        "bootstrap graph must not expose raw warehouse storage roots"
    );
    let graph_edges = body["graph"]["edges"].as_array().unwrap();
    assert!(graph_edges.iter().any(|edge| edge
        == &serde_json::json!({
            "from": "lakecat:catalog",
            "to": "lakecat:server:prod-server",
            "label": "HAS_SERVER"
        })));
    assert!(graph_edges.iter().any(|edge| edge
        == &serde_json::json!({
            "from": "lakecat:server:prod-server",
            "to": "lakecat:project:analytics",
            "label": "HAS_PROJECT"
        })));
    assert!(graph_edges.iter().any(|edge| edge
        == &serde_json::json!({
            "from": "lakecat:project:analytics",
            "to": "lakecat:warehouse:local",
            "label": "HAS_WAREHOUSE"
        })));
    assert_eq!(
        body["manifest"]["querygraph-import"]["schema-version"],
        serde_json::json!("lakecat.querygraph.import-compat.v1")
    );
    assert!(
        body["manifest"]["querygraph-import"]["table-only-bundle-hash"]
            .as_str()
            .is_some_and(|value| value.starts_with("sha256:"))
    );
    assert_eq!(
        body["manifest"]["querygraph-import"]["graph-hash"],
        body["manifest"]["graph-hash"]
    );
    assert_eq!(
        body["manifest"]["querygraph-import"]["view-count"],
        serde_json::json!(0)
    );
    assert_eq!(
        body["open-lineage"]["run"]["facets"]["queryGraph_semanticBundle"]["graphHash"],
        body["manifest"]["graph-hash"]
    );
    assert_eq!(
        body["open-lineage"]["run"]["facets"]["queryGraph_semanticBundle"]["tableArtifacts"][0]["stableId"],
        body["manifest"]["table-artifacts"][0]["stable-id"]
    );
    assert_eq!(
        body["open-lineage"]["run"]["facets"]["queryGraph_semanticBundle"]["tableArtifacts"][0]["croissantHash"],
        body["manifest"]["table-artifacts"][0]["croissant-hash"]
    );
    assert_eq!(
        body["open-lineage"]["run"]["facets"]["queryGraph_semanticBundle"]["tableArtifacts"][0]["policyBindingsHash"],
        body["manifest"]["table-artifacts"][0]["policy-bindings-hash"]
    );
    assert!(
        body["manifest"]["standards"]
            .as_array()
            .unwrap()
            .iter()
            .any(|standard| standard == "Grust catalog graph")
    );
    assert_eq!(
        body["tables"][0]["policy-bindings"][0]["policy-id"],
        "agent-read"
    );
    assert_eq!(
        body["tables"][0]["policy-bindings"][0]["odrl"]["lakecat:read-restriction"]["allowed-columns"],
        serde_json::json!(["event_id"])
    );
    assert_eq!(
        body["tables"][0]["odrl"]["lakecat:policy-bindings"][0]["odrl"]["lakecat:read-restriction"]
            ["allowed-columns"],
        serde_json::json!(["event_id"])
    );
}

#[tokio::test]
async fn querygraph_bootstrap_projects_catalog_views() {
    let app = test_app();
    let namespace = Request::builder()
        .method(Method::POST)
        .uri("/catalog/v1/namespaces")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"namespace":["default"]}"#))
        .unwrap();
    let response = app.clone().oneshot(namespace).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let view = Request::builder()
        .method(Method::PUT)
        .uri("/management/v1/warehouses/local/namespaces/default/views/active_customers")
        .header("content-type", "application/json")
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
    let response = app.clone().oneshot(view).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let bootstrap = Request::builder()
        .method(Method::GET)
        .uri("/querygraph/v1/bootstrap")
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(bootstrap).await.unwrap();
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
    assert_eq!(
        body["manifest"]["view-artifacts"].as_array().unwrap().len(),
        1
    );
    assert_eq!(
        body["manifest"]["querygraph-import"]["view-receipt-evidence"][0]["stable-id"],
        body["views"][0]["stable-id"]
    );
    assert_eq!(
        body["manifest"]["querygraph-import"]["view-receipt-evidence"][0]["view-version"],
        body["views"][0]["view-version"]
    );
    assert!(
        body["manifest"]["querygraph-import"]["view-receipt-evidence"][0]["receipt-hash"]
            .as_str()
            .is_some_and(|hash| hash.starts_with("sha256:"))
    );
    assert!(
        body["manifest"]["querygraph-import"]["view-receipt-evidence-hash"]
            .as_str()
            .is_some_and(|hash| hash.starts_with("sha256:"))
    );
    assert!(
        body["graph"]["edges"]
            .as_array()
            .unwrap()
            .iter()
            .any(|edge| edge["label"] == serde_json::json!("CONTAINS_VIEW"))
    );
    assert_eq!(
        body["open-lineage"]["run"]["facets"]["queryGraph_semanticBundle"]["viewCount"],
        serde_json::json!(1)
    );
    assert_eq!(
        body["open-lineage"]["run"]["facets"]["queryGraph_semanticBundle"]["viewArtifacts"][0]["stableId"],
        body["manifest"]["view-artifacts"][0]["stable-id"]
    );
    assert_eq!(
        body["open-lineage"]["run"]["facets"]["queryGraph_semanticBundle"]["viewArtifacts"][0]["osiHash"],
        body["manifest"]["view-artifacts"][0]["osi-hash"]
    );
}

#[tokio::test]
async fn view_mutations_reject_zero_expected_version_without_receipts() {
    let app = test_app();
    let upsert = Request::builder()
        .method(Method::PUT)
        .uri("/management/v1/warehouses/local/namespaces/default/views/guarded_view")
        .header("content-type", "application/json")
        .header("x-lakecat-principal", "operator@example.com")
        .body(Body::from(
            serde_json::json!({
                "sql": "select id from customers",
                "dialect": "sql",
                "schema-version": 1
            })
            .to_string(),
        ))
        .unwrap();
    let response = app.clone().oneshot(upsert).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let invalid_update = Request::builder()
        .method(Method::PUT)
        .uri("/management/v1/warehouses/local/namespaces/default/views/guarded_view")
        .header("content-type", "application/json")
        .header("x-lakecat-principal", "operator@example.com")
        .body(Body::from(
            serde_json::json!({
                "sql": "select email from customers",
                "dialect": "sql",
                "schema-version": 2,
                "expected-view-version": 0
            })
            .to_string(),
        ))
        .unwrap();
    let response = app.clone().oneshot(invalid_update).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(
        body["error"]["message"]
            .as_str()
            .unwrap()
            .contains("expected view version must be greater than zero")
    );

    let invalid_drop = Request::builder()
        .method(Method::DELETE)
        .uri("/management/v1/warehouses/local/namespaces/default/views/guarded_view?expected-view-version=0")
        .header("x-lakecat-principal", "operator@example.com")
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(invalid_drop).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(
        body["error"]["message"]
            .as_str()
            .unwrap()
            .contains("expected view version must be greater than zero")
    );

    let load = Request::builder()
        .method(Method::GET)
        .uri("/catalog/v1/local/namespaces/default/views/guarded_view")
        .header("x-lakecat-principal", "operator@example.com")
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(load).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(body["view-version"], serde_json::json!(1));
    assert_eq!(body["schema-version"], serde_json::json!(1));

    let receipts = Request::builder()
        .method(Method::GET)
        .uri(
            "/management/v1/warehouses/local/namespaces/default/views/guarded_view/version-receipts",
        )
        .header("x-lakecat-principal", "operator@example.com")
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(receipts).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let receipts = body["receipts"].as_array().unwrap();
    assert_eq!(receipts.len(), 1);
    assert_eq!(receipts[0]["operation"], serde_json::json!("upsert"));
    assert_eq!(receipts[0]["view-version"], serde_json::json!(1));
}

#[tokio::test]
async fn stale_view_mutation_guards_do_not_emit_replay_events() {
    let store = MemoryCatalogStore::new();
    let app = app(LakeCatState::new(
        WarehouseName::new("local").unwrap(),
        store.clone(),
    ));
    let create = Request::builder()
        .method(Method::PUT)
        .uri("/management/v1/warehouses/local/namespaces/default/views/guarded_view")
        .header("content-type", "application/json")
        .header("x-lakecat-principal", "operator@example.com")
        .body(Body::from(
            serde_json::json!({
                "sql": "select id from customers",
                "dialect": "sql",
                "schema-version": 1
            })
            .to_string(),
        ))
        .unwrap();
    let response = app.clone().oneshot(create).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let guarded_update = Request::builder()
        .method(Method::PUT)
        .uri("/management/v1/warehouses/local/namespaces/default/views/guarded_view")
        .header("content-type", "application/json")
        .header("x-lakecat-principal", "operator@example.com")
        .body(Body::from(
            serde_json::json!({
                "sql": "select id, email from customers",
                "dialect": "sql",
                "schema-version": 2,
                "expected-view-version": 1
            })
            .to_string(),
        ))
        .unwrap();
    let response = app.clone().oneshot(guarded_update).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let pending_before = store
        .pending_outbox_events(Some("lakecat.lineage-and-graph"), 100)
        .await
        .unwrap();
    assert_eq!(pending_before.len(), 2);
    assert!(
        pending_before
            .iter()
            .all(|event| event.event_type == "view.upserted")
    );

    let stale_update = Request::builder()
        .method(Method::PUT)
        .uri("/management/v1/warehouses/local/namespaces/default/views/guarded_view")
        .header("content-type", "application/json")
        .header("x-lakecat-principal", "operator@example.com")
        .body(Body::from(
            serde_json::json!({
                "sql": "select email from customers",
                "dialect": "sql",
                "schema-version": 3,
                "expected-view-version": 1
            })
            .to_string(),
        ))
        .unwrap();
    let response = app.clone().oneshot(stale_update).await.unwrap();
    assert_eq!(response.status(), StatusCode::CONFLICT);

    let stale_catalog_drop = Request::builder()
        .method(Method::DELETE)
        .uri("/catalog/v1/local/namespaces/default/views/guarded_view?expected-view-version=1")
        .header("x-lakecat-principal", "operator@example.com")
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(stale_catalog_drop).await.unwrap();
    assert_eq!(response.status(), StatusCode::CONFLICT);

    let pending_after = store
        .pending_outbox_events(Some("lakecat.lineage-and-graph"), 100)
        .await
        .unwrap();
    assert_eq!(
        pending_after
            .iter()
            .map(|event| event.event_id.as_str())
            .collect::<Vec<_>>(),
        pending_before
            .iter()
            .map(|event| event.event_id.as_str())
            .collect::<Vec<_>>()
    );
    assert_eq!(
        pending_after
            .iter()
            .map(|event| event.event_type.as_str())
            .collect::<Vec<_>>(),
        vec!["view.upserted", "view.upserted"]
    );

    let receipts = Request::builder()
        .method(Method::GET)
        .uri(
            "/management/v1/warehouses/local/namespaces/default/views/guarded_view/version-receipts",
        )
        .header("x-lakecat-principal", "operator@example.com")
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(receipts).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let receipts = body["receipts"].as_array().unwrap();
    assert_eq!(receipts.len(), 2);
    assert_eq!(receipts[0]["view-version"], serde_json::json!(1));
    assert_eq!(receipts[1]["view-version"], serde_json::json!(2));
}
