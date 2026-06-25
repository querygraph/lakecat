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
async fn outbox_drain_rejects_extra_view_list_fields() {
    let principal = Principal::new("agent:operator", PrincipalKind::Agent).unwrap();
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-view-list-extra-field".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "view.listed".to_string(),
            payload: json!({
                "audit-event-id": "audit-view-list-extra-field",
                "event-type": "view.listed",
                "payload": {
                    "authorization-receipt": {
                        "principal": principal,
                        "action": "view-load",
                        "allowed": true,
                        "engine": "test",
                        "policy_hash": null,
                        "checked_at": chrono::Utc::now(),
                    },
                    "warehouse": "local",
                    "namespace": ["default"],
                    "view-count": 1,
                    "view-names": ["active_events"],
                    "unverified-view-list-claim": "shadow",
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
        .expect_err("extra view-list fields should fail before delivery");

    let message = err.to_string();
    assert!(message.contains("outbox event view.listed (lakecat.lineage-and-graph) has invalid"));
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(message.contains("view list contains unexpected field unverified-view-list-claim"));
    assert!(!message.contains("evt-view-list-extra-field"));
    assert!(
        store.delivered.lock().await.is_empty(),
        "extra view-list fields must fail before acknowledgement"
    );
    assert!(
        graph.events.lock().await.is_empty(),
        "extra view-list fields must fail before graph projection"
    );
    assert!(
        lineage.events.lock().await.is_empty(),
        "extra view-list fields must fail before lineage projection"
    );
}

#[tokio::test]
async fn outbox_drain_rejects_malformed_view_list_evidence() {
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-secret-view-list-token".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "view.listed".to_string(),
            payload: json!({
                "audit-event-id": "audit-corrupt-view-list",
                "event-type": "view.listed",
                "payload": {
                    "warehouse": "local",
                    "namespace": ["default"],
                    "view-count": "one",
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
        .expect_err("malformed view-list evidence should fail");

    let message = err.to_string();
    assert!(message.contains("outbox event view.listed (lakecat.lineage-and-graph) has invalid"));
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(message.contains("view list evidence must contain unsigned view-count"));
    assert!(!message.contains("evt-secret-view-list-token"));
    assert!(
        store.delivered.lock().await.is_empty(),
        "malformed view-list evidence must fail before acknowledgement"
    );
    assert!(
        graph.events.lock().await.is_empty(),
        "malformed view-list evidence must fail before graph projection"
    );
    assert!(
        lineage.events.lock().await.is_empty(),
        "malformed view-list evidence must fail before lineage projection"
    );
}

#[tokio::test]
async fn outbox_drain_rejects_malformed_view_list_name_evidence() {
    let principal = Principal::new("agent:reader", PrincipalKind::Agent).unwrap();
    let cases = vec![
        (
            "missing-names",
            json!({
                "authorization-receipt": {
                    "principal": principal,
                    "action": "view-load",
                    "allowed": true,
                    "engine": "test",
                    "policy_hash": null,
                    "checked_at": chrono::Utc::now(),
                },
                "warehouse": "local",
                "namespace": ["default"],
                "view-count": 1,
            }),
            "view list evidence must contain view-names",
        ),
        (
            "count-mismatch",
            json!({
                "authorization-receipt": {
                    "principal": principal,
                    "action": "view-load",
                    "allowed": true,
                    "engine": "test",
                    "policy_hash": null,
                    "checked_at": chrono::Utc::now(),
                },
                "warehouse": "local",
                "namespace": ["default"],
                "view-count": 2,
                "view-names": ["active_customers"],
            }),
            "view list view-names count must match view list count",
        ),
        (
            "invalid-name",
            json!({
                "authorization-receipt": {
                    "principal": principal,
                    "action": "view-load",
                    "allowed": true,
                    "engine": "test",
                    "policy_hash": null,
                    "checked_at": chrono::Utc::now(),
                },
                "warehouse": "local",
                "namespace": ["default"],
                "view-count": 1,
                "view-names": ["../secret"],
            }),
            "view list view-names contains an invalid view name",
        ),
        (
            "duplicate-name",
            json!({
                "authorization-receipt": {
                    "principal": principal,
                    "action": "view-load",
                    "allowed": true,
                    "engine": "test",
                    "policy_hash": null,
                    "checked_at": chrono::Utc::now(),
                },
                "warehouse": "local",
                "namespace": ["default"],
                "view-count": 2,
                "view-names": ["active_customers", "active_customers"],
            }),
            "view list view-names must not contain duplicate view names",
        ),
    ];

    for (label, payload, expected_message) in cases {
        let event_id = format!("evt-view-list-{label}");
        let store = Arc::new(RecordingOutboxStore {
            events: Mutex::new(vec![OutboxEvent {
                event_id: event_id.clone(),
                sink: "lakecat.lineage-and-graph".to_string(),
                event_type: "view.listed".to_string(),
                payload: json!({
                    "audit-event-id": format!("audit-view-list-{label}"),
                    "event-type": "view.listed",
                    "payload": payload,
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
            .expect_err("malformed view-list name evidence should fail");

        let message = err.to_string();
        assert!(message.contains("view.listed"));
        assert!(message.contains(expected_message), "{message}");
        assert!(message.contains("event-id-hash=sha256:"));
        assert!(!message.contains(&event_id));
        assert!(
            store.delivered.lock().await.is_empty(),
            "malformed view-list name evidence must fail before acknowledgement"
        );
        assert!(
            graph.events.lock().await.is_empty(),
            "malformed view-list name evidence must fail before graph projection"
        );
        assert!(
            lineage.events.lock().await.is_empty(),
            "malformed view-list name evidence must fail before lineage projection"
        );
    }
}

#[tokio::test]
async fn outbox_drain_rejects_view_list_manage_receipt_action() {
    let principal = Principal::new("agent:reader", PrincipalKind::Agent).unwrap();
    let event_id = "evt-view-list-manage-receipt-action";
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: event_id.to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "view.listed".to_string(),
            payload: json!({
                "audit-event-id": "audit-view-list-manage-receipt-action",
                "event-type": "view.listed",
                "payload": {
                    "authorization-receipt": {
                        "principal": principal,
                        "action": "view-manage",
                        "allowed": true,
                        "engine": "test",
                        "policy_hash": null,
                        "checked_at": chrono::Utc::now(),
                    },
                    "warehouse": "local",
                    "namespace": ["default"],
                    "view-count": 1,
                    "view-names": ["active_customers"],
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
        .expect_err("view.listed replay should require view-load receipt action");

    let message = err.to_string();
    assert!(message.contains("view.listed"));
    assert!(
        message.contains("view list authorization receipt action does not match outbox event type")
    );
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains(event_id));
    assert!(
        store.delivered.lock().await.is_empty(),
        "mismatched view-list receipt action must fail before acknowledgement"
    );
    assert!(
        graph.events.lock().await.is_empty(),
        "mismatched view-list receipt action must fail before graph projection"
    );
    assert!(
        lineage.events.lock().await.is_empty(),
        "mismatched view-list receipt action must fail before lineage projection"
    );
}

#[tokio::test]
async fn outbox_drain_rejects_mismatched_view_lifecycle_receipt_actions() {
    let principal = Principal::new("agent:reader", PrincipalKind::Agent).unwrap();
    let cases = vec![
        ("view.upserted", "view-load"),
        ("view.loaded", "view-manage"),
        ("view.dropped", "view-manage"),
    ];

    for (event_type, wrong_action) in cases {
        let event_id = format!("evt-mismatched-{event_type}-action-token");
        let store = Arc::new(RecordingOutboxStore {
            events: Mutex::new(vec![OutboxEvent {
                event_id: event_id.clone(),
                sink: "lakecat.lineage-and-graph".to_string(),
                event_type: event_type.to_string(),
                payload: json!({
                    "audit-event-id": format!("audit-mismatched-{event_type}-action"),
                    "event-type": event_type,
                    "payload": {
                        "authorization-receipt": {
                            "principal": principal,
                            "action": wrong_action,
                            "allowed": true,
                            "engine": "test",
                            "policy_hash": null,
                            "checked_at": chrono::Utc::now(),
                        },
                        "view": {
                            "warehouse": "local",
                            "namespace": ["default"],
                            "name": "active_customers",
                            "view-version": 1,
                        }
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
            .expect_err("view lifecycle replay should require matching receipt action");

        let message = err.to_string();
        assert!(message.contains(event_type));
        assert!(
            message.contains(
                "view lifecycle authorization receipt action does not match outbox event type"
            ),
            "{event_type} should reject mismatched receipt action: {message}"
        );
        assert!(message.contains("event-id-hash=sha256:"));
        assert!(!message.contains(&event_id));
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

#[tokio::test]
async fn outbox_drain_rejects_mismatched_view_lifecycle_scope_evidence() {
    let principal = Principal::new("agent:operator", PrincipalKind::Agent).unwrap();
    let cases = vec![
        (
            "warehouse",
            json!({
                "warehouse": "local",
                "namespace": ["default"],
                "view": {
                    "warehouse": "shadow",
                    "namespace": ["default"],
                    "name": "active_customers",
                    "view-version": 1,
                },
                "authorization-receipt": {
                    "principal": principal,
                    "action": "view-manage",
                    "allowed": true,
                    "engine": "test",
                    "policy_hash": null,
                    "checked_at": chrono::Utc::now(),
                }
            }),
            "view lifecycle view warehouse must match payload warehouse",
        ),
        (
            "namespace",
            json!({
                "warehouse": "local",
                "namespace": ["default"],
                "view": {
                    "warehouse": "local",
                    "namespace": ["shadow"],
                    "name": "active_customers",
                    "view-version": 1,
                },
                "authorization-receipt": {
                    "principal": principal,
                    "action": "view-manage",
                    "allowed": true,
                    "engine": "test",
                    "policy_hash": null,
                    "checked_at": chrono::Utc::now(),
                }
            }),
            "view lifecycle view namespace must match payload namespace",
        ),
    ];

    for (label, payload, expected_message) in cases {
        let event_id = format!("evt-view-lifecycle-mismatched-{label}");
        let store = Arc::new(RecordingOutboxStore {
            events: Mutex::new(vec![OutboxEvent {
                event_id: event_id.clone(),
                sink: "lakecat.lineage-and-graph".to_string(),
                event_type: "view.upserted".to_string(),
                payload: json!({
                    "audit-event-id": format!("audit-view-lifecycle-mismatched-{label}"),
                    "event-type": "view.upserted",
                    "payload": payload,
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
            .expect_err("mismatched view lifecycle scope evidence should fail");

        let message = err.to_string();
        assert!(message.contains("view.upserted"), "{message}");
        assert!(message.contains(expected_message), "{message}");
        assert!(message.contains("event-id-hash=sha256:"), "{message}");
        assert!(!message.contains(&event_id), "{message}");
        assert!(
            store.delivered.lock().await.is_empty(),
            "mismatched view lifecycle scope must fail before acknowledgement"
        );
        assert!(
            graph.events.lock().await.is_empty(),
            "mismatched view lifecycle scope must fail before graph projection"
        );
        assert!(
            lineage.events.lock().await.is_empty(),
            "mismatched view lifecycle scope must fail before lineage projection"
        );
    }
}

#[tokio::test]
async fn outbox_drain_rejects_malformed_view_lifecycle_evidence() {
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-secret-view-token".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "view.upserted".to_string(),
            payload: json!({
                "audit-event-id": "audit-corrupt-view",
                "event-type": "view.upserted",
                "payload": {
                    "view": {
                        "warehouse": "local",
                        "namespace": ["default", ""],
                        "name": "events_view",
                    }
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
        .expect_err("malformed view lifecycle evidence should fail");

    let message = err.to_string();
    assert!(message.contains("outbox event view.upserted (lakecat.lineage-and-graph) has invalid"));
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(message.contains("view lifecycle namespace components must be non-empty strings"));
    assert!(!message.contains("evt-secret-view-token"));
    assert!(
        store.delivered.lock().await.is_empty(),
        "malformed view lifecycle evidence must fail before acknowledgement"
    );
    assert!(
        graph.events.lock().await.is_empty(),
        "malformed view lifecycle evidence must fail before graph projection"
    );
    assert!(
        lineage.events.lock().await.is_empty(),
        "malformed view lifecycle evidence must fail before lineage projection"
    );
}

#[tokio::test]
async fn outbox_drain_rejects_extra_view_lifecycle_fields() {
    let principal = Principal::new("agent:operator", PrincipalKind::Agent).unwrap();
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-secret-view-extra-token".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "view.upserted".to_string(),
            payload: json!({
                "audit-event-id": "audit-view-extra",
                "event-type": "view.upserted",
                "payload": {
                    "warehouse": "local",
                    "namespace": ["default"],
                    "view": {
                        "warehouse": "local",
                        "namespace": ["default"],
                        "name": "events_view",
                        "sql": "select event_id from default.events",
                        "dialect": "spark-sql",
                        "schema-version": 1,
                        "view-version": 1,
                        "columns": [{
                            "name": "event_id",
                            "data-type": {"type": "long"},
                            "nullable": false,
                            "comment": null
                        }],
                        "properties": {},
                        "unverified-querygraph-claim": true
                    },
                    "authorization-receipt": {
                        "principal": principal,
                        "action": "view-manage",
                        "allowed": true,
                        "engine": "test",
                        "policy_hash": null,
                        "checked_at": chrono::Utc::now(),
                    }
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
        .expect_err("extra view lifecycle fields should fail");

    let message = err.to_string();
    assert!(message.contains("view.upserted"));
    assert!(
        message
            .contains("view lifecycle view contains unexpected field unverified-querygraph-claim"),
        "extra view lifecycle field should be rejected: {message}"
    );
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains("evt-secret-view-extra-token"));
    assert!(
        store.delivered.lock().await.is_empty(),
        "extra view lifecycle evidence must fail before acknowledgement"
    );
    assert!(
        graph.events.lock().await.is_empty(),
        "extra view lifecycle evidence must fail before graph projection"
    );
    assert!(
        lineage.events.lock().await.is_empty(),
        "extra view lifecycle evidence must fail before lineage projection"
    );
}

#[tokio::test]
async fn outbox_drain_rejects_extra_top_level_view_lifecycle_fields() {
    let principal = Principal::new("agent:operator", PrincipalKind::Agent).unwrap();
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-secret-view-top-extra-token".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "view.upserted".to_string(),
            payload: json!({
                "audit-event-id": "audit-view-top-extra",
                "event-type": "view.upserted",
                "payload": {
                    "warehouse": "local",
                    "namespace": ["default"],
                    "view": {
                        "warehouse": "local",
                        "namespace": ["default"],
                        "name": "events_view",
                        "sql": "select event_id from default.events",
                        "dialect": "spark-sql",
                        "schema-version": 1,
                        "view-version": 1,
                        "columns": [{
                            "name": "event_id",
                            "data-type": {"type": "long"},
                            "nullable": false,
                            "comment": null
                        }],
                        "properties": {}
                    },
                    "expected-view-version": null,
                    "authorization-receipt": {
                        "principal": principal,
                        "action": "view-manage",
                        "allowed": true,
                        "engine": "test",
                        "policy_hash": null,
                        "checked_at": chrono::Utc::now(),
                    },
                    "unverified-view-lifecycle-claim": "shadow"
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
        .expect_err("extra top-level view lifecycle fields should fail");

    let message = err.to_string();
    assert!(message.contains("view.upserted"));
    assert!(
        message
            .contains("view lifecycle contains unexpected field unverified-view-lifecycle-claim"),
        "extra top-level view lifecycle field should be rejected: {message}"
    );
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains("evt-secret-view-top-extra-token"));
    assert!(
        store.delivered.lock().await.is_empty(),
        "extra top-level view lifecycle evidence must fail before acknowledgement"
    );
    assert!(
        graph.events.lock().await.is_empty(),
        "extra top-level view lifecycle evidence must fail before graph projection"
    );
    assert!(
        lineage.events.lock().await.is_empty(),
        "extra top-level view lifecycle evidence must fail before lineage projection"
    );
}

#[tokio::test]
async fn outbox_drain_rejects_extra_view_lifecycle_wrapper_fields() {
    let principal = Principal::new("agent:operator", PrincipalKind::Agent).unwrap();
    let cases = vec![
        ("view.upserted", "view-manage", "upserted"),
        ("view.loaded", "view-load", "loaded"),
        ("view.dropped", "view-drop", "dropped"),
    ];

    for (event_type, action, label) in cases {
        let event_id = format!("evt-view-{label}-extra-wrapper-field");
        let mut payload = json!({
            "warehouse": "local",
            "namespace": ["default"],
            "view": {
                "warehouse": "local",
                "namespace": ["default"],
                "name": "events_view",
                "view-version": 1,
            },
            "authorization-receipt": {
                "principal": principal,
                "action": action,
                "allowed": true,
                "engine": "test",
                "policy_hash": null,
                "checked_at": chrono::Utc::now(),
            },
        });
        if event_type == "view.dropped" {
            payload["expected-view-version"] = json!(1);
        }
        let store = Arc::new(RecordingOutboxStore {
            events: Mutex::new(vec![OutboxEvent {
                event_id: event_id.clone(),
                sink: "lakecat.lineage-and-graph".to_string(),
                event_type: event_type.to_string(),
                payload: json!({
                    "audit-event-id": format!("audit-view-{label}-extra-wrapper-field"),
                    "event-type": event_type,
                    "payload": payload,
                    "unverified-view-lifecycle-wrapper-claim": "shadow",
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
            .expect_err("extra view lifecycle wrapper fields should fail");

        let message = err.to_string();
        assert!(message.contains(event_type), "{message}");
        assert!(message.contains("event-id-hash=sha256:"), "{message}");
        assert!(
            message.contains(
                "view lifecycle outbox payload contains unexpected field unverified-view-lifecycle-wrapper-claim"
            ),
            "{message}"
        );
        assert!(!message.contains(&event_id), "{message}");
        assert!(
            store.delivered.lock().await.is_empty(),
            "{event_type} extra wrapper fields must fail before acknowledgement"
        );
        assert!(
            graph.events.lock().await.is_empty(),
            "{event_type} extra wrapper fields must fail before graph projection"
        );
        assert!(
            lineage.events.lock().await.is_empty(),
            "{event_type} extra wrapper fields must fail before lineage projection"
        );
    }
}

#[tokio::test]
async fn outbox_drain_rejects_malformed_view_lifecycle_version_evidence() {
    let principal = Principal::new("agent:operator", PrincipalKind::Agent).unwrap();
    let cases = vec![
        (
            "view.upserted",
            json!({
                "warehouse": "local",
                "namespace": ["default"],
                "view": {
                    "warehouse": "local",
                    "namespace": ["default"],
                    "name": "active_customers",
                },
                "authorization-receipt": {
                    "principal": principal,
                    "action": "view-manage",
                    "allowed": true,
                    "engine": "test",
                    "policy_hash": null,
                    "checked_at": chrono::Utc::now(),
                }
            }),
            "view lifecycle evidence must contain positive view-version",
        ),
        (
            "view.loaded",
            json!({
                "warehouse": "local",
                "namespace": ["default"],
                "view": {
                    "warehouse": "local",
                    "namespace": ["default"],
                    "name": "active_customers",
                    "view-version": 0,
                },
                "authorization-receipt": {
                    "principal": principal,
                    "action": "view-load",
                    "allowed": true,
                    "engine": "test",
                    "policy_hash": null,
                    "checked_at": chrono::Utc::now(),
                }
            }),
            "view lifecycle evidence must contain positive view-version",
        ),
        (
            "view.dropped",
            json!({
                "warehouse": "local",
                "namespace": ["default"],
                "view": {
                    "warehouse": "local",
                    "namespace": ["default"],
                    "name": "active_customers",
                    "view-version": 1,
                },
                "expected-view-version": 0,
                "authorization-receipt": {
                    "principal": principal,
                    "action": "view-drop",
                    "allowed": true,
                    "engine": "test",
                    "policy_hash": null,
                    "checked_at": chrono::Utc::now(),
                }
            }),
            "view lifecycle expected-view-version must be positive when present",
        ),
        (
            "view.upserted",
            json!({
                "warehouse": "local",
                "namespace": ["default"],
                "view": {
                    "warehouse": "local",
                    "namespace": ["default"],
                    "name": "bad name",
                    "view-version": 1,
                },
                "authorization-receipt": {
                    "principal": principal,
                    "action": "view-manage",
                    "allowed": true,
                    "engine": "test",
                    "policy_hash": null,
                    "checked_at": chrono::Utc::now(),
                }
            }),
            "view lifecycle evidence has invalid view name",
        ),
    ];

    for (event_type, payload, expected_message) in cases {
        let event_id = format!("evt-secret-{event_type}-version-token");
        let store = Arc::new(RecordingOutboxStore {
            events: Mutex::new(vec![OutboxEvent {
                event_id: event_id.clone(),
                sink: "lakecat.lineage-and-graph".to_string(),
                event_type: event_type.to_string(),
                payload: json!({
                    "audit-event-id": format!("audit-malformed-{event_type}-version"),
                    "event-type": event_type,
                    "payload": payload,
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
            .expect_err("malformed view lifecycle version evidence should fail");

        let message = err.to_string();
        assert!(message.contains(event_type));
        assert!(
            message.contains(expected_message),
            "{event_type} error should describe malformed version evidence: {message}"
        );
        assert!(message.contains("event-id-hash=sha256:"));
        assert!(!message.contains(&event_id));
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

#[tokio::test]
async fn outbox_drain_rejects_querygraph_bootstrap_view_receipt_manifest_drift() {
    let principal = Principal::new("agent:reader", PrincipalKind::Agent).unwrap();
    let mut payload = valid_querygraph_bootstrap_payload(principal);
    payload["payload"]["view-version-receipts"][0]["stable-id"] =
        json!("lakecat:view:local:default:forged_customers");
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-bootstrap-view-receipt-drift".to_string(),
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
        .expect_err("QueryGraph bootstrap view receipts must match verified view manifest");

    let message = err.to_string();
    assert!(message.contains("querygraph.bootstrap"));
    assert!(message.contains(
        "querygraph bootstrap view-version-receipts stable-id set must match verified manifest"
    ));
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains("evt-bootstrap-view-receipt-drift"));
    assert!(store.delivered.lock().await.is_empty());
    assert!(graph.events.lock().await.is_empty());
    assert!(lineage.events.lock().await.is_empty());
}

#[tokio::test]
async fn outbox_drain_rejects_duplicate_querygraph_verified_views() {
    let principal = Principal::new("agent:reader", PrincipalKind::Agent).unwrap();
    let mut payload = valid_querygraph_bootstrap_payload(principal);
    payload["payload"]["view-count"] = json!(2);
    payload["payload"]["verified-views"] = json!([
        "lakecat:view:local:default:active_customers",
        "lakecat:view:local:default:active_customers"
    ]);
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-bootstrap-duplicate-verified-views".to_string(),
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
        .expect_err("duplicate QueryGraph verified views must fail");

    let message = err.to_string();
    assert!(message.contains("querygraph.bootstrap"));
    assert!(message.contains("querygraph bootstrap verified-views must be duplicate-free"));
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains("evt-bootstrap-duplicate-verified-views"));
    assert!(store.delivered.lock().await.is_empty());
    assert!(graph.events.lock().await.is_empty());
    assert!(lineage.events.lock().await.is_empty());
}

#[tokio::test]
async fn outbox_drain_projects_view_events_to_graph_and_lineage() {
    let principal = Principal::new("agent:operator", PrincipalKind::Agent).unwrap();
    let view_payload = json!({
        "warehouse": "local",
        "namespace": ["default"],
        "view": {
            "warehouse": "local",
            "namespace": ["default"],
            "name": "events_view",
            "sql": "select event_id from default.events",
            "dialect": "spark-sql",
            "schema-version": 1,
            "view-version": 1,
            "columns": [{
                "name": "event_id",
                "data-type": {"type": "long"},
                "nullable": false,
                "comment": null
            }],
            "properties": {}
        },
        "authorization-receipt": {
            "principal": principal,
            "action": "view-manage",
            "allowed": true,
            "engine": "test",
            "policy_hash": null,
            "checked_at": chrono::Utc::now(),
        }
    });
    let mut guarded_view_payload = view_payload.clone();
    guarded_view_payload["expected-view-version"] = json!(1);
    let mut view_load_payload = view_payload.clone();
    view_load_payload["authorization-receipt"]["action"] = json!("view-load");
    let mut view_drop_payload = guarded_view_payload.clone();
    view_drop_payload["authorization-receipt"]["action"] = json!("view-drop");
    let view_upsert_hash = content_hash_json(&json!({
        "view": "events_view",
        "version": 1,
        "operation": "upsert"
    }))
    .unwrap();
    let view_drop_hash = content_hash_json(&json!({
        "view": "events_view",
        "version": 1,
        "operation": "drop"
    }))
    .unwrap();
    let view_upsert_receipt_hash = content_hash_json(&json!({
        "stable-id": "lakecat:view:local:default:events_view",
        "view-version": 1,
        "operation": "upsert",
        "view-hash": view_upsert_hash
    }))
    .unwrap();
    let view_drop_receipt_hash = content_hash_json(&json!({
        "stable-id": "lakecat:view:local:default:events_view",
        "view-version": 1,
        "previous-view-version": 1,
        "previous-receipt-hash": view_upsert_receipt_hash,
        "operation": "drop",
        "view-hash": view_drop_hash
    }))
    .unwrap();
    let view_receipt_chain_hash = content_hash_json(&json!({
        "stable-id": "lakecat:view:local:default:events_view",
        "warehouse": "local",
        "namespace": ["default"],
        "name": "events_view",
        "latest-view-version": 1,
        "latest-operation": "drop",
        "tombstoned": true,
        "receipt-hashes": [view_upsert_receipt_hash, view_drop_receipt_hash],
    }))
    .unwrap();
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![
            OutboxEvent {
                event_id: "evt-view-list".to_string(),
                sink: "lakecat.lineage-and-graph".to_string(),
                event_type: "view.listed".to_string(),
                payload: json!({
                    "audit-event-id": "audit-view-list",
                    "event-type": "view.listed",
                    "payload": {
                        "warehouse": "local",
                        "namespace": ["default"],
                        "view-count": 1,
                        "view-names": ["events_view"],
                        "authorization-receipt": {
                            "principal": principal,
                            "action": "view-load",
                            "allowed": true,
                            "engine": "test",
                            "policy_hash": null,
                            "checked_at": chrono::Utc::now(),
                        }
                    },
                }),
                created_at: chrono::Utc::now(),
                delivered_at: None,
            },
            OutboxEvent {
                event_id: "evt-view-upsert".to_string(),
                sink: "lakecat.lineage-and-graph".to_string(),
                event_type: "view.upserted".to_string(),
                payload: json!({
                    "audit-event-id": "audit-view-upsert",
                    "event-type": "view.upserted",
                    "payload": guarded_view_payload.clone(),
                }),
                created_at: chrono::Utc::now(),
                delivered_at: None,
            },
            OutboxEvent {
                event_id: "evt-view-load".to_string(),
                sink: "lakecat.lineage-and-graph".to_string(),
                event_type: "view.loaded".to_string(),
                payload: json!({
                    "audit-event-id": "audit-view-load",
                    "event-type": "view.loaded",
                    "payload": view_load_payload,
                }),
                created_at: chrono::Utc::now(),
                delivered_at: None,
            },
            OutboxEvent {
                event_id: "evt-view-drop".to_string(),
                sink: "lakecat.lineage-and-graph".to_string(),
                event_type: "view.dropped".to_string(),
                payload: json!({
                    "audit-event-id": "audit-view-drop",
                    "event-type": "view.dropped",
                    "payload": view_drop_payload,
                }),
                created_at: chrono::Utc::now(),
                delivered_at: None,
            },
            OutboxEvent {
                event_id: "evt-view-receipts".to_string(),
                sink: "lakecat.lineage-and-graph".to_string(),
                event_type: "view.version-receipts-listed".to_string(),
                payload: json!({
                    "audit-event-id": "audit-view-receipts",
                    "event-type": "view.version-receipts-listed",
                    "payload": {
                        "warehouse": "local",
                        "namespace": ["default"],
                        "view": "events_view",
                        "receipt-count": 2,
                        "receipt-hashes": [&view_upsert_receipt_hash, &view_drop_receipt_hash],
                        "drop-receipt-hashes": [&view_drop_receipt_hash],
                        "authorization-receipt": {
                            "principal": principal,
                            "action": "view-load",
                            "allowed": true,
                            "engine": "test",
                            "policy_hash": null,
                            "checked_at": chrono::Utc::now(),
                        }
                    },
                }),
                created_at: chrono::Utc::now(),
                delivered_at: None,
            },
            OutboxEvent {
                event_id: "evt-view-chains".to_string(),
                sink: "lakecat.lineage-and-graph".to_string(),
                event_type: "view.version-receipt-chains-listed".to_string(),
                payload: json!({
                    "audit-event-id": "audit-view-chains",
                    "event-type": "view.version-receipt-chains-listed",
                    "payload": {
                        "warehouse": "local",
                        "namespace": ["default"],
                        "chain-count": 1,
                        "receipt-count": 2,
                        "tombstone-count": 1,
                        "chain-verified-count": 1,
                        "view-version-receipt-chains": [{
                            "stable-id": "lakecat:view:local:default:events_view",
                            "warehouse": "local",
                            "namespace": ["default"],
                            "name": "events_view",
                            "chain-hash": view_receipt_chain_hash,
                            "chain-verified": true,
                            "latest-view-version": 1,
                            "latest-operation": "drop",
                            "tombstoned": true,
                            "receipt-count": 2,
                            "receipts": [
                                {
                                    "stable-id": "lakecat:view:local:default:events_view",
                                    "warehouse": "local",
                                    "namespace": ["default"],
                                    "name": "events_view",
                                    "view-version": 1,
                                    "previous-view-version": null,
                                    "operation": "upsert",
                                    "view-hash": view_upsert_hash,
                                    "receipt-hash": view_upsert_receipt_hash,
                                    "principal-subject": "agent:operator",
                                    "principal-kind": "agent",
                                    "recorded-at": "2026-06-20T00:00:00Z"
                                },
                                {
                                    "stable-id": "lakecat:view:local:default:events_view",
                                    "warehouse": "local",
                                    "namespace": ["default"],
                                    "name": "events_view",
                                    "view-version": 1,
                                    "previous-view-version": 1,
                                    "previous-receipt-hash": view_upsert_receipt_hash,
                                    "operation": "drop",
                                    "view-hash": view_drop_hash,
                                    "receipt-hash": view_drop_receipt_hash,
                                    "principal-subject": "agent:operator",
                                    "principal-kind": "agent",
                                    "recorded-at": "2026-06-20T00:00:01Z"
                                }
                            ]
                        }],
                        "chain-hashes": [view_receipt_chain_hash],
                        "receipt-hashes": [&view_upsert_receipt_hash, &view_drop_receipt_hash],
                        "drop-receipt-hashes": [&view_drop_receipt_hash],
                        "authorization-receipt": {
                            "principal": principal,
                            "action": "view-load",
                            "allowed": true,
                            "engine": "test",
                            "policy_hash": null,
                            "checked_at": chrono::Utc::now(),
                        }
                    },
                }),
                created_at: chrono::Utc::now(),
                delivered_at: None,
            },
        ]),
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

    let drain = drain_outbox_once(&state, 10).await.unwrap();
    assert_eq!(drain.delivered, 6);
    assert_eq!(
        drain.event_types,
        vec![
            "view.listed".to_string(),
            "view.upserted".to_string(),
            "view.loaded".to_string(),
            "view.dropped".to_string(),
            "view.version-receipts-listed".to_string(),
            "view.version-receipt-chains-listed".to_string()
        ]
    );
    assert_eq!(drain.graph_events, 10);
    assert_eq!(drain.lineage_events, 6);
    assert_eq!(
        store.delivered.lock().await.as_slice(),
        &[
            "evt-view-list".to_string(),
            "evt-view-upsert".to_string(),
            "evt-view-load".to_string(),
            "evt-view-drop".to_string(),
            "evt-view-receipts".to_string(),
            "evt-view-chains".to_string()
        ]
    );
    assert_eq!(drain.events.len(), 6);
    assert_eq!(
        drain.events[1].view_stable_id.as_deref(),
        Some("lakecat:view:local:default:events_view")
    );
    assert_eq!(drain.events[1].view_warehouse.as_deref(), Some("local"));
    assert_eq!(drain.events[1].view_namespace, vec!["default"]);
    assert_eq!(drain.events[1].view_name.as_deref(), Some("events_view"));
    assert_eq!(drain.events[1].view_version, Some(1));
    assert_eq!(drain.events[1].expected_view_version, Some(1));
    assert_eq!(drain.events[2].expected_view_version, None);
    assert_eq!(drain.events[3].expected_view_version, Some(1));
    assert_eq!(
        drain.events[4].view_stable_id.as_deref(),
        Some("lakecat:view:local:default:events_view")
    );
    assert_eq!(
        drain.events[4].view_version_receipt_hashes,
        vec![
            view_upsert_receipt_hash.clone(),
            view_drop_receipt_hash.clone()
        ]
    );
    assert_eq!(
        drain.events[5].view_version_receipt_hashes,
        vec![
            view_upsert_receipt_hash.clone(),
            view_drop_receipt_hash.clone()
        ]
    );
    assert_eq!(
        drain.events[5].view_version_receipt_chain_hashes,
        vec![view_receipt_chain_hash.clone()]
    );
    assert_eq!(drain.events[5].view_version_receipt_chain_verified_count, 1);

    let graph_events = graph.events.lock().await;
    assert_eq!(graph_events.len(), 10);
    let view_events = graph_events
        .iter()
        .filter(|event| event.label == GraphNodeLabel::View)
        .collect::<Vec<_>>();
    assert_eq!(view_events.len(), 3);
    assert_eq!(view_events[0].action, GraphAction::Upserted);
    assert_eq!(
        view_events[0].subject,
        "lakecat:warehouse:local:namespace:default:view:events_view"
    );
    assert_eq!(view_events[1].action, GraphAction::Loaded);
    assert_eq!(view_events[2].action, GraphAction::Deleted);
    assert!(graph_events.iter().any(|event| {
        event.label == GraphNodeLabel::Namespace
            && event.action == GraphAction::Loaded
            && event.event_id.as_deref() == Some("evt-view-list")
    }));
    drop(graph_events);

    let lineage_events = lineage.events.lock().await;
    assert_eq!(lineage_events.len(), 6);
    assert_eq!(lineage_events[0].event_type, LineageEventType::ViewListed);
    assert_eq!(
        lineage_events[0].payload["view-names"],
        serde_json::json!(["events_view"])
    );
    assert_eq!(lineage_events[1].event_type, LineageEventType::ViewUpserted);
    assert_eq!(lineage_events[2].event_type, LineageEventType::ViewLoaded);
    assert_eq!(lineage_events[3].event_type, LineageEventType::ViewDropped);
    assert_eq!(
        lineage_events[4].event_type,
        LineageEventType::ViewVersionReceiptsListed
    );
    assert_eq!(
        lineage_events[5].event_type,
        LineageEventType::ViewVersionReceiptChainsListed
    );
    assert_eq!(
        lineage_events[1].payload["view"]["name"],
        serde_json::json!("events_view")
    );
}

#[tokio::test]
async fn outbox_drain_rejects_malformed_view_receipt_chain_evidence() {
    let principal = Principal::new("agent:operator", PrincipalKind::Agent).unwrap();
    let receipt_hash_1 = content_hash_json(&json!({
        "stable-id": "lakecat:view:local:default:events_view",
        "view-version": 1,
        "operation": "upsert"
    }))
    .unwrap();
    let receipt_hash_2 = content_hash_json(&json!({
        "stable-id": "lakecat:view:local:default:events_view",
        "view-version": 2,
        "operation": "upsert"
    }))
    .unwrap();
    let wrong_previous_hash = content_hash_json(&json!({
        "stable-id": "lakecat:view:local:default:other_view",
        "view-version": 1,
        "operation": "upsert"
    }))
    .unwrap();
    let chain_hash = view_version_receipt_chain_hash(&[
        test_view_receipt(1, None, None, "upsert", &receipt_hash_1),
        test_view_receipt(2, Some(1), Some(&receipt_hash_1), "upsert", &receipt_hash_2),
    ])
    .unwrap();
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-view-chain-malformed".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "view.version-receipt-chains-listed".to_string(),
            payload: json!({
                "audit-event-id": "audit-view-chain-malformed",
                "event-type": "view.version-receipt-chains-listed",
                "payload": {
                    "warehouse": "local",
                    "namespace": ["default"],
                    "chain-count": 1,
                    "receipt-count": 2,
                    "tombstone-count": 0,
                    "chain-verified-count": 1,
                    "view-version-receipt-chains": [{
                        "stable-id": "lakecat:view:local:default:events_view",
                        "warehouse": "local",
                        "namespace": ["default"],
                        "name": "events_view",
                        "chain-hash": chain_hash,
                        "chain-verified": true,
                        "latest-view-version": 2,
                        "latest-operation": "upsert",
                        "tombstoned": false,
                        "receipt-count": 2,
                        "receipts": [
                            {
                                "stable-id": "lakecat:view:local:default:events_view",
                                "warehouse": "local",
                                "namespace": ["default"],
                                "name": "events_view",
                                "view-version": 1,
                                "previous-view-version": null,
                                "previous-receipt-hash": null,
                                "operation": "upsert",
                                "view-hash": content_hash_json(&json!({"view": "events_view", "version": 1})).unwrap(),
                                "receipt-hash": receipt_hash_1,
                                "principal-subject": "agent:operator",
                                "principal-kind": "agent",
                                "recorded-at": "2026-06-20T00:00:00Z"
                            },
                            {
                                "stable-id": "lakecat:view:local:default:events_view",
                                "warehouse": "local",
                                "namespace": ["default"],
                                "name": "events_view",
                                "view-version": 2,
                                "previous-view-version": 1,
                                "previous-receipt-hash": wrong_previous_hash,
                                "operation": "upsert",
                                "view-hash": content_hash_json(&json!({"view": "events_view", "version": 2})).unwrap(),
                                "receipt-hash": receipt_hash_2,
                                "principal-subject": "agent:operator",
                                "principal-kind": "agent",
                                "recorded-at": "2026-06-20T00:00:01Z"
                            }
                        ]
                    }],
                    "chain-hashes": [chain_hash],
                    "receipt-hashes": [&receipt_hash_1, &receipt_hash_2],
                    "drop-receipt-hashes": [],
                    "authorization-receipt": {
                        "principal": principal,
                        "action": "view-load",
                        "allowed": true,
                        "engine": "test",
                        "policy_hash": null,
                        "checked_at": chrono::Utc::now(),
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

    let err = drain_outbox_once(&state, 10).await.unwrap_err();
    assert!(
        err.to_string()
            .contains("previous links must match the prior receipt"),
        "{err}"
    );
    assert!(store.delivered.lock().await.is_empty());
    assert!(graph.events.lock().await.is_empty());
    assert!(lineage.events.lock().await.is_empty());
}

#[tokio::test]
async fn outbox_drain_rejects_duplicate_view_receipt_chain_hash_arrays() {
    let principal = Principal::new("agent:operator", PrincipalKind::Agent).unwrap();
    let receipt_hash = content_hash_json(&json!({
        "stable-id": "lakecat:view:local:default:events_view",
        "view-version": 1,
        "operation": "upsert"
    }))
    .unwrap();
    let chain_hash = view_version_receipt_chain_hash(&[test_view_receipt(
        1,
        None,
        None,
        "upsert",
        &receipt_hash,
    )])
    .unwrap();
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-view-chain-duplicate-hash-array".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "view.version-receipt-chains-listed".to_string(),
            payload: json!({
                "audit-event-id": "audit-view-chain-duplicate-hash-array",
                "event-type": "view.version-receipt-chains-listed",
                "payload": {
                    "warehouse": "local",
                    "namespace": ["default"],
                    "chain-count": 1,
                    "receipt-count": 1,
                    "tombstone-count": 0,
                    "chain-verified-count": 1,
                    "view-version-receipt-chains": [{
                        "stable-id": "lakecat:view:local:default:events_view",
                        "warehouse": "local",
                        "namespace": ["default"],
                        "name": "events_view",
                        "chain-hash": chain_hash,
                        "chain-verified": true,
                        "latest-view-version": 1,
                        "latest-operation": "upsert",
                        "tombstoned": false,
                        "receipt-count": 1,
                        "receipts": [{
                            "stable-id": "lakecat:view:local:default:events_view",
                            "warehouse": "local",
                            "namespace": ["default"],
                            "name": "events_view",
                            "view-version": 1,
                            "previous-view-version": null,
                            "previous-receipt-hash": null,
                            "operation": "upsert",
                            "view-hash": content_hash_json(&json!({"view": "events_view", "version": 1})).unwrap(),
                            "receipt-hash": receipt_hash,
                            "principal-subject": "agent:operator",
                            "principal-kind": "agent",
                            "recorded-at": "2026-06-20T00:00:00Z"
                        }]
                    }],
                    "chain-hashes": [chain_hash, chain_hash],
                    "receipt-hashes": [receipt_hash],
                    "drop-receipt-hashes": [],
                    "authorization-receipt": {
                        "principal": principal,
                        "action": "view-load",
                        "allowed": true,
                        "engine": "test",
                        "policy_hash": null,
                        "checked_at": chrono::Utc::now(),
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
        .expect_err("duplicate receipt-chain hash arrays should fail before projection");

    let message = err.to_string();
    assert!(message.contains("view.version-receipt-chains-listed"));
    assert!(message.contains("view receipt-chain chain-hashes must not contain duplicate hashes"));
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains("evt-view-chain-duplicate-hash-array"));
    assert!(store.delivered.lock().await.is_empty());
    assert!(graph.events.lock().await.is_empty());
    assert!(lineage.events.lock().await.is_empty());
}

#[tokio::test]
async fn outbox_drain_rejects_malformed_view_receipt_chain_scope_and_counts() {
    let principal = Principal::new("agent:operator", PrincipalKind::Agent).unwrap();
    let receipt_hash = content_hash_json(&json!({
        "stable-id": "lakecat:view:local:default:events_view",
        "view-version": 1,
        "operation": "upsert"
    }))
    .unwrap();
    let chain_hash = view_version_receipt_chain_hash(&[test_view_receipt(
        1,
        None,
        None,
        "upsert",
        &receipt_hash,
    )])
    .unwrap();
    let valid_chain = json!({
        "stable-id": "lakecat:view:local:default:events_view",
        "warehouse": "local",
        "namespace": ["default"],
        "name": "events_view",
        "chain-hash": &chain_hash,
        "chain-verified": true,
        "latest-view-version": 1,
        "latest-operation": "upsert",
        "tombstoned": false,
        "receipt-count": 1,
        "receipts": [{
            "stable-id": "lakecat:view:local:default:events_view",
            "warehouse": "local",
            "namespace": ["default"],
            "name": "events_view",
            "view-version": 1,
            "previous-view-version": null,
            "previous-receipt-hash": null,
            "operation": "upsert",
            "view-hash": content_hash_json(&json!({"view": "events_view", "version": 1})).unwrap(),
            "receipt-hash": &receipt_hash,
            "principal-subject": "agent:operator",
            "principal-kind": "agent",
            "recorded-at": "2026-06-20T00:00:00Z"
        }]
    });
    let base_payload = json!({
        "warehouse": "local",
        "namespace": ["default"],
        "chain-count": 1,
        "receipt-count": 1,
        "tombstone-count": 0,
        "chain-verified-count": 1,
        "view-version-receipt-chains": [valid_chain],
        "chain-hashes": [&chain_hash],
        "receipt-hashes": [&receipt_hash],
        "drop-receipt-hashes": [],
        "authorization-receipt": {
            "principal": &principal,
            "action": "view-load",
            "allowed": true,
            "engine": "test",
            "policy_hash": null,
            "checked_at": chrono::Utc::now(),
        },
    });
    let mut cases = Vec::new();

    let mut missing_namespace = base_payload.clone();
    missing_namespace
        .as_object_mut()
        .unwrap()
        .remove("namespace");
    cases.push((
        "evt-view-chain-missing-namespace",
        "view receipt-chain evidence must contain namespace",
        missing_namespace,
    ));

    let mut missing_principal = base_payload.clone();
    missing_principal["authorization-receipt"]
        .as_object_mut()
        .unwrap()
        .remove("principal");
    cases.push((
        "evt-view-chain-missing-principal",
        "view receipt-chain evidence must contain authorization receipt principal",
        missing_principal,
    ));

    let mut chain_warehouse_drift = base_payload.clone();
    chain_warehouse_drift["view-version-receipt-chains"][0]["warehouse"] = json!("shadow");
    cases.push((
        "evt-view-chain-warehouse-drift",
        "view receipt-chain chain warehouse must match payload warehouse",
        chain_warehouse_drift,
    ));

    let mut chain_namespace_drift = base_payload.clone();
    chain_namespace_drift["view-version-receipt-chains"][0]["namespace"] = json!(["shadow"]);
    cases.push((
        "evt-view-chain-namespace-drift",
        "view receipt-chain chain namespace must match payload namespace",
        chain_namespace_drift,
    ));

    let mut chain_name_drift = base_payload.clone();
    chain_name_drift["view-version-receipt-chains"][0]["name"] = json!("other_view");
    cases.push((
        "evt-view-chain-name-drift",
        "view receipt-chain chain stable-id must match warehouse, namespace, and name",
        chain_name_drift,
    ));

    let mut chain_stable_id_drift = base_payload.clone();
    chain_stable_id_drift["view-version-receipt-chains"][0]["stable-id"] =
        json!("lakecat:view:local:default:other_view");
    cases.push((
        "evt-view-chain-stable-id-drift",
        "view receipt-chain chain stable-id must match warehouse, namespace, and name",
        chain_stable_id_drift,
    ));

    let mut receipt_warehouse_drift = base_payload.clone();
    receipt_warehouse_drift["view-version-receipt-chains"][0]["receipts"][0]["warehouse"] =
        json!("shadow");
    cases.push((
        "evt-view-chain-receipt-warehouse-drift",
        "view receipt-chain receipt warehouse must match payload warehouse",
        receipt_warehouse_drift,
    ));

    let mut receipt_namespace_drift = base_payload.clone();
    receipt_namespace_drift["view-version-receipt-chains"][0]["receipts"][0]["namespace"] =
        json!(["shadow"]);
    cases.push((
        "evt-view-chain-receipt-namespace-drift",
        "view receipt-chain receipt namespace must match payload namespace",
        receipt_namespace_drift,
    ));

    let mut receipt_name_drift = base_payload.clone();
    receipt_name_drift["view-version-receipt-chains"][0]["receipts"][0]["name"] =
        json!("other_view");
    cases.push((
        "evt-view-chain-receipt-name-drift",
        "view receipt-chain receipt stable-id must match warehouse, namespace, and name",
        receipt_name_drift,
    ));

    let mut receipt_stable_id_drift = base_payload.clone();
    receipt_stable_id_drift["view-version-receipt-chains"][0]["receipts"][0]["stable-id"] =
        json!("lakecat:view:local:default:other_view");
    cases.push((
        "evt-view-chain-receipt-stable-id-drift",
        "view receipt-chain receipt stable-id must match warehouse, namespace, and name",
        receipt_stable_id_drift,
    ));

    let mut chain_count_drift = base_payload.clone();
    chain_count_drift["chain-count"] = json!(2);
    cases.push((
        "evt-view-chain-count-drift",
        "view receipt-chain chain-count does not match chains",
        chain_count_drift,
    ));

    let mut receipt_count_drift = base_payload.clone();
    receipt_count_drift["receipt-count"] = json!(2);
    cases.push((
        "evt-view-chain-receipt-count-drift",
        "view receipt-chain receipt-count does not match chains",
        receipt_count_drift,
    ));

    let mut tombstone_count_drift = base_payload.clone();
    tombstone_count_drift["tombstone-count"] = json!(1);
    cases.push((
        "evt-view-chain-tombstone-count-drift",
        "view receipt-chain tombstone-count does not match chains",
        tombstone_count_drift,
    ));

    let mut unverified_chain = base_payload.clone();
    unverified_chain["view-version-receipt-chains"][0]["chain-verified"] = json!(false);
    cases.push((
        "evt-view-chain-unverified",
        "view receipt-chain chain must be structurally verified",
        unverified_chain,
    ));

    let mut chain_receipt_count_missing = base_payload.clone();
    chain_receipt_count_missing["view-version-receipt-chains"][0]
        .as_object_mut()
        .unwrap()
        .remove("receipt-count");
    cases.push((
        "evt-view-chain-missing-chain-receipt-count",
        "view receipt-chain chain evidence must contain receipt-count",
        chain_receipt_count_missing,
    ));

    let mut chain_receipt_count_drift = base_payload.clone();
    chain_receipt_count_drift["receipt-count"] = json!(2);
    chain_receipt_count_drift["view-version-receipt-chains"][0]["receipt-count"] = json!(2);
    cases.push((
        "evt-view-chain-chain-receipt-count-drift",
        "verified view receipt-chain receipt-count must match receipts",
        chain_receipt_count_drift,
    ));

    let mut chain_latest_version_drift = base_payload.clone();
    chain_latest_version_drift["view-version-receipt-chains"][0]["latest-view-version"] = json!(2);
    cases.push((
        "evt-view-chain-latest-version-drift",
        "verified view receipt-chain latest-view-version must match the last receipt",
        chain_latest_version_drift,
    ));

    let mut chain_latest_operation_drift = base_payload.clone();
    chain_latest_operation_drift["view-version-receipt-chains"][0]["latest-operation"] =
        json!("drop");
    cases.push((
        "evt-view-chain-latest-operation-drift",
        "verified view receipt-chain latest-operation must match the last receipt",
        chain_latest_operation_drift,
    ));

    let mut chain_tombstoned_drift = base_payload.clone();
    chain_tombstoned_drift["view-version-receipt-chains"][0]["tombstoned"] = json!(true);
    chain_tombstoned_drift["tombstone-count"] = json!(1);
    cases.push((
        "evt-view-chain-tombstoned-drift",
        "verified view receipt-chain tombstoned flag must match the last receipt operation",
        chain_tombstoned_drift,
    ));

    let mut chain_hash_content_drift = base_payload.clone();
    let forged_chain_hash = content_hash_bytes(b"forged-view-receipt-chain");
    chain_hash_content_drift["view-version-receipt-chains"][0]["chain-hash"] =
        json!(&forged_chain_hash);
    chain_hash_content_drift["chain-hashes"] = json!([forged_chain_hash]);
    cases.push((
        "evt-view-chain-hash-content-drift",
        "verified view receipt-chain chain-hash must match structural receipt-chain evidence",
        chain_hash_content_drift,
    ));

    let mut chain_hash_coverage_drift = base_payload.clone();
    chain_hash_coverage_drift["chain-hashes"] =
        json!([content_hash_bytes(b"forged-view-receipt-chain")]);
    cases.push((
        "evt-view-chain-chain-hash-coverage-drift",
        "view receipt-chain chain-hashes must match structural receipt-chain evidence",
        chain_hash_coverage_drift,
    ));

    let mut receipt_hash_coverage_drift = base_payload.clone();
    receipt_hash_coverage_drift["receipt-hashes"] =
        json!([content_hash_bytes(b"forged-view-receipt")]);
    cases.push((
        "evt-view-chain-receipt-hash-coverage-drift",
        "view receipt-chain receipt-hashes must match structural receipt-chain evidence",
        receipt_hash_coverage_drift,
    ));

    let mut drop_receipt_hash_coverage_drift = base_payload.clone();
    drop_receipt_hash_coverage_drift["drop-receipt-hashes"] =
        json!([content_hash_bytes(b"forged-view-drop-receipt")]);
    cases.push((
        "evt-view-chain-drop-receipt-hash-coverage-drift",
        "view receipt-chain drop-receipt-hashes must match structural receipt-chain evidence",
        drop_receipt_hash_coverage_drift,
    ));

    for (event_id, expected_message, payload) in cases {
        let store = Arc::new(RecordingOutboxStore {
            events: Mutex::new(vec![OutboxEvent {
                event_id: event_id.to_string(),
                sink: "lakecat.lineage-and-graph".to_string(),
                event_type: "view.version-receipt-chains-listed".to_string(),
                payload: json!({
                    "audit-event-id": format!("audit-{event_id}"),
                    "event-type": "view.version-receipt-chains-listed",
                    "payload": payload,
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
            .expect_err("malformed view receipt-chain scope/count evidence should fail");

        let message = err.to_string();
        assert!(message.contains("view.version-receipt-chains-listed"));
        assert!(
            message.contains(expected_message),
            "{event_id} should describe malformed receipt-chain evidence: {message}"
        );
        assert!(message.contains("event-id-hash=sha256:"));
        assert!(!message.contains(event_id));
        assert!(store.delivered.lock().await.is_empty());
        assert!(graph.events.lock().await.is_empty());
        assert!(lineage.events.lock().await.is_empty());
    }
}

#[tokio::test]
async fn outbox_drain_rejects_extra_view_receipt_read_fields() {
    let principal = Principal::new("agent:operator", PrincipalKind::Agent).unwrap();
    let receipt_hash = content_hash_json(&json!({
        "stable-id": "lakecat:view:local:default:events_view",
        "view-version": 1,
        "operation": "upsert"
    }))
    .unwrap();
    let chain_hash = content_hash_json(&json!({
        "stable-id": "lakecat:view:local:default:events_view",
        "receipt-hashes": [&receipt_hash]
    }))
    .unwrap();
    let valid_chain = json!({
        "stable-id": "lakecat:view:local:default:events_view",
        "warehouse": "local",
        "namespace": ["default"],
        "name": "events_view",
        "chain-hash": &chain_hash,
        "chain-verified": true,
        "latest-view-version": 1,
        "latest-operation": "upsert",
        "tombstoned": false,
        "receipt-count": 1,
        "receipts": [{
            "stable-id": "lakecat:view:local:default:events_view",
            "warehouse": "local",
            "namespace": ["default"],
            "name": "events_view",
            "view-version": 1,
            "previous-view-version": null,
            "previous-receipt-hash": null,
            "operation": "upsert",
            "view-hash": content_hash_json(&json!({"view": "events_view", "version": 1})).unwrap(),
            "receipt-hash": &receipt_hash,
            "principal-subject": "agent:operator",
            "principal-kind": "agent",
            "recorded-at": "2026-06-20T00:00:00Z"
        }]
    });
    let cases = vec![
        (
            "view.version-receipts-listed",
            "evt-view-receipts-extra-field",
            "view receipt-list",
            "unverified-receipt-list-claim",
            json!({
                "event-type": "view.version-receipts-listed",
                "warehouse": "local",
                "namespace": ["default"],
                "view": "events_view",
                "receipt-count": 1,
                "receipt-hashes": [&receipt_hash],
                "drop-receipt-hashes": [],
                "authorization-receipt": {
                    "principal": &principal,
                    "action": "view-load",
                    "allowed": true,
                    "engine": "test",
                    "policy_hash": null,
                    "checked_at": chrono::Utc::now(),
                },
            }),
        ),
        (
            "view.version-receipt-chains-listed",
            "evt-view-receipt-chains-extra-field",
            "view receipt-chain",
            "unverified-receipt-chain-claim",
            json!({
                "event-type": "view.version-receipt-chains-listed",
                "warehouse": "local",
                "namespace": ["default"],
                "chain-count": 1,
                "receipt-count": 1,
                "tombstone-count": 0,
                "chain-verified-count": 1,
                "view-version-receipt-chains": [valid_chain],
                "chain-hashes": [&chain_hash],
                "receipt-hashes": [&receipt_hash],
                "drop-receipt-hashes": [],
                "authorization-receipt": {
                    "principal": &principal,
                    "action": "view-load",
                    "allowed": true,
                    "engine": "test",
                    "policy_hash": null,
                    "checked_at": chrono::Utc::now(),
                },
            }),
        ),
    ];

    for (event_type, event_id, label, extra_field, mut payload) in cases {
        payload[extra_field] = json!("shadow");
        let store = Arc::new(RecordingOutboxStore {
            events: Mutex::new(vec![OutboxEvent {
                event_id: event_id.to_string(),
                sink: "lakecat.lineage-and-graph".to_string(),
                event_type: event_type.to_string(),
                payload: json!({
                    "audit-event-id": format!("audit-{event_id}"),
                    "event-type": event_type,
                    "payload": payload,
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
            .expect_err("extra view receipt read fields should fail before delivery");

        let message = err.to_string();
        assert!(message.contains(event_type));
        assert!(
            message.contains(&format!("{label} contains unexpected field {extra_field}")),
            "{event_id} should reject extra view receipt read field: {message}"
        );
        assert!(message.contains("event-id-hash=sha256:"));
        assert!(!message.contains(event_id));
        assert!(store.delivered.lock().await.is_empty());
        assert!(graph.events.lock().await.is_empty());
        assert!(lineage.events.lock().await.is_empty());
    }
}

#[tokio::test]
async fn outbox_drain_rejects_extra_view_receipt_read_wrapper_fields() {
    let principal = Principal::new("agent:operator", PrincipalKind::Agent).unwrap();
    let receipt_hash = content_hash_json(&json!({
        "stable-id": "lakecat:view:local:default:events_view",
        "view-version": 1,
        "operation": "upsert"
    }))
    .unwrap();
    let chain_hash = content_hash_json(&json!({
        "stable-id": "lakecat:view:local:default:events_view",
        "receipt-hashes": [&receipt_hash]
    }))
    .unwrap();
    let valid_chain = json!({
        "stable-id": "lakecat:view:local:default:events_view",
        "warehouse": "local",
        "namespace": ["default"],
        "name": "events_view",
        "chain-hash": &chain_hash,
        "chain-verified": true,
        "latest-view-version": 1,
        "latest-operation": "upsert",
        "tombstoned": false,
        "receipt-count": 1,
        "receipts": [{
            "stable-id": "lakecat:view:local:default:events_view",
            "warehouse": "local",
            "namespace": ["default"],
            "name": "events_view",
            "view-version": 1,
            "previous-view-version": null,
            "previous-receipt-hash": null,
            "operation": "upsert",
            "view-hash": content_hash_json(&json!({"view": "events_view", "version": 1})).unwrap(),
            "receipt-hash": &receipt_hash,
            "principal-subject": "agent:operator",
            "principal-kind": "agent",
            "recorded-at": "2026-06-20T00:00:00Z"
        }]
    });
    let cases = vec![
        (
            "view.version-receipts-listed",
            "evt-view-receipts-wrapper-extra-field",
            json!({
                "event-type": "view.version-receipts-listed",
                "warehouse": "local",
                "namespace": ["default"],
                "view": "events_view",
                "receipt-count": 1,
                "receipt-hashes": [&receipt_hash],
                "drop-receipt-hashes": [],
                "authorization-receipt": {
                    "principal": &principal,
                    "action": "view-load",
                    "allowed": true,
                    "engine": "test",
                    "policy_hash": null,
                    "checked_at": chrono::Utc::now(),
                },
            }),
        ),
        (
            "view.version-receipt-chains-listed",
            "evt-view-receipt-chains-wrapper-extra-field",
            json!({
                "event-type": "view.version-receipt-chains-listed",
                "warehouse": "local",
                "namespace": ["default"],
                "chain-count": 1,
                "receipt-count": 1,
                "tombstone-count": 0,
                "chain-verified-count": 1,
                "view-version-receipt-chains": [valid_chain],
                "chain-hashes": [&chain_hash],
                "receipt-hashes": [&receipt_hash],
                "drop-receipt-hashes": [],
                "authorization-receipt": {
                    "principal": &principal,
                    "action": "view-load",
                    "allowed": true,
                    "engine": "test",
                    "policy_hash": null,
                    "checked_at": chrono::Utc::now(),
                },
            }),
        ),
    ];

    for (event_type, event_id, payload) in cases {
        let store = Arc::new(RecordingOutboxStore {
            events: Mutex::new(vec![OutboxEvent {
                event_id: event_id.to_string(),
                sink: "lakecat.lineage-and-graph".to_string(),
                event_type: event_type.to_string(),
                payload: json!({
                    "audit-event-id": format!("audit-{event_id}"),
                    "event-type": event_type,
                    "payload": payload,
                    "unverified-receipt-read-wrapper-claim": "shadow",
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
            .expect_err("extra view receipt read wrapper fields should fail before delivery");

        let message = err.to_string();
        assert!(message.contains(event_type));
        assert!(
            message.contains(
                "view receipt-read outbox payload contains unexpected field unverified-receipt-read-wrapper-claim"
            ),
            "{event_id} should reject extra receipt-read wrapper field: {message}"
        );
        assert!(message.contains("event-id-hash=sha256:"));
        assert!(!message.contains(event_id));
        assert!(store.delivered.lock().await.is_empty());
        assert!(graph.events.lock().await.is_empty());
        assert!(lineage.events.lock().await.is_empty());
    }
}

#[tokio::test]
async fn outbox_drain_rejects_malformed_view_receipt_list_scope_evidence() {
    let principal = Principal::new("agent:operator", PrincipalKind::Agent).unwrap();
    let receipt_hash = content_hash_json(&json!({
        "stable-id": "lakecat:view:local:default:events_view",
        "view-version": 1,
        "operation": "upsert"
    }))
    .unwrap();
    let cases = vec![
        (
            "evt-view-receipts-missing-namespace",
            "view receipt-list evidence must contain namespace",
            json!({
                "audit-event-id": "audit-view-receipts-missing-namespace",
                "event-type": "view.version-receipts-listed",
                "payload": {
                    "warehouse": "local",
                    "view": "events_view",
                    "receipt-count": 1,
                    "receipt-hashes": [&receipt_hash],
                    "drop-receipt-hashes": [],
                    "authorization-receipt": {
                        "principal": &principal,
                        "action": "view-load",
                        "allowed": true,
                        "engine": "test",
                        "policy_hash": null,
                        "checked_at": chrono::Utc::now(),
                    },
                },
            }),
        ),
        (
            "evt-view-receipts-invalid-view",
            "view receipt-list evidence must contain view",
            json!({
                "audit-event-id": "audit-view-receipts-invalid-view",
                "event-type": "view.version-receipts-listed",
                "payload": {
                    "warehouse": "local",
                    "namespace": ["default"],
                    "view": "",
                    "receipt-count": 1,
                    "receipt-hashes": [&receipt_hash],
                    "drop-receipt-hashes": [],
                    "authorization-receipt": {
                        "principal": &principal,
                        "action": "view-load",
                        "allowed": true,
                        "engine": "test",
                        "policy_hash": null,
                        "checked_at": chrono::Utc::now(),
                    },
                },
            }),
        ),
        (
            "evt-view-receipts-missing-principal",
            "view receipt-list evidence must contain authorization receipt principal",
            json!({
                "audit-event-id": "audit-view-receipts-missing-principal",
                "event-type": "view.version-receipts-listed",
                "payload": {
                    "warehouse": "local",
                    "namespace": ["default"],
                    "view": "events_view",
                    "receipt-count": 1,
                    "receipt-hashes": [&receipt_hash],
                    "drop-receipt-hashes": [],
                    "authorization-receipt": {
                        "action": "view-load",
                        "allowed": true,
                        "engine": "test",
                        "policy_hash": null,
                        "checked_at": chrono::Utc::now(),
                    },
                },
            }),
        ),
        (
            "evt-view-receipts-malformed-principal",
            "view receipt-list authorization receipt principal",
            json!({
                "audit-event-id": "audit-view-receipts-malformed-principal",
                "event-type": "view.version-receipts-listed",
                "payload": {
                    "warehouse": "local",
                    "namespace": ["default"],
                    "view": "events_view",
                    "receipt-count": 1,
                    "receipt-hashes": [&receipt_hash],
                    "drop-receipt-hashes": [],
                    "authorization-receipt": {
                        "principal": {
                            "subject": "agent:operator",
                            "kind": "unknown"
                        },
                        "action": "view-load",
                        "allowed": true,
                        "engine": "test",
                        "policy_hash": null,
                        "checked_at": chrono::Utc::now(),
                    },
                },
            }),
        ),
    ];

    for (event_id, expected_message, payload) in cases {
        let store = Arc::new(RecordingOutboxStore {
            events: Mutex::new(vec![OutboxEvent {
                event_id: event_id.to_string(),
                sink: "lakecat.lineage-and-graph".to_string(),
                event_type: "view.version-receipts-listed".to_string(),
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
            .expect_err("malformed view receipt-list scope evidence should fail");

        let message = err.to_string();
        assert!(message.contains("view.version-receipts-listed"));
        assert!(
            message.contains(expected_message),
            "{event_id} should describe malformed receipt-list scope: {message}"
        );
        assert!(message.contains("event-id-hash=sha256:"));
        assert!(!message.contains(event_id));
        assert!(store.delivered.lock().await.is_empty());
        assert!(graph.events.lock().await.is_empty());
        assert!(lineage.events.lock().await.is_empty());
    }
}

#[tokio::test]
async fn outbox_drain_rejects_missing_or_denied_view_receipt_list_allowed_decision() {
    let principal = Principal::new("agent:operator", PrincipalKind::Agent).unwrap();
    let receipt_hash = content_hash_json(&json!({
        "stable-id": "lakecat:view:local:default:events_view",
        "view-version": 1,
        "operation": "upsert"
    }))
    .unwrap();
    let base_receipt = json!({
        "principal": principal,
        "action": "view-load",
        "allowed": true,
        "engine": "test",
        "policy_hash": null,
        "checked_at": chrono::Utc::now(),
    });
    let mut missing_allowed_receipt = base_receipt.clone();
    missing_allowed_receipt
        .as_object_mut()
        .unwrap()
        .remove("allowed");
    let mut denied_receipt = base_receipt;
    denied_receipt["allowed"] = json!(false);

    for (event_id, receipt, expected_message) in [
        (
            "evt-view-receipts-missing-receipt-allowed",
            missing_allowed_receipt,
            "view receipt-list evidence must contain authorization receipt allowed decision",
        ),
        (
            "evt-view-receipts-denied-receipt",
            denied_receipt,
            "view receipt-list authorization receipt must allow replay projection",
        ),
    ] {
        let store = Arc::new(RecordingOutboxStore {
            events: Mutex::new(vec![OutboxEvent {
                event_id: event_id.to_string(),
                sink: "lakecat.lineage-and-graph".to_string(),
                event_type: "view.version-receipts-listed".to_string(),
                payload: json!({
                    "audit-event-id": format!("audit-{event_id}"),
                    "event-type": "view.version-receipts-listed",
                    "payload": {
                        "warehouse": "local",
                        "namespace": ["default"],
                        "view": "events_view",
                        "receipt-count": 1,
                        "receipt-hashes": [&receipt_hash],
                        "drop-receipt-hashes": [],
                        "authorization-receipt": receipt,
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
            .expect_err("missing or denied view receipt-list decision should fail");

        let message = err.to_string();
        assert!(message.contains("view.version-receipts-listed"));
        assert!(message.contains(expected_message), "{message}");
        assert!(message.contains("event-id-hash=sha256:"));
        assert!(!message.contains(event_id));
        assert!(
            store.delivered.lock().await.is_empty(),
            "view receipt-list decision failures must fail before acknowledgement"
        );
        assert!(
            graph.events.lock().await.is_empty(),
            "view receipt-list decision failures must fail before graph projection"
        );
        assert!(
            lineage.events.lock().await.is_empty(),
            "view receipt-list decision failures must fail before lineage projection"
        );
    }
}

#[tokio::test]
async fn outbox_drain_rejects_extra_view_receipt_chain_fields() {
    let principal = Principal::new("agent:operator", PrincipalKind::Agent).unwrap();
    let receipt_hash = content_hash_json(&json!({
        "stable-id": "lakecat:view:local:default:events_view",
        "view-version": 1,
        "operation": "upsert"
    }))
    .unwrap();
    let chain_hash = content_hash_json(&json!({
        "stable-id": "lakecat:view:local:default:events_view",
        "receipt-hashes": [&receipt_hash]
    }))
    .unwrap();
    let valid_chain = json!({
        "stable-id": "lakecat:view:local:default:events_view",
        "warehouse": "local",
        "namespace": ["default"],
        "name": "events_view",
        "chain-hash": &chain_hash,
        "chain-verified": true,
        "latest-view-version": 1,
        "latest-operation": "upsert",
        "tombstoned": false,
        "receipt-count": 1,
        "receipts": [{
            "stable-id": "lakecat:view:local:default:events_view",
            "warehouse": "local",
            "namespace": ["default"],
            "name": "events_view",
            "view-version": 1,
            "previous-view-version": null,
            "previous-receipt-hash": null,
            "operation": "upsert",
            "view-hash": content_hash_json(&json!({"view": "events_view", "version": 1})).unwrap(),
            "receipt-hash": &receipt_hash,
            "principal-subject": "agent:operator",
            "principal-kind": "agent",
            "recorded-at": "2026-06-20T00:00:00Z"
        }]
    });
    let base_payload = json!({
        "warehouse": "local",
        "namespace": ["default"],
        "chain-count": 1,
        "receipt-count": 1,
        "tombstone-count": 0,
        "chain-verified-count": 1,
        "view-version-receipt-chains": [valid_chain],
        "chain-hashes": [&chain_hash],
        "receipt-hashes": [&receipt_hash],
        "drop-receipt-hashes": [],
        "authorization-receipt": {
            "principal": &principal,
            "action": "view-load",
            "allowed": true,
            "engine": "test",
            "policy_hash": null,
            "checked_at": chrono::Utc::now(),
        },
    });
    let cases = vec![
        (
            "evt-view-chain-extra-chain-field",
            "/view-version-receipt-chains/0",
            "unverified-chain-claim",
            "view receipt-chain chain contains unexpected field unverified-chain-claim",
        ),
        (
            "evt-view-chain-extra-receipt-field",
            "/view-version-receipt-chains/0/receipts/0",
            "unverified-receipt-claim",
            "view receipt-chain receipt contains unexpected field unverified-receipt-claim",
        ),
    ];

    for (event_id, pointer, field, expected_message) in cases {
        let mut payload = base_payload.clone();
        payload
            .pointer_mut(pointer)
            .and_then(Value::as_object_mut)
            .expect("valid receipt-chain payload should contain target object")
            .insert(field.to_string(), json!(true));
        let store = Arc::new(RecordingOutboxStore {
            events: Mutex::new(vec![OutboxEvent {
                event_id: event_id.to_string(),
                sink: "lakecat.lineage-and-graph".to_string(),
                event_type: "view.version-receipt-chains-listed".to_string(),
                payload: json!({
                    "audit-event-id": format!("audit-{event_id}"),
                    "event-type": "view.version-receipt-chains-listed",
                    "payload": payload,
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
            .expect_err("extra view receipt-chain fields should fail");

        let message = err.to_string();
        assert!(message.contains("view.version-receipt-chains-listed"));
        assert!(
            message.contains(expected_message),
            "{event_id} should reject extra receipt-chain field: {message}"
        );
        assert!(message.contains("event-id-hash=sha256:"));
        assert!(!message.contains(event_id));
        assert!(store.delivered.lock().await.is_empty());
        assert!(graph.events.lock().await.is_empty());
        assert!(lineage.events.lock().await.is_empty());
    }
}

#[tokio::test]
async fn outbox_drain_rejects_missing_or_denied_view_receipt_chain_allowed_decision() {
    let principal = Principal::new("agent:operator", PrincipalKind::Agent).unwrap();
    let receipt_hash = content_hash_json(&json!({
        "stable-id": "lakecat:view:local:default:events_view",
        "view-version": 1,
        "operation": "upsert"
    }))
    .unwrap();
    let chain_hash = content_hash_json(&json!({
        "stable-id": "lakecat:view:local:default:events_view",
        "receipt-hashes": [&receipt_hash]
    }))
    .unwrap();
    let valid_chain = json!({
        "stable-id": "lakecat:view:local:default:events_view",
        "warehouse": "local",
        "namespace": ["default"],
        "name": "events_view",
        "chain-hash": &chain_hash,
        "chain-verified": true,
        "latest-view-version": 1,
        "latest-operation": "upsert",
        "tombstoned": false,
        "receipt-count": 1,
        "receipts": [{
            "stable-id": "lakecat:view:local:default:events_view",
            "warehouse": "local",
            "namespace": ["default"],
            "name": "events_view",
            "view-version": 1,
            "previous-view-version": null,
            "previous-receipt-hash": null,
            "operation": "upsert",
            "view-hash": content_hash_json(&json!({"view": "events_view", "version": 1})).unwrap(),
            "receipt-hash": &receipt_hash,
            "principal-subject": "agent:operator",
            "principal-kind": "agent",
            "recorded-at": "2026-06-20T00:00:00Z"
        }]
    });
    let base_receipt = json!({
        "principal": principal,
        "action": "view-load",
        "allowed": true,
        "engine": "test",
        "policy_hash": null,
        "checked_at": chrono::Utc::now(),
    });
    let mut missing_allowed_receipt = base_receipt.clone();
    missing_allowed_receipt
        .as_object_mut()
        .unwrap()
        .remove("allowed");
    let mut denied_receipt = base_receipt;
    denied_receipt["allowed"] = json!(false);

    for (event_id, receipt, expected_message) in [
        (
            "evt-view-chain-missing-receipt-allowed",
            missing_allowed_receipt,
            "view receipt-chain evidence must contain authorization receipt allowed decision",
        ),
        (
            "evt-view-chain-denied-receipt",
            denied_receipt,
            "view receipt-chain authorization receipt must allow replay projection",
        ),
    ] {
        let store = Arc::new(RecordingOutboxStore {
            events: Mutex::new(vec![OutboxEvent {
                event_id: event_id.to_string(),
                sink: "lakecat.lineage-and-graph".to_string(),
                event_type: "view.version-receipt-chains-listed".to_string(),
                payload: json!({
                    "audit-event-id": format!("audit-{event_id}"),
                    "event-type": "view.version-receipt-chains-listed",
                    "payload": {
                        "warehouse": "local",
                        "namespace": ["default"],
                        "chain-count": 1,
                        "receipt-count": 1,
                        "tombstone-count": 0,
                        "chain-verified-count": 1,
                        "view-version-receipt-chains": [valid_chain],
                        "chain-hashes": [&chain_hash],
                        "receipt-hashes": [&receipt_hash],
                        "drop-receipt-hashes": [],
                        "authorization-receipt": receipt,
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
            .expect_err("missing or denied view receipt-chain decision should fail");

        let message = err.to_string();
        assert!(message.contains("view.version-receipt-chains-listed"));
        assert!(message.contains(expected_message), "{message}");
        assert!(message.contains("event-id-hash=sha256:"));
        assert!(!message.contains(event_id));
        assert!(
            store.delivered.lock().await.is_empty(),
            "view receipt-chain decision failures must fail before acknowledgement"
        );
        assert!(
            graph.events.lock().await.is_empty(),
            "view receipt-chain decision failures must fail before graph projection"
        );
        assert!(
            lineage.events.lock().await.is_empty(),
            "view receipt-chain decision failures must fail before lineage projection"
        );
    }
}

#[tokio::test]
async fn outbox_drain_rejects_mismatched_view_receipt_read_actions() {
    let principal = Principal::new("agent:operator", PrincipalKind::Agent).unwrap();
    let receipt_hash = content_hash_json(&json!({
        "stable-id": "lakecat:view:local:default:events_view",
        "view-version": 1,
        "operation": "upsert"
    }))
    .unwrap();
    let chain_hash = content_hash_json(&json!({
        "stable-id": "lakecat:view:local:default:events_view",
        "receipt-hashes": [&receipt_hash]
    }))
    .unwrap();
    let valid_chain = json!({
        "stable-id": "lakecat:view:local:default:events_view",
        "warehouse": "local",
        "namespace": ["default"],
        "name": "events_view",
        "chain-hash": &chain_hash,
        "chain-verified": true,
        "latest-view-version": 1,
        "latest-operation": "upsert",
        "tombstoned": false,
        "receipt-count": 1,
        "receipts": [{
            "stable-id": "lakecat:view:local:default:events_view",
            "warehouse": "local",
            "namespace": ["default"],
            "name": "events_view",
            "view-version": 1,
            "previous-view-version": null,
            "previous-receipt-hash": null,
            "operation": "upsert",
            "view-hash": content_hash_json(&json!({"view": "events_view", "version": 1})).unwrap(),
            "receipt-hash": &receipt_hash,
            "principal-subject": "agent:operator",
            "principal-kind": "agent",
            "recorded-at": "2026-06-20T00:00:00Z"
        }]
    });
    let cases = vec![
        (
            "view.version-receipts-listed",
            "view receipt-list",
            "evt-view-receipts-mismatched-action",
            json!({
                "warehouse": "local",
                "namespace": ["default"],
                "view": "events_view",
                "receipt-count": 1,
                "receipt-hashes": [&receipt_hash],
                "drop-receipt-hashes": [],
            }),
        ),
        (
            "view.version-receipt-chains-listed",
            "view receipt-chain",
            "evt-view-chains-mismatched-action",
            json!({
                "warehouse": "local",
                "namespace": ["default"],
                "chain-count": 1,
                "receipt-count": 1,
                "tombstone-count": 0,
                "chain-verified-count": 1,
                "view-version-receipt-chains": [valid_chain],
                "chain-hashes": [&chain_hash],
                "receipt-hashes": [&receipt_hash],
                "drop-receipt-hashes": [],
            }),
        ),
    ];

    for (event_type, label, event_id, mut payload) in cases {
        payload["authorization-receipt"] = json!({
            "principal": principal,
            "action": "view-manage",
            "allowed": true,
            "engine": "test",
            "policy_hash": null,
            "checked_at": chrono::Utc::now(),
        });
        let store = Arc::new(RecordingOutboxStore {
            events: Mutex::new(vec![OutboxEvent {
                event_id: event_id.to_string(),
                sink: "lakecat.lineage-and-graph".to_string(),
                event_type: event_type.to_string(),
                payload: json!({
                    "audit-event-id": format!("audit-{event_id}"),
                    "event-type": event_type,
                    "payload": payload,
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
            .expect_err("mismatched view receipt read action should fail before delivery");

        let message = err.to_string();
        assert!(
            message.contains(event_type),
            "{event_type} error should include event type: {message}"
        );
        assert!(
            message.contains(&format!(
                "{label} authorization receipt action does not match outbox event type"
            )),
            "{event_type} error should describe receipt action drift: {message}"
        );
        assert!(message.contains("event-id-hash=sha256:"));
        assert!(!message.contains(event_id));
        assert!(store.delivered.lock().await.is_empty());
        assert!(graph.events.lock().await.is_empty());
        assert!(lineage.events.lock().await.is_empty());
    }
}

#[tokio::test]
async fn outbox_drain_rejects_malformed_view_receipt_list_evidence() {
    let principal = Principal::new("agent:operator", PrincipalKind::Agent).unwrap();
    let receipt_hash = content_hash_json(&json!({
        "stable-id": "lakecat:view:local:default:events_view",
        "view-version": 1,
        "operation": "upsert"
    }))
    .unwrap();
    let unrelated_drop_receipt_hash = content_hash_json(&json!({
        "stable-id": "lakecat:view:local:default:other_view",
        "view-version": 1,
        "operation": "drop"
    }))
    .unwrap();
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-view-receipts-malformed".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "view.version-receipts-listed".to_string(),
            payload: json!({
                "audit-event-id": "audit-view-receipts-malformed",
                "event-type": "view.version-receipts-listed",
                "payload": {
                    "warehouse": "local",
                    "namespace": ["default"],
                    "view": "events_view",
                    "receipt-count": 1,
                    "receipt-hashes": [receipt_hash],
                    "drop-receipt-hashes": [unrelated_drop_receipt_hash],
                    "authorization-receipt": {
                        "principal": principal,
                        "action": "view-load",
                        "allowed": true,
                        "engine": "test",
                        "policy_hash": null,
                        "checked_at": chrono::Utc::now(),
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

    let err = drain_outbox_once(&state, 10).await.unwrap_err();
    assert!(
        err.to_string()
            .contains("drop-receipt-hashes must be included"),
        "{err}"
    );
    assert!(store.delivered.lock().await.is_empty());
    assert!(graph.events.lock().await.is_empty());
    assert!(lineage.events.lock().await.is_empty());
}

#[tokio::test]
async fn outbox_drain_rejects_duplicate_view_receipt_list_hashes() {
    let principal = Principal::new("agent:operator", PrincipalKind::Agent).unwrap();
    let receipt_hash = content_hash_json(&json!({
        "stable-id": "lakecat:view:local:default:events_view",
        "view-version": 1,
        "operation": "upsert"
    }))
    .unwrap();
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-view-receipts-duplicate-hash".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "view.version-receipts-listed".to_string(),
            payload: json!({
                "audit-event-id": "audit-view-receipts-duplicate-hash",
                "event-type": "view.version-receipts-listed",
                "payload": {
                    "warehouse": "local",
                    "namespace": ["default"],
                    "view": "events_view",
                    "receipt-count": 2,
                    "receipt-hashes": [receipt_hash, receipt_hash],
                    "drop-receipt-hashes": [],
                    "authorization-receipt": {
                        "principal": principal,
                        "action": "view-load",
                        "allowed": true,
                        "engine": "test",
                        "policy_hash": null,
                        "checked_at": chrono::Utc::now(),
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
        .expect_err("duplicate view receipt-list hashes should fail before projection");

    let message = err.to_string();
    assert!(message.contains("view.version-receipts-listed"));
    assert!(message.contains("view receipt-list receipt-hashes must not contain duplicate hashes"));
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains("evt-view-receipts-duplicate-hash"));
    assert!(store.delivered.lock().await.is_empty());
    assert!(graph.events.lock().await.is_empty());
    assert!(lineage.events.lock().await.is_empty());
}
