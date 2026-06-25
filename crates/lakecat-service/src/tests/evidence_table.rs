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
async fn outbox_drain_rejects_short_table_commit_hash_evidence() {
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
            event_id: "evt-short-commit-hash".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "table.commit".to_string(),
            payload: json!({
                "audit-event-id": "audit-short-commit-hash",
                "event-type": "table.commit",
                "table": table,
                "commit": {
                    "table": table,
                    "previous_metadata_location": "file:///tmp/events/metadata/00000.json",
                    "new_metadata_location": "file:///tmp/events/metadata/00001.json",
                    "sequence_number": 7,
                    "principal": principal,
                    "format_version": 3,
                    "snapshot_id": 42,
                    "policy_hash": null,
                    "request_hash": "sha256:request",
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
        .expect_err("short table commit hash evidence should fail before delivery");

    let message = err.to_string();
    assert!(message.contains("table.commit"));
    assert!(message.contains("request_hash"));
    assert!(message.contains("full SHA-256"));
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(store.delivered.lock().await.is_empty());
    assert!(graph.events.lock().await.is_empty());
    assert!(lineage.events.lock().await.is_empty());
}

#[tokio::test]
async fn outbox_drain_accepts_kebab_case_table_commit_alias_evidence() {
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let principal = Principal::new("agent:writer", PrincipalKind::Agent).unwrap();
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-kebab-commit-aliases".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "table.commit".to_string(),
            payload: json!({
                "audit-event-id": "audit-kebab-commit-aliases",
                "event-type": "table.commit",
                "table": table,
                "commit": {
                    "table": table,
                    "previous-metadata-location": "file:///tmp/events/metadata/00000.json",
                    "new-metadata-location": "file:///tmp/events/metadata/00001.json",
                    "sequence-number": 7,
                    "principal": principal,
                    "format-version": 3,
                    "snapshot-id": 42,
                    "policy-hash": null,
                    "request-hash": content_hash_json(&json!({"request": "commit"})).unwrap(),
                    "response-hash": content_hash_json(&json!({"response": "commit"})).unwrap(),
                    "idempotency-key-sha256": content_hash_bytes("commit:events:0001".as_bytes()),
                    "committed-at": chrono::Utc::now(),
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

    let summary = drain_outbox_once(&state, 10)
        .await
        .expect("kebab-case table commit aliases should be accepted");

    assert_eq!(summary.delivered, 1);
    assert_eq!(store.delivered.lock().await.len(), 1);
    assert!(
        graph.events.lock().await.iter().any(|event| {
            event.label == GraphNodeLabel::Commit && event.action == GraphAction::Committed
        }),
        "kebab-case commit evidence should still project graph commit proof"
    );
    assert!(
        lineage
            .events
            .lock()
            .await
            .iter()
            .any(|event| event.event_type == LineageEventType::TableCommitted),
        "kebab-case commit evidence should still project lineage commit proof"
    );
}

#[tokio::test]
async fn outbox_drain_rejects_duplicate_table_commit_alias_evidence() {
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let principal = Principal::new("agent:writer", PrincipalKind::Agent).unwrap();
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-duplicate-commit-aliases".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "table.commit".to_string(),
            payload: json!({
                "audit-event-id": "audit-duplicate-commit-aliases",
                "event-type": "table.commit",
                "table": table,
                "commit": {
                    "table": table,
                    "previous_metadata_location": "file:///tmp/events/metadata/00000.json",
                    "new_metadata_location": "file:///tmp/events/metadata/00001.json",
                    "new-metadata-location": "file:///tmp/events/metadata/00002.json",
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
        .expect_err("duplicate table commit aliases should fail before delivery");

    let message = err.to_string();
    assert!(message.contains("table.commit"));
    assert!(
        message.contains(
            "table commit must not carry both new_metadata_location and new-metadata-location"
        ),
        "{message}"
    );
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains("evt-duplicate-commit-aliases"));
    assert!(
        store.delivered.lock().await.is_empty(),
        "duplicate commit aliases must fail before acknowledgement"
    );
    assert!(
        graph.events.lock().await.is_empty(),
        "duplicate commit aliases must fail before graph projection"
    );
    assert!(
        lineage.events.lock().await.is_empty(),
        "duplicate commit aliases must fail before lineage projection"
    );
}

#[tokio::test]
async fn outbox_drain_rejects_missing_table_commit_hash_evidence() {
    for field in ["request_hash", "response_hash"] {
        let table = TableIdent::new(
            WarehouseName::new("local").unwrap(),
            "default".parse::<Namespace>().unwrap(),
            TableName::new("events").unwrap(),
        );
        let principal = Principal {
            subject: "agent:writer".to_string(),
            kind: PrincipalKind::Agent,
        };
        let mut commit = json!({
            "table": table,
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
        });
        commit.as_object_mut().unwrap().remove(field);
        let event_id = format!("evt-missing-commit-{field}");
        let store = Arc::new(RecordingOutboxStore {
            events: Mutex::new(vec![OutboxEvent {
                event_id: event_id.clone(),
                sink: "lakecat.lineage-and-graph".to_string(),
                event_type: "table.commit".to_string(),
                payload: json!({
                    "audit-event-id": format!("audit-missing-commit-{field}"),
                    "event-type": "table.commit",
                    "table": table,
                    "commit": commit,
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
            .expect_err("missing table commit hash evidence should fail before delivery");

        let message = err.to_string();
        assert!(message.contains("table.commit"));
        assert!(message.contains(field));
        assert!(message.contains("full SHA-256"));
        assert!(message.contains("event-id-hash=sha256:"));
        assert!(!message.contains(&event_id));
        assert!(store.delivered.lock().await.is_empty());
        assert!(graph.events.lock().await.is_empty());
        assert!(lineage.events.lock().await.is_empty());
    }
}

#[tokio::test]
async fn outbox_drain_rejects_malformed_table_commit_idempotency_hash() {
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
            event_id: "evt-malformed-commit-idempotency-hash".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "table.commit".to_string(),
            payload: json!({
                "audit-event-id": "audit-malformed-commit-idempotency-hash",
                "event-type": "table.commit",
                "table": table,
                "commit": {
                    "table": table,
                    "previous_metadata_location": "file:///tmp/events/metadata/00000.json",
                    "new_metadata_location": "file:///tmp/events/metadata/00001.json",
                    "sequence_number": 7,
                    "principal": principal,
                    "format_version": 3,
                    "snapshot_id": 42,
                    "policy_hash": null,
                    "request_hash": content_hash_json(&json!({"request": "commit"})).unwrap(),
                    "response_hash": content_hash_json(&json!({"response": "commit"})).unwrap(),
                    "idempotency_key_sha256": "sha256:short",
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
        .expect_err("malformed table commit idempotency hash should fail before delivery");

    let message = err.to_string();
    assert!(message.contains("table.commit"));
    assert!(message.contains("idempotency_key_sha256"));
    assert!(message.contains("full SHA-256"));
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(store.delivered.lock().await.is_empty());
    assert!(graph.events.lock().await.is_empty());
    assert!(lineage.events.lock().await.is_empty());
}

#[tokio::test]
async fn outbox_drain_rejects_extra_table_commit_fields() {
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let principal = Principal::new("agent:writer", PrincipalKind::Agent).unwrap();
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-extra-table-commit-field".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "table.commit".to_string(),
            payload: json!({
                "audit-event-id": "audit-extra-table-commit-field",
                "event-type": "table.commit",
                "table": table,
                "commit": {
                    "table": table,
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
                    "unverified-commit-claim": "already-authorized"
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
        .expect_err("table commit evidence should reject extra fields");

    let message = err.to_string();
    assert!(message.contains("table.commit"));
    assert!(message.contains("table commit contains unexpected field unverified-commit-claim"));
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains("evt-extra-table-commit-field"));
    assert!(
        store.delivered.lock().await.is_empty(),
        "extra table commit replay must fail before acknowledgement"
    );
    assert!(
        graph.events.lock().await.is_empty(),
        "extra table commit replay must fail before graph projection"
    );
    assert!(
        lineage.events.lock().await.is_empty(),
        "extra table commit replay must fail before lineage projection"
    );
}

#[tokio::test]
async fn outbox_drain_rejects_extra_top_level_table_commit_fields() {
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let principal = Principal::new("agent:writer", PrincipalKind::Agent).unwrap();
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-extra-top-level-table-commit-field".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "table.commit".to_string(),
            payload: json!({
                "audit-event-id": "audit-extra-top-level-table-commit-field",
                "event-type": "table.commit",
                "table": table,
                "commit": {
                    "table": table,
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
                "unverified-commit-claim": "already-authorized"
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
        .expect_err("table commit payload should reject extra fields");

    let message = err.to_string();
    assert!(message.contains("table.commit"));
    assert!(
        message.contains("table commit payload contains unexpected field unverified-commit-claim")
    );
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains("evt-extra-top-level-table-commit-field"));
    assert!(
        store.delivered.lock().await.is_empty(),
        "extra table commit payload must fail before acknowledgement"
    );
    assert!(
        graph.events.lock().await.is_empty(),
        "extra table commit payload must fail before graph projection"
    );
    assert!(
        lineage.events.lock().await.is_empty(),
        "extra table commit payload must fail before lineage projection"
    );
}

#[tokio::test]
async fn outbox_drain_rejects_extra_table_commit_wrapper_fields() {
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let principal = Principal::new("agent:writer", PrincipalKind::Agent).unwrap();
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-extra-table-commit-wrapper-field".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "table.commit".to_string(),
            payload: json!({
                "audit-event-id": "audit-extra-table-commit-wrapper-field",
                "event-type": "table.commit",
                "table": table,
                "payload": {
                    "event-type": "table.commit",
                    "table": table,
                    "commit": {
                        "table": table,
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
                },
                "unverified-commit-wrapper-claim": "already-authorized"
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
        .expect_err("table commit wrapper should reject extra fields");

    let message = err.to_string();
    assert!(message.contains("table.commit"));
    assert!(message.contains(
        "table commit outbox payload contains unexpected field unverified-commit-wrapper-claim"
    ));
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains("evt-extra-table-commit-wrapper-field"));
    assert!(
        store.delivered.lock().await.is_empty(),
        "extra table commit wrapper must fail before acknowledgement"
    );
    assert!(
        graph.events.lock().await.is_empty(),
        "extra table commit wrapper must fail before graph projection"
    );
    assert!(
        lineage.events.lock().await.is_empty(),
        "extra table commit wrapper must fail before lineage projection"
    );
}

#[tokio::test]
async fn outbox_drain_rejects_malformed_table_commit_format_snapshot_evidence() {
    for (case, mutate, expected) in [
        (
            "missing-format-version",
            "remove-format-version",
            "table commit evidence must contain unsigned format version",
        ),
        (
            "zero-format-version",
            "zero-format-version",
            "table commit evidence format version must be positive",
        ),
        (
            "missing-snapshot-id",
            "remove-snapshot-id",
            "table commit evidence must contain signed snapshot id",
        ),
        (
            "negative-snapshot-id",
            "negative-snapshot-id",
            "table commit evidence snapshot id must be non-negative",
        ),
    ] {
        let table = TableIdent::new(
            WarehouseName::new("local").unwrap(),
            "default".parse::<Namespace>().unwrap(),
            TableName::new("events").unwrap(),
        );
        let principal = Principal::new("agent:writer", PrincipalKind::Agent).unwrap();
        let mut commit = json!({
            "table": table,
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
        });
        match mutate {
            "remove-format-version" => {
                commit.as_object_mut().unwrap().remove("format_version");
            }
            "zero-format-version" => {
                commit["format_version"] = json!(0);
            }
            "remove-snapshot-id" => {
                commit.as_object_mut().unwrap().remove("snapshot_id");
            }
            "negative-snapshot-id" => {
                commit["snapshot_id"] = json!(-1);
            }
            _ => unreachable!("unknown table commit evidence mutation"),
        }
        let event_id = format!("evt-table-commit-{case}");
        let store = Arc::new(RecordingOutboxStore {
            events: Mutex::new(vec![OutboxEvent {
                event_id: event_id.clone(),
                sink: "lakecat.lineage-and-graph".to_string(),
                event_type: "table.commit".to_string(),
                payload: json!({
                    "audit-event-id": format!("audit-table-commit-{case}"),
                    "event-type": "table.commit",
                    "table": table,
                    "commit": commit,
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
            .expect_err("malformed table commit format/snapshot evidence should fail");
        let message = err.to_string();
        assert!(message.contains("table.commit"));
        assert!(message.contains(expected), "{case}: {message}");
        assert!(message.contains("event-id-hash=sha256:"));
        assert!(!message.contains(&event_id));
        assert!(store.delivered.lock().await.is_empty());
        assert!(graph.events.lock().await.is_empty());
        assert!(lineage.events.lock().await.is_empty());
    }
}

#[tokio::test]
async fn outbox_drain_rejects_malformed_table_commit_committed_at_evidence() {
    for (case, mutate, expected) in [
        (
            "missing-committed-at",
            "remove-committed-at",
            "table commit evidence committed_at timestamp must be present",
        ),
        (
            "blank-committed-at",
            "blank-committed-at",
            "table commit evidence committed_at timestamp must be non-empty",
        ),
        (
            "malformed-committed-at",
            "malformed-committed-at",
            "table commit evidence committed_at timestamp must be RFC3339",
        ),
    ] {
        let table = TableIdent::new(
            WarehouseName::new("local").unwrap(),
            "default".parse::<Namespace>().unwrap(),
            TableName::new("events").unwrap(),
        );
        let principal = Principal::new("agent:writer", PrincipalKind::Agent).unwrap();
        let mut commit = json!({
            "table": table,
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
        });
        match mutate {
            "remove-committed-at" => {
                commit.as_object_mut().unwrap().remove("committed_at");
            }
            "blank-committed-at" => {
                commit["committed_at"] = json!(" ");
            }
            "malformed-committed-at" => {
                commit["committed_at"] = json!("not-a-timestamp");
            }
            _ => unreachable!("unknown table commit committed_at mutation"),
        }
        let event_id = format!("evt-table-commit-{case}");
        let store = Arc::new(RecordingOutboxStore {
            events: Mutex::new(vec![OutboxEvent {
                event_id: event_id.clone(),
                sink: "lakecat.lineage-and-graph".to_string(),
                event_type: "table.commit".to_string(),
                payload: json!({
                    "audit-event-id": format!("audit-table-commit-{case}"),
                    "event-type": "table.commit",
                    "table": table,
                    "commit": commit,
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
            .expect_err("malformed table commit committed_at evidence should fail");
        let message = err.to_string();
        assert!(message.contains("table.commit"));
        assert!(message.contains(expected), "{case}: {message}");
        assert!(message.contains("event-id-hash=sha256:"));
        assert!(!message.contains(&event_id));
        assert!(store.delivered.lock().await.is_empty());
        assert!(graph.events.lock().await.is_empty());
        assert!(lineage.events.lock().await.is_empty());
    }
}

#[tokio::test]
async fn outbox_drain_rejects_decorated_table_commit_metadata_locations() {
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let principal = Principal::new("agent:writer", PrincipalKind::Agent).unwrap();
    let cases = [
        (
            "evt-decorated-commit-new-metadata",
            "file:///tmp/events/metadata/00000.json",
            "s3://lakecat-demo/events/metadata/00001.json?token=secret",
            "table commit new metadata location must not contain decorated location material",
        ),
        (
            "evt-credential-commit-new-metadata",
            "file:///tmp/events/metadata/00000.json",
            "s3://lakecat-demo/events/metadata/session_token=secret.json",
            "table commit new metadata location must not contain credential material",
        ),
        (
            "evt-userinfo-commit-new-metadata",
            "file:///tmp/events/metadata/00000.json",
            "s3://access:secret@lakecat-demo/events/metadata/00001.json",
            "table commit new metadata location must not include userinfo",
        ),
        (
            "evt-decorated-commit-previous-metadata",
            "s3://lakecat-demo/events/metadata/00000.json#secret",
            "file:///tmp/events/metadata/00001.json",
            "table commit previous metadata location must not contain decorated location material",
        ),
        (
            "evt-credential-commit-previous-metadata",
            "s3://lakecat-demo/events/metadata/access_key=secret.json",
            "file:///tmp/events/metadata/00001.json",
            "table commit previous metadata location must not contain credential material",
        ),
        (
            "evt-userinfo-commit-previous-metadata",
            "s3://access:secret@lakecat-demo/events/metadata/00000.json",
            "file:///tmp/events/metadata/00001.json",
            "table commit previous metadata location must not include userinfo",
        ),
    ];

    for (event_id, previous_metadata_location, new_metadata_location, expected) in cases {
        let store = Arc::new(RecordingOutboxStore {
            events: Mutex::new(vec![OutboxEvent {
                event_id: event_id.to_string(),
                sink: "lakecat.lineage-and-graph".to_string(),
                event_type: "table.commit".to_string(),
                payload: json!({
                    "audit-event-id": format!("audit-{event_id}"),
                    "event-type": "table.commit",
                    "table": table,
                    "commit": {
                        "table": table,
                        "previous_metadata_location": previous_metadata_location,
                        "new_metadata_location": new_metadata_location,
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
            .expect_err("malformed table commit metadata location should fail before delivery");

        let message = err.to_string();
        assert!(message.contains("table.commit"), "{message}");
        assert!(message.contains(expected), "{message}");
        assert!(message.contains("event-id-hash=sha256:"), "{message}");
        assert!(!message.contains(event_id), "{message}");
        assert!(!message.contains("access:secret"), "{message}");
        assert!(!message.contains("lakecat-demo"), "{message}");
        assert!(
            store.delivered.lock().await.is_empty(),
            "malformed commit metadata location must fail before acknowledgement"
        );
        assert!(
            graph.events.lock().await.is_empty(),
            "malformed commit metadata location must fail before graph projection"
        );
        assert!(
            lineage.events.lock().await.is_empty(),
            "malformed commit metadata location must fail before lineage projection"
        );
    }
}

#[tokio::test]
async fn outbox_drain_rejects_missing_table_commit_principal_evidence() {
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let principal = Principal::new("agent:writer", PrincipalKind::Agent).unwrap();
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-missing-commit-principal".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "table.commit".to_string(),
            payload: json!({
                "audit-event-id": "audit-missing-commit-principal",
                "event-type": "table.commit",
                "table": table,
                "commit": {
                    "table": table,
                    "previous_metadata_location": "file:///tmp/events/metadata/00000.json",
                    "new_metadata_location": "file:///tmp/events/metadata/00001.json",
                    "sequence_number": 7,
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
        .expect_err("missing table commit principal should fail before delivery");

    let message = err.to_string();
    assert!(message.contains("table.commit"));
    assert!(message.contains("table commit evidence must contain commit principal"));
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains("evt-missing-commit-principal"));
    assert!(
        store.delivered.lock().await.is_empty(),
        "missing commit principal must fail before acknowledgement"
    );
    assert!(
        graph.events.lock().await.is_empty(),
        "missing commit principal must fail before graph projection"
    );
    assert!(
        lineage.events.lock().await.is_empty(),
        "missing commit principal must fail before lineage projection"
    );
}

#[tokio::test]
async fn outbox_drain_rejects_malformed_table_commit_principal_evidence() {
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let receipt_principal = Principal::new("agent:writer", PrincipalKind::Agent).unwrap();
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-malformed-commit-principal".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "table.commit".to_string(),
            payload: json!({
                "audit-event-id": "audit-malformed-commit-principal",
                "event-type": "table.commit",
                "table": table,
                "commit": {
                    "table": table,
                    "previous_metadata_location": "file:///tmp/events/metadata/00000.json",
                    "new_metadata_location": "file:///tmp/events/metadata/00001.json",
                    "sequence_number": 7,
                    "principal": {
                        "subject": "agent:writer",
                        "kind": "unknown"
                    },
                    "format_version": 3,
                    "snapshot_id": 42,
                    "policy_hash": null,
                    "request_hash": content_hash_json(&json!({"request": "commit"})).unwrap(),
                    "response_hash": content_hash_json(&json!({"response": "commit"})).unwrap(),
                    "idempotency_key_sha256": content_hash_bytes("commit:events:0001".as_bytes()),
                    "committed_at": chrono::Utc::now(),
                },
                "authorization-receipt": {
                    "principal": receipt_principal,
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
        .expect_err("malformed table commit principal should fail before delivery");

    let message = err.to_string();
    assert!(message.contains("table.commit"));
    assert!(message.contains("table commit principal"));
    assert!(message.contains("must be a valid principal"));
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains("evt-malformed-commit-principal"));
    assert!(
        store.delivered.lock().await.is_empty(),
        "malformed commit principal must fail before acknowledgement"
    );
    assert!(
        graph.events.lock().await.is_empty(),
        "malformed commit principal must fail before graph projection"
    );
    assert!(
        lineage.events.lock().await.is_empty(),
        "malformed commit principal must fail before lineage projection"
    );
}

#[tokio::test]
async fn outbox_drain_rejects_missing_table_commit_receipt_principal_evidence() {
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let principal = Principal::new("agent:writer", PrincipalKind::Agent).unwrap();
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-missing-commit-receipt-principal".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "table.commit".to_string(),
            payload: json!({
                "audit-event-id": "audit-missing-commit-receipt-principal",
                "event-type": "table.commit",
                "table": table,
                "commit": {
                    "table": table,
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
        .expect_err("missing table commit receipt principal should fail before delivery");

    let message = err.to_string();
    assert!(message.contains("table.commit"));
    assert!(message.contains("table commit evidence must contain authorization receipt principal"));
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains("evt-missing-commit-receipt-principal"));
    assert!(
        store.delivered.lock().await.is_empty(),
        "missing receipt principal must fail before acknowledgement"
    );
    assert!(
        graph.events.lock().await.is_empty(),
        "missing receipt principal must fail before graph projection"
    );
    assert!(
        lineage.events.lock().await.is_empty(),
        "missing receipt principal must fail before lineage projection"
    );
}

#[tokio::test]
async fn outbox_drain_rejects_malformed_table_commit_receipt_principal_evidence() {
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let commit_principal = Principal::new("agent:writer", PrincipalKind::Agent).unwrap();
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-malformed-commit-receipt-principal".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "table.commit".to_string(),
            payload: json!({
                "audit-event-id": "audit-malformed-commit-receipt-principal",
                "event-type": "table.commit",
                "table": table,
                "commit": {
                    "table": table,
                    "previous_metadata_location": "file:///tmp/events/metadata/00000.json",
                    "new_metadata_location": "file:///tmp/events/metadata/00001.json",
                    "sequence_number": 7,
                    "principal": commit_principal,
                    "format_version": 3,
                    "snapshot_id": 42,
                    "policy_hash": null,
                    "request_hash": content_hash_json(&json!({"request": "commit"})).unwrap(),
                    "response_hash": content_hash_json(&json!({"response": "commit"})).unwrap(),
                    "idempotency_key_sha256": content_hash_bytes("commit:events:0001".as_bytes()),
                    "committed_at": chrono::Utc::now(),
                },
                "authorization-receipt": {
                    "principal": {
                        "subject": "agent:writer",
                        "kind": "unknown"
                    },
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
        .expect_err("malformed table commit receipt principal should fail before delivery");

    let message = err.to_string();
    assert!(message.contains("table.commit"));
    assert!(message.contains("table commit authorization receipt principal"));
    assert!(message.contains("must be a valid principal"));
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains("evt-malformed-commit-receipt-principal"));
    assert!(
        store.delivered.lock().await.is_empty(),
        "malformed commit receipt principal must fail before acknowledgement"
    );
    assert!(
        graph.events.lock().await.is_empty(),
        "malformed commit receipt principal must fail before graph projection"
    );
    assert!(
        lineage.events.lock().await.is_empty(),
        "malformed commit receipt principal must fail before lineage projection"
    );
}

#[tokio::test]
async fn outbox_drain_rejects_mismatched_table_commit_payload_scope() {
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let principal = Principal::new("agent:writer", PrincipalKind::Agent).unwrap();
    let commit = json!({
        "table": table,
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
    });
    let receipt = json!({
        "principal": principal,
        "action": "table-commit",
        "allowed": true,
        "engine": "test",
        "policy_hash": null,
        "checked_at": chrono::Utc::now(),
    });
    let cases = vec![
        (
            "warehouse",
            json!({
                "audit-event-id": "audit-mismatched-commit-warehouse",
                "event-type": "table.commit",
                "table": table,
                "warehouse": "other",
                "commit": commit,
                "authorization-receipt": receipt,
            }),
            "table commit warehouse does not match table identity",
        ),
        (
            "namespace",
            json!({
                "audit-event-id": "audit-mismatched-commit-namespace",
                "event-type": "table.commit",
                "table": table,
                "namespace": ["other"],
                "commit": commit,
                "authorization-receipt": receipt,
            }),
            "table commit namespace does not match table identity",
        ),
        (
            "table-name",
            json!({
                "audit-event-id": "audit-mismatched-commit-table-name",
                "event-type": "table.commit",
                "table": table,
                "payload": {
                    "table": "shadow_events",
                    "commit": commit,
                    "authorization-receipt": receipt,
                },
            }),
            "table commit table name does not match table identity",
        ),
    ];

    for (case, payload, expected) in cases {
        let event_id = format!("evt-mismatched-commit-payload-scope-{case}");
        let store = Arc::new(RecordingOutboxStore {
            events: Mutex::new(vec![OutboxEvent {
                event_id: event_id.clone(),
                sink: "lakecat.lineage-and-graph".to_string(),
                event_type: "table.commit".to_string(),
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
            .expect_err("mismatched table commit payload scope should fail before delivery");

        let message = err.to_string();
        assert!(message.contains("table.commit"), "{message}");
        assert!(message.contains(expected), "{message}");
        assert!(message.contains("event-id-hash=sha256:"), "{message}");
        assert!(!message.contains(&event_id), "{message}");
        assert!(
            store.delivered.lock().await.is_empty(),
            "mismatched commit payload scope must fail before acknowledgement"
        );
        assert!(
            graph.events.lock().await.is_empty(),
            "mismatched commit payload scope must fail before graph projection"
        );
        assert!(
            lineage.events.lock().await.is_empty(),
            "mismatched commit payload scope must fail before lineage projection"
        );
    }
}

#[tokio::test]
async fn outbox_drain_rejects_mismatched_table_commit_principal_evidence() {
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let commit_principal = Principal::new("agent:writer", PrincipalKind::Agent).unwrap();
    let receipt_principal = Principal::new("human:operator", PrincipalKind::Human).unwrap();
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-mismatched-commit-principal".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "table.commit".to_string(),
            payload: json!({
                "audit-event-id": "audit-mismatched-commit-principal",
                "event-type": "table.commit",
                "table": table,
                "commit": {
                    "table": table,
                    "previous_metadata_location": "file:///tmp/events/metadata/00000.json",
                    "new_metadata_location": "file:///tmp/events/metadata/00001.json",
                    "sequence_number": 7,
                    "principal": commit_principal,
                    "format_version": 3,
                    "snapshot_id": 42,
                    "policy_hash": null,
                    "request_hash": content_hash_json(&json!({"request": "commit"})).unwrap(),
                    "response_hash": content_hash_json(&json!({"response": "commit"})).unwrap(),
                    "idempotency_key_sha256": content_hash_bytes("commit:events:0001".as_bytes()),
                    "committed_at": chrono::Utc::now(),
                },
                "authorization-receipt": {
                    "principal": receipt_principal,
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
        .expect_err("mismatched table commit principal should fail before delivery");

    let message = err.to_string();
    assert!(message.contains("table.commit"));
    assert!(
        message.contains("table commit principal does not match authorization receipt principal")
    );
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains("evt-mismatched-commit-principal"));
    assert!(
        store.delivered.lock().await.is_empty(),
        "mismatched commit principal must fail before acknowledgement"
    );
    assert!(
        graph.events.lock().await.is_empty(),
        "mismatched commit principal must fail before graph projection"
    );
    assert!(
        lineage.events.lock().await.is_empty(),
        "mismatched commit principal must fail before lineage projection"
    );
}

#[tokio::test]
async fn outbox_drain_rejects_blank_table_commit_receipt_action() {
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let principal = Principal::new("agent:writer", PrincipalKind::Agent).unwrap();
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-blank-commit-receipt-action".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "table.commit".to_string(),
            payload: json!({
                "audit-event-id": "audit-blank-commit-receipt-action",
                "event-type": "table.commit",
                "table": table,
                "commit": {
                    "table": table,
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
                    "action": " ",
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
        .expect_err("blank table commit receipt action should fail before delivery");

    let message = err.to_string();
    assert!(message.contains("table.commit"));
    assert!(message.contains("table commit authorization receipt action must be non-empty"));
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains("evt-blank-commit-receipt-action"));
    assert!(
        store.delivered.lock().await.is_empty(),
        "blank commit receipt action must fail before acknowledgement"
    );
    assert!(
        graph.events.lock().await.is_empty(),
        "blank commit receipt action must fail before graph projection"
    );
    assert!(
        lineage.events.lock().await.is_empty(),
        "blank commit receipt action must fail before lineage projection"
    );
}

#[tokio::test]
async fn outbox_drain_rejects_unknown_table_commit_receipt_action() {
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let principal = Principal::new("agent:writer", PrincipalKind::Agent).unwrap();
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-unknown-commit-receipt-action".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "table.commit".to_string(),
            payload: json!({
                "audit-event-id": "audit-unknown-commit-receipt-action",
                "event-type": "table.commit",
                "table": table,
                "commit": {
                    "table": table,
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
                    "action": "table-force-push",
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
        .expect_err("unknown table commit receipt action should fail before delivery");

    let message = err.to_string();
    assert!(message.contains("table.commit"));
    assert!(
        message
            .contains("table commit authorization receipt action must be a known catalog action"),
        "{message}"
    );
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains("evt-unknown-commit-receipt-action"));
    assert!(
        store.delivered.lock().await.is_empty(),
        "unknown commit receipt action must fail before acknowledgement"
    );
    assert!(
        graph.events.lock().await.is_empty(),
        "unknown commit receipt action must fail before graph projection"
    );
    assert!(
        lineage.events.lock().await.is_empty(),
        "unknown commit receipt action must fail before lineage projection"
    );
}

#[tokio::test]
async fn outbox_drain_rejects_mismatched_table_commit_receipt_action() {
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let principal = Principal::new("agent:writer", PrincipalKind::Agent).unwrap();
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-mismatched-commit-receipt-action".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "table.commit".to_string(),
            payload: json!({
                "audit-event-id": "audit-mismatched-commit-receipt-action",
                "event-type": "table.commit",
                "table": table,
                "commit": {
                    "table": table,
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
                    "action": "table-load",
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
        .expect_err("mismatched table commit receipt action should fail before delivery");

    let message = err.to_string();
    assert!(message.contains("table.commit"));
    assert!(
        message
            .contains("table commit authorization receipt action does not match outbox event type"),
        "{message}"
    );
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains("evt-mismatched-commit-receipt-action"));
    assert!(
        store.delivered.lock().await.is_empty(),
        "mismatched commit receipt action must fail before acknowledgement"
    );
    assert!(
        graph.events.lock().await.is_empty(),
        "mismatched commit receipt action must fail before graph projection"
    );
    assert!(
        lineage.events.lock().await.is_empty(),
        "mismatched commit receipt action must fail before lineage projection"
    );
}

#[tokio::test]
async fn outbox_drain_rejects_missing_table_commit_receipt_engine() {
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let principal = Principal::new("agent:writer", PrincipalKind::Agent).unwrap();
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-missing-commit-receipt-engine".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "table.commit".to_string(),
            payload: json!({
                "audit-event-id": "audit-missing-commit-receipt-engine",
                "event-type": "table.commit",
                "table": table,
                "commit": {
                    "table": table,
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
        .expect_err("missing table commit receipt engine should fail before delivery");

    let message = err.to_string();
    assert!(message.contains("table.commit"));
    assert!(message.contains("table commit evidence must contain authorization receipt engine"));
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains("evt-missing-commit-receipt-engine"));
    assert!(
        store.delivered.lock().await.is_empty(),
        "missing commit receipt engine must fail before acknowledgement"
    );
    assert!(
        graph.events.lock().await.is_empty(),
        "missing commit receipt engine must fail before graph projection"
    );
    assert!(
        lineage.events.lock().await.is_empty(),
        "missing commit receipt engine must fail before lineage projection"
    );
}

#[tokio::test]
async fn outbox_drain_rejects_denied_table_commit_receipt() {
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let principal = Principal::new("agent:writer", PrincipalKind::Agent).unwrap();
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-denied-commit-receipt".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "table.commit".to_string(),
            payload: json!({
                "audit-event-id": "audit-denied-commit-receipt",
                "event-type": "table.commit",
                "table": table,
                "commit": {
                    "table": table,
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
                    "allowed": false,
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
        .expect_err("denied table commit receipt should fail before delivery");

    let message = err.to_string();
    assert!(message.contains("table.commit"));
    assert!(message.contains("table commit authorization receipt must allow replay projection"));
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains("evt-denied-commit-receipt"));
    assert!(
        store.delivered.lock().await.is_empty(),
        "denied commit receipt must fail before acknowledgement"
    );
    assert!(
        graph.events.lock().await.is_empty(),
        "denied commit receipt must fail before graph projection"
    );
    assert!(
        lineage.events.lock().await.is_empty(),
        "denied commit receipt must fail before lineage projection"
    );
}

#[tokio::test]
async fn outbox_drain_rejects_blank_table_commit_receipt_checked_at() {
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let principal = Principal::new("agent:writer", PrincipalKind::Agent).unwrap();
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-blank-commit-receipt-checked-at".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "table.commit".to_string(),
            payload: json!({
                "audit-event-id": "audit-blank-commit-receipt-checked-at",
                "event-type": "table.commit",
                "table": table,
                "commit": {
                    "table": table,
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
                    "checked_at": " ",
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
        .expect_err("blank table commit receipt checked_at should fail before delivery");

    let message = err.to_string();
    assert!(message.contains("table.commit"));
    assert!(
        message
            .contains("table commit authorization receipt checked_at timestamp must be non-empty")
    );
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains("evt-blank-commit-receipt-checked-at"));
    assert!(
        store.delivered.lock().await.is_empty(),
        "blank commit receipt timestamp must fail before acknowledgement"
    );
    assert!(
        graph.events.lock().await.is_empty(),
        "blank commit receipt timestamp must fail before graph projection"
    );
    assert!(
        lineage.events.lock().await.is_empty(),
        "blank commit receipt timestamp must fail before lineage projection"
    );
}

#[tokio::test]
async fn outbox_drain_rejects_malformed_table_commit_history_evidence() {
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
            event_id: "evt-malformed-commit-history".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "table.commits-listed".to_string(),
            payload: json!({
                "audit-event-id": "audit-malformed-commit-history",
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
                    "namespace": ["default"],
                    "table": "events",
                    "commit-count": 2,
                    "commit-hashes": [
                        content_hash_json(&json!({"commit": 1})).unwrap()
                    ],
                    "sequence-numbers": [1, 2],
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
        .expect_err("malformed commit history evidence should fail before delivery");

    let message = err.to_string();
    assert!(message.contains("table.commits-listed"));
    assert!(message.contains("commit-count does not match commit-hashes"));
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(store.delivered.lock().await.is_empty());
    assert!(graph.events.lock().await.is_empty());
    assert!(lineage.events.lock().await.is_empty());
}

#[tokio::test]
async fn outbox_drain_rejects_non_increasing_table_commit_history_sequences() {
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
            event_id: "evt-non-increasing-commit-history".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "table.commits-listed".to_string(),
            payload: json!({
                "audit-event-id": "audit-non-increasing-commit-history",
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
                    "namespace": ["default"],
                    "table": "events",
                    "commit-count": 2,
                    "commit-hashes": [
                        content_hash_json(&json!({"commit": 1})).unwrap(),
                        content_hash_json(&json!({"commit": 2})).unwrap()
                    ],
                    "sequence-numbers": [1, 1],
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
        .expect_err("non-increasing commit history evidence should fail before delivery");

    let message = err.to_string();
    assert!(message.contains("table.commits-listed"));
    assert!(
        message.contains("table commit-history sequence-numbers must be strictly increasing"),
        "{message}"
    );
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains("evt-non-increasing-commit-history"));
    assert!(store.delivered.lock().await.is_empty());
    assert!(graph.events.lock().await.is_empty());
    assert!(lineage.events.lock().await.is_empty());
}

#[tokio::test]
async fn outbox_drain_rejects_zero_table_commit_history_sequence() {
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
            event_id: "evt-zero-commit-history-sequence".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "table.commits-listed".to_string(),
            payload: json!({
                "audit-event-id": "audit-zero-commit-history-sequence",
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
                    "namespace": ["default"],
                    "table": "events",
                    "commit-count": 1,
                    "commit-hashes": [
                        content_hash_json(&json!({"commit": 0})).unwrap()
                    ],
                    "sequence-numbers": [0],
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
        .expect_err("zero commit history sequence should fail before delivery");

    let message = err.to_string();
    assert!(message.contains("table.commits-listed"));
    assert!(
        message.contains("table commit-history sequence-numbers must be positive"),
        "{message}"
    );
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains("evt-zero-commit-history-sequence"));
    assert!(store.delivered.lock().await.is_empty());
    assert!(graph.events.lock().await.is_empty());
    assert!(lineage.events.lock().await.is_empty());
}

#[tokio::test]
async fn outbox_drain_rejects_duplicate_table_commit_history_hashes() {
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let principal = Principal {
        subject: "agent:writer".to_string(),
        kind: PrincipalKind::Agent,
    };
    let duplicate_commit_hash = content_hash_json(&json!({"commit": "same"})).unwrap();
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-duplicate-commit-history-hash".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "table.commits-listed".to_string(),
            payload: json!({
                "audit-event-id": "audit-duplicate-commit-history-hash",
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
                    "namespace": ["default"],
                    "table": "events",
                    "commit-count": 2,
                    "commit-hashes": [
                        duplicate_commit_hash,
                        duplicate_commit_hash
                    ],
                    "sequence-numbers": [1, 2],
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
        .expect_err("duplicate table commit-history hashes should fail before delivery");

    let message = err.to_string();
    assert!(message.contains("table.commits-listed"));
    assert!(
        message.contains("table commit-history commit-hashes must not contain duplicate hashes")
    );
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains("evt-duplicate-commit-history-hash"));
    assert!(store.delivered.lock().await.is_empty());
    assert!(graph.events.lock().await.is_empty());
    assert!(lineage.events.lock().await.is_empty());
}

#[tokio::test]
async fn outbox_drain_rejects_extra_table_commit_history_fields() {
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let principal = Principal {
        subject: "agent:writer".to_string(),
        kind: PrincipalKind::Agent,
    };
    let mut payload = json!({
        "event-type": "table.commits-listed",
        "authorization-receipt": {
            "principal": principal,
            "action": "table-load",
            "allowed": true,
            "engine": "test",
            "policy_hash": null,
            "checked_at": chrono::Utc::now(),
        },
        "warehouse": "local",
        "namespace": ["default"],
        "table": "events",
        "commit-count": 1,
        "commit-hashes": [
            content_hash_json(&json!({"commit": 1})).unwrap()
        ],
        "sequence-numbers": [1],
        "principal-subject": "agent:writer",
        "principal-kind": "agent",
    });
    payload["unverified-commit-history-claim"] = json!("shadow");
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-extra-commit-history-field".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "table.commits-listed".to_string(),
            payload: json!({
                "audit-event-id": "audit-extra-commit-history-field",
                "event-type": "table.commits-listed",
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
        .expect_err("extra commit-history fields should fail before delivery");

    let message = err.to_string();
    assert!(message.contains("table.commits-listed"));
    assert!(
        message.contains(
            "table commit-history contains unexpected field unverified-commit-history-claim"
        ),
        "{message}"
    );
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains("evt-extra-commit-history-field"));
    assert!(store.delivered.lock().await.is_empty());
    assert!(graph.events.lock().await.is_empty());
    assert!(lineage.events.lock().await.is_empty());
}

#[tokio::test]
async fn outbox_drain_rejects_extra_table_commit_history_wrapper_fields() {
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
            event_id: "evt-extra-commit-history-wrapper-field".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "table.commits-listed".to_string(),
            payload: json!({
                "audit-event-id": "audit-extra-commit-history-wrapper-field",
                "event-type": "table.commits-listed",
                "table": table,
                "payload": {
                    "event-type": "table.commits-listed",
                    "authorization-receipt": {
                        "principal": principal,
                        "action": "table-load",
                        "allowed": true,
                        "engine": "test",
                        "policy_hash": null,
                        "checked_at": chrono::Utc::now(),
                    },
                    "warehouse": "local",
                    "namespace": ["default"],
                    "table": "events",
                    "commit-count": 1,
                    "commit-hashes": [
                        content_hash_json(&json!({"commit": 1})).unwrap()
                    ],
                    "sequence-numbers": [1],
                    "principal-subject": "agent:writer",
                    "principal-kind": "agent",
                },
                "unverified-commit-history-wrapper-claim": "shadow",
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
        .expect_err("extra commit-history wrapper fields should fail before delivery");

    let message = err.to_string();
    assert!(message.contains("table.commits-listed"), "{message}");
    assert!(
        message.contains(
            "table commit-history outbox payload contains unexpected field unverified-commit-history-wrapper-claim"
        ),
        "{message}"
    );
    assert!(message.contains("event-id-hash=sha256:"), "{message}");
    assert!(!message.contains("evt-extra-commit-history-wrapper-field"));
    assert!(
        store.delivered.lock().await.is_empty(),
        "extra commit-history wrapper fields must fail before acknowledgement"
    );
    assert!(
        graph.events.lock().await.is_empty(),
        "extra commit-history wrapper fields must fail before graph projection"
    );
    assert!(
        lineage.events.lock().await.is_empty(),
        "extra commit-history wrapper fields must fail before lineage projection"
    );
}

#[tokio::test]
async fn outbox_drain_rejects_missing_table_commit_history_receipt_principal() {
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-commit-history-missing-receipt-principal".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "table.commits-listed".to_string(),
            payload: json!({
                "audit-event-id": "audit-commit-history-missing-receipt-principal",
                "event-type": "table.commits-listed",
                "table": table,
                "payload": {
                    "authorization-receipt": {
                        "action": "table-load",
                        "allowed": true,
                        "engine": "test",
                        "policy_hash": null,
                        "checked_at": chrono::Utc::now(),
                    },
                    "warehouse": "local",
                    "namespace": ["default"],
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
        .expect_err("missing commit-history receipt principal should fail before delivery");

    let message = err.to_string();
    assert!(message.contains("table.commits-listed"));
    assert!(
        message
            .contains("table commit-history evidence must contain authorization receipt principal"),
        "{message}"
    );
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains("evt-commit-history-missing-receipt-principal"));
    assert!(
        store.delivered.lock().await.is_empty(),
        "missing commit-history receipt principal must fail before acknowledgement"
    );
    assert!(
        graph.events.lock().await.is_empty(),
        "missing commit-history receipt principal must fail before graph projection"
    );
    assert!(
        lineage.events.lock().await.is_empty(),
        "missing commit-history receipt principal must fail before lineage projection"
    );
}

#[tokio::test]
async fn outbox_drain_rejects_malformed_table_commit_history_receipt_principal() {
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-commit-history-malformed-receipt-principal".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "table.commits-listed".to_string(),
            payload: json!({
                "audit-event-id": "audit-commit-history-malformed-receipt-principal",
                "event-type": "table.commits-listed",
                "table": table,
                "payload": {
                    "authorization-receipt": {
                        "principal": {
                            "subject": "agent:writer",
                            "kind": "unknown"
                        },
                        "action": "table-load",
                        "allowed": true,
                        "engine": "test",
                        "policy_hash": null,
                        "checked_at": chrono::Utc::now(),
                    },
                    "warehouse": "local",
                    "namespace": ["default"],
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
        .expect_err("malformed commit-history receipt principal should fail before delivery");

    let message = err.to_string();
    assert!(message.contains("table.commits-listed"));
    assert!(message.contains("table commit-history authorization receipt principal"));
    assert!(message.contains("valid principal"));
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains("evt-commit-history-malformed-receipt-principal"));
    assert!(
        store.delivered.lock().await.is_empty(),
        "malformed commit-history receipt principal must fail before acknowledgement"
    );
    assert!(
        graph.events.lock().await.is_empty(),
        "malformed commit-history receipt principal must fail before graph projection"
    );
    assert!(
        lineage.events.lock().await.is_empty(),
        "malformed commit-history receipt principal must fail before lineage projection"
    );
}

#[tokio::test]
async fn outbox_drain_rejects_missing_table_commit_history_receipt_action() {
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let principal = Principal::new("agent:writer", PrincipalKind::Agent).unwrap();
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-commit-history-missing-receipt-action".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "table.commits-listed".to_string(),
            payload: json!({
                "audit-event-id": "audit-commit-history-missing-receipt-action",
                "event-type": "table.commits-listed",
                "table": table,
                "payload": {
                    "authorization-receipt": {
                        "principal": principal,
                        "allowed": true,
                        "engine": "test",
                        "policy_hash": null,
                        "checked_at": chrono::Utc::now(),
                    },
                    "warehouse": "local",
                    "namespace": ["default"],
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
        .expect_err("missing commit-history receipt action should fail before delivery");

    let message = err.to_string();
    assert!(message.contains("table.commits-listed"));
    assert!(
        message.contains("table commit-history evidence must contain authorization receipt action")
    );
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains("evt-commit-history-missing-receipt-action"));
    assert!(
        store.delivered.lock().await.is_empty(),
        "missing commit-history receipt action must fail before acknowledgement"
    );
    assert!(
        graph.events.lock().await.is_empty(),
        "missing commit-history receipt action must fail before graph projection"
    );
    assert!(
        lineage.events.lock().await.is_empty(),
        "missing commit-history receipt action must fail before lineage projection"
    );
}

#[tokio::test]
async fn outbox_drain_rejects_mismatched_table_commit_history_receipt_action() {
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let principal = Principal::new("agent:writer", PrincipalKind::Agent).unwrap();
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-commit-history-mismatched-receipt-action".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "table.commits-listed".to_string(),
            payload: json!({
                "audit-event-id": "audit-commit-history-mismatched-receipt-action",
                "event-type": "table.commits-listed",
                "table": table,
                "payload": {
                    "authorization-receipt": {
                        "principal": principal,
                        "action": "table-commit",
                        "allowed": true,
                        "engine": "test",
                        "policy_hash": null,
                        "checked_at": chrono::Utc::now(),
                    },
                    "warehouse": "local",
                    "namespace": ["default"],
                    "table": "events",
                    "principal-subject": "agent:writer",
                    "principal-kind": "agent",
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
        .expect_err("mismatched commit-history receipt action should fail before delivery");

    let message = err.to_string();
    assert!(message.contains("table.commits-listed"));
    assert!(message.contains(
        "table commit-history authorization receipt action does not match outbox event type"
    ));
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains("evt-commit-history-mismatched-receipt-action"));
    assert!(
        store.delivered.lock().await.is_empty(),
        "mismatched commit-history receipt action must fail before acknowledgement"
    );
    assert!(
        graph.events.lock().await.is_empty(),
        "mismatched commit-history receipt action must fail before graph projection"
    );
    assert!(
        lineage.events.lock().await.is_empty(),
        "mismatched commit-history receipt action must fail before lineage projection"
    );
}

#[tokio::test]
async fn outbox_drain_rejects_blank_table_commit_history_receipt_engine() {
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let principal = Principal::new("agent:writer", PrincipalKind::Agent).unwrap();
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-commit-history-blank-receipt-engine".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "table.commits-listed".to_string(),
            payload: json!({
                "audit-event-id": "audit-commit-history-blank-receipt-engine",
                "event-type": "table.commits-listed",
                "table": table,
                "payload": {
                    "authorization-receipt": {
                        "principal": principal,
                        "action": "table-load",
                        "allowed": true,
                        "engine": " ",
                        "policy_hash": null,
                        "checked_at": chrono::Utc::now(),
                    },
                    "warehouse": "local",
                    "namespace": ["default"],
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
        .expect_err("blank commit-history receipt engine should fail before delivery");

    let message = err.to_string();
    assert!(message.contains("table.commits-listed"));
    assert!(
        message.contains("table commit-history authorization receipt engine must be non-empty")
    );
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains("evt-commit-history-blank-receipt-engine"));
    assert!(
        store.delivered.lock().await.is_empty(),
        "blank commit-history receipt engine must fail before acknowledgement"
    );
    assert!(
        graph.events.lock().await.is_empty(),
        "blank commit-history receipt engine must fail before graph projection"
    );
    assert!(
        lineage.events.lock().await.is_empty(),
        "blank commit-history receipt engine must fail before lineage projection"
    );
}

#[tokio::test]
async fn outbox_drain_rejects_missing_or_denied_table_commit_history_receipt_allowed_decision() {
    for (case, allowed, expected) in [
        (
            "missing",
            None,
            "table commit-history evidence must contain authorization receipt allowed decision",
        ),
        (
            "denied",
            Some(false),
            "table commit-history authorization receipt must allow replay projection",
        ),
    ] {
        let table = TableIdent::new(
            WarehouseName::new("local").unwrap(),
            "default".parse::<Namespace>().unwrap(),
            TableName::new("events").unwrap(),
        );
        let principal = Principal::new("agent:writer", PrincipalKind::Agent).unwrap();
        let mut authorization_receipt = json!({
            "principal": principal,
            "action": "table-load",
            "engine": "test",
            "policy_hash": null,
            "checked_at": chrono::Utc::now(),
        });
        if let Some(allowed) = allowed {
            authorization_receipt["allowed"] = json!(allowed);
        }
        let event_id = format!("evt-commit-history-{case}-receipt-allowed");
        let store = Arc::new(RecordingOutboxStore {
            events: Mutex::new(vec![OutboxEvent {
                event_id: event_id.clone(),
                sink: "lakecat.lineage-and-graph".to_string(),
                event_type: "table.commits-listed".to_string(),
                payload: json!({
                    "audit-event-id": format!("audit-commit-history-{case}-receipt-allowed"),
                    "event-type": "table.commits-listed",
                    "table": table,
                    "payload": {
                        "authorization-receipt": authorization_receipt,
                        "warehouse": "local",
                        "namespace": ["default"],
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

        let err = match drain_outbox_once(&state, 10).await {
            Ok(_) => {
                panic!("{case} commit-history receipt allowed decision should fail before delivery")
            }
            Err(err) => err,
        };

        let message = err.to_string();
        assert!(message.contains("table.commits-listed"));
        assert!(message.contains(expected));
        assert!(message.contains("event-id-hash=sha256:"));
        assert!(!message.contains(&event_id));
        assert!(
            store.delivered.lock().await.is_empty(),
            "{case} commit-history receipt allowed decision must fail before acknowledgement"
        );
        assert!(
            graph.events.lock().await.is_empty(),
            "{case} commit-history receipt allowed decision must fail before graph projection"
        );
        assert!(
            lineage.events.lock().await.is_empty(),
            "{case} commit-history receipt allowed decision must fail before lineage projection"
        );
    }
}

#[tokio::test]
async fn outbox_drain_rejects_malformed_table_commit_history_receipt_checked_at() {
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let principal = Principal::new("agent:writer", PrincipalKind::Agent).unwrap();
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-commit-history-malformed-receipt-checked-at".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "table.commits-listed".to_string(),
            payload: json!({
                "audit-event-id": "audit-commit-history-malformed-receipt-checked-at",
                "event-type": "table.commits-listed",
                "table": table,
                "payload": {
                    "authorization-receipt": {
                        "principal": principal,
                        "action": "table-load",
                        "allowed": true,
                        "engine": "test",
                        "policy_hash": null,
                        "checked_at": "not-a-timestamp",
                    },
                    "warehouse": "local",
                    "namespace": ["default"],
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
        .expect_err("malformed commit-history receipt checked_at should fail before delivery");

    let message = err.to_string();
    assert!(message.contains("table.commits-listed"));
    assert!(message.contains(
        "table commit-history authorization receipt checked_at timestamp must be RFC3339"
    ));
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains("evt-commit-history-malformed-receipt-checked-at"));
    assert!(
        store.delivered.lock().await.is_empty(),
        "malformed commit-history receipt timestamp must fail before acknowledgement"
    );
    assert!(
        graph.events.lock().await.is_empty(),
        "malformed commit-history receipt timestamp must fail before graph projection"
    );
    assert!(
        lineage.events.lock().await.is_empty(),
        "malformed commit-history receipt timestamp must fail before lineage projection"
    );
}

#[tokio::test]
async fn outbox_drain_rejects_malformed_table_commit_history_principal_summary() {
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let principal = Principal::new("agent:writer", PrincipalKind::Agent).unwrap();
    let base_payload = json!({
        "audit-event-id": "audit-commit-history-principal-summary",
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
            "namespace": ["default"],
            "table": "events",
            "commit-count": 1,
            "commit-hashes": [
                content_hash_json(&json!({"commit": 1})).unwrap()
            ],
            "sequence-numbers": [1],
            "principal-subject": "agent:writer",
            "principal-kind": "agent",
        },
    });
    let mut missing_subject = base_payload.clone();
    missing_subject["payload"]
        .as_object_mut()
        .unwrap()
        .remove("principal-subject");
    let mut drifted_subject = base_payload.clone();
    drifted_subject["payload"]["principal-subject"] = json!("agent:forged");
    let mut drifted_kind = base_payload.clone();
    drifted_kind["payload"]["principal-kind"] = json!("human");

    for (event_id, payload, expected_message) in [
        (
            "evt-commit-history-missing-principal-subject",
            missing_subject,
            "table commit-history principal-subject must be a non-empty string",
        ),
        (
            "evt-commit-history-drifted-principal-subject",
            drifted_subject,
            "table commit-history principal-subject must match catalog evidence",
        ),
        (
            "evt-commit-history-drifted-principal-kind",
            drifted_kind,
            "table commit-history principal-kind must match catalog evidence",
        ),
    ] {
        let store = Arc::new(RecordingOutboxStore {
            events: Mutex::new(vec![OutboxEvent {
                event_id: event_id.to_string(),
                sink: "lakecat.lineage-and-graph".to_string(),
                event_type: "table.commits-listed".to_string(),
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
            .expect_err("malformed commit-history principal summary should fail");

        let message = err.to_string();
        assert!(message.contains("table.commits-listed"));
        assert!(
            message.contains(expected_message),
            "{event_id} should reject malformed principal summary proof: {message}"
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
async fn outbox_drain_rejects_malformed_namespace_lifecycle_evidence() {
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-secret-tenant-token".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "namespace.created".to_string(),
            payload: json!({
                "audit-event-id": "audit-corrupt-namespace",
                "event-type": "namespace.created",
                "payload": {
                    "warehouse": "local",
                    "namespace": ["default", ""],
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
        .expect_err("corrupt pending outbox event should fail");

    let message = err.to_string();
    assert!(
        message.contains("outbox event namespace.created (lakecat.lineage-and-graph) has invalid")
    );
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(message.contains("namespace lifecycle namespace components must be non-empty strings"));
    assert!(
        !message.contains("evt-secret-tenant-token"),
        "corrupt event id should be redacted from the operator-facing error"
    );
    assert!(
        store.delivered.lock().await.is_empty(),
        "corrupt pending event must fail before acknowledgement"
    );
    assert!(
        graph.events.lock().await.is_empty(),
        "corrupt pending event must fail before graph projection"
    );
    assert!(
        lineage.events.lock().await.is_empty(),
        "corrupt pending event must fail before lineage projection"
    );
}

#[tokio::test]
async fn outbox_drain_rejects_extra_namespace_lifecycle_fields() {
    let principal = Principal::new("agent:operator", PrincipalKind::Agent).unwrap();
    let cases = vec![
        (
            "namespace.created",
            "namespace-create",
            "created",
            "unverified-created-claim",
        ),
        (
            "namespace.loaded",
            "namespace-load",
            "loaded",
            "unverified-loaded-claim",
        ),
        (
            "namespace.dropped",
            "namespace-drop",
            "dropped",
            "unverified-dropped-claim",
        ),
    ];

    for (event_type, action, label, extra_field) in cases {
        let event_id = format!("evt-namespace-{label}-extra-field");
        let mut payload = json!({
            "authorization-receipt": {
                "principal": principal,
                "action": action,
                "allowed": true,
                "engine": "test",
                "policy_hash": null,
                "checked_at": chrono::Utc::now(),
            },
            "warehouse": "local",
            "namespace": ["default"],
        });
        payload[extra_field] = json!("shadow");

        let store = Arc::new(RecordingOutboxStore {
            events: Mutex::new(vec![OutboxEvent {
                event_id: event_id.clone(),
                sink: "lakecat.lineage-and-graph".to_string(),
                event_type: event_type.to_string(),
                payload: json!({
                    "audit-event-id": format!("audit-namespace-{label}-extra-field"),
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
            .expect_err("extra namespace lifecycle fields should fail before delivery");

        let message = err.to_string();
        assert!(
            message.contains(&format!(
                "outbox event {event_type} (lakecat.lineage-and-graph) has invalid"
            )),
            "{event_type} should be identified in the validation error"
        );
        assert!(message.contains("event-id-hash=sha256:"));
        assert!(
            message.contains(&format!(
                "namespace lifecycle contains unexpected field {extra_field}"
            )),
            "{message}"
        );
        assert!(!message.contains(&event_id));
        assert!(
            store.delivered.lock().await.is_empty(),
            "extra namespace lifecycle fields must fail before acknowledgement"
        );
        assert!(
            graph.events.lock().await.is_empty(),
            "extra namespace lifecycle fields must fail before graph projection"
        );
        assert!(
            lineage.events.lock().await.is_empty(),
            "extra namespace lifecycle fields must fail before lineage projection"
        );
    }
}

#[tokio::test]
async fn outbox_drain_rejects_extra_namespace_lifecycle_wrapper_fields() {
    let principal = Principal::new("agent:operator", PrincipalKind::Agent).unwrap();
    let cases = vec![
        (
            "namespace.created",
            "namespace-create",
            "created",
            "unverified-created-wrapper-claim",
        ),
        (
            "namespace.loaded",
            "namespace-load",
            "loaded",
            "unverified-loaded-wrapper-claim",
        ),
        (
            "namespace.dropped",
            "namespace-drop",
            "dropped",
            "unverified-dropped-wrapper-claim",
        ),
    ];

    for (event_type, action, label, extra_field) in cases {
        let event_id = format!("evt-namespace-{label}-extra-wrapper-field");
        let payload = json!({
            "event-type": event_type,
            "authorization-receipt": {
                "principal": principal,
                "action": action,
                "allowed": true,
                "engine": "test",
                "policy_hash": null,
                "checked_at": chrono::Utc::now(),
            },
            "warehouse": "local",
            "namespace": ["default"],
        });

        let store = Arc::new(RecordingOutboxStore {
            events: Mutex::new(vec![OutboxEvent {
                event_id: event_id.clone(),
                sink: "lakecat.lineage-and-graph".to_string(),
                event_type: event_type.to_string(),
                payload: json!({
                    "audit-event-id": format!("audit-namespace-{label}-extra-wrapper-field"),
                    "event-type": event_type,
                    "payload": payload,
                    extra_field: "shadow",
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
            .expect_err("extra namespace lifecycle wrapper fields should fail");

        let message = err.to_string();
        assert!(
            message.contains(&format!(
                "outbox event {event_type} (lakecat.lineage-and-graph) has invalid"
            )),
            "{event_type} should be identified in the validation error"
        );
        assert!(message.contains("event-id-hash=sha256:"));
        assert!(
            message.contains(&format!(
                "namespace lifecycle outbox payload contains unexpected field {extra_field}"
            )),
            "{message}"
        );
        assert!(!message.contains(&event_id));
        assert!(
            store.delivered.lock().await.is_empty(),
            "extra namespace lifecycle wrapper fields must fail before acknowledgement"
        );
        assert!(
            graph.events.lock().await.is_empty(),
            "extra namespace lifecycle wrapper fields must fail before graph projection"
        );
        assert!(
            lineage.events.lock().await.is_empty(),
            "extra namespace lifecycle wrapper fields must fail before lineage projection"
        );
    }
}

#[tokio::test]
async fn outbox_drain_rejects_blank_table_lifecycle_location_evidence() {
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let principal = Principal::new("agent:writer", PrincipalKind::Agent).unwrap();
    let checked_at = chrono::Utc::now();
    let cases = vec![
        (
            "evt-blank-table-metadata-location",
            "table.created",
            "table lifecycle metadata-location must be non-empty when present",
            json!({
                "audit-event-id": "audit-blank-table-metadata-location",
                "event-type": "table.created",
                "table": &table,
                "payload": {
                    "authorization-receipt": {
                        "principal": &principal,
                        "action": "table-create",
                        "allowed": true,
                        "engine": "test",
                        "policy_hash": null,
                        "checked_at": checked_at,
                    },
                    "metadata-location": " ",
                    "format-version": 3,
                    "version": 0,
                }
            }),
        ),
        (
            "evt-blank-table-location",
            "table.created",
            "table lifecycle location must be non-empty when present",
            json!({
                "audit-event-id": "audit-blank-table-location",
                "event-type": "table.created",
                "table": &table,
                "payload": {
                    "authorization-receipt": {
                        "principal": &principal,
                        "action": "table-create",
                        "allowed": true,
                        "engine": "test",
                        "policy_hash": null,
                        "checked_at": checked_at,
                    },
                    "location": "\t",
                    "format-version": 3,
                    "version": 0,
                }
            }),
        ),
        (
            "evt-blank-soft-delete-metadata-location",
            "table.deleted",
            "table lifecycle soft-delete metadata-location must be non-empty when present",
            json!({
                "audit-event-id": "audit-blank-soft-delete-metadata-location",
                "event-type": "table.deleted",
                "table": &table,
                "soft-delete": {
                    "table": &table,
                    "metadata-location": "\n",
                    "version": 1,
                    "format-version": 3,
                    "principal": &principal,
                    "authorization-receipt": null,
                    "deleted-at": checked_at,
                },
                "authorization-receipt": {
                    "principal": &principal,
                    "action": "table-drop",
                    "allowed": true,
                    "engine": "test",
                    "policy_hash": null,
                    "checked_at": checked_at,
                },
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
            .expect_err("blank table lifecycle location evidence should fail");

        let message = err.to_string();
        assert!(message.contains(&format!(
            "outbox event {event_type} (lakecat.lineage-and-graph) has invalid"
        )));
        assert!(message.contains(expected_message));
        assert!(message.contains("event-id-hash=sha256:"));
        assert!(!message.contains(event_id));
        assert!(
            store.delivered.lock().await.is_empty(),
            "blank table lifecycle location evidence must fail before acknowledgement"
        );
        assert!(
            graph.events.lock().await.is_empty(),
            "blank table lifecycle location evidence must fail before graph projection"
        );
        assert!(
            lineage.events.lock().await.is_empty(),
            "blank table lifecycle location evidence must fail before lineage projection"
        );
    }
}

#[tokio::test]
async fn outbox_drain_rejects_decorated_table_lifecycle_location_evidence() {
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let principal = Principal::new("agent:writer", PrincipalKind::Agent).unwrap();
    let checked_at = chrono::Utc::now();
    let cases = vec![
        (
            "evt-decorated-table-metadata-location",
            "table.created",
            "table lifecycle metadata-location must not contain decorated location material",
            json!({
                "audit-event-id": "audit-decorated-table-metadata-location",
                "event-type": "table.created",
                "table": &table,
                "payload": {
                    "authorization-receipt": {
                        "principal": &principal,
                        "action": "table-create",
                        "allowed": true,
                        "engine": "test",
                        "policy_hash": null,
                        "checked_at": checked_at,
                    },
                    "metadata-location": "s3://lakecat-demo/events/metadata/00000.json?token=secret",
                    "format-version": 3,
                    "version": 1,
                }
            }),
        ),
        (
            "evt-credential-table-metadata-location",
            "table.loaded",
            "table lifecycle metadata-location must not contain credential material",
            json!({
                "audit-event-id": "audit-credential-table-metadata-location",
                "event-type": "table.loaded",
                "table": &table,
                "payload": {
                    "authorization-receipt": {
                        "principal": &principal,
                        "action": "table-load",
                        "allowed": true,
                        "engine": "test",
                        "policy_hash": null,
                        "checked_at": checked_at,
                    },
                    "metadata-location": "s3://lakecat-demo/events/metadata/session_token=secret.json",
                    "format-version": 3,
                    "version": 1,
                }
            }),
        ),
        (
            "evt-decorated-table-location",
            "table.restored",
            "table lifecycle location must not contain decorated location material",
            json!({
                "audit-event-id": "audit-decorated-table-location",
                "event-type": "table.restored",
                "table": &table,
                "payload": {
                    "authorization-receipt": {
                        "principal": &principal,
                        "action": "table-restore",
                        "allowed": true,
                        "engine": "test",
                        "policy_hash": null,
                        "checked_at": checked_at,
                    },
                    "location": "s3://lakecat-demo/events#raw-secret",
                    "metadata-location": "s3://lakecat-demo/events/metadata/00000.json",
                    "format-version": 3,
                    "version": 1,
                }
            }),
        ),
        (
            "evt-credential-soft-delete-metadata-location",
            "table.deleted",
            "table lifecycle soft-delete metadata-location must not contain credential material",
            json!({
                "audit-event-id": "audit-credential-soft-delete-metadata-location",
                "event-type": "table.deleted",
                "table": &table,
                "soft-delete": {
                    "table": &table,
                    "metadata-location": "s3://lakecat-demo/events/metadata/access_key=secret.json",
                    "version": 1,
                    "format-version": 3,
                    "principal": &principal,
                    "authorization-receipt": null,
                    "deleted-at": checked_at,
                },
                "authorization-receipt": {
                    "principal": &principal,
                    "action": "table-drop",
                    "allowed": true,
                    "engine": "test",
                    "policy_hash": null,
                    "checked_at": checked_at,
                },
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
            .expect_err("decorated table lifecycle location evidence should fail");

        let message = err.to_string();
        assert!(message.contains(&format!(
            "outbox event {event_type} (lakecat.lineage-and-graph) has invalid"
        )));
        assert!(message.contains(expected_message), "{message}");
        assert!(message.contains("event-id-hash=sha256:"), "{message}");
        assert!(!message.contains(event_id), "{message}");
        assert!(
            store.delivered.lock().await.is_empty(),
            "decorated table lifecycle location evidence must fail before acknowledgement"
        );
        assert!(
            graph.events.lock().await.is_empty(),
            "decorated table lifecycle location evidence must fail before graph projection"
        );
        assert!(
            lineage.events.lock().await.is_empty(),
            "decorated table lifecycle location evidence must fail before lineage projection"
        );
    }
}

#[tokio::test]
async fn outbox_drain_rejects_malformed_table_lifecycle_version_evidence() {
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let principal = Principal::new("agent:writer", PrincipalKind::Agent).unwrap();
    let cases = vec![
        (
            "evt-created-missing-version",
            "table.created",
            "table lifecycle evidence must contain unsigned version",
            json!({
                "audit-event-id": "audit-created-missing-version",
                "event-type": "table.created",
                "table": &table,
                "payload": {
                    "authorization-receipt": {
                        "principal": &principal,
                        "action": "table-create",
                        "allowed": true,
                        "engine": "test",
                        "policy_hash": null,
                        "checked_at": chrono::Utc::now(),
                    },
                    "metadata-location": "file:///tmp/events/metadata/00000.json",
                    "format-version": 3,
                }
            }),
        ),
        (
            "evt-loaded-string-version",
            "table.loaded",
            "table lifecycle evidence must contain unsigned version",
            json!({
                "audit-event-id": "audit-loaded-string-version",
                "event-type": "table.loaded",
                "table": &table,
                "payload": {
                    "authorization-receipt": {
                        "principal": &principal,
                        "action": "table-load",
                        "allowed": true,
                        "engine": "test",
                        "policy_hash": null,
                        "checked_at": chrono::Utc::now(),
                    },
                    "metadata-location": "file:///tmp/events/metadata/00000.json",
                    "format-version": 3,
                    "version": "0",
                }
            }),
        ),
        (
            "evt-restored-missing-version",
            "table.restored",
            "table lifecycle evidence must contain unsigned version",
            json!({
                "audit-event-id": "audit-restored-missing-version",
                "event-type": "table.restored",
                "table": &table,
                "payload": {
                    "authorization-receipt": {
                        "principal": &principal,
                        "action": "table-restore",
                        "allowed": true,
                        "engine": "test",
                        "policy_hash": null,
                        "checked_at": chrono::Utc::now(),
                    },
                    "metadata-location": "file:///tmp/events/metadata/00000.json",
                    "format-version": 3,
                }
            }),
        ),
        (
            "evt-deleted-missing-soft-delete",
            "table.deleted",
            "table lifecycle delete evidence must contain soft-delete",
            json!({
                "audit-event-id": "audit-deleted-missing-soft-delete",
                "event-type": "table.deleted",
                "table": &table,
                "authorization-receipt": {
                    "principal": &principal,
                    "action": "table-drop",
                    "allowed": true,
                    "engine": "test",
                    "policy_hash": null,
                    "checked_at": chrono::Utc::now(),
                },
            }),
        ),
        (
            "evt-deleted-zero-soft-delete-version",
            "table.deleted",
            "table lifecycle soft-delete version must be positive",
            json!({
                "audit-event-id": "audit-deleted-zero-soft-delete-version",
                "event-type": "table.deleted",
                "table": &table,
                "soft-delete": {
                    "table": &table,
                    "metadata-location": "file:///tmp/events/metadata/00000.json",
                    "version": 0,
                    "format-version": 3,
                    "principal": &principal,
                    "authorization-receipt": null,
                    "deleted-at": chrono::Utc::now(),
                },
                "authorization-receipt": {
                    "principal": &principal,
                    "action": "table-drop",
                    "allowed": true,
                    "engine": "test",
                    "policy_hash": null,
                    "checked_at": chrono::Utc::now(),
                },
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
            .expect_err("malformed table lifecycle version evidence should fail");

        let message = err.to_string();
        assert!(message.contains(event_type));
        assert!(message.contains(expected_message), "{event_id}: {message}");
        assert!(message.contains("event-id-hash=sha256:"));
        assert!(!message.contains(event_id));
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
async fn outbox_drain_rejects_malformed_table_lifecycle_format_version_evidence() {
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
    let cases = vec![
        (
            "evt-created-missing-format-version",
            "table.created",
            "table lifecycle evidence must contain positive format-version",
            json!({
                "audit-event-id": "audit-created-missing-format-version",
                "event-type": "table.created",
                "table": &table,
                "payload": {
                    "authorization-receipt": receipt("table-create"),
                    "metadata-location": "file:///tmp/events/metadata/00000.json",
                    "version": 0,
                }
            }),
        ),
        (
            "evt-loaded-string-format-version",
            "table.loaded",
            "table lifecycle format-version must be a positive integer",
            json!({
                "audit-event-id": "audit-loaded-string-format-version",
                "event-type": "table.loaded",
                "table": &table,
                "payload": {
                    "authorization-receipt": receipt("table-load"),
                    "metadata-location": "file:///tmp/events/metadata/00000.json",
                    "format-version": "3",
                    "version": 0,
                }
            }),
        ),
        (
            "evt-restored-zero-format-version",
            "table.restored",
            "table lifecycle format-version must be positive",
            json!({
                "audit-event-id": "audit-restored-zero-format-version",
                "event-type": "table.restored",
                "table": &table,
                "payload": {
                    "authorization-receipt": receipt("table-restore"),
                    "metadata-location": "file:///tmp/events/metadata/00000.json",
                    "format-version": 0,
                    "version": 1,
                }
            }),
        ),
        (
            "evt-deleted-missing-soft-delete-format-version",
            "table.deleted",
            "table lifecycle soft-delete evidence must contain positive format-version",
            json!({
                "audit-event-id": "audit-deleted-missing-soft-delete-format-version",
                "event-type": "table.deleted",
                "table": &table,
                "soft-delete": {
                    "table": &table,
                    "metadata-location": "file:///tmp/events/metadata/00000.json",
                    "version": 1,
                    "principal": &principal,
                    "authorization-receipt": null,
                    "deleted-at": checked_at,
                },
                "authorization-receipt": receipt("table-drop"),
            }),
        ),
        (
            "evt-deleted-zero-soft-delete-format-version",
            "table.deleted",
            "table lifecycle soft-delete format-version must be positive",
            json!({
                "audit-event-id": "audit-deleted-zero-soft-delete-format-version",
                "event-type": "table.deleted",
                "table": &table,
                "soft-delete": {
                    "table": &table,
                    "metadata-location": "file:///tmp/events/metadata/00000.json",
                    "version": 1,
                    "format-version": 0,
                    "principal": &principal,
                    "authorization-receipt": null,
                    "deleted-at": checked_at,
                },
                "authorization-receipt": receipt("table-drop"),
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
            .expect_err("malformed table lifecycle format-version evidence should fail");

        let message = err.to_string();
        assert!(message.contains(event_type));
        assert!(message.contains(expected_message), "{event_id}: {message}");
        assert!(message.contains("event-id-hash=sha256:"));
        assert!(!message.contains(event_id));
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
async fn outbox_drain_accepts_snake_case_table_lifecycle_soft_delete_format_version() {
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let principal = Principal::new("agent:writer", PrincipalKind::Agent).unwrap();
    let checked_at = chrono::Utc::now();
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-snake-soft-delete-format-version".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "table.deleted".to_string(),
            payload: json!({
                "audit-event-id": "audit-snake-soft-delete-format-version",
                "event-type": "table.deleted",
                "table": &table,
                "soft-delete": {
                    "table": &table,
                    "metadata-location": "file:///tmp/events/metadata/00000.json",
                    "version": 1,
                    "format_version": 3,
                    "principal": &principal,
                    "authorization-receipt": null,
                    "deleted-at": checked_at,
                },
                "authorization-receipt": {
                    "principal": &principal,
                    "action": "table-drop",
                    "allowed": true,
                    "engine": "test",
                    "policy_hash": null,
                    "checked_at": checked_at,
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

    let summary = drain_outbox_once(&state, 10)
        .await
        .expect("snake_case soft-delete format-version evidence should be accepted");

    assert_eq!(summary.delivered, 1);
    assert_eq!(store.delivered.lock().await.len(), 1);
    assert!(
        graph.events.lock().await.iter().any(|event| {
            event.label == GraphNodeLabel::Table && event.action == GraphAction::Deleted
        }),
        "snake_case soft-delete evidence should still project graph delete proof"
    );
    assert!(
        lineage
            .events
            .lock()
            .await
            .iter()
            .any(|event| event.event_type == LineageEventType::TableDeleted),
        "snake_case soft-delete evidence should still project lineage delete proof"
    );
}

#[tokio::test]
async fn outbox_drain_rejects_duplicate_table_lifecycle_soft_delete_format_version_aliases() {
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let principal = Principal::new("agent:writer", PrincipalKind::Agent).unwrap();
    let checked_at = chrono::Utc::now();
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-duplicate-soft-delete-format-version".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "table.deleted".to_string(),
            payload: json!({
                "audit-event-id": "audit-duplicate-soft-delete-format-version",
                "event-type": "table.deleted",
                "table": &table,
                "soft-delete": {
                    "table": &table,
                    "metadata-location": "file:///tmp/events/metadata/00000.json",
                    "version": 1,
                    "format-version": 3,
                    "format_version": 3,
                    "principal": &principal,
                    "authorization-receipt": null,
                    "deleted-at": checked_at,
                },
                "authorization-receipt": {
                    "principal": &principal,
                    "action": "table-drop",
                    "allowed": true,
                    "engine": "test",
                    "policy_hash": null,
                    "checked_at": checked_at,
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
        .expect_err("duplicate soft-delete format-version aliases should fail");

    let message = err.to_string();
    assert!(message.contains("table.deleted"), "{message}");
    assert!(
        message.contains(
            "table lifecycle soft-delete must not carry both format-version and format_version evidence fields"
        ),
        "{message}"
    );
    assert!(message.contains("event-id-hash=sha256:"), "{message}");
    assert!(
        !message.contains("evt-duplicate-soft-delete-format-version"),
        "{message}"
    );
    assert!(
        store.delivered.lock().await.is_empty(),
        "duplicate soft-delete aliases must fail before acknowledgement"
    );
    assert!(
        graph.events.lock().await.is_empty(),
        "duplicate soft-delete aliases must fail before graph projection"
    );
    assert!(
        lineage.events.lock().await.is_empty(),
        "duplicate soft-delete aliases must fail before lineage projection"
    );
}

#[tokio::test]
async fn outbox_drain_rejects_extra_top_level_table_lifecycle_fields() {
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let principal = Principal::new("agent:writer", PrincipalKind::Agent).unwrap();
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-table-created-extra-top-level-field".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "table.created".to_string(),
            payload: json!({
                "audit-event-id": "audit-table-created-extra-top-level-field",
                "event-type": "table.created",
                "table": &table,
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
                    "version": 1,
                    "unverified-table-lifecycle-claim": "shadow",
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
        .expect_err("extra top-level table lifecycle fields should fail");
    let message = err.to_string();
    assert!(message.contains("table.created"), "{message}");
    assert!(
        message
            .contains("table lifecycle contains unexpected field unverified-table-lifecycle-claim"),
        "{message}"
    );
    assert!(message.contains("event-id-hash=sha256:"), "{message}");
    assert!(
        !message.contains("evt-table-created-extra-top-level-field"),
        "{message}"
    );
    assert!(store.delivered.lock().await.is_empty());
    assert!(graph.events.lock().await.is_empty());
    assert!(lineage.events.lock().await.is_empty());
}

#[tokio::test]
async fn outbox_drain_rejects_extra_table_lifecycle_wrapper_fields() {
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
    let cases = vec![
        (
            "table.created",
            "created",
            json!({
                "authorization-receipt": receipt("table-create"),
                "warehouse": "local",
                "namespace": ["default"],
                "table": "events",
                "metadata-location": "file:///tmp/events/metadata/00000.json",
                "format-version": 3,
                "version": 1,
            }),
            None,
        ),
        (
            "table.loaded",
            "loaded",
            json!({
                "authorization-receipt": receipt("table-load"),
                "warehouse": "local",
                "namespace": ["default"],
                "table": "events",
                "metadata-location": "file:///tmp/events/metadata/00000.json",
                "format-version": 3,
                "version": 1,
            }),
            None,
        ),
        (
            "table.deleted",
            "deleted",
            json!({
                "authorization-receipt": receipt("table-drop"),
                "warehouse": "local",
                "namespace": ["default"],
                "table": "events",
            }),
            Some(json!({
                "table": &table,
                "metadata-location": "file:///tmp/events/metadata/00000.json",
                "version": 1,
                "format-version": 3,
                "principal": &principal,
                "authorization-receipt": null,
                "deleted-at": checked_at,
            })),
        ),
        (
            "table.restored",
            "restored",
            json!({
                "authorization-receipt": receipt("table-restore"),
                "warehouse": "local",
                "namespace": ["default"],
                "table": "events",
                "metadata-location": "file:///tmp/events/metadata/00001.json",
                "format-version": 3,
                "version": 2,
            }),
            None,
        ),
    ];

    for (event_type, label, payload, soft_delete) in cases {
        let event_id = format!("evt-table-{label}-extra-wrapper-field");
        let mut wrapper = json!({
            "audit-event-id": format!("audit-table-{label}-extra-wrapper-field"),
            "event-type": event_type,
            "table": &table,
            "payload": payload,
            "unverified-table-lifecycle-wrapper-claim": "shadow",
        });
        if let Some(soft_delete) = soft_delete {
            wrapper["soft-delete"] = soft_delete;
        }
        let store = Arc::new(RecordingOutboxStore {
            events: Mutex::new(vec![OutboxEvent {
                event_id: event_id.clone(),
                sink: "lakecat.lineage-and-graph".to_string(),
                event_type: event_type.to_string(),
                payload: wrapper,
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
            .expect_err("extra table lifecycle wrapper fields should fail");

        let message = err.to_string();
        assert!(message.contains(event_type), "{message}");
        assert!(message.contains("event-id-hash=sha256:"), "{message}");
        assert!(
            message.contains(
                "table lifecycle outbox payload contains unexpected field unverified-table-lifecycle-wrapper-claim"
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
async fn outbox_drain_rejects_missing_table_lifecycle_receipt_principal() {
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let cases = vec![
        ("table.created", "table-create"),
        ("table.loaded", "table-load"),
        ("table.deleted", "table-drop"),
        ("table.restored", "table-restore"),
    ];

    for (event_type, action) in cases {
        let event_id = format!("evt-missing-{event_type}-principal");
        let store = Arc::new(RecordingOutboxStore {
            events: Mutex::new(vec![OutboxEvent {
                event_id: event_id.clone(),
                sink: "lakecat.lineage-and-graph".to_string(),
                event_type: event_type.to_string(),
                payload: json!({
                    "audit-event-id": format!("audit-missing-{event_type}-principal"),
                    "event-type": event_type,
                    "table": &table,
                    "payload": {
                        "authorization-receipt": {
                            "action": action,
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
            .expect_err("missing table lifecycle receipt principal should fail");

        let message = err.to_string();
        assert!(message.contains(event_type));
        assert!(
            message
                .contains("table lifecycle evidence must contain authorization receipt principal")
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
async fn outbox_drain_rejects_malformed_table_lifecycle_receipt_principal() {
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let malformed_principal = json!({
        "subject": "agent:writer",
        "kind": "unknown"
    });
    let cases = vec![
        ("table.created", "table-create"),
        ("table.loaded", "table-load"),
        ("table.deleted", "table-drop"),
        ("table.restored", "table-restore"),
    ];

    for (event_type, action) in cases {
        let event_id = format!("evt-malformed-{event_type}-principal");
        let store = Arc::new(RecordingOutboxStore {
            events: Mutex::new(vec![OutboxEvent {
                event_id: event_id.clone(),
                sink: "lakecat.lineage-and-graph".to_string(),
                event_type: event_type.to_string(),
                payload: json!({
                    "audit-event-id": format!("audit-malformed-{event_type}-principal"),
                    "event-type": event_type,
                    "table": &table,
                    "payload": {
                        "authorization-receipt": {
                            "principal": malformed_principal.clone(),
                            "action": action,
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
            .expect_err("malformed table lifecycle receipt principal should fail");

        let message = err.to_string();
        assert!(message.contains(event_type));
        assert!(message.contains("table lifecycle authorization receipt principal"));
        assert!(message.contains("must be a valid principal"));
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
async fn outbox_drain_rejects_missing_or_denied_table_lifecycle_receipt_allowed_decision() {
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let principal = Principal::new("agent:writer", PrincipalKind::Agent).unwrap();
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
    let cases = vec![
        (
            "table.created",
            "evt-table-created-missing-receipt-allowed",
            missing_allowed(receipt("table-create")),
            json!({
                "version": 1,
                "metadata-location": "file:///tmp/events/metadata/00000.json",
            }),
            "table lifecycle evidence must contain authorization receipt allowed decision",
        ),
        (
            "table.loaded",
            "evt-table-loaded-denied-receipt",
            denied(receipt("table-load")),
            json!({
                "version": 1,
                "metadata-location": "file:///tmp/events/metadata/00000.json",
            }),
            "table lifecycle authorization receipt must allow replay projection",
        ),
        (
            "table.deleted",
            "evt-table-deleted-missing-receipt-allowed",
            missing_allowed(receipt("table-drop")),
            json!({
                "soft-delete": {
                    "table": &table,
                    "metadata-location": "file:///tmp/events/metadata/00000.json",
                    "version": 1,
                    "principal": &principal,
                    "authorization-receipt": null,
                    "deleted-at": chrono::Utc::now(),
                },
            }),
            "table lifecycle evidence must contain authorization receipt allowed decision",
        ),
        (
            "table.restored",
            "evt-table-restored-denied-receipt",
            denied(receipt("table-restore")),
            json!({
                "version": 2,
                "metadata-location": "file:///tmp/events/metadata/00001.json",
            }),
            "table lifecycle authorization receipt must allow replay projection",
        ),
    ];

    for (event_type, event_id, receipt, mut payload, expected_message) in cases {
        payload["authorization-receipt"] = receipt;
        let store = Arc::new(RecordingOutboxStore {
            events: Mutex::new(vec![OutboxEvent {
                event_id: event_id.to_string(),
                sink: "lakecat.lineage-and-graph".to_string(),
                event_type: event_type.to_string(),
                payload: json!({
                    "audit-event-id": format!("audit-{event_id}"),
                    "event-type": event_type,
                    "table": &table,
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
            .expect_err("missing or denied table lifecycle decision should fail");

        let message = err.to_string();
        assert!(message.contains(event_type), "{message}");
        assert!(message.contains(expected_message), "{message}");
        assert!(message.contains("event-id-hash=sha256:"));
        assert!(!message.contains(event_id));
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
async fn outbox_drain_rejects_mismatched_table_lifecycle_receipt_actions() {
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let principal = Principal::new("agent:writer", PrincipalKind::Agent).unwrap();
    let cases = vec![
        (
            "table.created",
            json!({
                "version": 1,
                "metadata-location": "file:///tmp/events/metadata/00000.json",
            }),
        ),
        (
            "table.loaded",
            json!({
                "version": 1,
                "metadata-location": "file:///tmp/events/metadata/00000.json",
            }),
        ),
        (
            "table.deleted",
            json!({
                "soft-delete": {
                    "table": &table,
                    "metadata-location": "file:///tmp/events/metadata/00000.json",
                    "version": 1,
                    "principal": &principal,
                    "authorization-receipt": null,
                    "deleted-at": chrono::Utc::now(),
                },
            }),
        ),
        (
            "table.restored",
            json!({
                "version": 2,
                "metadata-location": "file:///tmp/events/metadata/00001.json",
            }),
        ),
    ];

    for (event_type, mut payload) in cases {
        payload["authorization-receipt"] = json!({
            "principal": &principal,
            "action": "namespace-list",
            "allowed": true,
            "engine": "test",
            "policy_hash": null,
            "checked_at": chrono::Utc::now(),
        });
        let event_id = format!("evt-mismatched-{event_type}-action-token");
        let store = Arc::new(RecordingOutboxStore {
            events: Mutex::new(vec![OutboxEvent {
                event_id: event_id.clone(),
                sink: "lakecat.lineage-and-graph".to_string(),
                event_type: event_type.to_string(),
                payload: json!({
                    "audit-event-id": format!("audit-mismatched-{event_type}-action"),
                    "event-type": event_type,
                    "table": &table,
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
            .expect_err("mismatched table lifecycle receipt action should fail");

        let message = err.to_string();
        assert!(message.contains(event_type));
        assert!(
            message.contains(
                "table lifecycle authorization receipt action does not match outbox event type"
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
