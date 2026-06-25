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
async fn outbox_drain_rejects_catalog_config_missing_querygraph_integration_endpoints() {
    let principal = Principal::new("agent:reader", PrincipalKind::Agent).unwrap();
    let endpoints = CatalogConfigResponse::default()
        .endpoints
        .into_iter()
        .filter(|endpoint| endpoint != "GET /querygraph/v1/bootstrap")
        .collect::<Vec<_>>();
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-config-missing-querygraph-endpoint".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "catalog.config-read".to_string(),
            payload: json!({
                "audit-event-id": "audit-config-missing-querygraph-endpoint",
                "event-type": "catalog.config-read",
                "payload": {
                    "authorization-receipt": {
                        "principal": principal,
                        "action": "catalog-config",
                        "allowed": true,
                        "engine": "test",
                        "policy_hash": null,
                        "checked_at": chrono::Utc::now(),
                    },
                    "warehouse": "local",
                    "defaults": catalog_config_defaults_json(),
                    "endpoints": endpoints
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
        .expect_err("catalog config replay must preserve QueryGraph integration endpoints");

    let message = err.to_string();
    assert!(message.contains("catalog.config-read"));
    assert!(
        message.contains("catalog config-read endpoints must include GET /querygraph/v1/bootstrap"),
        "{message}"
    );
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains("evt-config-missing-querygraph-endpoint"));
    assert!(store.delivered.lock().await.is_empty());
    assert!(graph.events.lock().await.is_empty());
    assert!(lineage.events.lock().await.is_empty());
}

#[tokio::test]
async fn outbox_drain_rejects_malformed_querygraph_bootstrap_evidence() {
    let principal = Principal::new("agent:reader", PrincipalKind::Agent).unwrap();
    let bundle_hash = content_hash_json(&json!({"querygraph-bootstrap": "bundle"})).unwrap();
    let open_lineage_hash =
        content_hash_json(&json!({"querygraph-bootstrap": "open-lineage"})).unwrap();
    let import_hash =
        content_hash_json(&json!({"querygraph-bootstrap": "querygraph-import"})).unwrap();
    let table_hash = content_hash_json(&json!({"querygraph-bootstrap": "table"})).unwrap();
    let view_hash = content_hash_json(&json!({"querygraph-bootstrap": "view"})).unwrap();
    let receipt_hash = content_hash_json(&json!({"querygraph-bootstrap": "view-receipt"})).unwrap();
    let receipt_chain_hash =
        content_hash_json(&json!({"querygraph-bootstrap": "view-chain"})).unwrap();
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-secret-bootstrap-token".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "querygraph.bootstrap".to_string(),
            payload: json!({
                "audit-event-id": "audit-corrupt-bootstrap",
                "event-type": "querygraph.bootstrap",
                "payload": {
                    "authorization-receipt": {
                        "principal": principal,
                        "action": "graph-read",
                        "allowed": true,
                        "engine": "test",
                        "policy_hash": null,
                        "checked_at": chrono::Utc::now(),
                    },
                    "warehouse": "local",
                    "table-count": 1,
                    "view-count": 1,
                    "policy-binding-count": 1,
                    "verified-tables": ["local.default.events"],
                    "verified-views": ["lakecat:view:local:default:active_customers"],
                    "bundle-hash": bundle_hash,
                    "graph-hash": "sha256:graph",
                    "open-lineage-hash": open_lineage_hash,
                    "querygraph-import-hash": import_hash,
                    "table-artifacts": [{
                        "stable-id": "local.default.events",
                        "croissant-hash": table_hash,
                        "cdif-hash": table_hash,
                        "osi-hash": table_hash,
                        "odrl-hash": table_hash,
                        "policy-bindings-hash": table_hash
                    }],
                    "view-artifacts": [{
                        "stable-id": "lakecat:view:local:default:active_customers",
                        "osi-hash": view_hash
                    }],
                    "view-version-receipts": [{
                        "stable-id": "lakecat:view:local:default:active_customers",
                        "view-version": 1,
                        "receipt-hash": receipt_hash,
                        "receipt-chain-hash": receipt_chain_hash
                    }],
                    "standards": [
                        "Iceberg REST",
                        "Croissant",
                        "CDIF",
                        "OSI handoff",
                        "ODRL",
                        "Grust catalog graph",
                        "OpenLineage"
                    ]
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
        .expect_err("short QueryGraph bootstrap hash evidence should fail");

    let message = err.to_string();
    assert!(
        message
            .contains("outbox event querygraph.bootstrap (lakecat.lineage-and-graph) has invalid")
    );
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(message.contains("graph-hash must contain full SHA-256 digest evidence"));
    assert!(!message.contains("evt-secret-bootstrap-token"));
    assert!(
        store.delivered.lock().await.is_empty(),
        "malformed QueryGraph bootstrap evidence must fail before acknowledgement"
    );
    assert!(
        graph.events.lock().await.is_empty(),
        "malformed QueryGraph bootstrap evidence must fail before graph projection"
    );
    assert!(
        lineage.events.lock().await.is_empty(),
        "malformed QueryGraph bootstrap evidence must fail before lineage projection"
    );
}

#[tokio::test]
async fn outbox_drain_rejects_malformed_querygraph_bootstrap_receipt_principal() {
    let cases = vec![
        (
            "evt-bootstrap-missing-principal",
            serde_json::Value::Null,
            "querygraph bootstrap evidence must contain authorization receipt principal",
        ),
        (
            "evt-bootstrap-malformed-principal",
            json!({
                "subject": "agent:reader",
                "kind": "unknown"
            }),
            "querygraph bootstrap authorization receipt principal must be a valid principal",
        ),
    ];

    for (event_id, principal, expected_message) in cases {
        let mut payload = valid_querygraph_bootstrap_payload(
            Principal::new("agent:reader", PrincipalKind::Agent).unwrap(),
        );
        if principal.is_null() {
            payload["payload"]["authorization-receipt"]
                .as_object_mut()
                .expect("authorization receipt should be an object")
                .remove("principal");
        } else {
            payload["payload"]["authorization-receipt"]["principal"] = principal;
        }

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
            .expect_err("actorless QueryGraph bootstrap replay should fail");

        let message = err.to_string();
        assert!(message.contains("querygraph.bootstrap"));
        assert!(
            message.contains(expected_message),
            "QueryGraph bootstrap error should describe principal proof: {message}"
        );
        assert!(message.contains("event-id-hash=sha256:"));
        assert!(!message.contains(event_id));
        assert!(
            store.delivered.lock().await.is_empty(),
            "actorless QueryGraph bootstrap replay must fail before acknowledgement"
        );
        assert!(
            graph.events.lock().await.is_empty(),
            "actorless QueryGraph bootstrap replay must fail before graph projection"
        );
        assert!(
            lineage.events.lock().await.is_empty(),
            "actorless QueryGraph bootstrap replay must fail before lineage projection"
        );
    }
}

#[tokio::test]
async fn outbox_drain_rejects_mismatched_querygraph_bootstrap_receipt_action() {
    let principal = Principal::new("agent:reader", PrincipalKind::Agent).unwrap();
    let mut payload = valid_querygraph_bootstrap_payload(principal);
    payload["payload"]["authorization-receipt"]["action"] = json!("lineage-read");
    let event_id = "evt-bootstrap-mismatched-receipt-action";
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
        .expect_err("mismatched QueryGraph bootstrap receipt action should fail");

    let message = err.to_string();
    assert!(message.contains("querygraph.bootstrap"));
    assert!(message.contains(
        "querygraph bootstrap authorization receipt action does not match outbox event type"
    ));
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains(event_id));
    assert!(
        store.delivered.lock().await.is_empty(),
        "mismatched QueryGraph bootstrap action must fail before acknowledgement"
    );
    assert!(
        graph.events.lock().await.is_empty(),
        "mismatched QueryGraph bootstrap action must fail before graph projection"
    );
    assert!(
        lineage.events.lock().await.is_empty(),
        "mismatched QueryGraph bootstrap action must fail before lineage projection"
    );
}

#[tokio::test]
async fn outbox_drain_rejects_missing_or_denied_querygraph_bootstrap_allowed_decision() {
    let principal = Principal::new("agent:reader", PrincipalKind::Agent).unwrap();
    let mut missing_allowed_payload = valid_querygraph_bootstrap_payload(principal.clone());
    missing_allowed_payload["payload"]["authorization-receipt"]
        .as_object_mut()
        .expect("authorization receipt should be an object")
        .remove("allowed");
    let mut denied_payload = valid_querygraph_bootstrap_payload(principal);
    denied_payload["payload"]["authorization-receipt"]["allowed"] = json!(false);

    for (event_id, payload, expected_message) in [
        (
            "evt-bootstrap-missing-receipt-allowed",
            missing_allowed_payload,
            "querygraph bootstrap evidence must contain authorization receipt allowed decision",
        ),
        (
            "evt-bootstrap-denied-receipt",
            denied_payload,
            "querygraph bootstrap authorization receipt must allow replay projection",
        ),
    ] {
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
            .expect_err("missing or denied QueryGraph bootstrap decision should fail");

        let message = err.to_string();
        assert!(message.contains("querygraph.bootstrap"));
        assert!(message.contains(expected_message), "{message}");
        assert!(message.contains("event-id-hash=sha256:"));
        assert!(!message.contains(event_id));
        assert!(
            store.delivered.lock().await.is_empty(),
            "QueryGraph bootstrap decision failures must fail before acknowledgement"
        );
        assert!(
            graph.events.lock().await.is_empty(),
            "QueryGraph bootstrap decision failures must fail before graph projection"
        );
        assert!(
            lineage.events.lock().await.is_empty(),
            "QueryGraph bootstrap decision failures must fail before lineage projection"
        );
    }
}

#[tokio::test]
async fn outbox_drain_rejects_querygraph_bootstrap_table_artifact_manifest_drift() {
    let principal = Principal::new("agent:reader", PrincipalKind::Agent).unwrap();
    let mut payload = valid_querygraph_bootstrap_payload(principal);
    payload["payload"]["table-artifacts"][0]["stable-id"] = json!("local.default.forged_events");
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-bootstrap-table-artifact-drift".to_string(),
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
        .expect_err("QueryGraph bootstrap table artifacts must match verified table manifest");

    let message = err.to_string();
    assert!(message.contains("querygraph.bootstrap"));
    assert!(message.contains(
        "querygraph bootstrap table-artifacts stable-id set must match verified manifest"
    ));
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains("evt-bootstrap-table-artifact-drift"));
    assert!(store.delivered.lock().await.is_empty());
    assert!(graph.events.lock().await.is_empty());
    assert!(lineage.events.lock().await.is_empty());
}

#[tokio::test]
async fn outbox_drain_rejects_extra_querygraph_bootstrap_entry_fields() {
    let cases = vec![
        (
            "evt-bootstrap-extra-table-artifact",
            "/payload/table-artifacts/0",
            "unverified-table-claim",
            "querygraph bootstrap table-artifacts entry contains unexpected field unverified-table-claim",
        ),
        (
            "evt-bootstrap-extra-view-artifact",
            "/payload/view-artifacts/0",
            "unverified-view-claim",
            "querygraph bootstrap view-artifacts entry contains unexpected field unverified-view-claim",
        ),
        (
            "evt-bootstrap-extra-view-receipt",
            "/payload/view-version-receipts/0",
            "unverified-receipt-claim",
            "querygraph bootstrap view-version receipt contains unexpected field unverified-receipt-claim",
        ),
    ];

    for (event_id, pointer, field, expected_message) in cases {
        let principal = Principal::new("agent:reader", PrincipalKind::Agent).unwrap();
        let mut payload = valid_querygraph_bootstrap_payload(principal);
        payload
            .pointer_mut(pointer)
            .and_then(Value::as_object_mut)
            .expect("valid bootstrap payload should contain target entry")
            .insert(field.to_string(), json!(true));
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
            .expect_err("extra QueryGraph bootstrap entry field should fail");

        let message = err.to_string();
        assert!(message.contains("querygraph.bootstrap"));
        assert!(
            message.contains(expected_message),
            "{event_id} should reject extra bootstrap entry field: {message}"
        );
        assert!(message.contains("event-id-hash=sha256:"));
        assert!(!message.contains(event_id));
        assert!(
            store.delivered.lock().await.is_empty(),
            "{event_id} must fail before acknowledgement"
        );
        assert!(
            graph.events.lock().await.is_empty(),
            "{event_id} must fail before graph projection"
        );
        assert!(
            lineage.events.lock().await.is_empty(),
            "{event_id} must fail before lineage projection"
        );
    }
}

#[tokio::test]
async fn outbox_drain_rejects_extra_top_level_querygraph_bootstrap_fields() {
    let principal = Principal::new("agent:reader", PrincipalKind::Agent).unwrap();
    let mut payload = valid_querygraph_bootstrap_payload(principal);
    payload
        .pointer_mut("/payload")
        .and_then(Value::as_object_mut)
        .expect("valid bootstrap payload should contain payload object")
        .insert("unverified-bootstrap-claim".to_string(), json!(true));
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-bootstrap-extra-top-level".to_string(),
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
        .expect_err("QueryGraph bootstrap replay must reject extra top-level fields");

    let message = err.to_string();
    assert!(message.contains("querygraph.bootstrap"));
    assert!(
        message
            .contains("querygraph bootstrap contains unexpected field unverified-bootstrap-claim"),
        "extra QueryGraph bootstrap top-level field should be rejected: {message}"
    );
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains("evt-bootstrap-extra-top-level"));
    assert!(store.delivered.lock().await.is_empty());
    assert!(graph.events.lock().await.is_empty());
    assert!(lineage.events.lock().await.is_empty());
}

#[tokio::test]
async fn outbox_drain_rejects_duplicate_querygraph_verified_tables() {
    let principal = Principal::new("agent:reader", PrincipalKind::Agent).unwrap();
    let mut payload = valid_querygraph_bootstrap_payload(principal);
    payload["payload"]["table-count"] = json!(2);
    payload["payload"]["verified-tables"] = json!(["local.default.events", "local.default.events"]);
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-bootstrap-duplicate-verified-tables".to_string(),
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
        .expect_err("duplicate QueryGraph verified tables must fail");

    let message = err.to_string();
    assert!(message.contains("querygraph.bootstrap"));
    assert!(message.contains("querygraph bootstrap verified-tables must be duplicate-free"));
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains("evt-bootstrap-duplicate-verified-tables"));
    assert!(store.delivered.lock().await.is_empty());
    assert!(graph.events.lock().await.is_empty());
    assert!(lineage.events.lock().await.is_empty());
}
