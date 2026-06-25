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
async fn outbox_drain_rejects_partial_acknowledgement() {
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
            event_id: "evt-partial-ack".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "table.created".to_string(),
            payload: json!({
                "audit-event-id": "audit-partial-ack",
                "event-type": "table.created",
                "table": table,
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
                    "format-version": 3,
                    "version": 0,
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
        .expect_err("partial acknowledgement must fail the drain");

    let message = err.to_string();
    assert!(message.contains("outbox drain acknowledgement mismatch"));
    assert!(message.contains("projected 1 event(s)"));
    assert!(message.contains("marked 0 delivered"));
    assert_eq!(
        store.delivered.lock().await.as_slice(),
        &["evt-partial-ack".to_string()]
    );
    assert!(
        !graph.events.lock().await.is_empty(),
        "graph projection should have happened before the short acknowledgement"
    );
    assert!(
        !lineage.events.lock().await.is_empty(),
        "lineage projection should have happened before the short acknowledgement"
    );
}

#[tokio::test]
async fn outbox_drain_rejects_short_read_restriction_policy_hashes() {
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
            event_id: "evt-short-policy".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "table.scan-tasks-fetched".to_string(),
            payload: json!({
                "audit-event-id": "audit-short-policy",
                "event-type": "table.scan-tasks-fetched",
                "table": table,
                "payload": {
                    "authorization-receipt": {
                        "principal": principal,
                        "action": "table-plan-scan",
                        "allowed": true,
                        "engine": "test",
                        "policy_hash": null,
                        "checked_at": chrono::Utc::now(),
                    },
                    "read-restriction": {
                        "allowed-columns": ["event_id"],
                        "row-predicate": {
                            "type": "not-eq",
                            "term": "severity",
                            "value": "debug"
                        },
                        "policy-hashes": ["sha256:policy"]
                    },
                    "required-projection": ["event_id"],
                    "effective-projection": ["event_id"],
                    "requested-stats-fields": ["event_id"],
                    "effective-stats-fields": ["event_id"],
                    "required-filters": [{
                        "type": "not-eq",
                        "term": "severity",
                        "value": "debug"
                    }],
                    "file-scan-task-count": 1,
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
        .expect_err("short read restriction policy hash should fail before delivery");

    let message = err.to_string();
    assert!(message.contains("table.scan-tasks-fetched"));
    assert!(message.contains("read restriction policy-hashes"));
    assert!(message.contains("full SHA-256"));
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(store.delivered.lock().await.is_empty());
    assert!(graph.events.lock().await.is_empty());
    assert!(lineage.events.lock().await.is_empty());
}

#[tokio::test]
async fn outbox_drain_rejects_empty_read_restriction_policy_hashes() {
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
            event_id: "evt-empty-policy-hashes".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "table.scan-planned".to_string(),
            payload: json!({
                "audit-event-id": "audit-empty-policy-hashes",
                "event-type": "table.scan-planned",
                "table": table,
                "payload": {
                    "authorization-receipt": {
                        "principal": principal,
                        "action": "table-plan-scan",
                        "allowed": true,
                        "engine": "test",
                        "policy_hash": null,
                        "checked_at": chrono::Utc::now(),
                    },
                    "read-restriction": {
                        "allowed-columns": ["event_id"],
                        "row-predicate": {
                            "type": "not-eq",
                            "term": "severity",
                            "value": "debug"
                        },
                        "policy-hashes": []
                    },
                    "requested-projection": ["event_id"],
                    "effective-projection": ["event_id"],
                    "requested-stats-fields": ["event_id"],
                    "effective-stats-fields": ["event_id"],
                    "scan-task-count": 1,
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
        .expect_err("empty read restriction policy hashes should fail before delivery");

    let message = err.to_string();
    assert!(message.contains("table.scan-planned"));
    assert!(message.contains("read restriction policy-hashes must not be empty"));
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains("evt-empty-policy-hashes"));
    assert!(store.delivered.lock().await.is_empty());
    assert!(graph.events.lock().await.is_empty());
    assert!(lineage.events.lock().await.is_empty());
}

#[tokio::test]
async fn outbox_drain_rejects_duplicate_read_restriction_policy_hashes() {
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let principal = Principal {
        subject: "agent:writer".to_string(),
        kind: PrincipalKind::Agent,
    };
    let policy_hash = "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-duplicate-policy-hashes".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "table.scan-planned".to_string(),
            payload: json!({
                "audit-event-id": "audit-duplicate-policy-hashes",
                "event-type": "table.scan-planned",
                "table": table,
                "payload": {
                    "authorization-receipt": {
                        "principal": principal,
                        "action": "table-plan-scan",
                        "allowed": true,
                        "engine": "test",
                        "policy_hash": null,
                        "checked_at": chrono::Utc::now(),
                    },
                    "read-restriction": {
                        "allowed-columns": ["event_id"],
                        "row-predicate": {
                            "type": "not-eq",
                            "term": "severity",
                            "value": "debug"
                        },
                        "policy-hashes": [policy_hash, policy_hash]
                    },
                    "requested-projection": ["event_id"],
                    "effective-projection": ["event_id"],
                    "requested-stats-fields": ["event_id"],
                    "effective-stats-fields": ["event_id"],
                    "scan-task-count": 1,
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
        .expect_err("duplicate read restriction policy hashes should fail before delivery");

    let message = err.to_string();
    assert!(message.contains("table.scan-planned"));
    assert!(message.contains("read restriction policy-hashes must be duplicate-free"));
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains("evt-duplicate-policy-hashes"));
    assert!(store.delivered.lock().await.is_empty());
    assert!(graph.events.lock().await.is_empty());
    assert!(lineage.events.lock().await.is_empty());
}

#[tokio::test]
async fn outbox_drain_rejects_empty_authorization_receipt_policy_hashes() {
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
            event_id: "evt-empty-receipt-policy-hashes".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "table.scan-planned".to_string(),
            payload: json!({
                "audit-event-id": "audit-empty-receipt-policy-hashes",
                "event-type": "table.scan-planned",
                "table": table,
                "payload": {
                    "authorization-receipt": {
                        "principal": principal,
                        "action": "table-plan-scan",
                        "allowed": true,
                        "engine": "test",
                        "policy_hash": null,
                        "checked_at": chrono::Utc::now(),
                        "context": {
                            "read-restriction": {
                                "allowed-columns": ["event_id"],
                                "row-predicate": {
                                    "type": "not-eq",
                                    "term": "severity",
                                    "value": "debug"
                                },
                                "policy-hashes": []
                            }
                        }
                    },
                    "read-restriction": {
                        "allowed-columns": ["event_id"],
                        "row-predicate": {
                            "type": "not-eq",
                            "term": "severity",
                            "value": "debug"
                        },
                        "policy-hashes": [
                            "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
                        ]
                    },
                    "requested-projection": ["event_id"],
                    "effective-projection": ["event_id"],
                    "requested-stats-fields": ["event_id"],
                    "effective-stats-fields": ["event_id"],
                    "scan-task-count": 1,
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
        .expect_err("empty receipt policy hashes should fail before delivery");

    let message = err.to_string();
    assert!(message.contains("table.scan-planned"));
    assert!(
        message.contains("authorization receipt read restriction policy-hashes must not be empty")
    );
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains("evt-empty-receipt-policy-hashes"));
    assert!(store.delivered.lock().await.is_empty());
    assert!(graph.events.lock().await.is_empty());
    assert!(lineage.events.lock().await.is_empty());
}

#[tokio::test]
async fn outbox_drain_rejects_duplicate_authorization_receipt_policy_hashes() {
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let principal = Principal {
        subject: "agent:writer".to_string(),
        kind: PrincipalKind::Agent,
    };
    let policy_hash = "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    let read_restriction = json!({
        "allowed-columns": ["event_id"],
        "row-predicate": {
            "type": "not-eq",
            "term": "severity",
            "value": "debug"
        },
        "policy-hashes": [policy_hash, policy_hash]
    });
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-duplicate-receipt-policy-hashes".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "table.scan-planned".to_string(),
            payload: json!({
                "audit-event-id": "audit-duplicate-receipt-policy-hashes",
                "event-type": "table.scan-planned",
                "table": table,
                "payload": {
                    "authorization-receipt": {
                        "principal": principal,
                        "action": "table-plan-scan",
                        "allowed": true,
                        "engine": "test",
                        "policy_hash": null,
                        "checked_at": chrono::Utc::now(),
                        "context": {
                            "read-restriction": read_restriction
                        }
                    },
                    "read-restriction": {
                        "allowed-columns": ["event_id"],
                        "row-predicate": {
                            "type": "not-eq",
                            "term": "severity",
                            "value": "debug"
                        },
                        "policy-hashes": [
                            "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                        ]
                    },
                    "requested-projection": ["event_id"],
                    "effective-projection": ["event_id"],
                    "requested-stats-fields": ["event_id"],
                    "effective-stats-fields": ["event_id"],
                    "scan-task-count": 1,
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
        .expect_err("duplicate receipt policy hashes should fail before delivery");

    let message = err.to_string();
    assert!(message.contains("table.scan-planned"));
    assert!(
        message.contains(
            "authorization receipt read restriction policy-hashes must be duplicate-free"
        )
    );
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains("evt-duplicate-receipt-policy-hashes"));
    assert!(store.delivered.lock().await.is_empty());
    assert!(graph.events.lock().await.is_empty());
    assert!(lineage.events.lock().await.is_empty());
}

#[tokio::test]
async fn outbox_drain_rejects_malformed_catalog_read_evidence() {
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-secret-config-token".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "catalog.config-read".to_string(),
            payload: json!({
                "audit-event-id": "audit-corrupt-config",
                "event-type": "catalog.config-read",
                "payload": {
                    "warehouse": "",
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
        .expect_err("malformed catalog read evidence should fail");

    let message = err.to_string();
    assert!(
        message
            .contains("outbox event catalog.config-read (lakecat.lineage-and-graph) has invalid")
    );
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(message.contains("catalog config-read evidence must contain warehouse"));
    assert!(!message.contains("evt-secret-config-token"));
    assert!(
        store.delivered.lock().await.is_empty(),
        "malformed catalog read evidence must fail before acknowledgement"
    );
    assert!(
        graph.events.lock().await.is_empty(),
        "malformed catalog read evidence must fail before graph projection"
    );
    assert!(
        lineage.events.lock().await.is_empty(),
        "malformed catalog read evidence must fail before lineage projection"
    );
}

#[tokio::test]
async fn outbox_drain_rejects_catalog_config_without_v4_bridge_defaults() {
    let principal = Principal::new("agent:reader", PrincipalKind::Agent).unwrap();
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-config-missing-v4-bridge".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "catalog.config-read".to_string(),
            payload: json!({
                "audit-event-id": "audit-config-missing-v4-bridge",
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
                    "defaults": [
                        {"key": "lakecat.compatibility", "value": "iceberg-rest"},
                        {"key": "lakecat.format.baseline", "value": "iceberg-v1-v3"},
                        {"key": "lakecat.format.v4", "value": "extension-ready"}
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
        .expect_err("catalog config replay must include v4 bridge posture");

    let message = err.to_string();
    assert!(message.contains("catalog.config-read"));
    assert!(
        message.contains(
            "catalog config-read defaults must include lakecat.format.v4.bridge=json-passthrough"
        ),
        "{message}"
    );
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains("evt-config-missing-v4-bridge"));
    assert!(store.delivered.lock().await.is_empty());
    assert!(graph.events.lock().await.is_empty());
    assert!(lineage.events.lock().await.is_empty());
}

#[tokio::test]
async fn outbox_drain_rejects_malformed_catalog_config_default_entries() {
    let principal = Principal::new("agent:reader", PrincipalKind::Agent).unwrap();
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-config-malformed-default-entry".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "catalog.config-read".to_string(),
            payload: json!({
                "audit-event-id": "audit-config-malformed-default-entry",
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
                    "defaults": [
                        {"key": "lakecat.compatibility", "value": "iceberg-rest"},
                        {"key": "lakecat.format.baseline", "value": "iceberg-v1-v3"},
                        {"key": "lakecat.format.v4", "value": "extension-ready"},
                        {"key": "lakecat.format.v4.bridge", "value": "json-passthrough"},
                        {"key": "lakecat.format.v4.typed-sail", "value": 42}
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
        .expect_err("catalog config replay defaults must be structured key/value entries");

    let message = err.to_string();
    assert!(message.contains("catalog.config-read"));
    assert!(message.contains("catalog config-read defaults must contain string values"));
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains("evt-config-malformed-default-entry"));
    assert!(store.delivered.lock().await.is_empty());
    assert!(graph.events.lock().await.is_empty());
    assert!(lineage.events.lock().await.is_empty());
}

#[tokio::test]
async fn outbox_drain_rejects_extra_catalog_config_entry_fields() {
    let principal = Principal::new("agent:reader", PrincipalKind::Agent).unwrap();
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-config-extra-default-entry".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "catalog.config-read".to_string(),
            payload: json!({
                "audit-event-id": "audit-config-extra-default-entry",
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
                    "defaults": [
                        {"key": "lakecat.compatibility", "value": "iceberg-rest"},
                        {"key": "lakecat.format.baseline", "value": "iceberg-v1-v3"},
                        {"key": "lakecat.format.v4", "value": "extension-ready"},
                        {"key": "lakecat.format.v4.bridge", "value": "json-passthrough"},
                        {
                            "key": "lakecat.format.v4.typed-sail",
                            "value": "unavailable",
                            "unverified-config-claim": true
                        }
                    ],
                    "endpoints": catalog_config_endpoints_json()
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
        .expect_err("catalog config replay must reject extra key/value entry fields");

    let message = err.to_string();
    assert!(message.contains("catalog.config-read"));
    assert!(
        message.contains(
            "catalog config-read defaults contains unexpected field unverified-config-claim"
        ),
        "extra catalog config entry field should be rejected: {message}"
    );
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains("evt-config-extra-default-entry"));
    assert!(store.delivered.lock().await.is_empty());
    assert!(graph.events.lock().await.is_empty());
    assert!(lineage.events.lock().await.is_empty());
}

#[tokio::test]
async fn outbox_drain_rejects_extra_top_level_catalog_config_fields() {
    let principal = Principal::new("agent:reader", PrincipalKind::Agent).unwrap();
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-config-extra-top-level".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "catalog.config-read".to_string(),
            payload: json!({
                "audit-event-id": "audit-config-extra-top-level",
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
                    "endpoints": catalog_config_endpoints_json(),
                    "unverified-config-claim": true
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
        .expect_err("catalog config replay must reject extra top-level fields");

    let message = err.to_string();
    assert!(message.contains("catalog.config-read"));
    assert!(
        message.contains("catalog config-read contains unexpected field unverified-config-claim"),
        "extra catalog config top-level field should be rejected: {message}"
    );
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains("evt-config-extra-top-level"));
    assert!(store.delivered.lock().await.is_empty());
    assert!(graph.events.lock().await.is_empty());
    assert!(lineage.events.lock().await.is_empty());
}

#[tokio::test]
async fn outbox_drain_rejects_extra_catalog_config_tenant_record_fields() {
    let principal = Principal::new("agent:reader", PrincipalKind::Agent).unwrap();
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-config-extra-tenant-record".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "catalog.config-read".to_string(),
            payload: json!({
                "audit-event-id": "audit-config-extra-tenant-record",
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
                    "endpoints": catalog_config_endpoints_json(),
                    "warehouse-record": {
                        "warehouse": "local",
                        "project-id": "default",
                        "storage-root": "file:///tmp/lakecat/config",
                        "properties": {"purpose": "config-read"},
                        "unverified-tenant-claim": true
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
        .expect_err("catalog config replay must reject extra tenant record fields");

    let message = err.to_string();
    assert!(message.contains("catalog.config-read"));
    assert!(
        message.contains(
            "catalog config-read warehouse-record contains unexpected field unverified-tenant-claim"
        ),
        "extra catalog config tenant record field should be rejected: {message}"
    );
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains("evt-config-extra-tenant-record"));
    assert!(store.delivered.lock().await.is_empty());
    assert!(graph.events.lock().await.is_empty());
    assert!(lineage.events.lock().await.is_empty());
}

#[tokio::test]
async fn outbox_drain_rejects_unbound_catalog_config_tenant_roots() {
    let principal = Principal::new("agent:reader", PrincipalKind::Agent).unwrap();
    let cases = vec![
        (
            "evt-config-unhashed-server-record",
            json!({
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
                "endpoints": catalog_config_endpoints_json(),
                "server-record": {
                    "server-id": "prod",
                    "display-name": "Production",
                    "endpoint-url": "https://lakecat.example?token=raw-secret",
                    "properties": {"region": "global"}
                }
            }),
            "endpoint-url-hash must contain full SHA-256 digest evidence",
        ),
        (
            "evt-config-storage-root-hash-drift",
            json!({
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
                "endpoints": catalog_config_endpoints_json(),
                "warehouse-record": {
                    "warehouse": "local",
                    "project-id": "default",
                    "storage-root": "file:///tmp/lakecat/config?token=raw-secret",
                    "storage-root-hash": content_hash_json(&json!({
                        "storage-root": "file:///tmp/lakecat/other"
                    })).unwrap(),
                    "properties": {"purpose": "config-read"}
                }
            }),
            "catalog config-read warehouse-record storage-root-hash must match storage-root",
        ),
    ];

    for (event_id, payload, expected_message) in cases {
        let store = Arc::new(RecordingOutboxStore {
            events: Mutex::new(vec![OutboxEvent {
                event_id: event_id.to_string(),
                sink: "lakecat.lineage-and-graph".to_string(),
                event_type: "catalog.config-read".to_string(),
                payload: json!({
                    "audit-event-id": format!("audit-{event_id}"),
                    "event-type": "catalog.config-read",
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
            .expect_err("catalog config tenant roots must be hash-bound before projection");

        let message = err.to_string();
        assert!(message.contains("catalog.config-read"));
        assert!(message.contains(expected_message), "{message}");
        assert!(message.contains("event-id-hash=sha256:"));
        assert!(!message.contains(event_id));
        assert!(
            !message.contains("raw-secret"),
            "config tenant-root replay errors must not expose decorated roots"
        );
        assert!(store.delivered.lock().await.is_empty());
        assert!(graph.events.lock().await.is_empty());
        assert!(lineage.events.lock().await.is_empty());
    }
}

#[tokio::test]
async fn outbox_drain_rejects_stale_catalog_config_typed_sail_default() {
    let principal = Principal::new("agent:reader", PrincipalKind::Agent).unwrap();
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-config-stale-typed-sail-default".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "catalog.config-read".to_string(),
            payload: json!({
                "audit-event-id": "audit-config-stale-typed-sail-default",
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
                    "defaults": [
                        {"key": "lakecat.compatibility", "value": "iceberg-rest"},
                        {"key": "lakecat.format.baseline", "value": "iceberg-v1-v3"},
                        {"key": "lakecat.format.v4", "value": "extension-ready"},
                        {"key": "lakecat.format.v4.bridge", "value": "json-passthrough"},
                        {"key": "lakecat.format.v4.typed-sail", "value": "available"}
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
        .expect_err("catalog config replay must reject stale typed Sail v4 defaults");

    let message = err.to_string();
    assert!(message.contains("catalog.config-read"));
    assert!(
        message.contains(
            "catalog config-read defaults must include lakecat.format.v4.typed-sail=unavailable"
        ),
        "{message}"
    );
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains("evt-config-stale-typed-sail-default"));
    assert!(store.delivered.lock().await.is_empty());
    assert!(graph.events.lock().await.is_empty());
    assert!(lineage.events.lock().await.is_empty());
}

#[tokio::test]
async fn outbox_drain_rejects_duplicate_catalog_config_default_keys() {
    let principal = Principal::new("agent:reader", PrincipalKind::Agent).unwrap();
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-config-duplicate-default-key".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "catalog.config-read".to_string(),
            payload: json!({
                "audit-event-id": "audit-config-duplicate-default-key",
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
                    "defaults": [
                        {"key": "lakecat.compatibility", "value": "iceberg-rest"},
                        {"key": "lakecat.format.baseline", "value": "iceberg-v1-v3"},
                        {"key": "lakecat.format.v4", "value": "extension-ready"},
                        {"key": "lakecat.format.v4.bridge", "value": "json-passthrough"},
                        {"key": "lakecat.format.v4.typed-sail", "value": "unavailable"},
                        {"key": "lakecat.format.v4.typed-sail", "value": "available"}
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
        .expect_err("catalog config replay defaults must not carry contradictory keys");

    let message = err.to_string();
    assert!(message.contains("catalog.config-read"));
    assert!(message.contains("catalog config-read defaults must not contain duplicate keys"));
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains("evt-config-duplicate-default-key"));
    assert!(store.delivered.lock().await.is_empty());
    assert!(graph.events.lock().await.is_empty());
    assert!(lineage.events.lock().await.is_empty());
}

#[tokio::test]
async fn outbox_drain_rejects_unsupported_catalog_config_v4_defaults() {
    let principal = Principal::new("agent:reader", PrincipalKind::Agent).unwrap();
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-config-unsupported-v4-default".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "catalog.config-read".to_string(),
            payload: json!({
                "audit-event-id": "audit-config-unsupported-v4-default",
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
                    "defaults": [
                        {"key": "lakecat.compatibility", "value": "iceberg-rest"},
                        {"key": "lakecat.format.baseline", "value": "iceberg-v1-v3"},
                        {"key": "lakecat.format.v4", "value": "extension-ready"},
                        {"key": "lakecat.format.v4.bridge", "value": "json-passthrough"},
                        {"key": "lakecat.format.v4.typed-sail", "value": "unavailable"},
                        {"key": "lakecat.format.v4.typed-sail-preview", "value": "available"}
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
        .expect_err("catalog config replay must reject unsupported v4 bridge defaults");

    let message = err.to_string();
    assert!(message.contains("catalog.config-read"));
    assert!(
        message
            .contains("catalog config-read defaults must not contain unsupported v4 bridge keys"),
        "{message}"
    );
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains("evt-config-unsupported-v4-default"));
    assert!(store.delivered.lock().await.is_empty());
    assert!(graph.events.lock().await.is_empty());
    assert!(lineage.events.lock().await.is_empty());
}

#[tokio::test]
async fn outbox_drain_rejects_unsupported_catalog_config_v4_overrides() {
    let principal = Principal::new("agent:reader", PrincipalKind::Agent).unwrap();
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-config-unsupported-v4-override".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "catalog.config-read".to_string(),
            payload: json!({
                "audit-event-id": "audit-config-unsupported-v4-override",
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
                    "endpoints": catalog_config_endpoints_json(),
                    "overrides": [
                        {"key": "lakecat.format.v4.typed-sail", "value": "available"}
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
        .expect_err("catalog config replay must reject v4 bridge overrides");

    let message = err.to_string();
    assert!(message.contains("catalog.config-read"));
    assert!(
        message.contains("catalog config-read overrides must not contain v4 bridge keys"),
        "{message}"
    );
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains("evt-config-unsupported-v4-override"));
    assert!(store.delivered.lock().await.is_empty());
    assert!(graph.events.lock().await.is_empty());
    assert!(lineage.events.lock().await.is_empty());
}

#[tokio::test]
async fn outbox_drain_rejects_catalog_config_missing_standard_endpoints() {
    let principal = Principal::new("agent:reader", PrincipalKind::Agent).unwrap();
    let endpoints = CatalogConfigResponse::default()
        .endpoints
        .into_iter()
        .filter(|endpoint| endpoint != "POST /catalog/v1/namespaces/{namespace}/tables")
        .collect::<Vec<_>>();
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-config-missing-standard-endpoint".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "catalog.config-read".to_string(),
            payload: json!({
                "audit-event-id": "audit-config-missing-standard-endpoint",
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
        .expect_err("catalog config replay must preserve standard endpoints");

    let message = err.to_string();
    assert!(message.contains("catalog.config-read"));
    assert!(
        message.contains(
            "catalog config-read endpoints must include POST /catalog/v1/namespaces/{namespace}/tables"
        ),
        "{message}"
    );
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains("evt-config-missing-standard-endpoint"));
    assert!(store.delivered.lock().await.is_empty());
    assert!(graph.events.lock().await.is_empty());
    assert!(lineage.events.lock().await.is_empty());
}

#[tokio::test]
async fn outbox_drain_rejects_catalog_config_missing_governed_access_endpoints() {
    let principal = Principal::new("agent:reader", PrincipalKind::Agent).unwrap();
    let endpoints = CatalogConfigResponse::default()
        .endpoints
        .into_iter()
        .filter(|endpoint| {
            endpoint != "POST /catalog/v1/namespaces/{namespace}/tables/{table}/plan"
        })
        .collect::<Vec<_>>();
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-config-missing-governed-endpoint".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "catalog.config-read".to_string(),
            payload: json!({
                "audit-event-id": "audit-config-missing-governed-endpoint",
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
        .expect_err("catalog config replay must preserve governed access endpoints");

    let message = err.to_string();
    assert!(message.contains("catalog.config-read"));
    assert!(
        message.contains(
            "catalog config-read endpoints must include POST /catalog/v1/namespaces/{namespace}/tables/{table}/plan"
        ),
        "{message}"
    );
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains("evt-config-missing-governed-endpoint"));
    assert!(store.delivered.lock().await.is_empty());
    assert!(graph.events.lock().await.is_empty());
    assert!(lineage.events.lock().await.is_empty());
}

#[tokio::test]
async fn outbox_drain_rejects_duplicate_catalog_config_endpoints() {
    let principal = Principal::new("agent:reader", PrincipalKind::Agent).unwrap();
    let mut endpoints = CatalogConfigResponse::default().endpoints;
    endpoints.push("GET /querygraph/v1/bootstrap".to_string());
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-config-duplicate-endpoint".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "catalog.config-read".to_string(),
            payload: json!({
                "audit-event-id": "audit-config-duplicate-endpoint",
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
        .expect_err("catalog config replay must reject duplicate endpoints");

    let message = err.to_string();
    assert!(message.contains("catalog.config-read"));
    assert!(
        message.contains("catalog config-read endpoints must not contain duplicate entries"),
        "{message}"
    );
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains("evt-config-duplicate-endpoint"));
    assert!(store.delivered.lock().await.is_empty());
    assert!(graph.events.lock().await.is_empty());
    assert!(lineage.events.lock().await.is_empty());
}

#[tokio::test]
async fn outbox_drain_rejects_extra_inventory_list_outbox_payload_fields() {
    let principal = Principal::new("agent:operator", PrincipalKind::Agent).unwrap();
    let cases = vec![
        (
            "namespace.listed",
            "namespace",
            "namespace list outbox payload contains unexpected field unverified-inventory-claim",
            json!({
                "authorization-receipt": {
                    "principal": principal,
                    "action": "namespace-list",
                    "allowed": true,
                    "engine": "test",
                    "policy_hash": null,
                    "checked_at": chrono::Utc::now(),
                },
                "warehouse": "local",
                "namespace-count": 1,
                "namespace-paths": ["default"],
            }),
        ),
        (
            "view.listed",
            "view",
            "view list outbox payload contains unexpected field unverified-inventory-claim",
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
                "view-names": ["active_events"],
            }),
        ),
        (
            "policy-binding.listed",
            "policy-binding",
            "management list outbox payload contains unexpected field unverified-inventory-claim",
            json!({
                "authorization-receipt": {
                    "principal": principal,
                    "action": "policy-manage",
                    "allowed": true,
                    "engine": "test",
                    "policy_hash": null,
                    "checked_at": chrono::Utc::now(),
                },
                "warehouse": "local",
                "policy-count": 1,
                "policy-ids": ["restricted-events"],
            }),
        ),
        (
            "project.listed",
            "project",
            "management list outbox payload contains unexpected field unverified-inventory-claim",
            json!({
                "authorization-receipt": {
                    "principal": principal,
                    "action": "project-manage",
                    "allowed": true,
                    "engine": "test",
                    "policy_hash": null,
                    "checked_at": chrono::Utc::now(),
                },
                "project-count": 1,
                "project-ids": ["analytics"],
            }),
        ),
        (
            "server.listed",
            "server",
            "management list outbox payload contains unexpected field unverified-inventory-claim",
            json!({
                "authorization-receipt": {
                    "principal": principal,
                    "action": "server-manage",
                    "allowed": true,
                    "engine": "test",
                    "policy_hash": null,
                    "checked_at": chrono::Utc::now(),
                },
                "server-count": 1,
                "server-ids": ["prod-us"],
            }),
        ),
        (
            "storage-profile.listed",
            "storage-profile",
            "management list outbox payload contains unexpected field unverified-inventory-claim",
            json!({
                "authorization-receipt": {
                    "principal": principal,
                    "action": "storage-profile-manage",
                    "allowed": true,
                    "engine": "test",
                    "policy_hash": null,
                    "checked_at": chrono::Utc::now(),
                },
                "warehouse": "local",
                "storage-profile-count": 1,
                "storage-profile-ids": ["events-local"],
            }),
        ),
        (
            "warehouse.listed",
            "warehouse",
            "management list outbox payload contains unexpected field unverified-inventory-claim",
            json!({
                "authorization-receipt": {
                    "principal": principal,
                    "action": "warehouse-manage",
                    "allowed": true,
                    "engine": "test",
                    "policy_hash": null,
                    "checked_at": chrono::Utc::now(),
                },
                "project-id": "analytics",
                "warehouse-count": 1,
                "warehouse-names": ["local"],
            }),
        ),
    ];

    for (event_type, label, expected_message, payload) in cases {
        let event_id = format!("evt-{label}-list-extra-wrapper-field");
        let store = Arc::new(RecordingOutboxStore {
            events: Mutex::new(vec![OutboxEvent {
                event_id: event_id.clone(),
                sink: "lakecat.lineage-and-graph".to_string(),
                event_type: event_type.to_string(),
                payload: json!({
                    "audit-event-id": format!("audit-{label}-list-extra-wrapper-field"),
                    "event-type": event_type,
                    "payload": payload,
                    "unverified-inventory-claim": "shadow",
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
            .expect_err("extra inventory-list wrapper fields should fail before delivery");

        let message = err.to_string();
        assert!(
            message.contains(&format!(
                "outbox event {event_type} (lakecat.lineage-and-graph) has invalid"
            )),
            "{event_type} should be identified in the validation error"
        );
        assert!(message.contains("event-id-hash=sha256:"));
        assert!(message.contains(expected_message), "{message}");
        assert!(!message.contains(&event_id));
        assert!(
            store.delivered.lock().await.is_empty(),
            "extra inventory-list wrapper fields must fail before acknowledgement"
        );
        assert!(
            graph.events.lock().await.is_empty(),
            "extra inventory-list wrapper fields must fail before graph projection"
        );
        assert!(
            lineage.events.lock().await.is_empty(),
            "extra inventory-list wrapper fields must fail before lineage projection"
        );
    }
}

#[tokio::test]
async fn outbox_drain_rejects_mismatched_wrapped_event_type_evidence() {
    let principal = Principal::new("agent:operator", PrincipalKind::Agent).unwrap();
    let base_payload = json!({
        "authorization-receipt": {
            "principal": principal,
            "action": "namespace-list",
            "allowed": true,
            "engine": "test",
            "policy_hash": null,
            "checked_at": chrono::Utc::now(),
        },
        "warehouse": "local",
        "namespace-count": 1,
        "namespace-paths": ["default"],
    });
    let cases = vec![
        (
            "evt-wrapper-event-type-drift",
            json!({
                "audit-event-id": "audit-wrapper-event-type-drift",
                "event-type": "table.loaded",
                "payload": base_payload.clone(),
            }),
            "outbox payload event-type must match outbox event type",
        ),
        (
            "evt-inner-event-type-drift",
            json!({
                "audit-event-id": "audit-inner-event-type-drift",
                "event-type": "namespace.listed",
                "payload": {
                    "event-type": "table.loaded",
                    "authorization-receipt": {
                        "principal": principal,
                        "action": "namespace-list",
                        "allowed": true,
                        "engine": "test",
                        "policy_hash": null,
                        "checked_at": chrono::Utc::now(),
                    },
                    "warehouse": "local",
                    "namespace-count": 1,
                    "namespace-paths": ["default"],
                },
            }),
            "outbox inner payload event-type must match outbox event type",
        ),
    ];

    for (event_id, payload, expected_message) in cases {
        let store = Arc::new(RecordingOutboxStore {
            events: Mutex::new(vec![OutboxEvent {
                event_id: event_id.to_string(),
                sink: "lakecat.lineage-and-graph".to_string(),
                event_type: "namespace.listed".to_string(),
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
            .expect_err("mismatched event-type evidence should fail before delivery");

        let message = err.to_string();
        assert!(
            message
                .contains("outbox event namespace.listed (lakecat.lineage-and-graph) has invalid"),
            "namespace.listed should be identified in the validation error"
        );
        assert!(message.contains("event-id-hash=sha256:"));
        assert!(message.contains(expected_message), "{message}");
        assert!(!message.contains(event_id));
        assert!(
            store.delivered.lock().await.is_empty(),
            "mismatched event-type evidence must fail before acknowledgement"
        );
        assert!(
            graph.events.lock().await.is_empty(),
            "mismatched event-type evidence must fail before graph projection"
        );
        assert!(
            lineage.events.lock().await.is_empty(),
            "mismatched event-type evidence must fail before lineage projection"
        );
    }
}

#[tokio::test]
async fn outbox_drain_rejects_missing_standard_catalog_receipt_principal() {
    let cases = vec![
        (
            "catalog.config-read",
            "catalog config-read",
            json!({
                "authorization-receipt": {
                    "action": "catalog-config",
                    "allowed": true,
                    "engine": "test",
                    "policy_hash": null,
                    "checked_at": chrono::Utc::now(),
                },
                "warehouse": "local",
                "defaults": catalog_config_defaults_json(),
            }),
        ),
        (
            "namespace.listed",
            "namespace list",
            json!({
                "authorization-receipt": {
                    "action": "namespace-list",
                    "allowed": true,
                    "engine": "test",
                    "policy_hash": null,
                    "checked_at": chrono::Utc::now(),
                },
                "warehouse": "local",
                "namespace-count": 1,
                "namespace-paths": ["default"],
            }),
        ),
        (
            "namespace.created",
            "namespace lifecycle",
            json!({
                "authorization-receipt": {
                    "action": "namespace-create",
                    "allowed": true,
                    "engine": "test",
                    "policy_hash": null,
                    "checked_at": chrono::Utc::now(),
                },
                "warehouse": "local",
                "namespace": ["default"],
            }),
        ),
        (
            "namespace.loaded",
            "namespace lifecycle",
            json!({
                "authorization-receipt": {
                    "action": "namespace-load",
                    "allowed": true,
                    "engine": "test",
                    "policy_hash": null,
                    "checked_at": chrono::Utc::now(),
                },
                "warehouse": "local",
                "namespace": ["default"],
            }),
        ),
        (
            "namespace.dropped",
            "namespace lifecycle",
            json!({
                "authorization-receipt": {
                    "action": "namespace-drop",
                    "allowed": true,
                    "engine": "test",
                    "policy_hash": null,
                    "checked_at": chrono::Utc::now(),
                },
                "warehouse": "local",
                "namespace": ["archived"],
            }),
        ),
        (
            "view.listed",
            "view list",
            json!({
                "authorization-receipt": {
                    "action": "view-load",
                    "allowed": true,
                    "engine": "test",
                    "policy_hash": null,
                    "checked_at": chrono::Utc::now(),
                },
                "warehouse": "local",
                "namespace": ["default"],
                "view-count": 1,
                "view-names": ["active_customers"],
            }),
        ),
        (
            "view.upserted",
            "view lifecycle",
            json!({
                "authorization-receipt": {
                    "action": "view-manage",
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
            }),
        ),
        (
            "view.loaded",
            "view lifecycle",
            json!({
                "authorization-receipt": {
                    "action": "view-load",
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
            }),
        ),
        (
            "view.dropped",
            "view lifecycle",
            json!({
                "authorization-receipt": {
                    "action": "view-drop",
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
            }),
        ),
    ];

    for (event_type, label, payload) in cases {
        let event_id = format!("evt-secret-{}-principal-token", event_type);
        let store = Arc::new(RecordingOutboxStore {
            events: Mutex::new(vec![OutboxEvent {
                event_id: event_id.clone(),
                sink: "lakecat.lineage-and-graph".to_string(),
                event_type: event_type.to_string(),
                payload: json!({
                    "audit-event-id": format!("audit-missing-{event_type}-principal"),
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
            .expect_err("missing standard catalog receipt principal should fail");

        let message = err.to_string();
        assert!(
            message.contains(event_type),
            "{event_type} error should include event type: {message}"
        );
        assert!(
            message.contains(&format!(
                "{label} evidence must contain authorization receipt principal"
            )),
            "{event_type} error should describe missing receipt principal: {message}"
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
async fn outbox_drain_rejects_malformed_standard_catalog_receipt_principal() {
    let malformed_principal = json!({
        "subject": "agent:reader",
        "kind": "unknown"
    });
    let cases = vec![
        (
            "catalog.config-read",
            "catalog config-read",
            json!({
                "authorization-receipt": {
                    "principal": malformed_principal,
                    "action": "catalog-config",
                    "allowed": true,
                    "engine": "test",
                    "policy_hash": null,
                    "checked_at": chrono::Utc::now(),
                },
                "warehouse": "local",
                "defaults": catalog_config_defaults_json(),
            }),
        ),
        (
            "namespace.listed",
            "namespace list",
            json!({
                "authorization-receipt": {
                    "principal": malformed_principal,
                    "action": "namespace-list",
                    "allowed": true,
                    "engine": "test",
                    "policy_hash": null,
                    "checked_at": chrono::Utc::now(),
                },
                "warehouse": "local",
                "namespace-count": 1,
                "namespace-paths": ["default"],
            }),
        ),
        (
            "namespace.created",
            "namespace lifecycle",
            json!({
                "authorization-receipt": {
                    "principal": malformed_principal,
                    "action": "namespace-create",
                    "allowed": true,
                    "engine": "test",
                    "policy_hash": null,
                    "checked_at": chrono::Utc::now(),
                },
                "warehouse": "local",
                "namespace": ["default"],
            }),
        ),
        (
            "namespace.loaded",
            "namespace lifecycle",
            json!({
                "authorization-receipt": {
                    "principal": malformed_principal,
                    "action": "namespace-load",
                    "allowed": true,
                    "engine": "test",
                    "policy_hash": null,
                    "checked_at": chrono::Utc::now(),
                },
                "warehouse": "local",
                "namespace": ["default"],
            }),
        ),
        (
            "namespace.dropped",
            "namespace lifecycle",
            json!({
                "authorization-receipt": {
                    "principal": malformed_principal,
                    "action": "namespace-drop",
                    "allowed": true,
                    "engine": "test",
                    "policy_hash": null,
                    "checked_at": chrono::Utc::now(),
                },
                "warehouse": "local",
                "namespace": ["archived"],
            }),
        ),
        (
            "view.listed",
            "view list",
            json!({
                "authorization-receipt": {
                    "principal": malformed_principal,
                    "action": "view-load",
                    "allowed": true,
                    "engine": "test",
                    "policy_hash": null,
                    "checked_at": chrono::Utc::now(),
                },
                "warehouse": "local",
                "namespace": ["default"],
                "view-count": 1,
                "view-names": ["active_customers"],
            }),
        ),
        (
            "view.upserted",
            "view lifecycle",
            json!({
                "authorization-receipt": {
                    "principal": malformed_principal,
                    "action": "view-manage",
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
            }),
        ),
        (
            "view.loaded",
            "view lifecycle",
            json!({
                "authorization-receipt": {
                    "principal": malformed_principal,
                    "action": "view-load",
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
            }),
        ),
        (
            "view.dropped",
            "view lifecycle",
            json!({
                "authorization-receipt": {
                    "principal": malformed_principal,
                    "action": "view-drop",
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
            }),
        ),
    ];

    for (event_type, label, payload) in cases {
        let event_id = format!("evt-malformed-{}-principal-token", event_type);
        let store = Arc::new(RecordingOutboxStore {
            events: Mutex::new(vec![OutboxEvent {
                event_id: event_id.clone(),
                sink: "lakecat.lineage-and-graph".to_string(),
                event_type: event_type.to_string(),
                payload: json!({
                    "audit-event-id": format!("audit-malformed-{event_type}-principal"),
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
            .expect_err("malformed standard catalog receipt principal should fail");

        let message = err.to_string();
        assert!(
            message.contains(event_type),
            "{event_type} error should include event type: {message}"
        );
        assert!(
            message.contains(&format!("{label} authorization receipt principal")),
            "{event_type} error should describe malformed receipt principal: {message}"
        );
        assert!(
            message.contains("must be a valid principal"),
            "{event_type} error should reject malformed principal shape: {message}"
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
async fn outbox_drain_rejects_extra_authorization_receipt_fields() {
    let principal = Principal::new("agent:reader", PrincipalKind::Agent).unwrap();
    let event_id = "evt-config-extra-auth-receipt-field";
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: event_id.to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "catalog.config-read".to_string(),
            payload: json!({
                "audit-event-id": format!("audit-{event_id}"),
                "event-type": "catalog.config-read",
                "payload": {
                    "authorization-receipt": {
                        "principal": principal,
                        "action": "catalog-config",
                        "allowed": true,
                        "engine": "test",
                        "policy_hash": null,
                        "checked_at": chrono::Utc::now(),
                        "unverified-receipt-claim": "allowed",
                    },
                    "warehouse": "local",
                    "defaults": catalog_config_defaults_json(),
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
        .expect_err("extra authorization receipt fields should fail before delivery");

    let message = err.to_string();
    assert!(message.contains("catalog.config-read"));
    assert!(
        message.contains(
            "catalog config-read authorization receipt contains unexpected field unverified-receipt-claim"
        ),
        "catalog config-read error should reject extra receipt fields: {message}"
    );
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains(event_id));
    assert!(
        store.delivered.lock().await.is_empty(),
        "receipt schema failures must fail before acknowledgement"
    );
    assert!(
        graph.events.lock().await.is_empty(),
        "receipt schema failures must fail before graph projection"
    );
    assert!(
        lineage.events.lock().await.is_empty(),
        "receipt schema failures must fail before lineage projection"
    );
}

#[tokio::test]
async fn outbox_drain_rejects_extra_authorization_receipt_context_fields() {
    let principal = Principal::new("agent:reader", PrincipalKind::Agent).unwrap();
    let event_id = "evt-config-extra-auth-context-field";
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: event_id.to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "catalog.config-read".to_string(),
            payload: json!({
                "audit-event-id": format!("audit-{event_id}"),
                "event-type": "catalog.config-read",
                "payload": {
                    "authorization-receipt": {
                        "principal": principal,
                        "action": "catalog-config",
                        "allowed": true,
                        "engine": "test",
                        "policy_hash": null,
                        "checked_at": chrono::Utc::now(),
                        "context": {
                            "unverified-context-claim": "delegated-admin",
                        },
                    },
                    "warehouse": "local",
                    "defaults": catalog_config_defaults_json(),
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
        .expect_err("extra authorization receipt context fields should fail before delivery");

    let message = err.to_string();
    assert!(message.contains("catalog.config-read"));
    assert!(
        message.contains(
            "catalog config-read authorization receipt context contains unexpected field unverified-context-claim"
        ),
        "catalog config-read error should reject extra receipt context fields: {message}"
    );
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains(event_id));
    assert!(
        store.delivered.lock().await.is_empty(),
        "receipt context schema failures must fail before acknowledgement"
    );
    assert!(
        graph.events.lock().await.is_empty(),
        "receipt context schema failures must fail before graph projection"
    );
    assert!(
        lineage.events.lock().await.is_empty(),
        "receipt context schema failures must fail before lineage projection"
    );
}

#[tokio::test]
async fn outbox_drain_rejects_extra_authorization_receipt_principal_fields() {
    let event_id = "evt-config-extra-auth-principal-field";
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: event_id.to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "catalog.config-read".to_string(),
            payload: json!({
                "audit-event-id": format!("audit-{event_id}"),
                "event-type": "catalog.config-read",
                "payload": {
                    "authorization-receipt": {
                        "principal": {
                            "subject": "agent:reader",
                            "kind": "agent",
                            "unverified-principal-claim": "admin",
                        },
                        "action": "catalog-config",
                        "allowed": true,
                        "engine": "test",
                        "policy_hash": null,
                        "checked_at": chrono::Utc::now(),
                    },
                    "warehouse": "local",
                    "defaults": catalog_config_defaults_json(),
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
        .expect_err("extra authorization receipt principal fields should fail before delivery");

    let message = err.to_string();
    assert!(message.contains("catalog.config-read"));
    assert!(
        message.contains(
            "catalog config-read authorization receipt principal contains unexpected field unverified-principal-claim"
        ),
        "catalog config-read error should reject extra receipt principal fields: {message}"
    );
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains(event_id));
    assert!(
        store.delivered.lock().await.is_empty(),
        "receipt principal schema failures must fail before acknowledgement"
    );
    assert!(
        graph.events.lock().await.is_empty(),
        "receipt principal schema failures must fail before graph projection"
    );
    assert!(
        lineage.events.lock().await.is_empty(),
        "receipt principal schema failures must fail before lineage projection"
    );
}

#[tokio::test]
async fn outbox_drain_rejects_missing_or_blank_standard_catalog_receipt_engine() {
    let principal = Principal::new("agent:reader", PrincipalKind::Agent).unwrap();
    let base_receipt = json!({
        "principal": principal,
        "action": "catalog-config",
        "allowed": true,
        "engine": "test",
        "policy_hash": null,
        "checked_at": chrono::Utc::now(),
    });
    let mut missing_engine_receipt = base_receipt.clone();
    missing_engine_receipt
        .as_object_mut()
        .unwrap()
        .remove("engine");
    let mut blank_engine_receipt = base_receipt;
    blank_engine_receipt["engine"] = json!(" ");

    for (event_id, receipt, expected_message) in [
        (
            "evt-config-missing-receipt-engine",
            missing_engine_receipt,
            "catalog config-read evidence must contain authorization receipt engine",
        ),
        (
            "evt-config-blank-receipt-engine",
            blank_engine_receipt,
            "catalog config-read authorization receipt engine must be non-empty",
        ),
    ] {
        let store = Arc::new(RecordingOutboxStore {
            events: Mutex::new(vec![OutboxEvent {
                event_id: event_id.to_string(),
                sink: "lakecat.lineage-and-graph".to_string(),
                event_type: "catalog.config-read".to_string(),
                payload: json!({
                    "audit-event-id": format!("audit-{event_id}"),
                    "event-type": "catalog.config-read",
                    "payload": {
                        "authorization-receipt": receipt,
                        "warehouse": "local",
                        "defaults": catalog_config_defaults_json(),
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
            .expect_err("missing or blank receipt engine should fail before delivery");

        let message = err.to_string();
        assert!(message.contains("catalog.config-read"));
        assert!(
            message.contains(expected_message),
            "catalog config-read error should describe receipt engine failure: {message}"
        );
        assert!(message.contains("event-id-hash=sha256:"));
        assert!(!message.contains(event_id));
        assert!(
            store.delivered.lock().await.is_empty(),
            "receipt engine failures must fail before acknowledgement"
        );
        assert!(
            graph.events.lock().await.is_empty(),
            "receipt engine failures must fail before graph projection"
        );
        assert!(
            lineage.events.lock().await.is_empty(),
            "receipt engine failures must fail before lineage projection"
        );
    }
}

#[tokio::test]
async fn outbox_drain_rejects_missing_or_denied_standard_catalog_receipt_allowed_decision() {
    let principal = Principal::new("agent:reader", PrincipalKind::Agent).unwrap();
    let receipt = |action: &str| {
        json!({
            "principal": &principal,
            "action": action,
            "allowed": true,
            "engine": "test",
            "policy_hash": null,
            "checked_at": chrono::Utc::now(),
        })
    };
    let missing_allowed = |mut receipt: Value| {
        receipt.as_object_mut().unwrap().remove("allowed");
        receipt
    };
    let denied = |mut receipt: Value| {
        receipt["allowed"] = json!(false);
        receipt
    };

    for (event_type, event_id, payload, expected_message) in [
        (
            "catalog.config-read",
            "evt-config-missing-receipt-allowed",
            json!({
                "authorization-receipt": missing_allowed(receipt("catalog-config")),
                "warehouse": "local",
                "defaults": catalog_config_defaults_json(),
            }),
            "catalog config-read evidence must contain authorization receipt allowed decision",
        ),
        (
            "catalog.config-read",
            "evt-config-denied-receipt",
            json!({
                "authorization-receipt": denied(receipt("catalog-config")),
                "warehouse": "local",
                "defaults": catalog_config_defaults_json(),
            }),
            "catalog config-read authorization receipt must allow replay projection",
        ),
        (
            "namespace.listed",
            "evt-namespace-list-missing-receipt-allowed",
            json!({
                "authorization-receipt": missing_allowed(receipt("namespace-list")),
                "warehouse": "local",
                "namespace-count": 1,
                "namespace-paths": ["default"],
            }),
            "namespace list evidence must contain authorization receipt allowed decision",
        ),
        (
            "namespace.created",
            "evt-namespace-created-denied-receipt",
            json!({
                "authorization-receipt": denied(receipt("namespace-create")),
                "warehouse": "local",
                "namespace": ["default"],
            }),
            "namespace lifecycle authorization receipt must allow replay projection",
        ),
        (
            "namespace.loaded",
            "evt-namespace-loaded-missing-receipt-allowed",
            json!({
                "authorization-receipt": missing_allowed(receipt("namespace-load")),
                "warehouse": "local",
                "namespace": ["default"],
            }),
            "namespace lifecycle evidence must contain authorization receipt allowed decision",
        ),
        (
            "namespace.dropped",
            "evt-namespace-dropped-denied-receipt",
            json!({
                "authorization-receipt": denied(receipt("namespace-drop")),
                "warehouse": "local",
                "namespace": ["archived"],
            }),
            "namespace lifecycle authorization receipt must allow replay projection",
        ),
        (
            "view.listed",
            "evt-view-list-missing-receipt-allowed",
            json!({
                "authorization-receipt": missing_allowed(receipt("view-load")),
                "warehouse": "local",
                "namespace": ["default"],
                "view-count": 1,
                "view-names": ["active_customers"],
            }),
            "view list evidence must contain authorization receipt allowed decision",
        ),
        (
            "view.upserted",
            "evt-view-upserted-denied-receipt",
            json!({
                "authorization-receipt": denied(receipt("view-manage")),
                "view": {
                    "warehouse": "local",
                    "namespace": ["default"],
                    "name": "active_customers",
                    "view-version": 1,
                }
            }),
            "view lifecycle authorization receipt must allow replay projection",
        ),
        (
            "view.loaded",
            "evt-view-loaded-missing-receipt-allowed",
            json!({
                "authorization-receipt": missing_allowed(receipt("view-load")),
                "view": {
                    "warehouse": "local",
                    "namespace": ["default"],
                    "name": "active_customers",
                    "view-version": 1,
                }
            }),
            "view lifecycle evidence must contain authorization receipt allowed decision",
        ),
        (
            "view.dropped",
            "evt-view-dropped-denied-receipt",
            json!({
                "authorization-receipt": denied(receipt("view-drop")),
                "view": {
                    "warehouse": "local",
                    "namespace": ["default"],
                    "name": "active_customers",
                    "view-version": 1,
                }
            }),
            "view lifecycle authorization receipt must allow replay projection",
        ),
    ] {
        let store = Arc::new(RecordingOutboxStore {
            events: Mutex::new(vec![OutboxEvent {
                event_id: event_id.to_string(),
                sink: "lakecat.lineage-and-graph".to_string(),
                event_type: event_type.to_string(),
                payload: json!({
                    "audit-event-id": format!("audit-{event_id}"),
                    "event-type": event_type,
                    "payload": payload
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
            .expect_err("missing or denied receipt decision should fail before delivery");

        let message = err.to_string();
        assert!(message.contains(event_type), "{message}");
        assert!(
            message.contains(expected_message),
            "{event_type} error should describe receipt decision failure: {message}"
        );
        assert!(message.contains("event-id-hash=sha256:"));
        assert!(!message.contains(event_id));
        assert!(
            store.delivered.lock().await.is_empty(),
            "receipt decision failures must fail before acknowledgement"
        );
        assert!(
            graph.events.lock().await.is_empty(),
            "receipt decision failures must fail before graph projection"
        );
        assert!(
            lineage.events.lock().await.is_empty(),
            "receipt decision failures must fail before lineage projection"
        );
    }
}

#[tokio::test]
async fn outbox_drain_rejects_invalid_standard_catalog_receipt_actions() {
    let principal = Principal::new("agent:reader", PrincipalKind::Agent).unwrap();
    let base_receipt = json!({
        "principal": principal,
        "action": "catalog-config",
        "allowed": true,
        "engine": "test",
        "policy_hash": null,
        "checked_at": chrono::Utc::now(),
    });
    let mut missing_action_receipt = base_receipt.clone();
    missing_action_receipt
        .as_object_mut()
        .unwrap()
        .remove("action");
    let mut blank_action_receipt = base_receipt.clone();
    blank_action_receipt["action"] = json!(" ");
    let mut unknown_action_receipt = base_receipt;
    unknown_action_receipt["action"] = json!("catalog-administer-everything");
    let mismatched_action_receipt = json!({
        "principal": principal,
        "action": "namespace-list",
        "allowed": true,
        "engine": "test",
        "policy_hash": null,
        "checked_at": chrono::Utc::now(),
    });

    for (event_id, receipt, expected_message) in [
        (
            "evt-config-missing-receipt-action",
            missing_action_receipt,
            "catalog config-read evidence must contain authorization receipt action",
        ),
        (
            "evt-config-blank-receipt-action",
            blank_action_receipt,
            "catalog config-read authorization receipt action must be non-empty",
        ),
        (
            "evt-config-unknown-receipt-action",
            unknown_action_receipt,
            "catalog config-read authorization receipt action must be a known catalog action",
        ),
        (
            "evt-config-mismatched-receipt-action",
            mismatched_action_receipt,
            "catalog config-read authorization receipt action does not match outbox event type",
        ),
    ] {
        let store = Arc::new(RecordingOutboxStore {
            events: Mutex::new(vec![OutboxEvent {
                event_id: event_id.to_string(),
                sink: "lakecat.lineage-and-graph".to_string(),
                event_type: "catalog.config-read".to_string(),
                payload: json!({
                    "audit-event-id": format!("audit-{event_id}"),
                    "event-type": "catalog.config-read",
                    "payload": {
                        "authorization-receipt": receipt,
                        "warehouse": "local",
                        "defaults": catalog_config_defaults_json(),
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
            .expect_err("missing or blank receipt action should fail before delivery");

        let message = err.to_string();
        assert!(message.contains("catalog.config-read"));
        assert!(
            message.contains(expected_message),
            "catalog config-read error should describe receipt action failure: {message}"
        );
        assert!(message.contains("event-id-hash=sha256:"));
        assert!(!message.contains(event_id));
        assert!(
            store.delivered.lock().await.is_empty(),
            "receipt action failures must fail before acknowledgement"
        );
        assert!(
            graph.events.lock().await.is_empty(),
            "receipt action failures must fail before graph projection"
        );
        assert!(
            lineage.events.lock().await.is_empty(),
            "receipt action failures must fail before lineage projection"
        );
    }
}

#[tokio::test]
async fn outbox_drain_rejects_missing_standard_catalog_receipt_checked_at() {
    let principal = Principal::new("agent:reader", PrincipalKind::Agent).unwrap();
    let mut receipt = json!({
        "principal": principal,
        "action": "catalog-config",
        "allowed": true,
        "engine": "test",
        "policy_hash": null,
        "checked_at": chrono::Utc::now(),
    });
    receipt.as_object_mut().unwrap().remove("checked_at");
    let event_id = "evt-config-missing-receipt-checked-at";
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: event_id.to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "catalog.config-read".to_string(),
            payload: json!({
                "audit-event-id": format!("audit-{event_id}"),
                "event-type": "catalog.config-read",
                "payload": {
                    "authorization-receipt": receipt,
                    "warehouse": "local",
                    "defaults": catalog_config_defaults_json(),
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
        .expect_err("missing receipt checked_at should fail before delivery");

    let message = err.to_string();
    assert!(message.contains("catalog.config-read"));
    assert!(message.contains(
        "catalog config-read evidence must contain authorization receipt checked_at timestamp"
    ));
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains(event_id));
    assert!(
        store.delivered.lock().await.is_empty(),
        "missing receipt timestamp must fail before acknowledgement"
    );
    assert!(
        graph.events.lock().await.is_empty(),
        "missing receipt timestamp must fail before graph projection"
    );
    assert!(
        lineage.events.lock().await.is_empty(),
        "missing receipt timestamp must fail before lineage projection"
    );
}

#[tokio::test]
async fn outbox_drain_rejects_extra_config_and_bootstrap_outbox_payload_fields() {
    let principal = Principal::new("agent:reader", PrincipalKind::Agent).unwrap();
    let cases = vec![
        (
            "catalog.config-read",
            "evt-config-extra-wrapper-field",
            "catalog config-read outbox payload contains unexpected field unverified-wrapper-claim",
            json!({
                "audit-event-id": "audit-config-extra-wrapper-field",
                "event-type": "catalog.config-read",
                "payload": {
                    "authorization-receipt": {
                        "principal": principal.clone(),
                        "action": "catalog-config",
                        "allowed": true,
                        "engine": "test",
                        "policy_hash": null,
                        "checked_at": chrono::Utc::now(),
                    },
                    "warehouse": "local",
                    "defaults": catalog_config_defaults_json(),
                    "endpoints": catalog_config_endpoints_json()
                },
                "unverified-wrapper-claim": "shadow"
            }),
        ),
        (
            "querygraph.bootstrap",
            "evt-bootstrap-extra-wrapper-field",
            "querygraph bootstrap outbox payload contains unexpected field unverified-wrapper-claim",
            {
                let mut payload = valid_querygraph_bootstrap_payload(principal);
                payload
                    .as_object_mut()
                    .expect("valid bootstrap wrapper should be an object")
                    .insert("unverified-wrapper-claim".to_string(), json!("shadow"));
                payload
            },
        ),
    ];

    for (event_type, event_id, expected_message, payload) in cases {
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
            .expect_err("extra config/bootstrap wrapper fields should fail before delivery");

        let message = err.to_string();
        assert!(message.contains(event_type), "{message}");
        assert!(message.contains("event-id-hash=sha256:"));
        assert!(message.contains(expected_message), "{message}");
        assert!(!message.contains(event_id));
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
async fn outbox_drain_rejects_mismatched_table_soft_delete_evidence() {
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
            event_id: "evt-secret-delete-token".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "table.deleted".to_string(),
            payload: json!({
                "audit-event-id": "audit-corrupt-delete",
                "event-type": "table.deleted",
                "table": table,
                "soft-delete": {
                    "table": other_table,
                    "metadata-location": "file:///tmp/events/metadata/00000.json",
                    "version": 1,
                    "principal": principal,
                    "authorization-receipt": null,
                    "deleted-at": chrono::Utc::now(),
                },
                "authorization-receipt": {
                    "principal": principal,
                    "action": "table-drop",
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
        .expect_err("mismatched soft-delete evidence should fail");

    let message = err.to_string();
    assert!(message.contains("outbox event table.deleted (lakecat.lineage-and-graph) has invalid"));
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(message.contains("table lifecycle soft-delete table does not match table identity"));
    assert!(!message.contains("evt-secret-delete-token"));
    assert!(
        store.delivered.lock().await.is_empty(),
        "mismatched table soft-delete evidence must fail before acknowledgement"
    );
    assert!(
        graph.events.lock().await.is_empty(),
        "mismatched table soft-delete evidence must fail before graph projection"
    );
    assert!(
        lineage.events.lock().await.is_empty(),
        "mismatched table soft-delete evidence must fail before lineage projection"
    );
}

#[tokio::test]
async fn outbox_drain_hashes_malformed_table_decode_errors() {
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-secret-table-token".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "table.created".to_string(),
            payload: json!({
                "audit-event-id": "audit-corrupt-table",
                "event-type": "table.created",
                "table": "not-a-table-identity",
                "payload": {
                    "authorization-receipt": {
                        "principal": {
                            "subject": "agent:writer",
                            "kind": "agent"
                        },
                        "action": "table-create",
                        "allowed": true,
                        "engine": "test",
                        "policy_hash": null,
                        "checked_at": chrono::Utc::now(),
                    },
                }
            }),
            created_at: chrono::Utc::now(),
            delivered_at: None,
        }]),
        delivered: Mutex::default(),
    });
    let state = LakeCatState::new(WarehouseName::new("local").unwrap(), store.clone());

    let err = drain_outbox_once(&state, 10)
        .await
        .expect_err("malformed table identity should fail the drain");

    let message = err.to_string();
    assert!(message.contains("outbox event table.created (lakecat.lineage-and-graph) has invalid"));
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(message.contains("table lifecycle evidence has invalid table identity"));
    assert!(!message.contains("evt-secret-table-token"));
    assert!(store.delivered.lock().await.is_empty());
}

#[tokio::test]
async fn outbox_drain_hashes_malformed_principal_admission_errors() {
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-secret-principal-token".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "catalog.config-read".to_string(),
            payload: json!({
                "audit-event-id": "audit-corrupt-principal",
                "event-type": "catalog.config-read",
                "payload": {
                    "warehouse": "local",
                    "authorization-receipt": {
                        "principal": "not-a-principal",
                        "action": "catalog-config",
                        "allowed": true,
                        "engine": "test",
                        "policy_hash": null,
                        "checked_at": chrono::Utc::now(),
                    },
                    "defaults": catalog_config_defaults_json(),
                }
            }),
            created_at: chrono::Utc::now(),
            delivered_at: None,
        }]),
        delivered: Mutex::default(),
    });
    let state = LakeCatState::new(WarehouseName::new("local").unwrap(), store.clone());

    let err = drain_outbox_once(&state, 10)
        .await
        .expect_err("malformed principal identity should fail the drain");

    let message = err.to_string();
    assert!(
        message
            .contains("outbox event catalog.config-read (lakecat.lineage-and-graph) has invalid")
    );
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(message.contains("catalog config-read authorization receipt principal"));
    assert!(message.contains("must be a valid principal"));
    assert!(!message.contains("evt-secret-principal-token"));
    assert!(store.delivered.lock().await.is_empty());
}
