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
async fn outbox_drain_rejects_extra_scan_payload_fields() {
    let cases = [
        (
            "table.scan-planned",
            "scan-planned",
            "evt-scan-plan-extra-field",
            "unverified-scan-plan-claim",
        ),
        (
            "table.scan-tasks-fetched",
            "scan-tasks-fetched",
            "evt-scan-fetch-extra-field",
            "unverified-scan-fetch-claim",
        ),
    ];

    for (event_type, label, event_id, extra_field) in cases {
        let principal = Principal::new("agent:reader", PrincipalKind::Agent).unwrap();
        let ident = table_ident("local", "default", "events").unwrap();
        let policy_hash =
            content_hash_json(&json!({"policy-id": "agent-read", "scope": "default.events"}))
                .unwrap();
        let read_restriction = json!({
            "allowed-columns": ["event_id"],
            "row-predicate": {
                "type": "not-eq",
                "term": "severity",
                "value": "debug"
            },
            "purpose": "qglake-agent-demo",
            "max-credential-ttl-seconds": 300,
            "policy-hashes": [policy_hash]
        });
        let mut payload = if event_type == "table.scan-planned" {
            json!({
                "event-type": event_type,
                "table": ident,
                "authorization-receipt": {
                    "principal": principal,
                    "action": "table-plan-scan",
                    "allowed": true,
                    "engine": "test",
                    "policy_hash": null,
                    "context": {
                        "read-restriction": read_restriction
                    },
                    "checked_at": chrono::Utc::now(),
                },
                "planned-by": "test",
                "snapshot-id": 1,
                "read-restriction": read_restriction,
                "requested-projection": ["event_id"],
                "effective-projection": ["event_id"],
                "requested-stats-fields": ["event_id"],
                "effective-stats-fields": ["event_id"],
                "scan-task-count": 1,
                "storage-location": "s3://bucket/events",
                "metadata-location": "s3://bucket/events/metadata/v1.json"
            })
        } else {
            json!({
                "event-type": event_type,
                "table": ident,
                "authorization-receipt": {
                    "principal": principal,
                    "action": "table-plan-scan",
                    "allowed": true,
                    "engine": "test",
                    "policy_hash": null,
                    "context": {
                        "read-restriction": read_restriction
                    },
                    "checked_at": chrono::Utc::now(),
                },
                "planned-by": "test",
                "snapshot-id": 1,
                "plan-task": "lakecat:plan:task-1",
                "read-restriction": read_restriction,
                "required-projection": ["event_id"],
                "effective-projection": ["event_id"],
                "requested-stats-fields": ["event_id"],
                "effective-stats-fields": ["event_id"],
                "stats-fields": ["event_id"],
                "required-filters": [{
                    "type": "not-eq",
                    "term": "severity",
                    "value": "debug"
                }],
                "file-scan-task-count": 1,
                "delete-file-count": 0,
                "child-plan-task-count": 0,
                "storage-location": "s3://bucket/events",
                "metadata-location": "s3://bucket/events/metadata/v1.json"
            })
        };
        payload[extra_field] = json!("shadow");
        let store = Arc::new(RecordingOutboxStore {
            events: Mutex::new(vec![OutboxEvent {
                event_id: event_id.to_string(),
                sink: "lakecat.lineage-and-graph".to_string(),
                event_type: event_type.to_string(),
                payload: json!({
                    "audit-event-id": format!("audit-{event_id}"),
                    "event-type": event_type,
                    "table": ident,
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
            .expect_err("extra scan payload fields should fail before delivery");

        let message = err.to_string();
        assert!(message.contains(event_type));
        assert!(
            message.contains(&format!("{label} contains unexpected field {extra_field}")),
            "{event_type} should reject extra scan payload field: {message}"
        );
        assert!(message.contains("event-id-hash=sha256:"));
        assert!(!message.contains(event_id));
        assert!(store.delivered.lock().await.is_empty());
        assert!(graph.events.lock().await.is_empty());
        assert!(lineage.events.lock().await.is_empty());
    }
}

#[tokio::test]
async fn outbox_drain_rejects_malformed_scan_fetch_plan_task_evidence() {
    let principal = Principal::new("agent:reader", PrincipalKind::Agent).unwrap();
    let ident = table_ident("local", "default", "events").unwrap();
    let policy_hash =
        content_hash_json(&json!({"policy-id": "agent-read", "scope": "default.events"})).unwrap();
    let read_restriction = json!({
        "allowed-columns": ["event_id"],
        "row-predicate": {
            "type": "not-eq",
            "term": "severity",
            "value": "debug"
        },
        "purpose": "qglake-agent-demo",
        "max-credential-ttl-seconds": 300,
        "policy-hashes": [policy_hash]
    });
    let cases = [
        (
            "evt-scan-fetch-foreign-plan-task",
            "foreign:plan:abc",
            "scan-tasks-fetched plan-task must be LakeCat-issued evidence",
        ),
        (
            "evt-scan-fetch-decorated-plan-task",
            "lakecat:plan:abc?token=raw-secret",
            "scan-tasks-fetched plan-task must not contain decorated location material",
        ),
        (
            "evt-scan-fetch-credential-plan-task",
            "lakecat:plan:abc:session_token=raw-secret",
            "scan-tasks-fetched plan-task must not contain credential material",
        ),
    ];

    for (event_id, plan_task, expected_message) in cases {
        let payload = json!({
            "event-type": "table.scan-tasks-fetched",
            "table": ident,
            "authorization-receipt": {
                "principal": principal,
                "action": "table-plan-scan",
                "allowed": true,
                "engine": "test",
                "policy_hash": null,
                "context": {
                    "read-restriction": read_restriction
                },
                "checked_at": chrono::Utc::now(),
            },
            "planned-by": "test",
            "snapshot-id": 1,
            "plan-task": plan_task,
            "read-restriction": read_restriction,
            "required-projection": ["event_id"],
            "effective-projection": ["event_id"],
            "requested-stats-fields": ["event_id"],
            "effective-stats-fields": ["event_id"],
            "stats-fields": ["event_id"],
            "required-filters": [{
                "type": "not-eq",
                "term": "severity",
                "value": "debug"
            }],
            "file-scan-task-count": 1,
            "delete-file-count": 0,
            "child-plan-task-count": 0,
            "storage-location": "s3://bucket/events",
            "metadata-location": "s3://bucket/events/metadata/v1.json"
        });
        let store = Arc::new(RecordingOutboxStore {
            events: Mutex::new(vec![OutboxEvent {
                event_id: event_id.to_string(),
                sink: "lakecat.lineage-and-graph".to_string(),
                event_type: "table.scan-tasks-fetched".to_string(),
                payload: json!({
                    "audit-event-id": format!("audit-{event_id}"),
                    "event-type": "table.scan-tasks-fetched",
                    "table": ident,
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
            .expect_err("malformed plan-task evidence should fail before delivery");

        let message = err.to_string();
        assert!(message.contains("table.scan-tasks-fetched"));
        assert!(message.contains(expected_message), "{message}");
        assert!(message.contains("event-id-hash=sha256:"));
        assert!(!message.contains(event_id));
        assert!(
            !message.contains(plan_task),
            "operator-facing errors must not expose raw plan-task material"
        );
        assert!(!message.contains("raw-secret"));
        assert!(store.delivered.lock().await.is_empty());
        assert!(graph.events.lock().await.is_empty());
        assert!(lineage.events.lock().await.is_empty());
    }
}

#[tokio::test]
async fn outbox_drain_rejects_malformed_scan_planned_plan_task_evidence() {
    let principal = Principal::new("agent:reader", PrincipalKind::Agent).unwrap();
    let ident = table_ident("local", "default", "events").unwrap();
    let policy_hash =
        content_hash_json(&json!({"policy-id": "agent-read", "scope": "default.events"})).unwrap();
    let read_restriction = json!({
        "allowed-columns": ["event_id"],
        "row-predicate": {
            "type": "not-eq",
            "term": "severity",
            "value": "debug"
        },
        "purpose": "qglake-agent-demo",
        "max-credential-ttl-seconds": 300,
        "policy-hashes": [policy_hash]
    });
    let cases = [
        (
            "evt-scan-plan-foreign-plan-task",
            "foreign:plan:abc",
            "scan-planned plan-task must be LakeCat-issued evidence",
        ),
        (
            "evt-scan-plan-decorated-plan-task",
            "lakecat:plan:abc?token=raw-secret",
            "scan-planned plan-task must not contain decorated location material",
        ),
        (
            "evt-scan-plan-credential-plan-task",
            "lakecat:plan:abc:session_token=raw-secret",
            "scan-planned plan-task must not contain credential material",
        ),
    ];

    for (event_id, plan_task, expected_message) in cases {
        let payload = json!({
            "event-type": "table.scan-planned",
            "table": ident,
            "authorization-receipt": {
                "principal": principal,
                "action": "table-plan-scan",
                "allowed": true,
                "engine": "test",
                "policy_hash": null,
                "context": {
                    "read-restriction": read_restriction
                },
                "checked_at": chrono::Utc::now(),
            },
            "planned-by": "test",
            "snapshot-id": 1,
            "plan-task": plan_task,
            "read-restriction": read_restriction,
            "requested-projection": ["event_id"],
            "effective-projection": ["event_id"],
            "requested-stats-fields": ["event_id"],
            "effective-stats-fields": ["event_id"],
            "required-filters": [{
                "type": "not-eq",
                "term": "severity",
                "value": "debug"
            }],
            "scan-task-count": 1,
            "storage-location": "s3://bucket/events",
            "metadata-location": "s3://bucket/events/metadata/v1.json"
        });
        let store = Arc::new(RecordingOutboxStore {
            events: Mutex::new(vec![OutboxEvent {
                event_id: event_id.to_string(),
                sink: "lakecat.lineage-and-graph".to_string(),
                event_type: "table.scan-planned".to_string(),
                payload: json!({
                    "audit-event-id": format!("audit-{event_id}"),
                    "event-type": "table.scan-planned",
                    "table": ident,
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
            .expect_err("malformed planned plan-task evidence should fail before delivery");

        let message = err.to_string();
        assert!(message.contains("table.scan-planned"));
        assert!(message.contains(expected_message), "{message}");
        assert!(message.contains("event-id-hash=sha256:"));
        assert!(!message.contains(event_id));
        assert!(
            !message.contains(plan_task),
            "operator-facing errors must not expose raw plan-task material"
        );
        assert!(!message.contains("raw-secret"));
        assert!(store.delivered.lock().await.is_empty());
        assert!(graph.events.lock().await.is_empty());
        assert!(lineage.events.lock().await.is_empty());
    }
}

#[tokio::test]
async fn outbox_drain_rejects_malformed_scan_fetch_stats_fields_evidence() {
    let principal = Principal::new("agent:reader", PrincipalKind::Agent).unwrap();
    let ident = table_ident("local", "default", "events").unwrap();
    let policy_hash =
        content_hash_json(&json!({"policy-id": "agent-read", "scope": "default.events"})).unwrap();
    let read_restriction = json!({
        "allowed-columns": ["event_id", "payload"],
        "row-predicate": {
            "type": "not-eq",
            "term": "severity",
            "value": "debug"
        },
        "purpose": "qglake-agent-demo",
        "max-credential-ttl-seconds": 300,
        "policy-hashes": [policy_hash]
    });
    let cases = [
        (
            "evt-scan-fetch-empty-stats-fields",
            json!(["event_id"]),
            json!(["event_id"]),
            json!([]),
            "scan-tasks-fetched stats-fields must not be empty",
        ),
        (
            "evt-scan-fetch-duplicate-stats-fields",
            json!(["event_id"]),
            json!(["event_id"]),
            json!(["event_id", "event_id"]),
            "scan-tasks-fetched stats-fields must be duplicate-free",
        ),
        (
            "evt-scan-fetch-drifted-stats-fields",
            json!(["event_id"]),
            json!(["event_id"]),
            json!(["payload"]),
            "scan-tasks-fetched stats-fields must be a subset of effective-stats-fields",
        ),
        (
            "evt-scan-fetch-narrowed-stats-fields",
            json!(["event_id", "payload"]),
            json!(["event_id", "payload"]),
            json!(["event_id"]),
            "scan-tasks-fetched stats-fields must match effective-stats-fields",
        ),
    ];

    for (
        event_id,
        requested_stats_fields,
        effective_stats_fields,
        stats_fields,
        expected_message,
    ) in cases
    {
        let payload = json!({
            "event-type": "table.scan-tasks-fetched",
            "table": ident,
            "authorization-receipt": {
                "principal": principal,
                "action": "table-plan-scan",
                "allowed": true,
                "engine": "test",
                "policy_hash": null,
                "context": {
                    "read-restriction": read_restriction
                },
                "checked_at": chrono::Utc::now(),
            },
            "planned-by": "test",
            "snapshot-id": 1,
            "plan-task": "lakecat:plan:abc",
            "read-restriction": read_restriction,
            "required-projection": ["event_id"],
            "effective-projection": ["event_id"],
            "requested-stats-fields": requested_stats_fields,
            "effective-stats-fields": effective_stats_fields,
            "stats-fields": stats_fields,
            "required-filters": [{
                "type": "not-eq",
                "term": "severity",
                "value": "debug"
            }],
            "file-scan-task-count": 1,
            "delete-file-count": 0,
            "child-plan-task-count": 0,
            "storage-location": "s3://bucket/events",
            "metadata-location": "s3://bucket/events/metadata/v1.json"
        });
        let store = Arc::new(RecordingOutboxStore {
            events: Mutex::new(vec![OutboxEvent {
                event_id: event_id.to_string(),
                sink: "lakecat.lineage-and-graph".to_string(),
                event_type: "table.scan-tasks-fetched".to_string(),
                payload: json!({
                    "audit-event-id": format!("audit-{event_id}"),
                    "event-type": "table.scan-tasks-fetched",
                    "table": ident,
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
            .expect_err("malformed stats-fields evidence should fail before delivery");

        let message = err.to_string();
        assert!(message.contains("table.scan-tasks-fetched"));
        assert!(message.contains(expected_message), "{message}");
        assert!(message.contains("event-id-hash=sha256:"));
        assert!(!message.contains(event_id));
        assert!(store.delivered.lock().await.is_empty());
        assert!(graph.events.lock().await.is_empty());
        assert!(lineage.events.lock().await.is_empty());
    }
}

#[tokio::test]
async fn outbox_drain_rejects_mismatched_scan_receipt_actions() {
    let cases = [
        (
            "table.scan-planned",
            "scan-planned",
            "evt-scan-plan-action-drift",
            "audit-scan-plan-action-drift",
        ),
        (
            "table.scan-tasks-fetched",
            "scan-tasks-fetched",
            "evt-scan-fetch-action-drift",
            "audit-scan-fetch-action-drift",
        ),
    ];

    for (event_type, label, event_id, audit_event_id) in cases {
        let principal = Principal::new("agent:reader", PrincipalKind::Agent).unwrap();
        let ident = table_ident("local", "default", "events").unwrap();
        let policy_hash =
            content_hash_json(&json!({"policy-id": "agent-read", "scope": "default.events"}))
                .unwrap();
        let read_restriction = json!({
            "allowed-columns": ["event_id"],
            "row-predicate": {
                "type": "not-eq",
                "term": "severity",
                "value": "debug"
            },
            "purpose": "qglake-agent-demo",
            "max-credential-ttl-seconds": 300,
            "policy-hashes": [policy_hash]
        });
        let payload = if event_type == "table.scan-planned" {
            json!({
                "table": ident,
                "authorization-receipt": {
                    "principal": principal,
                    "action": "table-load",
                    "allowed": true,
                    "engine": "test",
                    "policy_hash": null,
                    "context": {
                        "read-restriction": read_restriction
                    },
                    "checked_at": chrono::Utc::now(),
                },
                "read-restriction": read_restriction,
                "requested-projection": ["event_id"],
                "effective-projection": ["event_id"],
                "requested-stats-fields": ["event_id"],
                "effective-stats-fields": ["event_id"],
                "scan-task-count": 1
            })
        } else {
            json!({
                "table": ident,
                "authorization-receipt": {
                    "principal": principal,
                    "action": "table-load",
                    "allowed": true,
                    "engine": "test",
                    "policy_hash": null,
                    "context": {
                        "read-restriction": read_restriction
                    },
                    "checked_at": chrono::Utc::now(),
                },
                "read-restriction": read_restriction,
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
                "delete-file-count": 0,
                "child-plan-task-count": 0
            })
        };
        let store = Arc::new(RecordingOutboxStore {
            events: Mutex::new(vec![OutboxEvent {
                event_id: event_id.to_string(),
                sink: "lakecat.lineage-and-graph".to_string(),
                event_type: event_type.to_string(),
                payload: json!({
                    "audit-event-id": audit_event_id,
                    "event-type": event_type,
                    "table": ident,
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
            .expect_err("mismatched scan receipt action should fail");

        let message = err.to_string();
        assert!(message.contains(event_type));
        assert!(message.contains(&format!(
            "{label} authorization receipt action does not match outbox event type"
        )));
        assert!(message.contains("event-id-hash=sha256:"));
        assert!(!message.contains(event_id));
        assert!(
            store.delivered.lock().await.is_empty(),
            "{event_type} mismatched receipt action must fail before acknowledgement"
        );
        assert!(
            graph.events.lock().await.is_empty(),
            "{event_type} mismatched receipt action must fail before graph projection"
        );
        assert!(
            lineage.events.lock().await.is_empty(),
            "{event_type} mismatched receipt action must fail before lineage projection"
        );
    }
}

#[tokio::test]
async fn outbox_drain_rejects_incomplete_scan_authorization_receipts() {
    let cases = [
        (
            "table.scan-planned",
            "scan-planned",
            "missing-action",
            "evidence must contain authorization receipt action",
        ),
        (
            "table.scan-planned",
            "scan-planned",
            "missing-allowed",
            "evidence must contain authorization receipt allowed decision",
        ),
        (
            "table.scan-planned",
            "scan-planned",
            "missing-engine",
            "evidence must contain authorization receipt engine",
        ),
        (
            "table.scan-planned",
            "scan-planned",
            "missing-checked-at",
            "evidence must contain authorization receipt checked_at timestamp",
        ),
        (
            "table.scan-planned",
            "scan-planned",
            "denied",
            "authorization receipt must allow replay projection",
        ),
        (
            "table.scan-planned",
            "scan-planned",
            "blank-engine",
            "authorization receipt engine must be non-empty",
        ),
        (
            "table.scan-planned",
            "scan-planned",
            "malformed-checked-at",
            "authorization receipt checked_at timestamp must be RFC3339",
        ),
        (
            "table.scan-tasks-fetched",
            "scan-tasks-fetched",
            "missing-action",
            "evidence must contain authorization receipt action",
        ),
        (
            "table.scan-tasks-fetched",
            "scan-tasks-fetched",
            "missing-allowed",
            "evidence must contain authorization receipt allowed decision",
        ),
        (
            "table.scan-tasks-fetched",
            "scan-tasks-fetched",
            "missing-engine",
            "evidence must contain authorization receipt engine",
        ),
        (
            "table.scan-tasks-fetched",
            "scan-tasks-fetched",
            "missing-checked-at",
            "evidence must contain authorization receipt checked_at timestamp",
        ),
        (
            "table.scan-tasks-fetched",
            "scan-tasks-fetched",
            "denied",
            "authorization receipt must allow replay projection",
        ),
        (
            "table.scan-tasks-fetched",
            "scan-tasks-fetched",
            "blank-engine",
            "authorization receipt engine must be non-empty",
        ),
        (
            "table.scan-tasks-fetched",
            "scan-tasks-fetched",
            "malformed-checked-at",
            "authorization receipt checked_at timestamp must be RFC3339",
        ),
    ];

    for (event_type, label, invalid_case, expected_message) in cases {
        let principal = Principal::new("agent:reader", PrincipalKind::Agent).unwrap();
        let ident = table_ident("local", "default", "events").unwrap();
        let policy_hash =
            content_hash_json(&json!({"policy-id": "agent-read", "scope": "default.events"}))
                .unwrap();
        let read_restriction = json!({
            "allowed-columns": ["event_id"],
            "row-predicate": {
                "type": "not-eq",
                "term": "severity",
                "value": "debug"
            },
            "purpose": "qglake-agent-demo",
            "max-credential-ttl-seconds": 300,
            "policy-hashes": [policy_hash]
        });
        let mut payload = if event_type == "table.scan-planned" {
            json!({
                "table": ident.clone(),
                "authorization-receipt": {
                    "principal": principal,
                    "action": "table-plan-scan",
                    "allowed": true,
                    "engine": "test",
                    "policy_hash": null,
                    "context": {
                        "read-restriction": read_restriction
                    },
                    "checked_at": chrono::Utc::now(),
                },
                "read-restriction": read_restriction,
                "requested-projection": ["event_id"],
                "effective-projection": ["event_id"],
                "requested-stats-fields": ["event_id"],
                "effective-stats-fields": ["event_id"],
                "scan-task-count": 1
            })
        } else {
            json!({
                "table": ident.clone(),
                "authorization-receipt": {
                    "principal": principal,
                    "action": "table-plan-scan",
                    "allowed": true,
                    "engine": "test",
                    "policy_hash": null,
                    "context": {
                        "read-restriction": read_restriction
                    },
                    "checked_at": chrono::Utc::now(),
                },
                "read-restriction": read_restriction,
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
                "delete-file-count": 0,
                "child-plan-task-count": 0
            })
        };
        match invalid_case {
            "missing-action" => {
                payload["authorization-receipt"]
                    .as_object_mut()
                    .unwrap()
                    .remove("action");
            }
            "missing-allowed" => {
                payload["authorization-receipt"]
                    .as_object_mut()
                    .unwrap()
                    .remove("allowed");
            }
            "missing-engine" => {
                payload["authorization-receipt"]
                    .as_object_mut()
                    .unwrap()
                    .remove("engine");
            }
            "missing-checked-at" => {
                payload["authorization-receipt"]
                    .as_object_mut()
                    .unwrap()
                    .remove("checked_at");
            }
            "denied" => payload["authorization-receipt"]["allowed"] = json!(false),
            "blank-engine" => payload["authorization-receipt"]["engine"] = json!("   "),
            "malformed-checked-at" => {
                payload["authorization-receipt"]["checked_at"] = json!("not-a-timestamp")
            }
            _ => unreachable!("unexpected scan authorization receipt invalid case"),
        }
        let event_id = format!("evt-{label}-{invalid_case}");
        let store = Arc::new(RecordingOutboxStore {
            events: Mutex::new(vec![OutboxEvent {
                event_id: event_id.clone(),
                sink: "lakecat.lineage-and-graph".to_string(),
                event_type: event_type.to_string(),
                payload: json!({
                    "audit-event-id": format!("audit-{label}-{invalid_case}"),
                    "event-type": event_type,
                    "table": ident,
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
            .expect_err("incomplete scan authorization receipt should fail");

        let message = err.to_string();
        assert!(message.contains(event_type));
        assert!(message.contains(&format!("{label} {expected_message}")));
        assert!(message.contains("event-id-hash=sha256:"));
        assert!(!message.contains(&event_id));
        assert!(
            store.delivered.lock().await.is_empty(),
            "{event_type} {invalid_case} receipt must fail before acknowledgement"
        );
        assert!(
            graph.events.lock().await.is_empty(),
            "{event_type} {invalid_case} receipt must fail before graph projection"
        );
        assert!(
            lineage.events.lock().await.is_empty(),
            "{event_type} {invalid_case} receipt must fail before lineage projection"
        );
    }
}

#[tokio::test]
async fn outbox_drain_rejects_malformed_scan_location_evidence() {
    let cases = [
        (
            "table.scan-planned",
            "scan-planned",
            "storage-location",
            "blank-storage-location",
            json!(" "),
            "scan-planned storage-location must be a non-empty string",
        ),
        (
            "table.scan-planned",
            "scan-planned",
            "metadata-location",
            "decorated-metadata-location",
            json!("s3://lakecat-demo/events/metadata/00000.json?token=secret"),
            "scan-planned metadata-location must not contain decorated location material",
        ),
        (
            "table.scan-planned",
            "scan-planned",
            "storage-location",
            "credential-bearing-storage-location",
            json!("s3://lakecat-demo/events/session_token=secret"),
            "scan-planned storage-location must not contain credential material",
        ),
        (
            "table.scan-tasks-fetched",
            "scan-tasks-fetched",
            "storage-location",
            "blank-storage-location",
            json!(" "),
            "scan-tasks-fetched storage-location must be a non-empty string",
        ),
        (
            "table.scan-tasks-fetched",
            "scan-tasks-fetched",
            "metadata-location",
            "decorated-metadata-location",
            json!("s3://lakecat-demo/events/metadata/00000.json#secret"),
            "scan-tasks-fetched metadata-location must not contain decorated location material",
        ),
        (
            "table.scan-tasks-fetched",
            "scan-tasks-fetched",
            "storage-location",
            "credential-bearing-storage-location",
            json!("s3://lakecat-demo/events/access_key=secret"),
            "scan-tasks-fetched storage-location must not contain credential material",
        ),
    ];

    for (event_type, label, field, case, value, expected_message) in cases {
        let principal = Principal::new("agent:reader", PrincipalKind::Agent).unwrap();
        let ident = table_ident("local", "default", "events").unwrap();
        let policy_hash =
            content_hash_json(&json!({"policy-id": "agent-read", "scope": "default.events"}))
                .unwrap();
        let read_restriction = json!({
            "allowed-columns": ["event_id"],
            "row-predicate": {
                "type": "not-eq",
                "term": "severity",
                "value": "debug"
            },
            "purpose": "qglake-agent-demo",
            "max-credential-ttl-seconds": 300,
            "policy-hashes": [policy_hash]
        });
        let mut payload = if event_type == "table.scan-planned" {
            json!({
                "table": ident.clone(),
                "authorization-receipt": {
                    "principal": principal,
                    "action": "table-plan-scan",
                    "allowed": true,
                    "engine": "test",
                    "policy_hash": null,
                    "context": {
                        "read-restriction": read_restriction
                    },
                    "checked_at": chrono::Utc::now(),
                },
                "planned-by": "lakecat-sail",
                "snapshot-id": 42,
                "scan-task-count": 1,
                "storage-location": "s3://lakecat-demo/events",
                "metadata-location": "s3://lakecat-demo/events/metadata/00000.json",
                "read-restriction": read_restriction,
                "requested-projection": ["event_id"],
                "effective-projection": ["event_id"],
                "requested-stats-fields": ["event_id"],
                "effective-stats-fields": ["event_id"]
            })
        } else {
            json!({
                "table": ident.clone(),
                "authorization-receipt": {
                    "principal": principal,
                    "action": "table-plan-scan",
                    "allowed": true,
                    "engine": "test",
                    "policy_hash": null,
                    "context": {
                        "read-restriction": read_restriction
                    },
                    "checked_at": chrono::Utc::now(),
                },
                "planned-by": "lakecat-sail",
                "snapshot-id": 42,
                "plan-task": "lakecat:plan:abc",
                "file-scan-task-count": 1,
                "delete-file-count": 0,
                "child-plan-task-count": 0,
                "storage-location": "s3://lakecat-demo/events",
                "metadata-location": "s3://lakecat-demo/events/metadata/00000.json",
                "read-restriction": read_restriction,
                "required-projection": ["event_id"],
                "effective-projection": ["event_id"],
                "requested-stats-fields": ["event_id"],
                "effective-stats-fields": ["event_id"],
                "required-filters": [{
                    "type": "not-eq",
                    "term": "severity",
                    "value": "debug"
                }]
            })
        };
        payload[field] = value;
        let event_id = format!("evt-{label}-{case}");
        let store = Arc::new(RecordingOutboxStore {
            events: Mutex::new(vec![OutboxEvent {
                event_id: event_id.clone(),
                sink: "lakecat.lineage-and-graph".to_string(),
                event_type: event_type.to_string(),
                payload: json!({
                    "audit-event-id": format!("audit-{label}-{case}"),
                    "event-type": event_type,
                    "table": ident,
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
            .expect_err("malformed scan location evidence should fail");

        let message = err.to_string();
        assert!(message.contains(event_type));
        assert!(
            message.contains(expected_message),
            "{event_type} should reject {case}: {message}"
        );
        assert!(message.contains("event-id-hash=sha256:"));
        assert!(!message.contains(&event_id));
        assert!(
            store.delivered.lock().await.is_empty(),
            "{event_type} {case} must fail before acknowledgement"
        );
        assert!(
            graph.events.lock().await.is_empty(),
            "{event_type} {case} must fail before graph projection"
        );
        assert!(
            lineage.events.lock().await.is_empty(),
            "{event_type} {case} must fail before lineage projection"
        );
    }
}

#[tokio::test]
async fn outbox_drain_rejects_scan_missing_read_restriction_policy_hashes() {
    let cases = [
        (
            "table.scan-planned",
            "scan-planned",
            "evt-scan-plan-missing-policy-hashes",
            "audit-scan-plan-missing-policy-hashes",
        ),
        (
            "table.scan-tasks-fetched",
            "scan-tasks-fetched",
            "evt-scan-fetch-missing-policy-hashes",
            "audit-scan-fetch-missing-policy-hashes",
        ),
    ];

    for (event_type, label, event_id, audit_event_id) in cases {
        let principal = Principal::new("agent:reader", PrincipalKind::Agent).unwrap();
        let ident = table_ident("local", "default", "events").unwrap();
        let read_restriction = json!({
            "allowed-columns": ["event_id"],
            "row-predicate": {
                "type": "not-eq",
                "term": "severity",
                "value": "debug"
            },
            "purpose": "qglake-agent-demo",
            "max-credential-ttl-seconds": 300
        });
        let payload = if event_type == "table.scan-planned" {
            json!({
                "table": ident,
                "authorization-receipt": {
                    "principal": principal,
                    "action": "table-plan-scan",
                    "allowed": true,
                    "engine": "test",
                    "policy_hash": null,
                    "context": {
                        "read-restriction": read_restriction
                    },
                    "checked_at": chrono::Utc::now(),
                },
                "read-restriction": read_restriction,
                "requested-projection": ["event_id"],
                "effective-projection": ["event_id"],
                "requested-stats-fields": ["event_id"],
                "effective-stats-fields": ["event_id"],
                "scan-task-count": 1
            })
        } else {
            json!({
                "table": ident,
                "authorization-receipt": {
                    "principal": principal,
                    "action": "table-plan-scan",
                    "allowed": true,
                    "engine": "test",
                    "policy_hash": null,
                    "context": {
                        "read-restriction": read_restriction
                    },
                    "checked_at": chrono::Utc::now(),
                },
                "read-restriction": read_restriction,
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
                "delete-file-count": 0,
                "child-plan-task-count": 0
            })
        };
        let store = Arc::new(RecordingOutboxStore {
            events: Mutex::new(vec![OutboxEvent {
                event_id: event_id.to_string(),
                sink: "lakecat.lineage-and-graph".to_string(),
                event_type: event_type.to_string(),
                payload: json!({
                    "audit-event-id": audit_event_id,
                    "event-type": event_type,
                    "table": ident,
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
            .expect_err("scan read restriction policy hashes should be required");

        let message = err.to_string();
        assert!(message.contains(event_type));
        assert!(message.contains("event-id-hash=sha256:"));
        assert!(message.contains(&format!(
            "{label} read-restriction must contain policy-hashes"
        )));
        assert!(!message.contains(event_id));
        assert!(
            store.delivered.lock().await.is_empty(),
            "missing policy-hashes replay must fail before acknowledgement"
        );
        assert!(
            graph.events.lock().await.is_empty(),
            "missing policy-hashes replay must fail before graph projection"
        );
        assert!(
            lineage.events.lock().await.is_empty(),
            "missing policy-hashes replay must fail before lineage projection"
        );
    }
}

#[tokio::test]
async fn outbox_drain_rejects_scan_missing_receipt_read_restriction_policy_hashes() {
    let cases = [
        (
            "table.scan-planned",
            "scan-planned",
            "evt-scan-plan-missing-receipt-policy-hashes",
            "audit-scan-plan-missing-receipt-policy-hashes",
        ),
        (
            "table.scan-tasks-fetched",
            "scan-tasks-fetched",
            "evt-scan-fetch-missing-receipt-policy-hashes",
            "audit-scan-fetch-missing-receipt-policy-hashes",
        ),
    ];

    for (event_type, label, event_id, audit_event_id) in cases {
        let principal = Principal::new("agent:reader", PrincipalKind::Agent).unwrap();
        let ident = table_ident("local", "default", "events").unwrap();
        let policy_hash =
            content_hash_json(&json!({"policy-id": "agent-read", "scope": "default.events"}))
                .unwrap();
        let read_restriction = json!({
            "allowed-columns": ["event_id"],
            "row-predicate": {
                "type": "not-eq",
                "term": "severity",
                "value": "debug"
            },
            "purpose": "qglake-agent-demo",
            "max-credential-ttl-seconds": 300,
            "policy-hashes": [policy_hash]
        });
        let receipt_read_restriction = json!({
            "allowed-columns": ["event_id"],
            "row-predicate": {
                "type": "not-eq",
                "term": "severity",
                "value": "debug"
            },
            "purpose": "qglake-agent-demo",
            "max-credential-ttl-seconds": 300
        });
        let payload = if event_type == "table.scan-planned" {
            json!({
                "table": ident,
                "authorization-receipt": {
                    "principal": principal,
                    "action": "table-plan-scan",
                    "allowed": true,
                    "engine": "test",
                    "policy_hash": null,
                    "context": {
                        "read-restriction": receipt_read_restriction
                    },
                    "checked_at": chrono::Utc::now(),
                },
                "read-restriction": read_restriction,
                "requested-projection": ["event_id"],
                "effective-projection": ["event_id"],
                "requested-stats-fields": ["event_id"],
                "effective-stats-fields": ["event_id"],
                "scan-task-count": 1
            })
        } else {
            json!({
                "table": ident,
                "authorization-receipt": {
                    "principal": principal,
                    "action": "table-plan-scan",
                    "allowed": true,
                    "engine": "test",
                    "policy_hash": null,
                    "context": {
                        "read-restriction": receipt_read_restriction
                    },
                    "checked_at": chrono::Utc::now(),
                },
                "read-restriction": read_restriction,
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
                "delete-file-count": 0,
                "child-plan-task-count": 0
            })
        };
        let store = Arc::new(RecordingOutboxStore {
            events: Mutex::new(vec![OutboxEvent {
                event_id: event_id.to_string(),
                sink: "lakecat.lineage-and-graph".to_string(),
                event_type: event_type.to_string(),
                payload: json!({
                    "audit-event-id": audit_event_id,
                    "event-type": event_type,
                    "table": ident,
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
            .expect_err("scan receipt read restriction policy hashes should be required");

        let message = err.to_string();
        assert!(message.contains(event_type));
        assert!(message.contains("event-id-hash=sha256:"));
        assert!(message.contains(&format!(
            "{label} authorization receipt read-restriction must contain policy-hashes"
        )));
        assert!(!message.contains(event_id));
        assert!(
            store.delivered.lock().await.is_empty(),
            "missing receipt policy-hashes replay must fail before acknowledgement"
        );
        assert!(
            graph.events.lock().await.is_empty(),
            "missing receipt policy-hashes replay must fail before graph projection"
        );
        assert!(
            lineage.events.lock().await.is_empty(),
            "missing receipt policy-hashes replay must fail before lineage projection"
        );
    }
}

#[tokio::test]
async fn outbox_drain_rejects_scan_restriction_missing_from_receipt_context() {
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
            event_id: "evt-scan-restriction-missing-receipt-context".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "table.scan-planned".to_string(),
            payload: json!({
                "audit-event-id": "audit-scan-restriction-missing-receipt-context",
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
                    "table": table,
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
                    "requested-projection": ["event_id", "severity"],
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
        .expect_err("scan restriction must be copied into the receipt context");

    let message = err.to_string();
    assert!(message.contains("table.scan-planned"));
    assert!(message.contains(
        "scan-planned read-restriction must be captured in authorization receipt context"
    ));
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains("evt-scan-restriction-missing-receipt-context"));
    assert!(store.delivered.lock().await.is_empty());
    assert!(graph.events.lock().await.is_empty());
    assert!(lineage.events.lock().await.is_empty());
}

#[tokio::test]
async fn outbox_drain_rejects_scan_restriction_receipt_context_drift() {
    let cases = [
        (
            "table.scan-planned",
            "scan-planned",
            "evt-scan-planned-restriction-drift",
            "audit-scan-planned-restriction-drift",
        ),
        (
            "table.scan-tasks-fetched",
            "scan-tasks-fetched",
            "evt-scan-fetch-restriction-drift",
            "audit-scan-fetch-restriction-drift",
        ),
    ];

    for (event_type, label, event_id, audit_event_id) in cases {
        let table = TableIdent::new(
            WarehouseName::new("local").unwrap(),
            "default".parse::<Namespace>().unwrap(),
            TableName::new("events").unwrap(),
        );
        let principal = Principal {
            subject: "agent:writer".to_string(),
            kind: PrincipalKind::Agent,
        };
        let policy_hash =
            content_hash_json(&json!({"policy-id": "agent-read", "scope": "default.events"}))
                .unwrap();
        let read_restriction = json!({
            "allowed-columns": ["event_id"],
            "row-predicate": {
                "type": "not-eq",
                "term": "severity",
                "value": "debug"
            },
            "purpose": "resilience-demo",
            "max-credential-ttl-seconds": 60,
            "policy-hashes": [policy_hash]
        });
        let receipt_read_restriction = json!({
            "allowed-columns": ["severity"],
            "row-predicate": {
                "type": "not-eq",
                "term": "severity",
                "value": "debug"
            },
            "purpose": "resilience-demo",
            "max-credential-ttl-seconds": 60,
            "policy-hashes": [policy_hash]
        });
        let payload = if event_type == "table.scan-planned" {
            json!({
                "table": table,
                "authorization-receipt": {
                    "principal": principal,
                    "action": "table-plan-scan",
                    "allowed": true,
                    "engine": "test",
                    "policy_hash": null,
                    "checked_at": chrono::Utc::now(),
                    "context": {
                        "read-restriction": receipt_read_restriction
                    }
                },
                "read-restriction": read_restriction,
                "requested-projection": ["event_id", "severity"],
                "effective-projection": ["event_id"],
                "requested-stats-fields": ["event_id"],
                "effective-stats-fields": ["event_id"],
                "scan-task-count": 1,
            })
        } else {
            json!({
                "table": table,
                "authorization-receipt": {
                    "principal": principal,
                    "action": "table-plan-scan",
                    "allowed": true,
                    "engine": "test",
                    "policy_hash": null,
                    "checked_at": chrono::Utc::now(),
                    "context": {
                        "read-restriction": receipt_read_restriction
                    }
                },
                "read-restriction": read_restriction,
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
                "delete-file-count": 0,
                "child-plan-task-count": 0,
            })
        };
        let store = Arc::new(RecordingOutboxStore {
            events: Mutex::new(vec![OutboxEvent {
                event_id: event_id.to_string(),
                sink: "lakecat.lineage-and-graph".to_string(),
                event_type: event_type.to_string(),
                payload: json!({
                    "audit-event-id": audit_event_id,
                    "event-type": event_type,
                    "table": table,
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
            .expect_err("scan restriction drift must fail before delivery");

        let message = err.to_string();
        assert!(message.contains(event_type));
        assert!(
            message.contains(&format!(
                "{label} read-restriction must match authorization receipt context"
            )),
            "{message}"
        );
        assert!(message.contains("event-id-hash=sha256:"));
        assert!(!message.contains(event_id));
        assert!(store.delivered.lock().await.is_empty());
        assert!(graph.events.lock().await.is_empty());
        assert!(lineage.events.lock().await.is_empty());
    }
}

#[tokio::test]
async fn outbox_drain_rejects_scan_restriction_missing_purpose_before_projection() {
    let cases = [
        (
            "table.scan-planned",
            "scan-planned",
            "evt-scan-plan-missing-purpose",
            "audit-scan-plan-missing-purpose",
        ),
        (
            "table.scan-tasks-fetched",
            "scan-tasks-fetched",
            "evt-scan-fetch-missing-purpose",
            "audit-scan-fetch-missing-purpose",
        ),
    ];

    for (event_type, label, event_id, audit_event_id) in cases {
        let table = TableIdent::new(
            WarehouseName::new("local").unwrap(),
            "default".parse::<Namespace>().unwrap(),
            TableName::new("events").unwrap(),
        );
        let principal = Principal::new("agent:writer", PrincipalKind::Agent).unwrap();
        let policy_hash =
            content_hash_json(&json!({"policy-id": "agent-read", "scope": "default.events"}))
                .unwrap();
        let read_restriction = json!({
            "allowed-columns": ["event_id"],
            "row-predicate": {
                "type": "not-eq",
                "term": "severity",
                "value": "debug"
            },
            "max-credential-ttl-seconds": 300,
            "policy-hashes": [policy_hash]
        });
        let payload = if event_type == "table.scan-planned" {
            json!({
                "table": table,
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
                "read-restriction": read_restriction,
                "requested-projection": ["event_id"],
                "effective-projection": ["event_id"],
                "requested-stats-fields": ["event_id"],
                "effective-stats-fields": ["event_id"],
                "scan-task-count": 1,
            })
        } else {
            json!({
                "table": table,
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
                "read-restriction": read_restriction,
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
                "delete-file-count": 0,
                "child-plan-task-count": 0,
            })
        };
        let store = Arc::new(RecordingOutboxStore {
            events: Mutex::new(vec![OutboxEvent {
                event_id: event_id.to_string(),
                sink: "lakecat.lineage-and-graph".to_string(),
                event_type: event_type.to_string(),
                payload: json!({
                    "audit-event-id": audit_event_id,
                    "event-type": event_type,
                    "table": table,
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
            .expect_err("scan restriction purpose evidence should be required");

        let message = err.to_string();
        assert!(message.contains(event_type));
        assert!(
            message.contains(&format!(
                "{label} read-restriction purpose must not be blank"
            )),
            "{message}"
        );
        assert!(message.contains("event-id-hash=sha256:"));
        assert!(!message.contains(event_id));
        assert!(store.delivered.lock().await.is_empty());
        assert!(graph.events.lock().await.is_empty());
        assert!(lineage.events.lock().await.is_empty());
    }
}

#[tokio::test]
async fn outbox_drain_rejects_scan_restriction_zero_ttl_before_projection() {
    let cases = [
        (
            "table.scan-planned",
            "scan-planned",
            "evt-scan-plan-zero-ttl",
            "audit-scan-plan-zero-ttl",
        ),
        (
            "table.scan-tasks-fetched",
            "scan-tasks-fetched",
            "evt-scan-fetch-zero-ttl",
            "audit-scan-fetch-zero-ttl",
        ),
    ];

    for (event_type, label, event_id, audit_event_id) in cases {
        let table = TableIdent::new(
            WarehouseName::new("local").unwrap(),
            "default".parse::<Namespace>().unwrap(),
            TableName::new("events").unwrap(),
        );
        let principal = Principal::new("agent:writer", PrincipalKind::Agent).unwrap();
        let policy_hash =
            content_hash_json(&json!({"policy-id": "agent-read", "scope": "default.events"}))
                .unwrap();
        let read_restriction = json!({
            "allowed-columns": ["event_id"],
            "row-predicate": {
                "type": "not-eq",
                "term": "severity",
                "value": "debug"
            },
            "purpose": "qglake-agent-demo",
            "max-credential-ttl-seconds": 0,
            "policy-hashes": [policy_hash]
        });
        let payload = if event_type == "table.scan-planned" {
            json!({
                "table": table,
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
                "read-restriction": read_restriction,
                "requested-projection": ["event_id"],
                "effective-projection": ["event_id"],
                "requested-stats-fields": ["event_id"],
                "effective-stats-fields": ["event_id"],
                "scan-task-count": 1,
            })
        } else {
            json!({
                "table": table,
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
                "read-restriction": read_restriction,
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
                "delete-file-count": 0,
                "child-plan-task-count": 0,
            })
        };
        let store = Arc::new(RecordingOutboxStore {
            events: Mutex::new(vec![OutboxEvent {
                event_id: event_id.to_string(),
                sink: "lakecat.lineage-and-graph".to_string(),
                event_type: event_type.to_string(),
                payload: json!({
                    "audit-event-id": audit_event_id,
                    "event-type": event_type,
                    "table": table,
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
            .expect_err("scan restriction TTL evidence should be positive");

        let message = err.to_string();
        assert!(message.contains(event_type));
        assert!(
            message.contains(&format!(
                "{label} read-restriction max-credential-ttl-seconds must be positive"
            )),
            "{message}"
        );
        assert!(message.contains("event-id-hash=sha256:"));
        assert!(!message.contains(event_id));
        assert!(store.delivered.lock().await.is_empty());
        assert!(graph.events.lock().await.is_empty());
        assert!(lineage.events.lock().await.is_empty());
    }
}

#[tokio::test]
async fn outbox_drain_rejects_scan_restriction_malformed_ttl_before_projection() {
    let cases = [
        (
            "table.scan-planned",
            "scan-planned",
            "evt-scan-plan-malformed-ttl",
            "audit-scan-plan-malformed-ttl",
        ),
        (
            "table.scan-tasks-fetched",
            "scan-tasks-fetched",
            "evt-scan-fetch-malformed-ttl",
            "audit-scan-fetch-malformed-ttl",
        ),
    ];

    for (event_type, label, event_id, audit_event_id) in cases {
        let table = TableIdent::new(
            WarehouseName::new("local").unwrap(),
            "default".parse::<Namespace>().unwrap(),
            TableName::new("events").unwrap(),
        );
        let principal = Principal::new("agent:writer", PrincipalKind::Agent).unwrap();
        let policy_hash =
            content_hash_json(&json!({"policy-id": "agent-read", "scope": "default.events"}))
                .unwrap();
        let read_restriction = json!({
            "allowed-columns": ["event_id"],
            "row-predicate": {
                "type": "not-eq",
                "term": "severity",
                "value": "debug"
            },
            "purpose": "qglake-agent-demo",
            "max-credential-ttl-seconds": "300",
            "policy-hashes": [policy_hash]
        });
        let payload = if event_type == "table.scan-planned" {
            json!({
                "table": table,
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
                "read-restriction": read_restriction,
                "requested-projection": ["event_id"],
                "effective-projection": ["event_id"],
                "requested-stats-fields": ["event_id"],
                "effective-stats-fields": ["event_id"],
                "scan-task-count": 1,
            })
        } else {
            json!({
                "table": table,
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
                "read-restriction": read_restriction,
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
                "delete-file-count": 0,
                "child-plan-task-count": 0,
            })
        };
        let store = Arc::new(RecordingOutboxStore {
            events: Mutex::new(vec![OutboxEvent {
                event_id: event_id.to_string(),
                sink: "lakecat.lineage-and-graph".to_string(),
                event_type: event_type.to_string(),
                payload: json!({
                    "audit-event-id": audit_event_id,
                    "event-type": event_type,
                    "table": table,
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
            .expect_err("scan restriction TTL evidence should be an integer");

        let message = err.to_string();
        assert!(message.contains(event_type));
        assert!(
            message.contains(&format!(
                "{label} read-restriction max-credential-ttl-seconds must be a positive integer"
            )),
            "{message}"
        );
        assert!(message.contains("event-id-hash=sha256:"));
        assert!(!message.contains(event_id));
        assert!(
            store.delivered.lock().await.is_empty(),
            "malformed TTL scan replay must fail before acknowledgement"
        );
        assert!(
            graph.events.lock().await.is_empty(),
            "malformed TTL scan replay must fail before graph projection"
        );
        assert!(
            lineage.events.lock().await.is_empty(),
            "malformed TTL scan replay must fail before lineage projection"
        );
    }
}

#[tokio::test]
async fn outbox_drain_rejects_malformed_scan_planned_evidence() {
    let principal = Principal::new("agent:reader", PrincipalKind::Agent).unwrap();
    let ident = table_ident("local", "default", "events").unwrap();
    let policy_hash =
        content_hash_json(&json!({"policy-id": "agent-read", "scope": "default.events"})).unwrap();
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-secret-scan-plan-token".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "table.scan-planned".to_string(),
            payload: json!({
                "audit-event-id": "audit-corrupt-scan-plan",
                "event-type": "table.scan-planned",
                "table": ident,
                "payload": {
                    "table": ident,
                    "authorization-receipt": {
                        "principal": principal,
                        "action": "table-plan-scan",
                        "allowed": true,
                        "engine": "test",
                        "policy_hash": "sha256:policy",
                        "checked_at": chrono::Utc::now(),
                    },
                    "read-restriction": {
                        "allowed-columns": ["event_id"],
                        "policy-hashes": [policy_hash]
                    },
                    "requested-projection": ["event_id"],
                    "effective-projection": ["event_id", "payload"],
                    "requested-stats-fields": ["event_id"],
                    "effective-stats-fields": ["event_id"],
                    "scan-task-count": 1
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
        .expect_err("widened scan-plan projection evidence should fail");

    let message = err.to_string();
    assert!(
        message.contains("outbox event table.scan-planned (lakecat.lineage-and-graph) has invalid")
    );
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(
        message
            .contains("scan-planned effective-projection must be a subset of requested-projection")
    );
    assert!(!message.contains("evt-secret-scan-plan-token"));
    assert!(
        store.delivered.lock().await.is_empty(),
        "malformed scan-plan evidence must fail before acknowledgement"
    );
    assert!(
        graph.events.lock().await.is_empty(),
        "malformed scan-plan evidence must fail before graph projection"
    );
    assert!(
        lineage.events.lock().await.is_empty(),
        "malformed scan-plan evidence must fail before lineage projection"
    );
}

#[tokio::test]
async fn outbox_drain_rejects_scan_planned_empty_allowed_columns() {
    let principal = Principal::new("agent:reader", PrincipalKind::Agent).unwrap();
    let ident = table_ident("local", "default", "events").unwrap();
    let read_restriction = json!({
        "allowed-columns": [],
        "policy-hashes": [
            content_hash_json(&json!({"policy-id": "agent-read", "scope": "default.events"}))
                .unwrap()
        ]
    });
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-scan-plan-empty-allowed-columns".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "table.scan-planned".to_string(),
            payload: json!({
                "audit-event-id": "audit-scan-plan-empty-allowed-columns",
                "event-type": "table.scan-planned",
                "table": ident,
                "payload": {
                    "table": ident,
                    "authorization-receipt": {
                        "principal": principal,
                        "action": "table-plan-scan",
                        "allowed": true,
                        "engine": "test",
                        "policy_hash": "sha256:policy",
                        "context": {
                            "read-restriction": read_restriction
                        },
                        "checked_at": chrono::Utc::now(),
                    },
                    "read-restriction": read_restriction,
                    "requested-projection": ["event_id"],
                    "effective-projection": ["event_id"],
                    "requested-stats-fields": ["event_id"],
                    "effective-stats-fields": ["event_id"],
                    "scan-task-count": 1
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
        .expect_err("scan-plan empty allowed-columns evidence should fail closed");

    let message = err.to_string();
    assert!(
        message.contains("outbox event table.scan-planned (lakecat.lineage-and-graph) has invalid")
    );
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(message.contains("scan-planned read-restriction allowed-columns must not be empty"));
    assert!(!message.contains("evt-scan-plan-empty-allowed-columns"));
    assert!(
        store.delivered.lock().await.is_empty(),
        "empty allowed-column scan replay must fail before acknowledgement"
    );
    assert!(
        graph.events.lock().await.is_empty(),
        "empty allowed-column scan replay must fail before graph projection"
    );
    assert!(
        lineage.events.lock().await.is_empty(),
        "empty allowed-column scan replay must fail before lineage projection"
    );
}

#[tokio::test]
async fn outbox_drain_rejects_scan_planned_duplicate_allowed_columns() {
    let principal = Principal::new("agent:reader", PrincipalKind::Agent).unwrap();
    let ident = table_ident("local", "default", "events").unwrap();
    let read_restriction = json!({
        "allowed-columns": ["event_id", "event_id"],
        "policy-hashes": [
            content_hash_json(&json!({"policy-id": "agent-read", "scope": "default.events"}))
                .unwrap()
        ]
    });
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-scan-plan-duplicate-allowed-columns".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "table.scan-planned".to_string(),
            payload: json!({
                "audit-event-id": "audit-scan-plan-duplicate-allowed-columns",
                "event-type": "table.scan-planned",
                "table": ident,
                "payload": {
                    "table": ident,
                    "authorization-receipt": {
                        "principal": principal,
                        "action": "table-plan-scan",
                        "allowed": true,
                        "engine": "test",
                        "policy_hash": "sha256:policy",
                        "context": {
                            "read-restriction": read_restriction
                        },
                        "checked_at": chrono::Utc::now(),
                    },
                    "read-restriction": read_restriction,
                    "requested-projection": ["event_id"],
                    "effective-projection": ["event_id"],
                    "requested-stats-fields": ["event_id"],
                    "effective-stats-fields": ["event_id"],
                    "scan-task-count": 1
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
        .expect_err("scan-plan duplicate allowed-columns evidence should fail closed");

    let message = err.to_string();
    assert!(
        message.contains("outbox event table.scan-planned (lakecat.lineage-and-graph) has invalid")
    );
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(
        message.contains("scan-planned read-restriction allowed-columns must be duplicate-free")
    );
    assert!(!message.contains("evt-scan-plan-duplicate-allowed-columns"));
    assert!(
        store.delivered.lock().await.is_empty(),
        "duplicate allowed-column scan replay must fail before acknowledgement"
    );
    assert!(
        graph.events.lock().await.is_empty(),
        "duplicate allowed-column scan replay must fail before graph projection"
    );
    assert!(
        lineage.events.lock().await.is_empty(),
        "duplicate allowed-column scan replay must fail before lineage projection"
    );
}

#[tokio::test]
async fn outbox_drain_rejects_scan_planned_duplicate_requested_projection() {
    let principal = Principal::new("agent:reader", PrincipalKind::Agent).unwrap();
    let ident = table_ident("local", "default", "events").unwrap();
    let read_restriction = json!({
        "allowed-columns": ["event_id"],
        "policy-hashes": [
            content_hash_json(&json!({"policy-id": "agent-read", "scope": "default.events"}))
                .unwrap()
        ]
    });
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-scan-plan-duplicate-requested-projection".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "table.scan-planned".to_string(),
            payload: json!({
                "audit-event-id": "audit-scan-plan-duplicate-requested-projection",
                "event-type": "table.scan-planned",
                "table": ident,
                "payload": {
                    "table": ident,
                    "authorization-receipt": {
                        "principal": principal,
                        "action": "table-plan-scan",
                        "allowed": true,
                        "engine": "test",
                        "policy_hash": "sha256:policy",
                        "context": {
                            "read-restriction": read_restriction
                        },
                        "checked_at": chrono::Utc::now(),
                    },
                    "read-restriction": read_restriction,
                    "requested-projection": ["event_id", "event_id"],
                    "effective-projection": ["event_id"],
                    "requested-stats-fields": ["event_id"],
                    "effective-stats-fields": ["event_id"],
                    "scan-task-count": 1
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
        .expect_err("duplicate requested scan projection evidence should fail closed");

    let message = err.to_string();
    assert!(
        message.contains("outbox event table.scan-planned (lakecat.lineage-and-graph) has invalid")
    );
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(message.contains("scan-planned requested-projection must be duplicate-free"));
    assert!(!message.contains("evt-scan-plan-duplicate-requested-projection"));
    assert!(
        store.delivered.lock().await.is_empty(),
        "duplicate projection scan replay must fail before acknowledgement"
    );
    assert!(
        graph.events.lock().await.is_empty(),
        "duplicate projection scan replay must fail before graph projection"
    );
    assert!(
        lineage.events.lock().await.is_empty(),
        "duplicate projection scan replay must fail before lineage projection"
    );
}

#[tokio::test]
async fn outbox_drain_rejects_scan_planned_duplicate_effective_projection() {
    let principal = Principal::new("agent:reader", PrincipalKind::Agent).unwrap();
    let ident = table_ident("local", "default", "events").unwrap();
    let read_restriction = json!({
        "allowed-columns": ["event_id"],
        "policy-hashes": [
            content_hash_json(&json!({"policy-id": "agent-read", "scope": "default.events"}))
                .unwrap()
        ]
    });
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-scan-plan-duplicate-effective-projection".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "table.scan-planned".to_string(),
            payload: json!({
                "audit-event-id": "audit-scan-plan-duplicate-effective-projection",
                "event-type": "table.scan-planned",
                "table": ident,
                "payload": {
                    "table": ident,
                    "authorization-receipt": {
                        "principal": principal,
                        "action": "table-plan-scan",
                        "allowed": true,
                        "engine": "test",
                        "policy_hash": "sha256:policy",
                        "context": {
                            "read-restriction": read_restriction
                        },
                        "checked_at": chrono::Utc::now(),
                    },
                    "read-restriction": read_restriction,
                    "requested-projection": ["event_id"],
                    "effective-projection": ["event_id", "event_id"],
                    "requested-stats-fields": ["event_id"],
                    "effective-stats-fields": ["event_id"],
                    "scan-task-count": 1
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
        .expect_err("duplicate effective scan projection evidence should fail closed");

    let message = err.to_string();
    assert!(
        message.contains("outbox event table.scan-planned (lakecat.lineage-and-graph) has invalid")
    );
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(
        message.contains("scan-planned effective-projection must be duplicate-free"),
        "{message}"
    );
    assert!(!message.contains("evt-scan-plan-duplicate-effective-projection"));
    assert!(
        store.delivered.lock().await.is_empty(),
        "duplicate effective projection scan replay must fail before acknowledgement"
    );
    assert!(
        graph.events.lock().await.is_empty(),
        "duplicate effective projection scan replay must fail before graph projection"
    );
    assert!(
        lineage.events.lock().await.is_empty(),
        "duplicate effective projection scan replay must fail before lineage projection"
    );
}

#[tokio::test]
async fn outbox_drain_rejects_extra_scan_planned_outbox_payload_fields() {
    let principal = Principal::new("agent:reader", PrincipalKind::Agent).unwrap();
    let ident = table_ident("local", "default", "events").unwrap();
    let read_restriction = json!({
        "allowed-columns": ["event_id"],
        "purpose": "qglake-agent-demo",
        "max-credential-ttl-seconds": 300,
        "policy-hashes": [
            content_hash_json(&json!({"policy-id": "agent-read", "scope": "default.events"}))
                .unwrap()
        ]
    });
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-scan-plan-extra-wrapper-field".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "table.scan-planned".to_string(),
            payload: json!({
                "audit-event-id": "audit-scan-plan-extra-wrapper-field",
                "event-type": "table.scan-planned",
                "table": ident,
                "payload": {
                    "table": ident,
                    "authorization-receipt": {
                        "principal": principal,
                        "action": "table-plan-scan",
                        "allowed": true,
                        "engine": "test",
                        "policy_hash": "sha256:policy",
                        "context": {
                            "read-restriction": read_restriction
                        },
                        "checked_at": chrono::Utc::now(),
                    },
                    "read-restriction": read_restriction,
                    "requested-projection": ["event_id"],
                    "effective-projection": ["event_id"],
                    "requested-stats-fields": ["event_id"],
                    "effective-stats-fields": ["event_id"],
                    "scan-task-count": 1
                },
                "unverified-querygraph-claim": "accepted"
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
        .expect_err("extra scan-plan wrapper fields should fail closed");

    let message = err.to_string();
    assert!(
        message.contains("outbox event table.scan-planned (lakecat.lineage-and-graph) has invalid")
    );
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(message.contains(
        "scan-planned outbox payload contains unexpected field unverified-querygraph-claim"
    ));
    assert!(!message.contains("evt-scan-plan-extra-wrapper-field"));
    assert!(
        store.delivered.lock().await.is_empty(),
        "extra scan-plan wrapper proof must fail before acknowledgement"
    );
    assert!(
        graph.events.lock().await.is_empty(),
        "extra scan-plan wrapper proof must fail before graph projection"
    );
    assert!(
        lineage.events.lock().await.is_empty(),
        "extra scan-plan wrapper proof must fail before lineage projection"
    );
}

#[tokio::test]
async fn outbox_drain_rejects_scan_planned_empty_effective_stats_fields() {
    let principal = Principal::new("agent:reader", PrincipalKind::Agent).unwrap();
    let ident = table_ident("local", "default", "events").unwrap();
    let read_restriction = json!({
        "allowed-columns": ["event_id"],
        "row-predicate": {
            "type": "not-eq",
            "term": "severity",
            "value": "debug"
        },
        "policy-hashes": [
            content_hash_json(&json!({"policy-id": "agent-read", "scope": "default.events"}))
                .unwrap()
        ]
    });
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-scan-plan-empty-effective-stats".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "table.scan-planned".to_string(),
            payload: json!({
                "audit-event-id": "audit-scan-plan-empty-effective-stats",
                "event-type": "table.scan-planned",
                "table": ident,
                "payload": {
                    "table": ident,
                    "authorization-receipt": {
                        "principal": principal,
                        "action": "table-plan-scan",
                        "allowed": true,
                        "engine": "test",
                        "policy_hash": "sha256:policy",
                        "context": {
                            "read-restriction": read_restriction
                        },
                        "checked_at": chrono::Utc::now(),
                    },
                    "read-restriction": read_restriction,
                    "requested-projection": ["event_id"],
                    "effective-projection": ["event_id"],
                    "requested-stats-fields": ["event_id"],
                    "effective-stats-fields": [],
                    "scan-task-count": 1
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
        .expect_err("empty effective stats-field scan replay evidence should fail closed");

    let message = err.to_string();
    assert!(
        message.contains("outbox event table.scan-planned (lakecat.lineage-and-graph) has invalid")
    );
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(message.contains("scan-planned effective-stats-fields must not be empty"));
    assert!(!message.contains("evt-scan-plan-empty-effective-stats"));
    assert!(
        store.delivered.lock().await.is_empty(),
        "empty stats-field scan replay must fail before acknowledgement"
    );
    assert!(
        graph.events.lock().await.is_empty(),
        "empty stats-field scan replay must fail before graph projection"
    );
    assert!(
        lineage.events.lock().await.is_empty(),
        "empty stats-field scan replay must fail before lineage projection"
    );
}

#[tokio::test]
async fn outbox_drain_rejects_scan_planned_missing_row_predicate() {
    let principal = Principal::new("agent:reader", PrincipalKind::Agent).unwrap();
    let ident = table_ident("local", "default", "events").unwrap();
    let read_restriction = json!({
        "allowed-columns": ["event_id"],
        "policy-hashes": [
            content_hash_json(&json!({"policy-id": "agent-read", "scope": "default.events"}))
                .unwrap()
        ]
    });
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-scan-plan-missing-row-predicate".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "table.scan-planned".to_string(),
            payload: json!({
                "audit-event-id": "audit-scan-plan-missing-row-predicate",
                "event-type": "table.scan-planned",
                "table": ident,
                "payload": {
                    "table": ident,
                    "authorization-receipt": {
                        "principal": principal,
                        "action": "table-plan-scan",
                        "allowed": true,
                        "engine": "test",
                        "policy_hash": "sha256:policy",
                        "context": {
                            "read-restriction": read_restriction
                        },
                        "checked_at": chrono::Utc::now(),
                    },
                    "read-restriction": read_restriction,
                    "requested-projection": ["event_id"],
                    "effective-projection": ["event_id"],
                    "requested-stats-fields": ["event_id"],
                    "effective-stats-fields": ["event_id"],
                    "scan-task-count": 1
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
        .expect_err("scan-plan row-predicate evidence should be required");

    let message = err.to_string();
    assert!(
        message.contains("outbox event table.scan-planned (lakecat.lineage-and-graph) has invalid")
    );
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(message.contains("scan-planned read-restriction must contain row-predicate"));
    assert!(!message.contains("evt-scan-plan-missing-row-predicate"));
    assert!(
        store.delivered.lock().await.is_empty(),
        "missing row-predicate scan replay must fail before acknowledgement"
    );
    assert!(
        graph.events.lock().await.is_empty(),
        "missing row-predicate scan replay must fail before graph projection"
    );
    assert!(
        lineage.events.lock().await.is_empty(),
        "missing row-predicate scan replay must fail before lineage projection"
    );
}

#[tokio::test]
async fn outbox_drain_rejects_scan_fetch_missing_row_predicate() {
    let principal = Principal::new("agent:reader", PrincipalKind::Agent).unwrap();
    let ident = table_ident("local", "default", "events").unwrap();
    let read_restriction = json!({
        "allowed-columns": ["event_id"],
        "purpose": "qglake-agent-demo",
        "max-credential-ttl-seconds": 300,
        "policy-hashes": [
            content_hash_json(&json!({"policy-id": "agent-read", "scope": "default.events"}))
                .unwrap()
        ]
    });
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-scan-fetch-missing-row-predicate".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "table.scan-tasks-fetched".to_string(),
            payload: json!({
                "audit-event-id": "audit-scan-fetch-missing-row-predicate",
                "event-type": "table.scan-tasks-fetched",
                "table": ident,
                "payload": {
                    "table": ident,
                    "authorization-receipt": {
                        "principal": principal,
                        "action": "table-plan-scan",
                        "allowed": true,
                        "engine": "test",
                        "policy_hash": "sha256:policy",
                        "context": {
                            "read-restriction": read_restriction
                        },
                        "checked_at": chrono::Utc::now(),
                    },
                    "read-restriction": read_restriction,
                    "required-projection": ["event_id"],
                    "effective-projection": ["event_id"],
                    "requested-stats-fields": ["event_id"],
                    "effective-stats-fields": ["event_id"],
                    "required-filters": [],
                    "file-scan-task-count": 1,
                    "delete-file-count": 0,
                    "child-plan-task-count": 0
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
        .expect_err("scan-fetch row-predicate evidence should be required");

    let message = err.to_string();
    assert!(
        message.contains(
            "outbox event table.scan-tasks-fetched (lakecat.lineage-and-graph) has invalid"
        )
    );
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(message.contains("scan-tasks-fetched read-restriction must contain row-predicate"));
    assert!(!message.contains("evt-scan-fetch-missing-row-predicate"));
    assert!(
        store.delivered.lock().await.is_empty(),
        "missing fetched row-predicate replay must fail before acknowledgement"
    );
    assert!(
        graph.events.lock().await.is_empty(),
        "missing fetched row-predicate replay must fail before graph projection"
    );
    assert!(
        lineage.events.lock().await.is_empty(),
        "missing fetched row-predicate replay must fail before lineage projection"
    );
}

#[tokio::test]
async fn outbox_drain_rejects_scan_empty_row_predicate() {
    let cases = [
        (
            "table.scan-planned",
            "scan-planned",
            "evt-scan-plan-empty-row-predicate",
            "audit-scan-plan-empty-row-predicate",
        ),
        (
            "table.scan-tasks-fetched",
            "scan-tasks-fetched",
            "evt-scan-fetch-empty-row-predicate",
            "audit-scan-fetch-empty-row-predicate",
        ),
    ];

    for (event_type, label, event_id, audit_event_id) in cases {
        let principal = Principal::new("agent:reader", PrincipalKind::Agent).unwrap();
        let ident = table_ident("local", "default", "events").unwrap();
        let read_restriction = json!({
            "allowed-columns": ["event_id"],
            "row-predicate": {},
            "purpose": "qglake-agent-demo",
            "max-credential-ttl-seconds": 300,
            "policy-hashes": [
                content_hash_json(&json!({"policy-id": "agent-read", "scope": "default.events"}))
                    .unwrap()
            ]
        });
        let payload = if event_type == "table.scan-planned" {
            json!({
                "table": ident,
                "authorization-receipt": {
                    "principal": principal,
                    "action": "table-plan-scan",
                    "allowed": true,
                    "engine": "test",
                    "policy_hash": "sha256:policy",
                    "context": {
                        "read-restriction": read_restriction
                    },
                    "checked_at": chrono::Utc::now(),
                },
                "read-restriction": read_restriction,
                "requested-projection": ["event_id"],
                "effective-projection": ["event_id"],
                "requested-stats-fields": ["event_id"],
                "effective-stats-fields": ["event_id"],
                "scan-task-count": 1
            })
        } else {
            json!({
                "table": ident,
                "authorization-receipt": {
                    "principal": principal,
                    "action": "table-plan-scan",
                    "allowed": true,
                    "engine": "test",
                    "policy_hash": "sha256:policy",
                    "context": {
                        "read-restriction": read_restriction
                    },
                    "checked_at": chrono::Utc::now(),
                },
                "read-restriction": read_restriction,
                "required-projection": ["event_id"],
                "effective-projection": ["event_id"],
                "requested-stats-fields": ["event_id"],
                "effective-stats-fields": ["event_id"],
                "required-filters": [],
                "file-scan-task-count": 1,
                "delete-file-count": 0,
                "child-plan-task-count": 0
            })
        };
        let store = Arc::new(RecordingOutboxStore {
            events: Mutex::new(vec![OutboxEvent {
                event_id: event_id.to_string(),
                sink: "lakecat.lineage-and-graph".to_string(),
                event_type: event_type.to_string(),
                payload: json!({
                    "audit-event-id": audit_event_id,
                    "event-type": event_type,
                    "table": ident,
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
            .expect_err("scan row-predicate evidence must not be empty");

        let message = err.to_string();
        assert!(message.contains(event_type));
        assert!(message.contains("event-id-hash=sha256:"));
        assert!(message.contains(&format!(
            "{label} read-restriction row-predicate must contain predicate evidence"
        )));
        assert!(!message.contains(event_id));
        assert!(
            store.delivered.lock().await.is_empty(),
            "empty row-predicate replay must fail before acknowledgement"
        );
        assert!(
            graph.events.lock().await.is_empty(),
            "empty row-predicate replay must fail before graph projection"
        );
        assert!(
            lineage.events.lock().await.is_empty(),
            "empty row-predicate replay must fail before lineage projection"
        );
    }
}

#[tokio::test]
async fn outbox_drain_rejects_scan_malformed_row_predicate() {
    let cases = [
        (
            "table.scan-planned",
            "scan-planned",
            "evt-scan-plan-malformed-row-predicate",
            "audit-scan-plan-malformed-row-predicate",
        ),
        (
            "table.scan-tasks-fetched",
            "scan-tasks-fetched",
            "evt-scan-fetch-malformed-row-predicate",
            "audit-scan-fetch-malformed-row-predicate",
        ),
    ];

    for (event_type, label, event_id, audit_event_id) in cases {
        let principal = Principal::new("agent:reader", PrincipalKind::Agent).unwrap();
        let ident = table_ident("local", "default", "events").unwrap();
        let read_restriction = json!({
            "allowed-columns": ["event_id"],
            "row-predicate": "always-true",
            "purpose": "qglake-agent-demo",
            "max-credential-ttl-seconds": 300,
            "policy-hashes": [
                content_hash_json(&json!({"policy-id": "agent-read", "scope": "default.events"}))
                    .unwrap()
            ]
        });
        let payload = if event_type == "table.scan-planned" {
            json!({
                "table": ident,
                "authorization-receipt": {
                    "principal": principal,
                    "action": "table-plan-scan",
                    "allowed": true,
                    "engine": "test",
                    "policy_hash": "sha256:policy",
                    "context": {
                        "read-restriction": read_restriction
                    },
                    "checked_at": chrono::Utc::now(),
                },
                "read-restriction": read_restriction,
                "requested-projection": ["event_id"],
                "effective-projection": ["event_id"],
                "requested-stats-fields": ["event_id"],
                "effective-stats-fields": ["event_id"],
                "scan-task-count": 1
            })
        } else {
            json!({
                "table": ident,
                "authorization-receipt": {
                    "principal": principal,
                    "action": "table-plan-scan",
                    "allowed": true,
                    "engine": "test",
                    "policy_hash": "sha256:policy",
                    "context": {
                        "read-restriction": read_restriction
                    },
                    "checked_at": chrono::Utc::now(),
                },
                "read-restriction": read_restriction,
                "required-projection": ["event_id"],
                "effective-projection": ["event_id"],
                "requested-stats-fields": ["event_id"],
                "effective-stats-fields": ["event_id"],
                "required-filters": ["always-true"],
                "file-scan-task-count": 1,
                "delete-file-count": 0,
                "child-plan-task-count": 0
            })
        };
        let store = Arc::new(RecordingOutboxStore {
            events: Mutex::new(vec![OutboxEvent {
                event_id: event_id.to_string(),
                sink: "lakecat.lineage-and-graph".to_string(),
                event_type: event_type.to_string(),
                payload: json!({
                    "audit-event-id": audit_event_id,
                    "event-type": event_type,
                    "table": ident,
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
            .expect_err("scan row-predicate evidence must be object-shaped");

        let message = err.to_string();
        assert!(message.contains(event_type));
        assert!(message.contains("event-id-hash=sha256:"));
        assert!(message.contains(&format!(
            "{label} read-restriction row-predicate must be an object"
        )));
        assert!(!message.contains(event_id));
        assert!(
            store.delivered.lock().await.is_empty(),
            "malformed row-predicate replay must fail before acknowledgement"
        );
        assert!(
            graph.events.lock().await.is_empty(),
            "malformed row-predicate replay must fail before graph projection"
        );
        assert!(
            lineage.events.lock().await.is_empty(),
            "malformed row-predicate replay must fail before lineage projection"
        );
    }
}

#[tokio::test]
async fn outbox_drain_rejects_scan_extra_read_restriction_fields() {
    let cases = [
        (
            "table.scan-planned",
            "scan-planned",
            "evt-scan-plan-extra-read-restriction",
            "audit-scan-plan-extra-read-restriction",
        ),
        (
            "table.scan-tasks-fetched",
            "scan-tasks-fetched",
            "evt-scan-fetch-extra-read-restriction",
            "audit-scan-fetch-extra-read-restriction",
        ),
    ];

    for (event_type, label, event_id, audit_event_id) in cases {
        let principal = Principal::new("agent:reader", PrincipalKind::Agent).unwrap();
        let ident = table_ident("local", "default", "events").unwrap();
        let read_restriction = json!({
            "allowed-columns": ["event_id"],
            "row-predicate": {
                "type": "not-eq",
                "term": "severity",
                "value": "debug"
            },
            "purpose": "qglake-agent-demo",
            "max-credential-ttl-seconds": 300,
            "policy-hashes": [
                content_hash_json(&json!({"policy-id": "agent-read", "scope": "default.events"}))
                    .unwrap()
            ],
            "unverified-restriction-claim": "graph-import-safe"
        });
        let payload = if event_type == "table.scan-planned" {
            json!({
                "table": ident,
                "authorization-receipt": {
                    "principal": principal,
                    "action": "table-plan-scan",
                    "allowed": true,
                    "engine": "test",
                    "policy_hash": "sha256:policy",
                    "context": {
                        "read-restriction": read_restriction
                    },
                    "checked_at": chrono::Utc::now(),
                },
                "read-restriction": read_restriction,
                "requested-projection": ["event_id"],
                "effective-projection": ["event_id"],
                "requested-stats-fields": ["event_id"],
                "effective-stats-fields": ["event_id"],
                "scan-task-count": 1
            })
        } else {
            json!({
                "table": ident,
                "authorization-receipt": {
                    "principal": principal,
                    "action": "table-plan-scan",
                    "allowed": true,
                    "engine": "test",
                    "policy_hash": "sha256:policy",
                    "context": {
                        "read-restriction": read_restriction
                    },
                    "checked_at": chrono::Utc::now(),
                },
                "read-restriction": read_restriction,
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
                "delete-file-count": 0,
                "child-plan-task-count": 0
            })
        };
        let store = Arc::new(RecordingOutboxStore {
            events: Mutex::new(vec![OutboxEvent {
                event_id: event_id.to_string(),
                sink: "lakecat.lineage-and-graph".to_string(),
                event_type: event_type.to_string(),
                payload: json!({
                    "audit-event-id": audit_event_id,
                    "event-type": event_type,
                    "table": ident,
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
            .expect_err("scan read-restriction evidence should reject extra fields");

        let message = err.to_string();
        assert!(message.contains(event_type));
        assert!(message.contains("event-id-hash=sha256:"));
        assert!(message.contains(&format!(
            "{label} read-restriction contains unexpected field unverified-restriction-claim"
        )));
        assert!(!message.contains(event_id));
        assert!(
            store.delivered.lock().await.is_empty(),
            "extra read-restriction replay must fail before acknowledgement"
        );
        assert!(
            graph.events.lock().await.is_empty(),
            "extra read-restriction replay must fail before graph projection"
        );
        assert!(
            lineage.events.lock().await.is_empty(),
            "extra read-restriction replay must fail before lineage projection"
        );
    }
}

#[tokio::test]
async fn outbox_drain_rejects_scan_extra_row_predicate_fields() {
    let cases = [
        (
            "table.scan-planned",
            "scan-planned",
            "evt-scan-plan-extra-row-predicate",
            "audit-scan-plan-extra-row-predicate",
        ),
        (
            "table.scan-tasks-fetched",
            "scan-tasks-fetched",
            "evt-scan-fetch-extra-row-predicate",
            "audit-scan-fetch-extra-row-predicate",
        ),
    ];

    for (event_type, label, event_id, audit_event_id) in cases {
        let principal = Principal::new("agent:reader", PrincipalKind::Agent).unwrap();
        let ident = table_ident("local", "default", "events").unwrap();
        let read_restriction = json!({
            "allowed-columns": ["event_id"],
            "row-predicate": {
                "type": "not-eq",
                "term": "severity",
                "value": "debug",
                "unverified-predicate-claim": "already-pruned"
            },
            "purpose": "qglake-agent-demo",
            "max-credential-ttl-seconds": 300,
            "policy-hashes": [
                content_hash_json(&json!({"policy-id": "agent-read", "scope": "default.events"}))
                    .unwrap()
            ]
        });
        let payload = if event_type == "table.scan-planned" {
            json!({
                "table": ident,
                "authorization-receipt": {
                    "principal": principal,
                    "action": "table-plan-scan",
                    "allowed": true,
                    "engine": "test",
                    "policy_hash": "sha256:policy",
                    "context": {
                        "read-restriction": read_restriction
                    },
                    "checked_at": chrono::Utc::now(),
                },
                "read-restriction": read_restriction,
                "requested-projection": ["event_id"],
                "effective-projection": ["event_id"],
                "requested-stats-fields": ["event_id"],
                "effective-stats-fields": ["event_id"],
                "scan-task-count": 1
            })
        } else {
            json!({
                "table": ident,
                "authorization-receipt": {
                    "principal": principal,
                    "action": "table-plan-scan",
                    "allowed": true,
                    "engine": "test",
                    "policy_hash": "sha256:policy",
                    "context": {
                        "read-restriction": read_restriction
                    },
                    "checked_at": chrono::Utc::now(),
                },
                "read-restriction": read_restriction,
                "required-projection": ["event_id"],
                "effective-projection": ["event_id"],
                "requested-stats-fields": ["event_id"],
                "effective-stats-fields": ["event_id"],
                "required-filters": [{
                    "type": "not-eq",
                    "term": "severity",
                    "value": "debug",
                    "unverified-predicate-claim": "already-pruned"
                }],
                "file-scan-task-count": 1,
                "delete-file-count": 0,
                "child-plan-task-count": 0
            })
        };
        let store = Arc::new(RecordingOutboxStore {
            events: Mutex::new(vec![OutboxEvent {
                event_id: event_id.to_string(),
                sink: "lakecat.lineage-and-graph".to_string(),
                event_type: event_type.to_string(),
                payload: json!({
                    "audit-event-id": audit_event_id,
                    "event-type": event_type,
                    "table": ident,
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
            .expect_err("scan row-predicate evidence should reject extra fields");

        let message = err.to_string();
        assert!(message.contains(event_type));
        assert!(message.contains("event-id-hash=sha256:"));
        assert!(message.contains(&format!(
            "{label} read-restriction row-predicate contains unexpected field unverified-predicate-claim"
        )));
        assert!(!message.contains(event_id));
        assert!(
            store.delivered.lock().await.is_empty(),
            "extra row-predicate replay must fail before acknowledgement"
        );
        assert!(
            graph.events.lock().await.is_empty(),
            "extra row-predicate replay must fail before graph projection"
        );
        assert!(
            lineage.events.lock().await.is_empty(),
            "extra row-predicate replay must fail before lineage projection"
        );
    }
}

#[tokio::test]
async fn outbox_drain_rejects_scan_blank_row_predicate_type() {
    let cases = [
        (
            "table.scan-planned",
            "scan-planned",
            "evt-scan-plan-blank-row-predicate-type",
            "audit-scan-plan-blank-row-predicate-type",
        ),
        (
            "table.scan-tasks-fetched",
            "scan-tasks-fetched",
            "evt-scan-fetch-blank-row-predicate-type",
            "audit-scan-fetch-blank-row-predicate-type",
        ),
    ];

    for (event_type, label, event_id, audit_event_id) in cases {
        let principal = Principal::new("agent:reader", PrincipalKind::Agent).unwrap();
        let ident = table_ident("local", "default", "events").unwrap();
        let read_restriction = json!({
            "allowed-columns": ["event_id"],
            "row-predicate": {
                "type": " ",
                "term": "severity",
                "value": "debug"
            },
            "purpose": "qglake-agent-demo",
            "max-credential-ttl-seconds": 300,
            "policy-hashes": [
                content_hash_json(&json!({"policy-id": "agent-read", "scope": "default.events"}))
                    .unwrap()
            ]
        });
        let payload = if event_type == "table.scan-planned" {
            json!({
                "table": ident,
                "authorization-receipt": {
                    "principal": principal,
                    "action": "table-plan-scan",
                    "allowed": true,
                    "engine": "test",
                    "policy_hash": "sha256:policy",
                    "context": {
                        "read-restriction": read_restriction
                    },
                    "checked_at": chrono::Utc::now(),
                },
                "read-restriction": read_restriction,
                "requested-projection": ["event_id"],
                "effective-projection": ["event_id"],
                "requested-stats-fields": ["event_id"],
                "effective-stats-fields": ["event_id"],
                "scan-task-count": 1
            })
        } else {
            json!({
                "table": ident,
                "authorization-receipt": {
                    "principal": principal,
                    "action": "table-plan-scan",
                    "allowed": true,
                    "engine": "test",
                    "policy_hash": "sha256:policy",
                    "context": {
                        "read-restriction": read_restriction
                    },
                    "checked_at": chrono::Utc::now(),
                },
                "read-restriction": read_restriction,
                "required-projection": ["event_id"],
                "effective-projection": ["event_id"],
                "requested-stats-fields": ["event_id"],
                "effective-stats-fields": ["event_id"],
                "required-filters": [{
                    "type": " ",
                    "term": "severity",
                    "value": "debug"
                }],
                "file-scan-task-count": 1,
                "delete-file-count": 0,
                "child-plan-task-count": 0
            })
        };
        let store = Arc::new(RecordingOutboxStore {
            events: Mutex::new(vec![OutboxEvent {
                event_id: event_id.to_string(),
                sink: "lakecat.lineage-and-graph".to_string(),
                event_type: event_type.to_string(),
                payload: json!({
                    "audit-event-id": audit_event_id,
                    "event-type": event_type,
                    "table": ident,
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
            .expect_err("scan row-predicate type evidence must not be blank");

        let message = err.to_string();
        assert!(message.contains(event_type));
        assert!(message.contains("event-id-hash=sha256:"));
        assert!(message.contains(&format!(
            "{label} read-restriction row-predicate.type must not be blank"
        )));
        assert!(!message.contains(event_id));
        assert!(
            store.delivered.lock().await.is_empty(),
            "blank row-predicate type replay must fail before acknowledgement"
        );
        assert!(
            graph.events.lock().await.is_empty(),
            "blank row-predicate type replay must fail before graph projection"
        );
        assert!(
            lineage.events.lock().await.is_empty(),
            "blank row-predicate type replay must fail before lineage projection"
        );
    }
}

#[tokio::test]
async fn outbox_drain_rejects_scan_missing_row_predicate_value() {
    let cases = [
        (
            "table.scan-planned",
            "scan-planned",
            "evt-scan-plan-missing-row-predicate-value",
            "audit-scan-plan-missing-row-predicate-value",
        ),
        (
            "table.scan-tasks-fetched",
            "scan-tasks-fetched",
            "evt-scan-fetch-missing-row-predicate-value",
            "audit-scan-fetch-missing-row-predicate-value",
        ),
    ];

    for (event_type, label, event_id, audit_event_id) in cases {
        let principal = Principal::new("agent:reader", PrincipalKind::Agent).unwrap();
        let ident = table_ident("local", "default", "events").unwrap();
        let read_restriction = json!({
            "allowed-columns": ["event_id"],
            "row-predicate": {
                "type": "eq",
                "term": "severity"
            },
            "purpose": "qglake-agent-demo",
            "max-credential-ttl-seconds": 300,
            "policy-hashes": [
                content_hash_json(&json!({"policy-id": "agent-read", "scope": "default.events"}))
                    .unwrap()
            ]
        });
        let payload = if event_type == "table.scan-planned" {
            json!({
                "table": ident,
                "authorization-receipt": {
                    "principal": principal,
                    "action": "table-plan-scan",
                    "allowed": true,
                    "engine": "test",
                    "policy_hash": "sha256:policy",
                    "context": {
                        "read-restriction": read_restriction
                    },
                    "checked_at": chrono::Utc::now(),
                },
                "read-restriction": read_restriction,
                "requested-projection": ["event_id"],
                "effective-projection": ["event_id"],
                "requested-stats-fields": ["event_id"],
                "effective-stats-fields": ["event_id"],
                "scan-task-count": 1
            })
        } else {
            json!({
                "table": ident,
                "authorization-receipt": {
                    "principal": principal,
                    "action": "table-plan-scan",
                    "allowed": true,
                    "engine": "test",
                    "policy_hash": "sha256:policy",
                    "context": {
                        "read-restriction": read_restriction
                    },
                    "checked_at": chrono::Utc::now(),
                },
                "read-restriction": read_restriction,
                "required-projection": ["event_id"],
                "effective-projection": ["event_id"],
                "requested-stats-fields": ["event_id"],
                "effective-stats-fields": ["event_id"],
                "required-filters": [{
                    "type": "eq",
                    "term": "severity"
                }],
                "file-scan-task-count": 1,
                "delete-file-count": 0,
                "child-plan-task-count": 0
            })
        };
        let store = Arc::new(RecordingOutboxStore {
            events: Mutex::new(vec![OutboxEvent {
                event_id: event_id.to_string(),
                sink: "lakecat.lineage-and-graph".to_string(),
                event_type: event_type.to_string(),
                payload: json!({
                    "audit-event-id": audit_event_id,
                    "event-type": event_type,
                    "table": ident,
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
            .expect_err("scan row-predicate value evidence must be present");

        let message = err.to_string();
        assert!(message.contains(event_type));
        assert!(message.contains("event-id-hash=sha256:"));
        assert!(message.contains(&format!(
            "{label} read-restriction row-predicate.value is required for eq predicate evidence"
        )));
        assert!(!message.contains(event_id));
        assert!(
            store.delivered.lock().await.is_empty(),
            "missing row-predicate value replay must fail before acknowledgement"
        );
        assert!(
            graph.events.lock().await.is_empty(),
            "missing row-predicate value replay must fail before graph projection"
        );
        assert!(
            lineage.events.lock().await.is_empty(),
            "missing row-predicate value replay must fail before lineage projection"
        );
    }
}

#[tokio::test]
async fn outbox_drain_rejects_scan_planned_termless_row_predicate() {
    let principal = Principal::new("agent:reader", PrincipalKind::Agent).unwrap();
    let ident = table_ident("local", "default", "events").unwrap();
    let read_restriction = json!({
        "allowed-columns": ["event_id"],
        "row-predicate": {
            "type": "not-eq",
            "value": "debug"
        },
        "purpose": "qglake-agent-demo",
        "max-credential-ttl-seconds": 300,
        "policy-hashes": [
            content_hash_json(&json!({"policy-id": "agent-read", "scope": "default.events"}))
                .unwrap()
        ]
    });
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-scan-plan-termless-row-predicate".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "table.scan-planned".to_string(),
            payload: json!({
                "audit-event-id": "audit-scan-plan-termless-row-predicate",
                "event-type": "table.scan-planned",
                "table": ident,
                "payload": {
                    "table": ident,
                    "authorization-receipt": {
                        "principal": principal,
                        "action": "table-plan-scan",
                        "allowed": true,
                        "engine": "test",
                        "policy_hash": "sha256:policy",
                        "context": {
                            "read-restriction": read_restriction
                        },
                        "checked_at": chrono::Utc::now(),
                    },
                    "read-restriction": read_restriction,
                    "requested-projection": ["event_id"],
                    "effective-projection": ["event_id"],
                    "requested-stats-fields": ["event_id"],
                    "effective-stats-fields": ["event_id"],
                    "scan-task-count": 1
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
        .expect_err("scan-planned row-predicate term evidence should be required");

    let message = err.to_string();
    assert!(
        message.contains("outbox event table.scan-planned (lakecat.lineage-and-graph) has invalid")
    );
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(message.contains("scan-planned read-restriction row-predicate.term"));
    assert!(!message.contains("evt-scan-plan-termless-row-predicate"));
    assert!(
        store.delivered.lock().await.is_empty(),
        "termless planned row-predicate scan replay must fail before acknowledgement"
    );
    assert!(
        graph.events.lock().await.is_empty(),
        "termless planned row-predicate scan replay must fail before graph projection"
    );
    assert!(
        lineage.events.lock().await.is_empty(),
        "termless planned row-predicate scan replay must fail before lineage projection"
    );
}

#[tokio::test]
async fn outbox_drain_rejects_scan_planned_stats_field_policy_drift() {
    let principal = Principal::new("agent:reader", PrincipalKind::Agent).unwrap();
    let ident = table_ident("local", "default", "events").unwrap();
    let read_restriction = json!({
        "allowed-columns": ["event_id"],
        "row-predicate": {
            "type": "not-eq",
            "term": "severity",
            "value": "debug"
        },
        "policy-hashes": [
            content_hash_json(&json!({"policy-id": "agent-read", "scope": "default.events"}))
                .unwrap()
        ]
    });
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-scan-plan-stats-policy-drift".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "table.scan-planned".to_string(),
            payload: json!({
                "audit-event-id": "audit-scan-plan-stats-policy-drift",
                "event-type": "table.scan-planned",
                "table": ident,
                "payload": {
                    "table": ident,
                    "authorization-receipt": {
                        "principal": principal,
                        "action": "table-plan-scan",
                        "allowed": true,
                        "engine": "test",
                        "policy_hash": "sha256:policy",
                        "context": {
                            "read-restriction": read_restriction
                        },
                        "checked_at": chrono::Utc::now(),
                    },
                    "read-restriction": read_restriction,
                    "requested-projection": ["event_id"],
                    "effective-projection": ["event_id"],
                    "requested-stats-fields": ["event_id", "payload"],
                    "effective-stats-fields": ["event_id", "payload"],
                    "scan-task-count": 1
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
        .expect_err("scan-plan stats fields must stay inside the read restriction");

    let message = err.to_string();
    assert!(
        message.contains("outbox event table.scan-planned (lakecat.lineage-and-graph) has invalid")
    );
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(
        message.contains("scan-planned effective-stats-fields must be allowed by read-restriction")
    );
    assert!(!message.contains("evt-scan-plan-stats-policy-drift"));
    assert!(
        store.delivered.lock().await.is_empty(),
        "drifted scan-plan stats evidence must fail before acknowledgement"
    );
    assert!(
        graph.events.lock().await.is_empty(),
        "drifted scan-plan stats evidence must fail before graph projection"
    );
    assert!(
        lineage.events.lock().await.is_empty(),
        "drifted scan-plan stats evidence must fail before lineage projection"
    );
}

#[tokio::test]
async fn outbox_drain_rejects_scan_planned_duplicate_requested_stats_field() {
    let principal = Principal::new("agent:reader", PrincipalKind::Agent).unwrap();
    let ident = table_ident("local", "default", "events").unwrap();
    let read_restriction = json!({
        "allowed-columns": ["event_id"],
        "row-predicate": {
            "type": "not-eq",
            "term": "severity",
            "value": "debug"
        },
        "policy-hashes": [
            content_hash_json(&json!({"policy-id": "agent-read", "scope": "default.events"}))
                .unwrap()
        ]
    });
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-scan-plan-duplicate-requested-stats-field".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "table.scan-planned".to_string(),
            payload: json!({
                "audit-event-id": "audit-scan-plan-duplicate-requested-stats-field",
                "event-type": "table.scan-planned",
                "table": ident,
                "payload": {
                    "table": ident,
                    "authorization-receipt": {
                        "principal": principal,
                        "action": "table-plan-scan",
                        "allowed": true,
                        "engine": "test",
                        "policy_hash": "sha256:policy",
                        "context": {
                            "read-restriction": read_restriction
                        },
                        "checked_at": chrono::Utc::now(),
                    },
                    "read-restriction": read_restriction,
                    "requested-projection": ["event_id"],
                    "effective-projection": ["event_id"],
                    "requested-stats-fields": ["event_id", "event_id"],
                    "effective-stats-fields": ["event_id"],
                    "scan-task-count": 1
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
        .expect_err("duplicate requested stats-field evidence should fail closed");

    let message = err.to_string();
    assert!(
        message.contains("outbox event table.scan-planned (lakecat.lineage-and-graph) has invalid")
    );
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(
        message.contains("scan-planned requested-stats-fields must be duplicate-free"),
        "{message}"
    );
    assert!(!message.contains("evt-scan-plan-duplicate-requested-stats-field"));
    assert!(
        store.delivered.lock().await.is_empty(),
        "duplicate stats-field scan replay must fail before acknowledgement"
    );
    assert!(
        graph.events.lock().await.is_empty(),
        "duplicate stats-field scan replay must fail before graph projection"
    );
    assert!(
        lineage.events.lock().await.is_empty(),
        "duplicate stats-field scan replay must fail before lineage projection"
    );
}

#[tokio::test]
async fn outbox_drain_rejects_scan_planned_duplicate_effective_stats_field() {
    let principal = Principal::new("agent:reader", PrincipalKind::Agent).unwrap();
    let ident = table_ident("local", "default", "events").unwrap();
    let read_restriction = json!({
        "allowed-columns": ["event_id"],
        "row-predicate": {
            "type": "not-eq",
            "term": "severity",
            "value": "debug"
        },
        "policy-hashes": [
            content_hash_json(&json!({"policy-id": "agent-read", "scope": "default.events"}))
                .unwrap()
        ]
    });
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-scan-plan-duplicate-effective-stats-field".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "table.scan-planned".to_string(),
            payload: json!({
                "audit-event-id": "audit-scan-plan-duplicate-effective-stats-field",
                "event-type": "table.scan-planned",
                "table": ident,
                "payload": {
                    "table": ident,
                    "authorization-receipt": {
                        "principal": principal,
                        "action": "table-plan-scan",
                        "allowed": true,
                        "engine": "test",
                        "policy_hash": "sha256:policy",
                        "context": {
                            "read-restriction": read_restriction
                        },
                        "checked_at": chrono::Utc::now(),
                    },
                    "read-restriction": read_restriction,
                    "requested-projection": ["event_id"],
                    "effective-projection": ["event_id"],
                    "requested-stats-fields": ["event_id"],
                    "effective-stats-fields": ["event_id", "event_id"],
                    "scan-task-count": 1
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
        .expect_err("duplicate effective stats-field evidence should fail closed");

    let message = err.to_string();
    assert!(
        message.contains("outbox event table.scan-planned (lakecat.lineage-and-graph) has invalid")
    );
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(
        message.contains("scan-planned effective-stats-fields must be duplicate-free"),
        "{message}"
    );
    assert!(!message.contains("evt-scan-plan-duplicate-effective-stats-field"));
    assert!(
        store.delivered.lock().await.is_empty(),
        "duplicate effective stats-field scan replay must fail before acknowledgement"
    );
    assert!(
        graph.events.lock().await.is_empty(),
        "duplicate effective stats-field scan replay must fail before graph projection"
    );
    assert!(
        lineage.events.lock().await.is_empty(),
        "duplicate effective stats-field scan replay must fail before lineage projection"
    );
}

#[tokio::test]
async fn outbox_drain_rejects_scan_planned_blank_requested_stats_field() {
    let principal = Principal::new("agent:reader", PrincipalKind::Agent).unwrap();
    let ident = table_ident("local", "default", "events").unwrap();
    let read_restriction = json!({
        "allowed-columns": ["event_id"],
        "row-predicate": {
            "type": "not-eq",
            "term": "severity",
            "value": "debug"
        },
        "policy-hashes": [
            content_hash_json(&json!({"policy-id": "agent-read", "scope": "default.events"}))
                .unwrap()
        ]
    });
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-scan-plan-blank-requested-stats-field".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "table.scan-planned".to_string(),
            payload: json!({
                "audit-event-id": "audit-scan-plan-blank-requested-stats-field",
                "event-type": "table.scan-planned",
                "table": ident,
                "payload": {
                    "table": ident,
                    "authorization-receipt": {
                        "principal": principal,
                        "action": "table-plan-scan",
                        "allowed": true,
                        "engine": "test",
                        "policy_hash": "sha256:policy",
                        "context": {
                            "read-restriction": read_restriction
                        },
                        "checked_at": chrono::Utc::now(),
                    },
                    "read-restriction": read_restriction,
                    "requested-projection": ["event_id"],
                    "effective-projection": ["event_id"],
                    "requested-stats-fields": ["event_id", " "],
                    "effective-stats-fields": ["event_id"],
                    "scan-task-count": 1
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
        .expect_err("blank requested stats-field evidence should fail closed");

    let message = err.to_string();
    assert!(
        message.contains("outbox event table.scan-planned (lakecat.lineage-and-graph) has invalid")
    );
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(message.contains("scan-planned requested-stats-fields must contain non-empty strings"));
    assert!(!message.contains("evt-scan-plan-blank-requested-stats-field"));
    assert!(
        store.delivered.lock().await.is_empty(),
        "blank stats-field scan replay must fail before acknowledgement"
    );
    assert!(
        graph.events.lock().await.is_empty(),
        "blank stats-field scan replay must fail before graph projection"
    );
    assert!(
        lineage.events.lock().await.is_empty(),
        "blank stats-field scan replay must fail before lineage projection"
    );
}

#[tokio::test]
async fn outbox_drain_rejects_scan_fetch_empty_allowed_columns() {
    let principal = Principal::new("agent:reader", PrincipalKind::Agent).unwrap();
    let ident = table_ident("local", "default", "events").unwrap();
    let read_restriction = json!({
        "allowed-columns": [],
        "policy-hashes": [
            content_hash_json(&json!({"policy-id": "agent-read", "scope": "default.events"}))
                .unwrap()
        ]
    });
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-scan-fetch-empty-allowed-columns".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "table.scan-tasks-fetched".to_string(),
            payload: json!({
                "audit-event-id": "audit-scan-fetch-empty-allowed-columns",
                "event-type": "table.scan-tasks-fetched",
                "table": ident,
                "payload": {
                    "table": ident,
                    "authorization-receipt": {
                        "principal": principal,
                        "action": "table-plan-scan",
                        "allowed": true,
                        "engine": "test",
                        "policy_hash": "sha256:policy",
                        "context": {
                            "read-restriction": read_restriction
                        },
                        "checked_at": chrono::Utc::now(),
                    },
                    "read-restriction": read_restriction,
                    "required-projection": ["event_id"],
                    "effective-projection": ["event_id"],
                    "requested-stats-fields": ["event_id"],
                    "effective-stats-fields": ["event_id"],
                    "required-filters": [],
                    "file-scan-task-count": 1,
                    "delete-file-count": 0,
                    "child-plan-task-count": 0
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
        .expect_err("scan-fetch empty allowed-columns evidence should fail closed");

    let message = err.to_string();
    assert!(
        message.contains(
            "outbox event table.scan-tasks-fetched (lakecat.lineage-and-graph) has invalid"
        )
    );
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(
        message.contains("scan-tasks-fetched read-restriction allowed-columns must not be empty")
    );
    assert!(!message.contains("evt-scan-fetch-empty-allowed-columns"));
    assert!(
        store.delivered.lock().await.is_empty(),
        "empty allowed-column fetch replay must fail before acknowledgement"
    );
    assert!(
        graph.events.lock().await.is_empty(),
        "empty allowed-column fetch replay must fail before graph projection"
    );
    assert!(
        lineage.events.lock().await.is_empty(),
        "empty allowed-column fetch replay must fail before lineage projection"
    );
}

#[tokio::test]
async fn outbox_drain_rejects_scan_fetch_duplicate_allowed_columns() {
    let principal = Principal::new("agent:reader", PrincipalKind::Agent).unwrap();
    let ident = table_ident("local", "default", "events").unwrap();
    let read_restriction = json!({
        "allowed-columns": ["event_id", "event_id"],
        "policy-hashes": [
            content_hash_json(&json!({"policy-id": "agent-read", "scope": "default.events"}))
                .unwrap()
        ]
    });
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-scan-fetch-duplicate-allowed-columns".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "table.scan-tasks-fetched".to_string(),
            payload: json!({
                "audit-event-id": "audit-scan-fetch-duplicate-allowed-columns",
                "event-type": "table.scan-tasks-fetched",
                "table": ident,
                "payload": {
                    "table": ident,
                    "authorization-receipt": {
                        "principal": principal,
                        "action": "table-plan-scan",
                        "allowed": true,
                        "engine": "test",
                        "policy_hash": "sha256:policy",
                        "context": {
                            "read-restriction": read_restriction
                        },
                        "checked_at": chrono::Utc::now(),
                    },
                    "read-restriction": read_restriction,
                    "required-projection": ["event_id"],
                    "effective-projection": ["event_id"],
                    "requested-stats-fields": ["event_id"],
                    "effective-stats-fields": ["event_id"],
                    "required-filters": [],
                    "file-scan-task-count": 1,
                    "delete-file-count": 0,
                    "child-plan-task-count": 0
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
        .expect_err("scan-fetch duplicate allowed-columns evidence should fail closed");

    let message = err.to_string();
    assert!(
        message.contains(
            "outbox event table.scan-tasks-fetched (lakecat.lineage-and-graph) has invalid"
        )
    );
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(
        message
            .contains("scan-tasks-fetched read-restriction allowed-columns must be duplicate-free")
    );
    assert!(!message.contains("evt-scan-fetch-duplicate-allowed-columns"));
    assert!(
        store.delivered.lock().await.is_empty(),
        "duplicate allowed-column fetch replay must fail before acknowledgement"
    );
    assert!(
        graph.events.lock().await.is_empty(),
        "duplicate allowed-column fetch replay must fail before graph projection"
    );
    assert!(
        lineage.events.lock().await.is_empty(),
        "duplicate allowed-column fetch replay must fail before lineage projection"
    );
}

#[tokio::test]
async fn outbox_drain_rejects_malformed_scan_fetch_evidence() {
    let principal = Principal::new("agent:reader", PrincipalKind::Agent).unwrap();
    let ident = table_ident("local", "default", "events").unwrap();
    let policy_hash =
        content_hash_json(&json!({"policy-id": "agent-read", "scope": "default.events"})).unwrap();
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-secret-scan-fetch-token".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "table.scan-tasks-fetched".to_string(),
            payload: json!({
                "audit-event-id": "audit-corrupt-scan-fetch",
                "event-type": "table.scan-tasks-fetched",
                "table": ident,
                "payload": {
                    "table": ident,
                    "authorization-receipt": {
                        "principal": principal,
                        "action": "table-plan-scan",
                        "allowed": true,
                        "engine": "test",
                        "policy_hash": "sha256:policy",
                        "checked_at": chrono::Utc::now(),
                    },
                    "read-restriction": {
                        "allowed-columns": ["event_id"],
                        "policy-hashes": [policy_hash]
                    },
                    "required-projection": ["event_id"],
                    "effective-projection": ["payload"],
                    "required-filters": [],
                    "file-scan-task-count": 1,
                    "delete-file-count": 0,
                    "child-plan-task-count": 0
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
        .expect_err("drifted scan-task fetch projection evidence should fail");

    let message = err.to_string();
    assert!(
        message.contains(
            "outbox event table.scan-tasks-fetched (lakecat.lineage-and-graph) has invalid"
        )
    );
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(message.contains(
        "scan-tasks-fetched effective-projection must be a subset of required-projection"
    ));
    assert!(!message.contains("evt-secret-scan-fetch-token"));
    assert!(
        store.delivered.lock().await.is_empty(),
        "malformed scan-task fetch evidence must fail before acknowledgement"
    );
    assert!(
        graph.events.lock().await.is_empty(),
        "malformed scan-task fetch evidence must fail before graph projection"
    );
    assert!(
        lineage.events.lock().await.is_empty(),
        "malformed scan-task fetch evidence must fail before lineage projection"
    );
}

#[tokio::test]
async fn outbox_drain_rejects_scan_fetch_empty_required_projection() {
    let principal = Principal::new("agent:reader", PrincipalKind::Agent).unwrap();
    let ident = table_ident("local", "default", "events").unwrap();
    let read_restriction = json!({
        "allowed-columns": ["event_id"],
        "row-predicate": {
            "type": "not-eq",
            "term": "severity",
            "value": "debug"
        },
        "policy-hashes": [
            content_hash_json(&json!({"policy-id": "agent-read", "scope": "default.events"}))
                .unwrap()
        ]
    });
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-scan-fetch-empty-required-projection".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "table.scan-tasks-fetched".to_string(),
            payload: json!({
                "audit-event-id": "audit-scan-fetch-empty-required-projection",
                "event-type": "table.scan-tasks-fetched",
                "table": ident,
                "payload": {
                    "table": ident,
                    "authorization-receipt": {
                        "principal": principal,
                        "action": "table-plan-scan",
                        "allowed": true,
                        "engine": "test",
                        "policy_hash": "sha256:policy",
                        "context": {
                            "read-restriction": read_restriction
                        },
                        "checked_at": chrono::Utc::now(),
                    },
                    "read-restriction": read_restriction,
                    "required-projection": [],
                    "effective-projection": [],
                    "required-filters": [],
                    "file-scan-task-count": 1,
                    "delete-file-count": 0,
                    "child-plan-task-count": 0
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
        .expect_err("empty fetched required projection evidence should fail closed");

    let message = err.to_string();
    assert!(
        message.contains(
            "outbox event table.scan-tasks-fetched (lakecat.lineage-and-graph) has invalid"
        )
    );
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(
        message.contains("scan-tasks-fetched required-projection must not be empty"),
        "{message}"
    );
    assert!(!message.contains("evt-scan-fetch-empty-required-projection"));
    assert!(
        store.delivered.lock().await.is_empty(),
        "empty fetched projection replay must fail before acknowledgement"
    );
    assert!(
        graph.events.lock().await.is_empty(),
        "empty fetched projection replay must fail before graph projection"
    );
    assert!(
        lineage.events.lock().await.is_empty(),
        "empty fetched projection replay must fail before lineage projection"
    );
}

#[tokio::test]
async fn outbox_drain_rejects_scan_fetch_duplicate_required_projection() {
    let principal = Principal::new("agent:reader", PrincipalKind::Agent).unwrap();
    let ident = table_ident("local", "default", "events").unwrap();
    let read_restriction = json!({
        "allowed-columns": ["event_id"],
        "row-predicate": {
            "type": "not-eq",
            "term": "severity",
            "value": "debug"
        },
        "policy-hashes": [
            content_hash_json(&json!({"policy-id": "agent-read", "scope": "default.events"}))
                .unwrap()
        ]
    });
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-scan-fetch-duplicate-required-projection".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "table.scan-tasks-fetched".to_string(),
            payload: json!({
                "audit-event-id": "audit-scan-fetch-duplicate-required-projection",
                "event-type": "table.scan-tasks-fetched",
                "table": ident,
                "payload": {
                    "table": ident,
                    "authorization-receipt": {
                        "principal": principal,
                        "action": "table-plan-scan",
                        "allowed": true,
                        "engine": "test",
                        "policy_hash": "sha256:policy",
                        "context": {
                            "read-restriction": read_restriction
                        },
                        "checked_at": chrono::Utc::now(),
                    },
                    "read-restriction": read_restriction,
                    "required-projection": ["event_id", "event_id"],
                    "effective-projection": ["event_id"],
                    "requested-stats-fields": ["event_id"],
                    "effective-stats-fields": ["event_id"],
                    "required-filters": [],
                    "file-scan-task-count": 1,
                    "delete-file-count": 0,
                    "child-plan-task-count": 0
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
        .expect_err("duplicate fetched projection evidence should fail closed");

    let message = err.to_string();
    assert!(
        message.contains(
            "outbox event table.scan-tasks-fetched (lakecat.lineage-and-graph) has invalid"
        )
    );
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(
        message.contains("scan-tasks-fetched required-projection must be duplicate-free"),
        "{message}"
    );
    assert!(!message.contains("evt-scan-fetch-duplicate-required-projection"));
    assert!(
        store.delivered.lock().await.is_empty(),
        "duplicate fetched projection replay must fail before acknowledgement"
    );
    assert!(
        graph.events.lock().await.is_empty(),
        "duplicate fetched projection replay must fail before graph projection"
    );
    assert!(
        lineage.events.lock().await.is_empty(),
        "duplicate fetched projection replay must fail before lineage projection"
    );
}

#[tokio::test]
async fn outbox_drain_rejects_scan_fetch_duplicate_effective_projection() {
    let principal = Principal::new("agent:reader", PrincipalKind::Agent).unwrap();
    let ident = table_ident("local", "default", "events").unwrap();
    let read_restriction = json!({
        "allowed-columns": ["event_id"],
        "row-predicate": {
            "type": "not-eq",
            "term": "severity",
            "value": "debug"
        },
        "policy-hashes": [
            content_hash_json(&json!({"policy-id": "agent-read", "scope": "default.events"}))
                .unwrap()
        ]
    });
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-scan-fetch-duplicate-effective-projection".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "table.scan-tasks-fetched".to_string(),
            payload: json!({
                "audit-event-id": "audit-scan-fetch-duplicate-effective-projection",
                "event-type": "table.scan-tasks-fetched",
                "table": ident,
                "payload": {
                    "table": ident,
                    "authorization-receipt": {
                        "principal": principal,
                        "action": "table-plan-scan",
                        "allowed": true,
                        "engine": "test",
                        "policy_hash": "sha256:policy",
                        "context": {
                            "read-restriction": read_restriction
                        },
                        "checked_at": chrono::Utc::now(),
                    },
                    "read-restriction": read_restriction,
                    "required-projection": ["event_id"],
                    "effective-projection": ["event_id", "event_id"],
                    "required-filters": [],
                    "file-scan-task-count": 1,
                    "delete-file-count": 0,
                    "child-plan-task-count": 0
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
        .expect_err("duplicate fetched effective projection evidence should fail closed");

    let message = err.to_string();
    assert!(
        message.contains(
            "outbox event table.scan-tasks-fetched (lakecat.lineage-and-graph) has invalid"
        )
    );
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(
        message.contains("scan-tasks-fetched effective-projection must be duplicate-free"),
        "{message}"
    );
    assert!(!message.contains("evt-scan-fetch-duplicate-effective-projection"));
    assert!(
        store.delivered.lock().await.is_empty(),
        "duplicate effective projection replay must fail before acknowledgement"
    );
    assert!(
        graph.events.lock().await.is_empty(),
        "duplicate effective projection replay must fail before graph projection"
    );
    assert!(
        lineage.events.lock().await.is_empty(),
        "duplicate effective projection replay must fail before lineage projection"
    );
}

#[tokio::test]
async fn outbox_drain_rejects_extra_scan_fetch_outbox_payload_fields() {
    let principal = Principal::new("agent:reader", PrincipalKind::Agent).unwrap();
    let ident = table_ident("local", "default", "events").unwrap();
    let read_restriction = json!({
        "allowed-columns": ["event_id"],
        "row-predicate": {
            "type": "not-eq",
            "term": "severity",
            "value": "debug"
        },
        "purpose": "qglake-agent-demo",
        "max-credential-ttl-seconds": 300,
        "policy-hashes": [
            content_hash_json(&json!({"policy-id": "agent-read", "scope": "default.events"}))
                .unwrap()
        ]
    });
    let required_filter = json!({
        "type": "not-eq",
        "term": "severity",
        "value": "debug"
    });
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-scan-fetch-extra-wrapper-field".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "table.scan-tasks-fetched".to_string(),
            payload: json!({
                "audit-event-id": "audit-scan-fetch-extra-wrapper-field",
                "event-type": "table.scan-tasks-fetched",
                "table": ident,
                "payload": {
                    "table": ident,
                    "authorization-receipt": {
                        "principal": principal,
                        "action": "table-plan-scan",
                        "allowed": true,
                        "engine": "test",
                        "policy_hash": "sha256:policy",
                        "context": {
                            "read-restriction": read_restriction
                        },
                        "checked_at": chrono::Utc::now(),
                    },
                    "read-restriction": read_restriction,
                    "required-projection": ["event_id"],
                    "effective-projection": ["event_id"],
                    "requested-stats-fields": ["event_id"],
                    "effective-stats-fields": ["event_id"],
                    "stats-fields": ["event_id"],
                    "required-filters": [required_filter],
                    "file-scan-task-count": 1,
                    "delete-file-count": 0,
                    "child-plan-task-count": 0
                },
                "unverified-lineage-claim": "already-projected"
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
        .expect_err("extra scan-fetch wrapper fields should fail closed");

    let message = err.to_string();
    assert!(
        message.contains(
            "outbox event table.scan-tasks-fetched (lakecat.lineage-and-graph) has invalid"
        )
    );
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(message.contains(
        "scan-tasks-fetched outbox payload contains unexpected field unverified-lineage-claim"
    ));
    assert!(!message.contains("evt-scan-fetch-extra-wrapper-field"));
    assert!(
        store.delivered.lock().await.is_empty(),
        "extra scan-fetch wrapper proof must fail before acknowledgement"
    );
    assert!(
        graph.events.lock().await.is_empty(),
        "extra scan-fetch wrapper proof must fail before graph projection"
    );
    assert!(
        lineage.events.lock().await.is_empty(),
        "extra scan-fetch wrapper proof must fail before lineage projection"
    );
}

#[tokio::test]
async fn outbox_drain_rejects_scan_fetch_malformed_stats_field_evidence() {
    let principal = Principal::new("agent:reader", PrincipalKind::Agent).unwrap();
    let ident = table_ident("local", "default", "events").unwrap();
    let read_restriction = json!({
        "allowed-columns": ["event_id"],
        "row-predicate": {
            "type": "not-eq",
            "term": "severity",
            "value": "debug"
        },
        "purpose": "qglake-agent-demo",
        "max-credential-ttl-seconds": 300,
        "policy-hashes": [
            content_hash_json(&json!({"policy-id": "agent-read", "scope": "default.events"}))
                .unwrap()
        ]
    });
    let required_filter = json!({
        "type": "not-eq",
        "term": "severity",
        "value": "debug"
    });
    let base_payload = || {
        json!({
            "table": ident,
            "authorization-receipt": {
                "principal": principal,
                "action": "table-plan-scan",
                "allowed": true,
                "engine": "test",
                "policy_hash": "sha256:policy",
                "context": {
                    "read-restriction": read_restriction
                },
                "checked_at": chrono::Utc::now(),
            },
            "read-restriction": read_restriction,
            "required-projection": ["event_id"],
            "effective-projection": ["event_id"],
            "requested-stats-fields": ["event_id"],
            "effective-stats-fields": ["event_id"],
            "required-filters": [required_filter],
            "file-scan-task-count": 1,
            "delete-file-count": 0,
            "child-plan-task-count": 0
        })
    };
    let mut missing_requested = base_payload();
    missing_requested
        .as_object_mut()
        .unwrap()
        .remove("requested-stats-fields");
    let mut duplicate_effective = base_payload();
    duplicate_effective["effective-stats-fields"] = json!(["event_id", "event_id"]);

    for (event_id, payload, expected_message) in [
        (
            "evt-scan-fetch-missing-requested-stats-fields",
            missing_requested,
            "scan-tasks-fetched evidence must contain requested-stats-fields",
        ),
        (
            "evt-scan-fetch-duplicate-effective-stats-fields",
            duplicate_effective,
            "scan-tasks-fetched effective-stats-fields must be duplicate-free",
        ),
    ] {
        let store = Arc::new(RecordingOutboxStore {
            events: Mutex::new(vec![OutboxEvent {
                event_id: event_id.to_string(),
                sink: "lakecat.lineage-and-graph".to_string(),
                event_type: "table.scan-tasks-fetched".to_string(),
                payload: json!({
                    "audit-event-id": format!("audit-{event_id}"),
                    "event-type": "table.scan-tasks-fetched",
                    "table": ident,
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
            .expect_err("malformed fetched stats-field evidence should fail closed");

        let message = err.to_string();
        assert!(message.contains(
            "outbox event table.scan-tasks-fetched (lakecat.lineage-and-graph) has invalid"
        ));
        assert!(message.contains("event-id-hash=sha256:"));
        assert!(message.contains(expected_message), "{message}");
        assert!(!message.contains(event_id));
        assert!(
            store.delivered.lock().await.is_empty(),
            "malformed fetched stats-field replay must fail before acknowledgement"
        );
        assert!(
            graph.events.lock().await.is_empty(),
            "malformed fetched stats-field replay must fail before graph projection"
        );
        assert!(
            lineage.events.lock().await.is_empty(),
            "malformed fetched stats-field replay must fail before lineage projection"
        );
    }
}

#[tokio::test]
async fn outbox_drain_rejects_scan_fetch_termless_row_predicate() {
    let principal = Principal::new("agent:reader", PrincipalKind::Agent).unwrap();
    let ident = table_ident("local", "default", "events").unwrap();
    let read_restriction = json!({
        "allowed-columns": ["event_id"],
        "row-predicate": {
            "type": "not-eq",
            "value": "debug"
        },
        "policy-hashes": [
            content_hash_json(&json!({"policy-id": "agent-read", "scope": "default.events"}))
                .unwrap()
        ]
    });
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-scan-fetch-termless-row-predicate".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "table.scan-tasks-fetched".to_string(),
            payload: json!({
                "audit-event-id": "audit-scan-fetch-termless-row-predicate",
                "event-type": "table.scan-tasks-fetched",
                "table": ident,
                "payload": {
                    "table": ident,
                    "authorization-receipt": {
                        "principal": principal,
                        "action": "table-plan-scan",
                        "allowed": true,
                        "engine": "test",
                        "policy_hash": "sha256:policy",
                        "context": {
                            "read-restriction": read_restriction
                        },
                        "checked_at": chrono::Utc::now(),
                    },
                    "read-restriction": read_restriction,
                    "required-projection": ["event_id"],
                    "effective-projection": ["event_id"],
                    "requested-stats-fields": ["event_id"],
                    "effective-stats-fields": ["event_id"],
                    "required-filters": [{
                        "type": "not-eq",
                        "value": "debug"
                    }],
                    "file-scan-task-count": 1,
                    "delete-file-count": 0,
                    "child-plan-task-count": 0
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
        .expect_err("scan-fetch row-predicate term evidence should be required");

    let message = err.to_string();
    assert!(
        message.contains(
            "outbox event table.scan-tasks-fetched (lakecat.lineage-and-graph) has invalid"
        )
    );
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(message.contains("scan-tasks-fetched read-restriction row-predicate.term"));
    assert!(!message.contains("evt-scan-fetch-termless-row-predicate"));
    assert!(
        store.delivered.lock().await.is_empty(),
        "termless row-predicate scan replay must fail before acknowledgement"
    );
    assert!(
        graph.events.lock().await.is_empty(),
        "termless row-predicate scan replay must fail before graph projection"
    );
    assert!(
        lineage.events.lock().await.is_empty(),
        "termless row-predicate scan replay must fail before lineage projection"
    );
}

#[tokio::test]
async fn outbox_drain_rejects_scan_planned_missing_required_filters() {
    let principal = Principal::new("agent:reader", PrincipalKind::Agent).unwrap();
    let ident = table_ident("local", "default", "events").unwrap();
    let read_restriction = json!({
        "allowed-columns": ["event_id"],
        "row-predicate": {
            "type": "not-eq",
            "term": "severity",
            "value": "debug"
        },
        "purpose": "qglake-agent-demo",
        "max-credential-ttl-seconds": 300,
        "policy-hashes": [
            content_hash_json(&json!({"policy-id": "agent-read", "scope": "default.events"}))
                .unwrap()
        ]
    });
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-scan-plan-missing-required-filters".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "table.scan-planned".to_string(),
            payload: json!({
                "audit-event-id": "audit-scan-plan-missing-required-filters",
                "event-type": "table.scan-planned",
                "table": ident,
                "payload": {
                    "table": ident,
                    "authorization-receipt": {
                        "principal": principal,
                        "action": "table-plan-scan",
                        "allowed": true,
                        "engine": "test",
                        "policy_hash": "sha256:policy",
                        "context": {
                            "read-restriction": read_restriction
                        },
                        "checked_at": chrono::Utc::now(),
                    },
                    "read-restriction": read_restriction,
                    "requested-projection": ["event_id"],
                    "effective-projection": ["event_id"],
                    "requested-stats-fields": ["event_id"],
                    "effective-stats-fields": ["event_id"],
                    "scan-task-count": 1
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
        .expect_err("governed scan-plan required filters should be mandatory");

    let message = err.to_string();
    assert!(
        message.contains("outbox event table.scan-planned (lakecat.lineage-and-graph) has invalid")
    );
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(message.contains("scan-planned required-filters must be an array"));
    assert!(!message.contains("evt-scan-plan-missing-required-filters"));
    assert!(
        store.delivered.lock().await.is_empty(),
        "missing planned required-filters replay must fail before acknowledgement"
    );
    assert!(
        graph.events.lock().await.is_empty(),
        "missing planned required-filters replay must fail before graph projection"
    );
    assert!(
        lineage.events.lock().await.is_empty(),
        "missing planned required-filters replay must fail before lineage projection"
    );
}

#[tokio::test]
async fn outbox_drain_rejects_scan_planned_drifted_required_filters() {
    let principal = Principal::new("agent:reader", PrincipalKind::Agent).unwrap();
    let ident = table_ident("local", "default", "events").unwrap();
    let read_restriction = json!({
        "allowed-columns": ["event_id"],
        "row-predicate": {
            "type": "not-eq",
            "term": "severity",
            "value": "debug"
        },
        "purpose": "qglake-agent-demo",
        "max-credential-ttl-seconds": 300,
        "policy-hashes": [
            content_hash_json(&json!({"policy-id": "agent-read", "scope": "default.events"}))
                .unwrap()
        ]
    });
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-scan-plan-drifted-required-filters".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "table.scan-planned".to_string(),
            payload: json!({
                "audit-event-id": "audit-scan-plan-drifted-required-filters",
                "event-type": "table.scan-planned",
                "table": ident,
                "payload": {
                    "table": ident,
                    "authorization-receipt": {
                        "principal": principal,
                        "action": "table-plan-scan",
                        "allowed": true,
                        "engine": "test",
                        "policy_hash": "sha256:policy",
                        "context": {
                            "read-restriction": read_restriction
                        },
                        "checked_at": chrono::Utc::now(),
                    },
                    "read-restriction": read_restriction,
                    "requested-projection": ["event_id"],
                    "effective-projection": ["event_id"],
                    "requested-stats-fields": ["event_id"],
                    "effective-stats-fields": ["event_id"],
                    "required-filters": [{
                        "type": "eq",
                        "term": "severity",
                        "value": "info"
                    }],
                    "scan-task-count": 1
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
        .expect_err("scan-plan required filters should reject predicate drift");

    let message = err.to_string();
    assert!(
        message.contains("outbox event table.scan-planned (lakecat.lineage-and-graph) has invalid")
    );
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(message.contains(
        "scan-planned required-filters must exactly preserve read-restriction row-predicate"
    ));
    assert!(!message.contains("evt-scan-plan-drifted-required-filters"));
    assert!(
        store.delivered.lock().await.is_empty(),
        "drifted planned required-filters replay must fail before acknowledgement"
    );
    assert!(
        graph.events.lock().await.is_empty(),
        "drifted planned required-filters replay must fail before graph projection"
    );
    assert!(
        lineage.events.lock().await.is_empty(),
        "drifted planned required-filters replay must fail before lineage projection"
    );
}

#[tokio::test]
async fn outbox_drain_rejects_scan_planned_malformed_required_filters_without_restriction() {
    let principal = Principal::new("agent:reader", PrincipalKind::Agent).unwrap();
    let ident = table_ident("local", "default", "events").unwrap();
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-scan-plan-malformed-required-filters".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "table.scan-planned".to_string(),
            payload: json!({
                "audit-event-id": "audit-scan-plan-malformed-required-filters",
                "event-type": "table.scan-planned",
                "table": ident,
                "payload": {
                    "table": ident,
                    "authorization-receipt": {
                        "principal": principal,
                        "action": "table-plan-scan",
                        "allowed": true,
                        "engine": "test",
                        "policy_hash": "sha256:policy",
                        "checked_at": chrono::Utc::now(),
                    },
                    "requested-projection": ["event_id"],
                    "effective-projection": ["event_id"],
                    "requested-stats-fields": ["event_id"],
                    "effective-stats-fields": ["event_id"],
                    "required-filters": "always-true",
                    "scan-task-count": 1
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
        .expect_err("present planned required-filters must be array-shaped");

    let message = err.to_string();
    assert!(
        message.contains("outbox event table.scan-planned (lakecat.lineage-and-graph) has invalid")
    );
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(message.contains("scan-planned required-filters must be an array"));
    assert!(!message.contains("evt-scan-plan-malformed-required-filters"));
    assert!(
        store.delivered.lock().await.is_empty(),
        "malformed planned required-filters must fail before acknowledgement"
    );
    assert!(
        graph.events.lock().await.is_empty(),
        "malformed planned required-filters must fail before graph projection"
    );
    assert!(
        lineage.events.lock().await.is_empty(),
        "malformed planned required-filters must fail before lineage projection"
    );
}

#[tokio::test]
async fn outbox_drain_rejects_scan_planned_unsourced_required_filters() {
    let principal = Principal::new("agent:reader", PrincipalKind::Agent).unwrap();
    let ident = table_ident("local", "default", "events").unwrap();
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-scan-plan-unsourced-required-filters".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "table.scan-planned".to_string(),
            payload: json!({
                "audit-event-id": "audit-scan-plan-unsourced-required-filters",
                "event-type": "table.scan-planned",
                "table": ident,
                "payload": {
                    "table": ident,
                    "authorization-receipt": {
                        "principal": principal,
                        "action": "table-plan-scan",
                        "allowed": true,
                        "engine": "test",
                        "policy_hash": "sha256:policy",
                        "checked_at": chrono::Utc::now(),
                    },
                    "requested-projection": ["event_id"],
                    "effective-projection": ["event_id"],
                    "requested-stats-fields": ["event_id"],
                    "effective-stats-fields": ["event_id"],
                    "required-filters": [{
                        "type": "eq",
                        "term": "event_id",
                        "value": "evt-1"
                    }],
                    "scan-task-count": 1
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
        .expect_err("ungoverned scan-plan replay must not carry unsourced filters");

    let message = err.to_string();
    assert!(
        message.contains("outbox event table.scan-planned (lakecat.lineage-and-graph) has invalid")
    );
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(message.contains(
        "scan-planned required-filters must be empty without read-restriction row-predicate"
    ));
    assert!(!message.contains("evt-scan-plan-unsourced-required-filters"));
    assert!(
        store.delivered.lock().await.is_empty(),
        "unsourced required-filters replay must fail before acknowledgement"
    );
    assert!(
        graph.events.lock().await.is_empty(),
        "unsourced required-filters replay must fail before graph projection"
    );
    assert!(
        lineage.events.lock().await.is_empty(),
        "unsourced required-filters replay must fail before lineage projection"
    );
}

#[tokio::test]
async fn outbox_drain_rejects_scan_fetch_empty_required_filters() {
    let principal = Principal::new("agent:reader", PrincipalKind::Agent).unwrap();
    let ident = table_ident("local", "default", "events").unwrap();
    let read_restriction = json!({
        "allowed-columns": ["event_id"],
        "row-predicate": {
            "type": "not-eq",
            "term": "severity",
            "value": "debug"
        },
        "policy-hashes": [
            content_hash_json(&json!({"policy-id": "agent-read", "scope": "default.events"}))
                .unwrap()
        ]
    });
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-scan-fetch-empty-required-filters".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "table.scan-tasks-fetched".to_string(),
            payload: json!({
                "audit-event-id": "audit-scan-fetch-empty-required-filters",
                "event-type": "table.scan-tasks-fetched",
                "table": ident,
                "payload": {
                    "table": ident,
                    "authorization-receipt": {
                        "principal": principal,
                        "action": "table-plan-scan",
                        "allowed": true,
                        "engine": "test",
                        "policy_hash": "sha256:policy",
                        "context": {
                            "read-restriction": read_restriction
                        },
                        "checked_at": chrono::Utc::now(),
                    },
                    "read-restriction": read_restriction,
                    "required-projection": ["event_id"],
                    "effective-projection": ["event_id"],
                    "requested-stats-fields": ["event_id"],
                    "effective-stats-fields": ["event_id"],
                    "required-filters": [],
                    "file-scan-task-count": 1,
                    "delete-file-count": 0,
                    "child-plan-task-count": 0
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
        .expect_err("scan-fetch required filters should preserve row predicate");

    let message = err.to_string();
    assert!(
        message.contains(
            "outbox event table.scan-tasks-fetched (lakecat.lineage-and-graph) has invalid"
        )
    );
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(message.contains(
        "scan-tasks-fetched required-filters must exactly preserve read-restriction row-predicate"
    ));
    assert!(!message.contains("evt-scan-fetch-empty-required-filters"));
    assert!(
        store.delivered.lock().await.is_empty(),
        "empty required-filters replay must fail before acknowledgement"
    );
    assert!(
        graph.events.lock().await.is_empty(),
        "empty required-filters replay must fail before graph projection"
    );
    assert!(
        lineage.events.lock().await.is_empty(),
        "empty required-filters replay must fail before lineage projection"
    );
}

#[tokio::test]
async fn outbox_drain_rejects_scan_fetch_unsourced_required_filters() {
    let principal = Principal::new("agent:reader", PrincipalKind::Agent).unwrap();
    let ident = table_ident("local", "default", "events").unwrap();
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-scan-fetch-unsourced-required-filters".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "table.scan-tasks-fetched".to_string(),
            payload: json!({
                "audit-event-id": "audit-scan-fetch-unsourced-required-filters",
                "event-type": "table.scan-tasks-fetched",
                "table": ident,
                "payload": {
                    "table": ident,
                    "authorization-receipt": {
                        "principal": principal,
                        "action": "table-plan-scan",
                        "allowed": true,
                        "engine": "test",
                        "policy_hash": "sha256:policy",
                        "checked_at": chrono::Utc::now(),
                    },
                    "required-projection": ["event_id"],
                    "effective-projection": ["event_id"],
                    "requested-stats-fields": ["event_id"],
                    "effective-stats-fields": ["event_id"],
                    "required-filters": [{
                        "type": "eq",
                        "term": "event_id",
                        "value": "evt-1"
                    }],
                    "file-scan-task-count": 1,
                    "delete-file-count": 0,
                    "child-plan-task-count": 0
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
        .expect_err("ungoverned scan-fetch replay must not carry unsourced filters");

    let message = err.to_string();
    assert!(
        message.contains(
            "outbox event table.scan-tasks-fetched (lakecat.lineage-and-graph) has invalid"
        )
    );
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(message.contains(
        "scan-tasks-fetched required-filters must be empty without read-restriction row-predicate"
    ));
    assert!(!message.contains("evt-scan-fetch-unsourced-required-filters"));
    assert!(
        store.delivered.lock().await.is_empty(),
        "unsourced fetched required-filters replay must fail before acknowledgement"
    );
    assert!(
        graph.events.lock().await.is_empty(),
        "unsourced fetched required-filters replay must fail before graph projection"
    );
    assert!(
        lineage.events.lock().await.is_empty(),
        "unsourced fetched required-filters replay must fail before lineage projection"
    );
}

#[tokio::test]
async fn outbox_drain_rejects_scan_fetch_drifted_required_filters() {
    let principal = Principal::new("agent:reader", PrincipalKind::Agent).unwrap();
    let ident = table_ident("local", "default", "events").unwrap();
    let read_restriction = json!({
        "allowed-columns": ["event_id"],
        "row-predicate": {
            "type": "not-eq",
            "term": "severity",
            "value": "debug"
        },
        "policy-hashes": [
            content_hash_json(&json!({"policy-id": "agent-read", "scope": "default.events"}))
                .unwrap()
        ]
    });
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-scan-fetch-drifted-required-filters".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "table.scan-tasks-fetched".to_string(),
            payload: json!({
                "audit-event-id": "audit-scan-fetch-drifted-required-filters",
                "event-type": "table.scan-tasks-fetched",
                "table": ident,
                "payload": {
                    "table": ident,
                    "authorization-receipt": {
                        "principal": principal,
                        "action": "table-plan-scan",
                        "allowed": true,
                        "engine": "test",
                        "policy_hash": "sha256:policy",
                        "context": {
                            "read-restriction": read_restriction
                        },
                        "checked_at": chrono::Utc::now(),
                    },
                    "read-restriction": read_restriction,
                    "required-projection": ["event_id"],
                    "effective-projection": ["event_id"],
                    "requested-stats-fields": ["event_id"],
                    "effective-stats-fields": ["event_id"],
                    "required-filters": [{
                        "type": "eq",
                        "term": "severity",
                        "value": "info"
                    }],
                    "file-scan-task-count": 1,
                    "delete-file-count": 0,
                    "child-plan-task-count": 0
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
        .expect_err("scan-fetch required filters should reject predicate drift");

    let message = err.to_string();
    assert!(
        message.contains(
            "outbox event table.scan-tasks-fetched (lakecat.lineage-and-graph) has invalid"
        )
    );
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(message.contains(
        "scan-tasks-fetched required-filters must exactly preserve read-restriction row-predicate"
    ));
    assert!(!message.contains("evt-scan-fetch-drifted-required-filters"));
    assert!(
        store.delivered.lock().await.is_empty(),
        "drifted required-filters replay must fail before acknowledgement"
    );
    assert!(
        graph.events.lock().await.is_empty(),
        "drifted required-filters replay must fail before graph projection"
    );
    assert!(
        lineage.events.lock().await.is_empty(),
        "drifted required-filters replay must fail before lineage projection"
    );
}

#[tokio::test]
async fn outbox_drain_rejects_scan_replay_without_receipt_principal() {
    let cases = [
        (
            "table.scan-planned",
            "scan-planned",
            "evt-scan-plan-missing-receipt-principal",
            "audit-scan-plan-missing-receipt-principal",
        ),
        (
            "table.scan-tasks-fetched",
            "scan-tasks-fetched",
            "evt-scan-fetch-missing-receipt-principal",
            "audit-scan-fetch-missing-receipt-principal",
        ),
    ];

    for (event_type, label, event_id, audit_event_id) in cases {
        let table = TableIdent::new(
            WarehouseName::new("local").unwrap(),
            "default".parse::<Namespace>().unwrap(),
            TableName::new("events").unwrap(),
        );
        let policy_hash =
            content_hash_json(&json!({"policy-id": "agent-read", "scope": "default.events"}))
                .unwrap();
        let read_restriction = json!({
            "allowed-columns": ["event_id"],
            "row-predicate": {
                "type": "not-eq",
                "term": "severity",
                "value": "debug"
            },
            "purpose": "qglake-agent-demo",
            "max-credential-ttl-seconds": 300,
            "policy-hashes": [policy_hash]
        });
        let payload = if event_type == "table.scan-planned" {
            json!({
                "table": table,
                "authorization-receipt": {
                    "action": "table-plan-scan",
                    "allowed": true,
                    "engine": "test",
                    "policy_hash": null,
                    "checked_at": chrono::Utc::now(),
                    "context": {
                        "read-restriction": read_restriction
                    }
                },
                "read-restriction": read_restriction,
                "requested-projection": ["event_id"],
                "effective-projection": ["event_id"],
                "requested-stats-fields": ["event_id"],
                "effective-stats-fields": ["event_id"],
                "scan-task-count": 1,
            })
        } else {
            json!({
                "table": table,
                "authorization-receipt": {
                    "action": "table-plan-scan",
                    "allowed": true,
                    "engine": "test",
                    "policy_hash": null,
                    "checked_at": chrono::Utc::now(),
                    "context": {
                        "read-restriction": read_restriction
                    }
                },
                "read-restriction": read_restriction,
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
                "delete-file-count": 0,
                "child-plan-task-count": 0,
            })
        };
        let store = Arc::new(RecordingOutboxStore {
            events: Mutex::new(vec![OutboxEvent {
                event_id: event_id.to_string(),
                sink: "lakecat.lineage-and-graph".to_string(),
                event_type: event_type.to_string(),
                payload: json!({
                    "audit-event-id": audit_event_id,
                    "event-type": event_type,
                    "table": table,
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
            .expect_err("scan replay receipt principal should be required");

        let message = err.to_string();
        assert!(message.contains(event_type));
        assert!(
            message.contains(&format!(
                "{label} evidence must contain authorization receipt principal"
            )),
            "{message}"
        );
        assert!(message.contains("event-id-hash=sha256:"));
        assert!(!message.contains(event_id));
        assert!(
            store.delivered.lock().await.is_empty(),
            "scan replay missing receipt principal must fail before acknowledgement"
        );
        assert!(
            graph.events.lock().await.is_empty(),
            "scan replay missing receipt principal must fail before graph projection"
        );
        assert!(
            lineage.events.lock().await.is_empty(),
            "scan replay missing receipt principal must fail before lineage projection"
        );
    }
}
