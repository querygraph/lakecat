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
async fn outbox_drain_rejects_malformed_credential_vend_evidence() {
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let principal = Principal {
        subject: "agent:reader".to_string(),
        kind: PrincipalKind::Agent,
    };
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-malformed-credential-vend".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "credentials.vend-attempted".to_string(),
            payload: json!({
                "audit-event-id": "audit-malformed-credential-vend",
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
                    "credential-count": 0,
                    "credential-response-evidence": [],
                    "storage-profile-id": "events-local",
                    "storage-profile": {
                        "profile-id": "events-local",
                        "warehouse": "local",
                        "provider": "file",
                        "issuance-mode": "local-file-no-secret",
                        "secret-ref-present": false,
                        "location-prefix-hash": "sha256:location"
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
        .expect_err("malformed credential-vend evidence should fail before delivery");

    let message = err.to_string();
    assert!(message.contains("credentials.vend-attempted"));
    assert!(message.contains("location-prefix-hash"));
    assert!(message.contains("full SHA-256"));
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(store.delivered.lock().await.is_empty());
    assert!(graph.events.lock().await.is_empty());
    assert!(lineage.events.lock().await.is_empty());
}

#[tokio::test]
async fn outbox_drain_rejects_extra_top_level_credential_vend_fields() {
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let principal = Principal {
        subject: "agent:reader".to_string(),
        kind: PrincipalKind::Agent,
    };
    let raw_exception = json!({
        "requested": true,
        "allowed": false,
        "reason": "fine-grained read restriction requires Sail-planned reads",
    });
    let read_restriction = json!({
        "allowed-columns": ["event_id"],
        "row-predicate": {
            "type": "eq",
            "term": "tenant_id",
            "value": "tenant-a",
        },
        "purpose": "fraud-review",
        "policy-hashes": [content_hash_bytes(b"credential-vend-policy")],
        "max-credential-ttl-seconds": 300,
    });
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-credential-vend-extra-top-level-field".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "credentials.vend-attempted".to_string(),
            payload: json!({
                "audit-event-id": "audit-credential-vend-extra-top-level-field",
                "event-type": "credentials.vend-attempted",
                "table": &table,
                "payload": {
                    "authorization-receipt": {
                        "principal": principal,
                        "action": "credentials-vend",
                        "allowed": true,
                        "engine": "test",
                        "policy_hash": null,
                        "checked_at": chrono::Utc::now(),
                        "context": {
                            "read-restriction": read_restriction,
                            "lakecat:raw-credential-exception": raw_exception,
                        },
                    },
                    "read-restriction": read_restriction,
                    "lakecat:raw-credential-exception": raw_exception,
                    "lakecat:credential-block-reason": "fine-grained read restriction requires Sail-planned reads",
                    "storage-location": "file:///tmp/lakecat/events",
                    "storage-profile-id": "events-local",
                    "storage-profile": {
                        "profile-id": "events-local",
                        "warehouse": "local",
                        "provider": "file",
                        "issuance-mode": "local-file-no-secret",
                        "secret-ref-present": false,
                        "location-prefix-hash": content_hash_json(&json!({
                            "location-prefix": "file:///tmp/lakecat/events"
                        })).unwrap(),
                    },
                    "secret-ref-present": false,
                    "credential-count": 0,
                    "credential-response-evidence": [],
                    "mode": "local-file-no-secret",
                    "unverified-credential-vend-claim": "shadow",
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
        .expect_err("extra top-level credential-vend fields should fail");

    let message = err.to_string();
    assert!(message.contains("credentials.vend-attempted"));
    assert!(
        message
            .contains("credential-vend contains unexpected field unverified-credential-vend-claim")
    );
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains("evt-credential-vend-extra-top-level-field"));
    assert!(store.delivered.lock().await.is_empty());
    assert!(graph.events.lock().await.is_empty());
    assert!(lineage.events.lock().await.is_empty());
}

#[tokio::test]
async fn outbox_drain_rejects_extra_credential_vend_outbox_payload_fields() {
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let principal = Principal {
        subject: "human:operator".to_string(),
        kind: PrincipalKind::Human,
    };
    let event_id = "evt-credential-vend-extra-wrapper-field";
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: event_id.to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "credentials.vend-attempted".to_string(),
            payload: json!({
                "audit-event-id": "audit-credential-vend-extra-wrapper-field",
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
                "unverified-credential-scope-claim": "shadow"
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
        .expect_err("extra credential-vend wrapper fields should fail before delivery");

    let message = err.to_string();
    assert!(message.contains("credentials.vend-attempted"));
    assert!(
        message.contains(
            "credential-vend outbox payload contains unexpected field unverified-credential-scope-claim"
        ),
        "extra credential wrapper field should be rejected: {message}"
    );
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains(event_id));
    assert!(
        store.delivered.lock().await.is_empty(),
        "extra credential wrapper fields must fail before acknowledgement"
    );
    assert!(
        graph.events.lock().await.is_empty(),
        "extra credential wrapper fields must fail before graph projection"
    );
    assert!(
        lineage.events.lock().await.is_empty(),
        "extra credential wrapper fields must fail before lineage projection"
    );
}

#[tokio::test]
async fn outbox_drain_rejects_malformed_credential_vend_location_or_mode_evidence() {
    let principal = Principal {
        subject: "agent:reader".to_string(),
        kind: PrincipalKind::Agent,
    };
    let cases = [
        (
            "blank-storage-location",
            "storage-location",
            json!(" "),
            "credential-vend storage-location must be a non-empty string",
        ),
        (
            "decorated-storage-location",
            "storage-location",
            json!("s3://lakecat-demo/events?token=secret"),
            "credential-vend storage-location must not contain decorated location material",
        ),
        (
            "credential-bearing-storage-location",
            "storage-location",
            json!("s3://lakecat-demo/events/access_key=secret"),
            "credential-vend storage-location must not contain credential material",
        ),
        (
            "mode-drift",
            "mode",
            json!("short-lived-secret-ref"),
            "credential-vend evidence mode must match catalog evidence",
        ),
    ];

    for (case, field, value, expected_message) in cases {
        let table = TableIdent::new(
            WarehouseName::new("local").unwrap(),
            "default".parse::<Namespace>().unwrap(),
            TableName::new("events").unwrap(),
        );
        let mut payload = json!({
            "authorization-receipt": {
                "principal": principal.clone(),
                "action": "credentials-vend",
                "allowed": true,
                "engine": "test",
                "policy_hash": null,
                "checked_at": chrono::Utc::now(),
            },
            "storage-location": "s3://lakecat-demo/events",
            "storage-profile-id": "events-local",
            "storage-profile": {
                "profile-id": "events-local",
                "warehouse": "local",
                "provider": "file",
                "issuance-mode": "local-file-no-secret",
                "secret-ref-present": false,
                "location-prefix-hash": "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
            },
            "secret-ref-present": false,
            "credential-count": 0,
            "credential-response-evidence": [],
            "mode": "local-file-no-secret",
        });
        payload[field] = value;
        let event_id = format!("evt-credential-vend-{case}");
        let store = Arc::new(RecordingOutboxStore {
            events: Mutex::new(vec![OutboxEvent {
                event_id: event_id.clone(),
                sink: "lakecat.lineage-and-graph".to_string(),
                event_type: "credentials.vend-attempted".to_string(),
                payload: json!({
                    "audit-event-id": format!("audit-{event_id}"),
                    "event-type": "credentials.vend-attempted",
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
            .expect_err("malformed credential-vend location or mode proof should fail");

        let message = err.to_string();
        assert!(message.contains("credentials.vend-attempted"));
        assert!(
            message.contains(expected_message),
            "credential-vend should reject {case}: {message}"
        );
        assert!(message.contains("event-id-hash=sha256:"));
        assert!(!message.contains(&event_id));
        assert!(
            store.delivered.lock().await.is_empty(),
            "{case} credential-vend evidence must fail before acknowledgement"
        );
        assert!(
            graph.events.lock().await.is_empty(),
            "{case} credential-vend evidence must fail before graph projection"
        );
        assert!(
            lineage.events.lock().await.is_empty(),
            "{case} credential-vend evidence must fail before lineage projection"
        );
    }
}

#[tokio::test]
async fn outbox_drain_rejects_mismatched_credential_vend_receipt_action() {
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let principal = Principal {
        subject: "human:operator".to_string(),
        kind: PrincipalKind::Human,
    };
    let event_id = "evt-credential-vend-action-drift";
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: event_id.to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "credentials.vend-attempted".to_string(),
            payload: json!({
                "audit-event-id": "audit-credential-vend-action-drift",
                "event-type": "credentials.vend-attempted",
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
        .expect_err("mismatched credential-vend receipt action should fail");

    let message = err.to_string();
    assert!(message.contains("credentials.vend-attempted"));
    assert!(
        message.contains(
            "credential-vend authorization receipt action does not match outbox event type"
        )
    );
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains(event_id));
    assert!(
        store.delivered.lock().await.is_empty(),
        "mismatched credential-vend receipt action must fail before acknowledgement"
    );
    assert!(
        graph.events.lock().await.is_empty(),
        "mismatched credential-vend receipt action must fail before graph projection"
    );
    assert!(
        lineage.events.lock().await.is_empty(),
        "mismatched credential-vend receipt action must fail before lineage projection"
    );
}

#[tokio::test]
async fn outbox_drain_rejects_missing_or_denied_credential_vend_receipt_allowed_decision() {
    let principal = Principal {
        subject: "human:operator".to_string(),
        kind: PrincipalKind::Human,
    };
    for (case, allowed, expected_message) in [
        (
            "missing",
            None,
            "credential-vend evidence must contain authorization receipt allowed decision",
        ),
        (
            "denied",
            Some(false),
            "credential-vend authorization receipt must allow replay projection",
        ),
    ] {
        let table = TableIdent::new(
            WarehouseName::new("local").unwrap(),
            "default".parse::<Namespace>().unwrap(),
            TableName::new("events").unwrap(),
        );
        let mut authorization_receipt = json!({
            "principal": principal.clone(),
            "action": "credentials-vend",
            "engine": "test",
            "policy_hash": null,
            "checked_at": chrono::Utc::now(),
        });
        if let Some(allowed) = allowed {
            authorization_receipt["allowed"] = json!(allowed);
        }
        let event_id = format!("evt-credential-vend-{case}-receipt-allowed");
        let store = Arc::new(RecordingOutboxStore {
            events: Mutex::new(vec![OutboxEvent {
                event_id: event_id.clone(),
                sink: "lakecat.lineage-and-graph".to_string(),
                event_type: "credentials.vend-attempted".to_string(),
                payload: json!({
                    "audit-event-id": format!("audit-{event_id}"),
                    "event-type": "credentials.vend-attempted",
                    "table": table,
                    "payload": {
                        "authorization-receipt": authorization_receipt,
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
            .expect_err("missing or denied credential-vend receipt decision should fail");

        let message = err.to_string();
        assert!(message.contains("credentials.vend-attempted"));
        assert!(
            message.contains(expected_message),
            "credential-vend should reject {case} receipt allowed decision: {message}"
        );
        assert!(message.contains("event-id-hash=sha256:"));
        assert!(!message.contains(&event_id));
        assert!(
            store.delivered.lock().await.is_empty(),
            "{case} credential-vend receipt decision must fail before acknowledgement"
        );
        assert!(
            graph.events.lock().await.is_empty(),
            "{case} credential-vend receipt decision must fail before graph projection"
        );
        assert!(
            lineage.events.lock().await.is_empty(),
            "{case} credential-vend receipt decision must fail before lineage projection"
        );
    }
}

#[tokio::test]
async fn outbox_drain_rejects_malformed_credential_vend_receipt_engine_or_checked_at() {
    let principal = Principal {
        subject: "human:operator".to_string(),
        kind: PrincipalKind::Human,
    };
    let cases = [
        (
            "missing-engine",
            "credential-vend evidence must contain authorization receipt engine",
        ),
        (
            "blank-engine",
            "credential-vend authorization receipt engine must be non-empty",
        ),
        (
            "missing-checked-at",
            "credential-vend evidence must contain authorization receipt checked_at timestamp",
        ),
        (
            "malformed-checked-at",
            "credential-vend authorization receipt checked_at timestamp must be RFC3339",
        ),
    ];

    for (case, expected_message) in cases {
        let table = TableIdent::new(
            WarehouseName::new("local").unwrap(),
            "default".parse::<Namespace>().unwrap(),
            TableName::new("events").unwrap(),
        );
        let mut authorization_receipt = json!({
            "principal": principal.clone(),
            "action": "credentials-vend",
            "allowed": true,
            "engine": "test",
            "policy_hash": null,
            "checked_at": chrono::Utc::now(),
        });
        match case {
            "missing-engine" => {
                authorization_receipt
                    .as_object_mut()
                    .unwrap()
                    .remove("engine");
            }
            "blank-engine" => authorization_receipt["engine"] = json!("   "),
            "missing-checked-at" => {
                authorization_receipt
                    .as_object_mut()
                    .unwrap()
                    .remove("checked_at");
            }
            "malformed-checked-at" => {
                authorization_receipt["checked_at"] = json!("not-a-timestamp");
            }
            _ => unreachable!("unexpected credential-vend receipt case"),
        }
        let event_id = format!("evt-credential-vend-{case}-receipt-shape");
        let store = Arc::new(RecordingOutboxStore {
            events: Mutex::new(vec![OutboxEvent {
                event_id: event_id.clone(),
                sink: "lakecat.lineage-and-graph".to_string(),
                event_type: "credentials.vend-attempted".to_string(),
                payload: json!({
                    "audit-event-id": format!("audit-{event_id}"),
                    "event-type": "credentials.vend-attempted",
                    "table": table,
                    "payload": {
                        "authorization-receipt": authorization_receipt,
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
            .expect_err("malformed credential-vend receipt shape should fail");

        let message = err.to_string();
        assert!(message.contains("credentials.vend-attempted"));
        assert!(
            message.contains(expected_message),
            "credential-vend should reject {case} receipt shape: {message}"
        );
        assert!(message.contains("event-id-hash=sha256:"));
        assert!(!message.contains(&event_id));
        assert!(
            store.delivered.lock().await.is_empty(),
            "{case} credential-vend receipt shape must fail before acknowledgement"
        );
        assert!(
            graph.events.lock().await.is_empty(),
            "{case} credential-vend receipt shape must fail before graph projection"
        );
        assert!(
            lineage.events.lock().await.is_empty(),
            "{case} credential-vend receipt shape must fail before lineage projection"
        );
    }
}

#[tokio::test]
async fn outbox_drain_rejects_malformed_credential_vend_receipt_principal() {
    let cases = [
        (
            "evt-credential-vend-missing-receipt-principal",
            None,
            "credential-vend evidence must contain authorization receipt principal",
        ),
        (
            "evt-credential-vend-malformed-receipt-principal",
            Some(json!({
                "subject": "agent:reader",
                "kind": "unknown"
            })),
            "credential-vend authorization receipt principal must be a valid principal",
        ),
    ];

    for (event_id, principal, expected_message) in cases {
        let table = TableIdent::new(
            WarehouseName::new("local").unwrap(),
            "default".parse::<Namespace>().unwrap(),
            TableName::new("events").unwrap(),
        );
        let mut authorization_receipt = json!({
            "action": "credentials-vend",
            "allowed": true,
            "engine": "test",
            "policy_hash": null,
            "checked_at": chrono::Utc::now(),
        });
        if let Some(principal) = principal {
            authorization_receipt["principal"] = principal;
        }
        let store = Arc::new(RecordingOutboxStore {
            events: Mutex::new(vec![OutboxEvent {
                event_id: event_id.to_string(),
                sink: "lakecat.lineage-and-graph".to_string(),
                event_type: "credentials.vend-attempted".to_string(),
                payload: json!({
                    "audit-event-id": format!("audit-{event_id}"),
                    "event-type": "credentials.vend-attempted",
                    "table": table,
                    "payload": {
                        "authorization-receipt": authorization_receipt,
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
            .expect_err("malformed credential-vend receipt principal should fail");

        let message = err.to_string();
        assert!(message.contains("credentials.vend-attempted"));
        assert!(
            message.contains(expected_message),
            "{event_id} should reject malformed receipt principal: {message}"
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
async fn outbox_drain_rejects_credential_response_count_mismatch() {
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let principal = Principal {
        subject: "human:operator".to_string(),
        kind: PrincipalKind::Human,
    };
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-credential-response-count-mismatch".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "credentials.vend-attempted".to_string(),
            payload: json!({
                "audit-event-id": "audit-credential-response-count-mismatch",
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
                    "credential-count": 1,
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
        .expect_err("credential response count mismatch should fail before delivery");

    let message = err.to_string();
    assert!(message.contains("credentials.vend-attempted"));
    assert!(
        message.contains(
            "credential-vend credential-count does not match credential-response-evidence"
        )
    );
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains("evt-credential-response-count-mismatch"));
    assert!(store.delivered.lock().await.is_empty());
    assert!(graph.events.lock().await.is_empty());
    assert!(lineage.events.lock().await.is_empty());
}

#[tokio::test]
async fn outbox_drain_rejects_credential_response_profile_drift() {
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let principal = Principal {
        subject: "human:operator".to_string(),
        kind: PrincipalKind::Human,
    };
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-credential-response-profile-drift".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "credentials.vend-attempted".to_string(),
            payload: json!({
                "audit-event-id": "audit-credential-response-profile-drift",
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
                    "credential-count": 1,
                    "credential-response-evidence": [{
                        "prefix-hash": "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                        "issuer-config-hash": "sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
                        "storage-profile-id": "forged-profile",
                        "catalog-profile-id": "events-local",
                        "storage-provider": "file",
                        "credential-mode": "local-file-no-secret",
                        "authorization-principal": "human:operator",
                        "governed-read-required": "false",
                        "max-credential-ttl-seconds": null,
                        "issuer-config-entry-count": 0,
                        "receipt-principal": "human:operator"
                    }],
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
        .expect_err("credential response evidence must match selected storage profile");

    let message = err.to_string();
    assert!(message.contains("credentials.vend-attempted"));
    assert!(message.contains(
        "credential-vend credential-response storage-profile-id must match catalog evidence"
    ));
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains("evt-credential-response-profile-drift"));
    assert!(store.delivered.lock().await.is_empty());
    assert!(graph.events.lock().await.is_empty());
    assert!(lineage.events.lock().await.is_empty());
}

#[tokio::test]
async fn outbox_drain_rejects_credential_response_secret_ref_provider_drift() {
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let principal = Principal {
        subject: "human:operator".to_string(),
        kind: PrincipalKind::Human,
    };
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-credential-response-secret-provider-drift".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "credentials.vend-attempted".to_string(),
            payload: json!({
                "audit-event-id": "audit-credential-response-secret-provider-drift",
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
                    "credential-count": 1,
                    "credential-response-evidence": [{
                        "prefix-hash": "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                        "issuer-config-hash": "sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
                        "storage-profile-id": "events-prod",
                        "catalog-profile-id": "events-prod",
                        "storage-provider": "s3",
                        "credential-mode": "short-lived-secret-ref",
                        "authorization-principal": "human:operator",
                        "governed-read-required": "false",
                        "max-credential-ttl-seconds": null,
                        "secret-ref-provider": "vault",
                        "secret-ref-hash": "sha256:dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd",
                        "issuer-config-entry-count": 0,
                        "receipt-principal": "human:operator"
                    }],
                    "storage-profile-id": "events-prod",
                    "storage-profile": {
                        "profile-id": "events-prod",
                        "warehouse": "local",
                        "provider": "s3",
                        "issuance-mode": "short-lived-secret-ref",
                        "secret-ref-present": true,
                        "secret-ref-provider": "typesec",
                        "secret-ref-hash": "sha256:dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd",
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

    let err = drain_outbox_once(&state, 10).await.expect_err(
        "credential response secret-ref provider evidence must match selected storage profile",
    );

    let message = err.to_string();
    assert!(message.contains("credentials.vend-attempted"));
    assert!(message.contains(
        "credential-vend credential-response secret-ref-provider must match catalog evidence"
    ));
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains("evt-credential-response-secret-provider-drift"));
    assert!(store.delivered.lock().await.is_empty());
    assert!(graph.events.lock().await.is_empty());
    assert!(lineage.events.lock().await.is_empty());
}

#[tokio::test]
async fn outbox_drain_rejects_credential_response_secret_ref_hash_drift() {
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let principal = Principal {
        subject: "human:operator".to_string(),
        kind: PrincipalKind::Human,
    };
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-credential-response-secret-hash-drift".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "credentials.vend-attempted".to_string(),
            payload: json!({
                "audit-event-id": "audit-credential-response-secret-hash-drift",
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
                    "credential-count": 1,
                    "credential-response-evidence": [{
                        "prefix-hash": "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                        "issuer-config-hash": "sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
                        "storage-profile-id": "events-prod",
                        "catalog-profile-id": "events-prod",
                        "storage-provider": "s3",
                        "credential-mode": "short-lived-secret-ref",
                        "authorization-principal": "human:operator",
                        "governed-read-required": "false",
                        "max-credential-ttl-seconds": null,
                        "secret-ref-provider": "typesec",
                        "secret-ref-hash": "sha256:eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee",
                        "issuer-config-entry-count": 0,
                        "receipt-principal": "human:operator"
                    }],
                    "storage-profile-id": "events-prod",
                    "storage-profile": {
                        "profile-id": "events-prod",
                        "warehouse": "local",
                        "provider": "s3",
                        "issuance-mode": "short-lived-secret-ref",
                        "secret-ref-present": true,
                        "secret-ref-provider": "typesec",
                        "secret-ref-hash": "sha256:dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd",
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
        .expect_err("credential response secret-ref hash must match selected storage profile");

    let message = err.to_string();
    assert!(message.contains("credentials.vend-attempted"));
    assert!(message.contains(
        "credential-vend credential-response secret-ref-hash must match catalog evidence"
    ));
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains("evt-credential-response-secret-hash-drift"));
    assert!(store.delivered.lock().await.is_empty());
    assert!(graph.events.lock().await.is_empty());
    assert!(lineage.events.lock().await.is_empty());
}

#[tokio::test]
async fn outbox_drain_rejects_duplicate_credential_response_prefix_hashes() {
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let principal = Principal {
        subject: "human:operator".to_string(),
        kind: PrincipalKind::Human,
    };
    let duplicate_prefix_hash =
        "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-credential-response-duplicate-prefix".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "credentials.vend-attempted".to_string(),
            payload: json!({
                "audit-event-id": "audit-credential-response-duplicate-prefix",
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
                    "credential-count": 2,
                    "credential-response-evidence": [
                        {
                            "prefix-hash": duplicate_prefix_hash,
                            "issuer-config-hash": "sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
                            "storage-profile-id": "events-local",
                            "catalog-profile-id": "events-local",
                            "storage-provider": "file",
                            "credential-mode": "local-file-no-secret",
                            "authorization-principal": "human:operator",
                            "governed-read-required": "false",
                            "max-credential-ttl-seconds": null,
                            "issuer-config-entry-count": 1,
                            "receipt-principal": "human:operator"
                        },
                        {
                            "prefix-hash": duplicate_prefix_hash,
                            "issuer-config-hash": "sha256:dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd",
                            "storage-profile-id": "events-local",
                            "catalog-profile-id": "events-local",
                            "storage-provider": "file",
                            "credential-mode": "local-file-no-secret",
                            "authorization-principal": "human:operator",
                            "governed-read-required": "false",
                            "max-credential-ttl-seconds": null,
                            "issuer-config-entry-count": 1,
                            "receipt-principal": "human:operator"
                        }
                    ],
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
        .expect_err("duplicate credential response prefix hashes should fail before delivery");

    let message = err.to_string();
    assert!(message.contains("credentials.vend-attempted"));
    assert!(
        message.contains(
            "credential-vend credential-response-evidence must not contain duplicate prefix-hash values"
        ),
        "{message}"
    );
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains("evt-credential-response-duplicate-prefix"));
    assert!(store.delivered.lock().await.is_empty());
    assert!(graph.events.lock().await.is_empty());
    assert!(lineage.events.lock().await.is_empty());
}

#[tokio::test]
async fn outbox_drain_rejects_missing_credential_response_prefix_hash() {
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let principal = Principal {
        subject: "human:operator".to_string(),
        kind: PrincipalKind::Human,
    };
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-credential-response-missing-prefix".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "credentials.vend-attempted".to_string(),
            payload: json!({
                "audit-event-id": "audit-credential-response-missing-prefix",
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
                    "credential-count": 1,
                    "credential-response-evidence": [{
                        "issuer-config-hash": "sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
                        "storage-profile-id": "events-local",
                        "catalog-profile-id": "events-local",
                        "storage-provider": "file",
                        "credential-mode": "local-file-no-secret",
                        "authorization-principal": "human:operator",
                        "governed-read-required": "false",
                        "max-credential-ttl-seconds": null,
                        "issuer-config-entry-count": 0,
                        "receipt-principal": "human:operator"
                    }],
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
        .expect_err("credential response prefix hash should be required");

    let message = err.to_string();
    assert!(message.contains("credentials.vend-attempted"));
    assert!(message.contains("prefix-hash"));
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains("evt-credential-response-missing-prefix"));
    assert!(
        store.delivered.lock().await.is_empty(),
        "missing credential prefix hash must fail before acknowledgement"
    );
    assert!(
        graph.events.lock().await.is_empty(),
        "missing credential prefix hash must fail before graph projection"
    );
    assert!(
        lineage.events.lock().await.is_empty(),
        "missing credential prefix hash must fail before lineage projection"
    );
}

#[tokio::test]
async fn outbox_drain_rejects_extra_credential_response_fields() {
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let principal = Principal {
        subject: "human:operator".to_string(),
        kind: PrincipalKind::Human,
    };
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-credential-response-extra-field".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "credentials.vend-attempted".to_string(),
            payload: json!({
                "audit-event-id": "audit-credential-response-extra-field",
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
                    "credential-count": 1,
                    "credential-response-evidence": [{
                        "prefix-hash": "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                        "issuer-config-hash": "sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
                        "storage-profile-id": "events-local",
                        "catalog-profile-id": "events-local",
                        "storage-provider": "file",
                        "credential-mode": "local-file-no-secret",
                        "authorization-principal": "human:operator",
                        "governed-read-required": "false",
                        "max-credential-ttl-seconds": null,
                        "issuer-config-entry-count": 0,
                        "receipt-principal": "human:operator",
                        "unverified-credential-scope": "all-objects"
                    }],
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
        .expect_err("credential response entries should reject extra fields");

    let message = err.to_string();
    assert!(message.contains("credentials.vend-attempted"));
    assert!(message.contains(
        "credential-vend credential-response contains unexpected field unverified-credential-scope"
    ));
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains("evt-credential-response-extra-field"));
    assert!(
        store.delivered.lock().await.is_empty(),
        "extra credential response fields must fail before acknowledgement"
    );
    assert!(
        graph.events.lock().await.is_empty(),
        "extra credential response fields must fail before graph projection"
    );
    assert!(
        lineage.events.lock().await.is_empty(),
        "extra credential response fields must fail before lineage projection"
    );
}

#[tokio::test]
async fn outbox_drain_rejects_missing_credential_response_issuer_config_hash() {
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let principal = Principal {
        subject: "human:operator".to_string(),
        kind: PrincipalKind::Human,
    };
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-credential-response-missing-issuer-config".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "credentials.vend-attempted".to_string(),
            payload: json!({
                "audit-event-id": "audit-credential-response-missing-issuer-config",
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
                    "credential-count": 1,
                    "credential-response-evidence": [{
                        "prefix-hash": "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                        "storage-profile-id": "events-local",
                        "catalog-profile-id": "events-local",
                        "storage-provider": "file",
                        "credential-mode": "local-file-no-secret",
                        "authorization-principal": "human:operator",
                        "governed-read-required": "false",
                        "max-credential-ttl-seconds": null,
                        "issuer-config-entry-count": 0,
                        "receipt-principal": "human:operator"
                    }],
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
        .expect_err("credential response issuer config hash should be required");

    let message = err.to_string();
    assert!(message.contains("credentials.vend-attempted"));
    assert!(message.contains("issuer-config-hash"));
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains("evt-credential-response-missing-issuer-config"));
    assert!(
        store.delivered.lock().await.is_empty(),
        "missing issuer config hash must fail before acknowledgement"
    );
    assert!(
        graph.events.lock().await.is_empty(),
        "missing issuer config hash must fail before graph projection"
    );
    assert!(
        lineage.events.lock().await.is_empty(),
        "missing issuer config hash must fail before lineage projection"
    );
}

#[tokio::test]
async fn outbox_drain_rejects_missing_credential_response_principal_proof() {
    let cases = [
        (
            "authorization-principal",
            "evt-credential-response-missing-authorization-principal",
            "audit-credential-response-missing-authorization-principal",
        ),
        (
            "receipt-principal",
            "evt-credential-response-missing-receipt-principal",
            "audit-credential-response-missing-receipt-principal",
        ),
    ];

    for (missing_field, event_id, audit_event_id) in cases {
        let table = TableIdent::new(
            WarehouseName::new("local").unwrap(),
            "default".parse::<Namespace>().unwrap(),
            TableName::new("events").unwrap(),
        );
        let principal = Principal {
            subject: "human:operator".to_string(),
            kind: PrincipalKind::Human,
        };
        let mut response_entry = json!({
            "prefix-hash": "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            "issuer-config-hash": "sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
            "storage-profile-id": "events-local",
            "catalog-profile-id": "events-local",
            "storage-provider": "file",
            "credential-mode": "local-file-no-secret",
            "authorization-principal": "human:operator",
            "governed-read-required": "false",
            "max-credential-ttl-seconds": null,
            "issuer-config-entry-count": 0,
            "receipt-principal": "human:operator"
        });
        response_entry
            .as_object_mut()
            .unwrap()
            .remove(missing_field);
        let store = Arc::new(RecordingOutboxStore {
            events: Mutex::new(vec![OutboxEvent {
                event_id: event_id.to_string(),
                sink: "lakecat.lineage-and-graph".to_string(),
                event_type: "credentials.vend-attempted".to_string(),
                payload: json!({
                    "audit-event-id": audit_event_id,
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
                        "credential-count": 1,
                        "credential-response-evidence": [response_entry],
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
            .expect_err("credential response principal proof should be required");

        let message = err.to_string();
        assert!(message.contains("credentials.vend-attempted"));
        assert!(message.contains(&format!(
            "credential-vend credential-response {missing_field} must be a non-empty string"
        )));
        assert!(message.contains("event-id-hash=sha256:"));
        assert!(!message.contains(event_id));
        assert!(
            store.delivered.lock().await.is_empty(),
            "missing credential principal proof must fail before acknowledgement"
        );
        assert!(
            graph.events.lock().await.is_empty(),
            "missing credential principal proof must fail before graph projection"
        );
        assert!(
            lineage.events.lock().await.is_empty(),
            "missing credential principal proof must fail before lineage projection"
        );
    }
}

#[tokio::test]
async fn outbox_drain_rejects_missing_credential_response_storage_profile_proof() {
    let cases = [
        (
            "storage-profile-id",
            "evt-credential-response-missing-storage-profile-id",
            "audit-credential-response-missing-storage-profile-id",
        ),
        (
            "catalog-profile-id",
            "evt-credential-response-missing-catalog-profile-id",
            "audit-credential-response-missing-catalog-profile-id",
        ),
        (
            "storage-provider",
            "evt-credential-response-missing-storage-provider",
            "audit-credential-response-missing-storage-provider",
        ),
        (
            "credential-mode",
            "evt-credential-response-missing-credential-mode",
            "audit-credential-response-missing-credential-mode",
        ),
    ];

    for (missing_field, event_id, audit_event_id) in cases {
        let table = TableIdent::new(
            WarehouseName::new("local").unwrap(),
            "default".parse::<Namespace>().unwrap(),
            TableName::new("events").unwrap(),
        );
        let principal = Principal {
            subject: "human:operator".to_string(),
            kind: PrincipalKind::Human,
        };
        let mut response_entry = json!({
            "prefix-hash": "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            "issuer-config-hash": "sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
            "storage-profile-id": "events-local",
            "catalog-profile-id": "events-local",
            "storage-provider": "file",
            "credential-mode": "local-file-no-secret",
            "authorization-principal": "human:operator",
            "governed-read-required": "false",
            "max-credential-ttl-seconds": null,
            "issuer-config-entry-count": 0,
            "receipt-principal": "human:operator"
        });
        response_entry
            .as_object_mut()
            .unwrap()
            .remove(missing_field);
        let store = Arc::new(RecordingOutboxStore {
            events: Mutex::new(vec![OutboxEvent {
                event_id: event_id.to_string(),
                sink: "lakecat.lineage-and-graph".to_string(),
                event_type: "credentials.vend-attempted".to_string(),
                payload: json!({
                    "audit-event-id": audit_event_id,
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
                        "credential-count": 1,
                        "credential-response-evidence": [response_entry],
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
            .expect_err("credential response storage-profile proof should be required");

        let message = err.to_string();
        assert!(message.contains("credentials.vend-attempted"));
        assert!(message.contains(&format!(
            "credential-vend credential-response {missing_field} must be a non-empty string"
        )));
        assert!(message.contains("event-id-hash=sha256:"));
        assert!(!message.contains(event_id));
        assert!(
            store.delivered.lock().await.is_empty(),
            "missing credential storage-profile proof must fail before acknowledgement"
        );
        assert!(
            graph.events.lock().await.is_empty(),
            "missing credential storage-profile proof must fail before graph projection"
        );
        assert!(
            lineage.events.lock().await.is_empty(),
            "missing credential storage-profile proof must fail before lineage projection"
        );
    }
}

#[tokio::test]
async fn outbox_drain_rejects_malformed_credential_response_issuer_config_count() {
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let principal = Principal {
        subject: "human:operator".to_string(),
        kind: PrincipalKind::Human,
    };
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-credential-response-malformed-issuer-config-count".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "credentials.vend-attempted".to_string(),
            payload: json!({
                "audit-event-id": "audit-credential-response-malformed-issuer-config-count",
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
                    "credential-count": 1,
                    "credential-response-evidence": [{
                        "prefix-hash": "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                        "issuer-config-hash": "sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
                        "storage-profile-id": "events-local",
                        "catalog-profile-id": "events-local",
                        "storage-provider": "file",
                        "credential-mode": "local-file-no-secret",
                        "authorization-principal": "human:operator",
                        "governed-read-required": "false",
                        "max-credential-ttl-seconds": null,
                        "issuer-config-entry-count": "0",
                        "receipt-principal": "human:operator"
                    }],
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
        .expect_err("malformed issuer config count should fail before delivery");

    let message = err.to_string();
    assert!(message.contains("credentials.vend-attempted"));
    assert!(message.contains(
        "credential-vend credential-response issuer-config-entry-count must be unsigned"
    ));
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains("evt-credential-response-malformed-issuer-config-count"));
    assert!(store.delivered.lock().await.is_empty());
    assert!(graph.events.lock().await.is_empty());
    assert!(lineage.events.lock().await.is_empty());
}

#[tokio::test]
async fn outbox_drain_rejects_zero_credential_response_issuer_config_hash_drift() {
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let principal = Principal {
        subject: "human:operator".to_string(),
        kind: PrincipalKind::Human,
    };
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-credential-response-empty-issuer-config-hash-drift".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "credentials.vend-attempted".to_string(),
            payload: json!({
                "audit-event-id": "audit-credential-response-empty-issuer-config-hash-drift",
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
                    "credential-count": 1,
                    "credential-response-evidence": [{
                        "prefix-hash": "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                        "issuer-config-hash": "sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
                        "storage-profile-id": "events-local",
                        "catalog-profile-id": "events-local",
                        "storage-provider": "file",
                        "credential-mode": "local-file-no-secret",
                        "authorization-principal": "human:operator",
                        "governed-read-required": "false",
                        "max-credential-ttl-seconds": null,
                        "issuer-config-entry-count": 0,
                        "receipt-principal": "human:operator"
                    }],
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
        .expect_err("zero issuer config count must bind to the empty config hash");

    let message = err.to_string();
    assert!(message.contains("credentials.vend-attempted"));
    assert!(message.contains(
        "credential-vend credential-response issuer-config-hash must match catalog evidence"
    ));
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains("evt-credential-response-empty-issuer-config-hash-drift"));
    assert!(
        store.delivered.lock().await.is_empty(),
        "drifted empty issuer config hash must fail before acknowledgement"
    );
    assert!(
        graph.events.lock().await.is_empty(),
        "drifted empty issuer config hash must fail before graph projection"
    );
    assert!(
        lineage.events.lock().await.is_empty(),
        "drifted empty issuer config hash must fail before lineage projection"
    );
}

#[tokio::test]
async fn outbox_drain_rejects_credential_response_governed_read_required_drift() {
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let principal = Principal {
        subject: "agent:reader".to_string(),
        kind: PrincipalKind::Agent,
    };
    let read_restriction = json!({
        "allowed-columns": ["event_id"],
        "row-predicate": {
            "type": "eq",
            "term": "event_id",
            "value": "evt-1"
        },
        "purpose": "qglake-agent-demo",
        "max-credential-ttl-seconds": 300,
        "policy-hashes": [
            "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
        ]
    });
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-credential-response-governed-read-required-drift".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "credentials.vend-attempted".to_string(),
            payload: json!({
                "audit-event-id": "audit-credential-response-governed-read-required-drift",
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
                        "context": {
                            "read-restriction": read_restriction
                        }
                    },
                    "read-restriction": read_restriction,
                    "credential-count": 1,
                    "credential-response-evidence": [{
                        "prefix-hash": "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                        "issuer-config-hash": "sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
                        "storage-profile-id": "events-local",
                        "catalog-profile-id": "events-local",
                        "storage-provider": "file",
                        "credential-mode": "local-file-no-secret",
                        "authorization-principal": "agent:reader",
                        "governed-read-required": "false",
                        "max-credential-ttl-seconds": "300",
                        "issuer-config-entry-count": 0,
                        "receipt-principal": "agent:reader"
                    }],
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
        .expect_err("governed read credential response drift should fail before delivery");

    let message = err.to_string();
    assert!(message.contains("credentials.vend-attempted"));
    assert!(message.contains(
        "credential-vend credential-response governed-read-required must match catalog evidence"
    ));
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains("evt-credential-response-governed-read-required-drift"));
    assert!(store.delivered.lock().await.is_empty());
    assert!(graph.events.lock().await.is_empty());
    assert!(lineage.events.lock().await.is_empty());
}

#[tokio::test]
async fn outbox_drain_rejects_credential_response_ttl_drift() {
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let principal = Principal {
        subject: "agent:reader".to_string(),
        kind: PrincipalKind::Agent,
    };
    let read_restriction = json!({
        "allowed-columns": ["event_id"],
        "row-predicate": {
            "type": "eq",
            "term": "event_id",
            "value": "evt-1"
        },
        "purpose": "qglake-agent-demo",
        "max-credential-ttl-seconds": 300,
        "policy-hashes": [
            "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
        ]
    });
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-credential-response-ttl-drift".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "credentials.vend-attempted".to_string(),
            payload: json!({
                "audit-event-id": "audit-credential-response-ttl-drift",
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
                        "context": {
                            "read-restriction": read_restriction
                        }
                    },
                    "read-restriction": read_restriction,
                    "credential-count": 1,
                    "credential-response-evidence": [{
                        "prefix-hash": "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                        "issuer-config-hash": "sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
                        "storage-profile-id": "events-local",
                        "catalog-profile-id": "events-local",
                        "storage-provider": "file",
                        "credential-mode": "local-file-no-secret",
                        "authorization-principal": "agent:reader",
                        "governed-read-required": "true",
                        "max-credential-ttl-seconds": "600",
                        "issuer-config-entry-count": 0,
                        "receipt-principal": "agent:reader"
                    }],
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
        .expect_err("credential response TTL drift should fail before delivery");

    let message = err.to_string();
    assert!(message.contains("credentials.vend-attempted"));
    assert!(message.contains(
        "credential-vend credential-response max-credential-ttl-seconds must match catalog evidence"
    ));
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains("evt-credential-response-ttl-drift"));
    assert!(store.delivered.lock().await.is_empty());
    assert!(graph.events.lock().await.is_empty());
    assert!(lineage.events.lock().await.is_empty());
}

#[tokio::test]
async fn outbox_drain_rejects_credential_storage_profile_id_drift_without_credentials() {
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let principal = Principal {
        subject: "agent:reader".to_string(),
        kind: PrincipalKind::Agent,
    };
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-credential-storage-profile-id-drift".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "credentials.vend-attempted".to_string(),
            payload: json!({
                "audit-event-id": "audit-credential-storage-profile-id-drift",
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
                    "credential-count": 0,
                    "credential-response-evidence": [],
                    "storage-profile-id": "forged-profile",
                    "secret-ref-present": false,
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
        .expect_err("credential storage profile id must match nested profile evidence");

    let message = err.to_string();
    assert!(message.contains("credentials.vend-attempted"));
    assert!(
        message.contains("credential-vend evidence storage-profile-id must match catalog evidence")
    );
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains("evt-credential-storage-profile-id-drift"));
    assert!(store.delivered.lock().await.is_empty());
    assert!(graph.events.lock().await.is_empty());
    assert!(lineage.events.lock().await.is_empty());
}

#[tokio::test]
async fn outbox_drain_rejects_credential_storage_profile_invalid_profile_ids() {
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let principal = Principal {
        subject: "agent:reader".to_string(),
        kind: PrincipalKind::Agent,
    };
    let invalid_profile_id = "events-local?token=secret";
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-credential-storage-profile-invalid-profile-id".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "credentials.vend-attempted".to_string(),
            payload: json!({
                "audit-event-id": "audit-credential-storage-profile-invalid-profile-id",
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
                    "credential-count": 0,
                    "credential-response-evidence": [],
                    "storage-profile-id": invalid_profile_id,
                    "secret-ref-present": false,
                    "storage-profile": {
                        "profile-id": invalid_profile_id,
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
        .expect_err("credential storage-profile invalid profile-id should fail before delivery");

    let message = err.to_string();
    assert!(message.contains("credentials.vend-attempted"));
    assert!(message.contains("profile-id contains unsupported characters"));
    assert!(message.contains("storage-profile-id-hash=sha256:"));
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains(invalid_profile_id));
    assert!(!message.contains("token=secret"));
    assert!(store.delivered.lock().await.is_empty());
    assert!(graph.events.lock().await.is_empty());
    assert!(lineage.events.lock().await.is_empty());
}

#[tokio::test]
async fn outbox_drain_rejects_extra_credential_storage_profile_fields() {
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let principal = Principal {
        subject: "agent:reader".to_string(),
        kind: PrincipalKind::Agent,
    };
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-credential-storage-profile-extra-field".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "credentials.vend-attempted".to_string(),
            payload: json!({
                "audit-event-id": "audit-credential-storage-profile-extra-field",
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
                    "credential-count": 0,
                    "credential-response-evidence": [],
                    "storage-profile-id": "events-local",
                    "secret-ref-present": false,
                    "storage-profile": {
                        "profile-id": "events-local",
                        "warehouse": "local",
                        "provider": "file",
                        "issuance-mode": "local-file-no-secret",
                        "secret-ref-present": false,
                        "location-prefix-hash": "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                        "unverified-storage-claim": true,
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
        .expect_err("extra credential storage-profile fields should fail before delivery");

    let message = err.to_string();
    assert!(message.contains("credentials.vend-attempted"));
    assert!(message.contains(
        "credential-vend storage-profile contains unexpected field unverified-storage-claim"
    ));
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains("evt-credential-storage-profile-extra-field"));
    assert!(store.delivered.lock().await.is_empty());
    assert!(graph.events.lock().await.is_empty());
    assert!(lineage.events.lock().await.is_empty());
}

#[tokio::test]
async fn outbox_drain_rejects_reserved_credential_storage_profile_public_config() {
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let principal = Principal {
        subject: "agent:reader".to_string(),
        kind: PrincipalKind::Agent,
    };
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-credential-storage-profile-reserved-public-config".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "credentials.vend-attempted".to_string(),
            payload: json!({
                "audit-event-id": "audit-credential-storage-profile-reserved-public-config",
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
                    "credential-count": 0,
                    "credential-response-evidence": [],
                    "storage-profile-id": "events-local",
                    "secret-ref-present": false,
                    "storage-profile": {
                        "profile-id": "events-local",
                        "warehouse": "local",
                        "provider": "file",
                        "issuance-mode": "local-file-no-secret",
                        "secret-ref-present": false,
                        "location-prefix-hash": "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                        "public-config": {
                            "lakecat.storage-profile-id": "shadow-profile"
                        }
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
        .expect_err("reserved credential public-config evidence should fail before delivery");

    let message = err.to_string();
    assert!(message.contains("credentials.vend-attempted"));
    assert!(message.contains(
        "credential-vend storage-profile public-config key is reserved for LakeCat credential evidence"
    ));
    assert!(message.contains("public-config-key-hash=sha256:"));
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains("lakecat.storage-profile-id"));
    assert!(!message.contains("shadow-profile"));
    assert!(!message.contains("evt-credential-storage-profile-reserved-public-config"));
    assert!(store.delivered.lock().await.is_empty());
    assert!(graph.events.lock().await.is_empty());
    assert!(lineage.events.lock().await.is_empty());
}

#[tokio::test]
async fn outbox_drain_rejects_credential_storage_profile_warehouse_drift() {
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let principal = Principal {
        subject: "agent:reader".to_string(),
        kind: PrincipalKind::Agent,
    };
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-credential-storage-profile-warehouse-drift".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "credentials.vend-attempted".to_string(),
            payload: json!({
                "audit-event-id": "audit-credential-storage-profile-warehouse-drift",
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
                    "credential-count": 0,
                    "credential-response-evidence": [],
                    "storage-profile-id": "events-local",
                    "secret-ref-present": false,
                    "storage-profile": {
                        "profile-id": "events-local",
                        "warehouse": "forged",
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
        .expect_err("credential storage-profile warehouse must match table evidence");

    let message = err.to_string();
    assert!(message.contains("credentials.vend-attempted"));
    assert!(
        message.contains("credential-vend storage-profile warehouse must match catalog evidence")
    );
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains("evt-credential-storage-profile-warehouse-drift"));
    assert!(store.delivered.lock().await.is_empty());
    assert!(graph.events.lock().await.is_empty());
    assert!(lineage.events.lock().await.is_empty());
}

#[tokio::test]
async fn outbox_drain_rejects_missing_or_malformed_credential_secret_ref_presence() {
    for (case, mutate, expected) in [
        (
            "missing-secret-ref-present",
            "remove-secret-ref-present",
            "credential-vend evidence must contain secret-ref-present",
        ),
        (
            "malformed-secret-ref-present",
            "malformed-secret-ref-present",
            "credential-vend evidence secret-ref-present must be a boolean",
        ),
    ] {
        let table = TableIdent::new(
            WarehouseName::new("local").unwrap(),
            "default".parse::<Namespace>().unwrap(),
            TableName::new("events").unwrap(),
        );
        let principal = Principal {
            subject: "agent:reader".to_string(),
            kind: PrincipalKind::Agent,
        };
        let mut payload = json!({
            "authorization-receipt": {
                "principal": principal,
                "action": "credentials-vend",
                "allowed": true,
                "engine": "test",
                "policy_hash": null,
                "checked_at": chrono::Utc::now(),
            },
            "credential-count": 0,
            "credential-response-evidence": [],
            "storage-profile-id": "events-local",
            "secret-ref-present": false,
            "storage-profile": {
                "profile-id": "events-local",
                "warehouse": "local",
                "provider": "file",
                "issuance-mode": "local-file-no-secret",
                "secret-ref-present": false,
                "location-prefix-hash": "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
            },
        });
        match mutate {
            "remove-secret-ref-present" => {
                payload
                    .as_object_mut()
                    .unwrap()
                    .remove("secret-ref-present");
            }
            "malformed-secret-ref-present" => {
                payload["secret-ref-present"] = json!("false");
            }
            _ => unreachable!("unknown credential secret-ref-present mutation"),
        }
        let event_id = format!("evt-credential-{case}");
        let store = Arc::new(RecordingOutboxStore {
            events: Mutex::new(vec![OutboxEvent {
                event_id: event_id.clone(),
                sink: "lakecat.lineage-and-graph".to_string(),
                event_type: "credentials.vend-attempted".to_string(),
                payload: json!({
                    "audit-event-id": format!("audit-credential-{case}"),
                    "event-type": "credentials.vend-attempted",
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
            .expect_err("credential secret-ref-present proof should fail before delivery");

        let message = err.to_string();
        assert!(message.contains("credentials.vend-attempted"));
        assert!(message.contains(expected), "{case}: {message}");
        assert!(message.contains("event-id-hash=sha256:"));
        assert!(!message.contains(&event_id));
        assert!(store.delivered.lock().await.is_empty());
        assert!(graph.events.lock().await.is_empty());
        assert!(lineage.events.lock().await.is_empty());
    }
}

#[tokio::test]
async fn outbox_drain_rejects_credential_secret_ref_presence_drift() {
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let principal = Principal {
        subject: "agent:reader".to_string(),
        kind: PrincipalKind::Agent,
    };
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-credential-secret-ref-presence-drift".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "credentials.vend-attempted".to_string(),
            payload: json!({
                "audit-event-id": "audit-credential-secret-ref-presence-drift",
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
                    "credential-count": 0,
                    "credential-response-evidence": [],
                    "storage-profile-id": "events-local",
                    "secret-ref-present": true,
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
        .expect_err("credential secret-ref presence must match nested profile evidence");

    let message = err.to_string();
    assert!(message.contains("credentials.vend-attempted"));
    assert!(
        message.contains("credential-vend evidence secret-ref-present must match catalog evidence")
    );
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains("evt-credential-secret-ref-presence-drift"));
    assert!(store.delivered.lock().await.is_empty());
    assert!(graph.events.lock().await.is_empty());
    assert!(lineage.events.lock().await.is_empty());
}

#[tokio::test]
async fn outbox_drain_rejects_credential_unexpected_secret_ref_evidence() {
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let principal = Principal {
        subject: "agent:reader".to_string(),
        kind: PrincipalKind::Agent,
    };
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-credential-unexpected-secret-ref-evidence".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "credentials.vend-attempted".to_string(),
            payload: json!({
                "audit-event-id": "audit-credential-unexpected-secret-ref-evidence",
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
                    "credential-count": 0,
                    "credential-response-evidence": [],
                    "storage-profile-id": "events-local",
                    "secret-ref-present": false,
                    "storage-profile": {
                        "profile-id": "events-local",
                        "warehouse": "local",
                        "provider": "file",
                        "issuance-mode": "local-file-no-secret",
                        "secret-ref-present": false,
                        "secret-ref-hash": {
                            "unexpected": "sha256:dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd"
                        },
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
        .expect_err("unexpected credential secret-ref evidence should fail before delivery");

    let message = err.to_string();
    assert!(message.contains("credentials.vend-attempted"));
    assert!(message.contains(
        "credential-vend storage-profile cannot carry secret-ref evidence when secret-ref-present is false"
    ));
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains("evt-credential-unexpected-secret-ref-evidence"));
    assert!(store.delivered.lock().await.is_empty());
    assert!(graph.events.lock().await.is_empty());
    assert!(lineage.events.lock().await.is_empty());
}

#[tokio::test]
async fn outbox_drain_rejects_credential_unexpected_secret_ref_provider_object() {
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let principal = Principal {
        subject: "agent:reader".to_string(),
        kind: PrincipalKind::Agent,
    };
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-credential-unexpected-secret-ref-provider".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "credentials.vend-attempted".to_string(),
            payload: json!({
                "audit-event-id": "audit-credential-unexpected-secret-ref-provider",
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
                    "credential-count": 0,
                    "credential-response-evidence": [],
                    "storage-profile-id": "events-local",
                    "secret-ref-present": false,
                    "storage-profile": {
                        "profile-id": "events-local",
                        "warehouse": "local",
                        "provider": "file",
                        "issuance-mode": "local-file-no-secret",
                        "secret-ref-present": false,
                        "secret-ref-provider": {
                            "unexpected": "typesec"
                        },
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
        .expect_err("unexpected credential secret-ref provider should fail before delivery");

    let message = err.to_string();
    assert!(message.contains("credentials.vend-attempted"));
    assert!(message.contains(
        "credential-vend storage-profile cannot carry secret-ref evidence when secret-ref-present is false"
    ));
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains("evt-credential-unexpected-secret-ref-provider"));
    assert!(store.delivered.lock().await.is_empty());
    assert!(graph.events.lock().await.is_empty());
    assert!(lineage.events.lock().await.is_empty());
}

#[tokio::test]
async fn outbox_drain_rejects_credential_storage_profile_local_no_secret_remote_provider() {
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let principal = Principal {
        subject: "agent:reader".to_string(),
        kind: PrincipalKind::Agent,
    };
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-credential-local-no-secret-remote-provider".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "credentials.vend-attempted".to_string(),
            payload: json!({
                "audit-event-id": "audit-credential-local-no-secret-remote-provider",
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
                    "credential-count": 0,
                    "credential-response-evidence": [],
                    "storage-profile-id": "events-prod",
                    "storage-profile": {
                        "profile-id": "events-prod",
                        "warehouse": "local",
                        "provider": "s3",
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

    let err = drain_outbox_once(&state, 10).await.expect_err(
        "credential storage-profile local-file-no-secret mode with remote provider should fail",
    );

    let message = err.to_string();
    assert!(message.contains("credentials.vend-attempted"));
    assert!(message.contains(
        "credential-vend storage-profile local-file-no-secret issuance mode requires file provider"
    ));
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains("evt-credential-local-no-secret-remote-provider"));
    assert!(store.delivered.lock().await.is_empty());
    assert!(graph.events.lock().await.is_empty());
    assert!(lineage.events.lock().await.is_empty());
}

#[tokio::test]
async fn outbox_drain_rejects_credential_storage_profile_short_lived_file_provider() {
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let principal = Principal {
        subject: "agent:reader".to_string(),
        kind: PrincipalKind::Agent,
    };
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-credential-short-lived-file-provider".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "credentials.vend-attempted".to_string(),
            payload: json!({
                "audit-event-id": "audit-credential-short-lived-file-provider",
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
                    "credential-count": 0,
                    "credential-response-evidence": [],
                    "storage-profile-id": "events-local",
                    "storage-profile": {
                        "profile-id": "events-local",
                        "warehouse": "local",
                        "provider": "file",
                        "issuance-mode": "short-lived-secret-ref",
                        "secret-ref-present": true,
                        "secret-ref-provider": "typesec",
                        "secret-ref-hash": "sha256:dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd",
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

    let err = drain_outbox_once(&state, 10).await.expect_err(
        "credential storage-profile short-lived-secret-ref mode with file provider should fail",
    );

    let message = err.to_string();
    assert!(message.contains("credentials.vend-attempted"));
    assert!(message.contains(
        "credential-vend storage-profile short-lived-secret-ref issuance mode requires cloud object provider"
    ));
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains("evt-credential-short-lived-file-provider"));
    assert!(store.delivered.lock().await.is_empty());
    assert!(graph.events.lock().await.is_empty());
    assert!(lineage.events.lock().await.is_empty());
}

#[tokio::test]
async fn outbox_drain_rejects_credential_restriction_missing_from_receipt_context() {
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let principal = Principal {
        subject: "agent:reader".to_string(),
        kind: PrincipalKind::Agent,
    };
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-credential-restriction-missing-receipt-context".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "credentials.vend-attempted".to_string(),
            payload: json!({
                "audit-event-id": "audit-credential-restriction-missing-receipt-context",
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
                    "read-restriction": {
                        "allowed-columns": ["event_id"],
                        "row-predicate": {
                            "type": "eq",
                            "term": "event_id",
                            "value": "evt-1"
                        },
                        "max-credential-ttl-seconds": 300,
                        "policy-hashes": [
                            "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
                        ]
                    },
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
        .expect_err("credential restriction must be copied into the receipt context");

    let message = err.to_string();
    assert!(message.contains("credentials.vend-attempted"));
    assert!(message.contains(
        "credential-vend read-restriction must be captured in authorization receipt context"
    ));
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains("evt-credential-restriction-missing-receipt-context"));
    assert!(store.delivered.lock().await.is_empty());
    assert!(graph.events.lock().await.is_empty());
    assert!(lineage.events.lock().await.is_empty());
}

#[tokio::test]
async fn outbox_drain_rejects_credential_restriction_malformed_purpose_and_ttl() {
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let principal = Principal {
        subject: "agent:reader".to_string(),
        kind: PrincipalKind::Agent,
    };
    let base_restriction = || {
        json!({
            "allowed-columns": ["event_id"],
            "row-predicate": {
                "type": "eq",
                "term": "event_id",
                "value": "evt-1"
            },
            "purpose": "qglake-agent-demo",
            "max-credential-ttl-seconds": 300,
            "policy-hashes": [
                "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
            ]
        })
    };
    let mut missing_purpose = base_restriction();
    missing_purpose.as_object_mut().unwrap().remove("purpose");
    let mut blank_purpose = base_restriction();
    blank_purpose["purpose"] = json!(" ");
    let mut missing_ttl = base_restriction();
    missing_ttl
        .as_object_mut()
        .unwrap()
        .remove("max-credential-ttl-seconds");
    let mut zero_ttl = base_restriction();
    zero_ttl["max-credential-ttl-seconds"] = json!(0);
    let mut string_ttl = base_restriction();
    string_ttl["max-credential-ttl-seconds"] = json!("300");

    for (event_id, read_restriction, expected_message) in [
        (
            "evt-credential-restriction-missing-purpose",
            missing_purpose,
            "credential-vend read-restriction purpose must not be blank",
        ),
        (
            "evt-credential-restriction-blank-purpose",
            blank_purpose,
            "credential-vend read-restriction purpose must not be blank",
        ),
        (
            "evt-credential-restriction-missing-ttl",
            missing_ttl,
            "credential-vend read-restriction max-credential-ttl-seconds must be a positive integer",
        ),
        (
            "evt-credential-restriction-zero-ttl",
            zero_ttl,
            "credential-vend read-restriction max-credential-ttl-seconds must be positive",
        ),
        (
            "evt-credential-restriction-string-ttl",
            string_ttl,
            "credential-vend read-restriction max-credential-ttl-seconds must be a positive integer",
        ),
    ] {
        let store = Arc::new(RecordingOutboxStore {
            events: Mutex::new(vec![OutboxEvent {
                event_id: event_id.to_string(),
                sink: "lakecat.lineage-and-graph".to_string(),
                event_type: "credentials.vend-attempted".to_string(),
                payload: json!({
                    "audit-event-id": format!("audit-{event_id}"),
                    "event-type": "credentials.vend-attempted",
                    "table": table.clone(),
                    "payload": {
                        "authorization-receipt": {
                            "principal": principal.clone(),
                            "action": "credentials-vend",
                            "allowed": true,
                            "engine": "test",
                            "policy_hash": null,
                            "checked_at": chrono::Utc::now(),
                            "context": {
                                "read-restriction": read_restriction.clone()
                            }
                        },
                        "read-restriction": read_restriction,
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
                        "secret-ref-present": false
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
            .expect_err("malformed credential read restriction should fail before delivery");

        let message = err.to_string();
        assert!(message.contains("credentials.vend-attempted"));
        assert!(message.contains(expected_message), "{message}");
        assert!(message.contains("event-id-hash=sha256:"));
        assert!(!message.contains(event_id));
        assert!(
            store.delivered.lock().await.is_empty(),
            "malformed credential read restriction must fail before acknowledgement"
        );
        assert!(
            graph.events.lock().await.is_empty(),
            "malformed credential read restriction must fail before graph projection"
        );
        assert!(
            lineage.events.lock().await.is_empty(),
            "malformed credential read restriction must fail before lineage projection"
        );
    }
}

#[tokio::test]
async fn outbox_drain_rejects_raw_credential_exception_receipt_drift() {
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let principal = Principal {
        subject: "human:operator".to_string(),
        kind: PrincipalKind::Human,
    };
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-raw-credential-exception-drift".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "credentials.vend-attempted".to_string(),
            payload: json!({
                "audit-event-id": "audit-raw-credential-exception-drift",
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
                        "context": {
                            "lakecat:raw-credential-exception": {
                                "requested": true,
                                "allowed": true,
                                "reason": "trusted human principal may use audited raw credential vending"
                            }
                        }
                    },
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
                    "lakecat:raw-credential-exception": {
                        "requested": true,
                        "allowed": false,
                        "reason": "fine-grained read restriction requires Sail-planned reads"
                    }
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
        .expect_err("raw credential exception must match receipt context");

    let message = err.to_string();
    assert!(message.contains("credentials.vend-attempted"));
    assert!(message.contains(
        "credential-vend raw-credential exception must match authorization receipt context"
    ));
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains("evt-raw-credential-exception-drift"));
    assert!(store.delivered.lock().await.is_empty());
    assert!(graph.events.lock().await.is_empty());
    assert!(lineage.events.lock().await.is_empty());
}

#[tokio::test]
async fn outbox_drain_rejects_extra_raw_credential_exception_fields() {
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let principal = Principal {
        subject: "agent:reader".to_string(),
        kind: PrincipalKind::Agent,
    };
    let top_level_exception = json!({
        "requested": true,
        "allowed": false,
        "reason": "fine-grained read restriction requires Sail-planned reads",
        "unverified-raw-credential-claim": "credential-safe"
    });
    let receipt_exception = json!({
        "requested": true,
        "allowed": false,
        "reason": "fine-grained read restriction requires Sail-planned reads",
        "unverified-receipt-raw-credential-claim": "credential-safe"
    });
    let clean_exception = json!({
        "requested": true,
        "allowed": false,
        "reason": "fine-grained read restriction requires Sail-planned reads"
    });

    for (event_id, top_exception, receipt_exception, expected_message) in [
        (
            "evt-extra-raw-credential-exception",
            top_level_exception.clone(),
            top_level_exception,
            "credential-vend raw-credential exception contains unexpected field unverified-raw-credential-claim",
        ),
        (
            "evt-extra-receipt-raw-credential-exception",
            clean_exception,
            receipt_exception,
            "credential-vend authorization receipt raw-credential exception contains unexpected field unverified-receipt-raw-credential-claim",
        ),
    ] {
        let store = Arc::new(RecordingOutboxStore {
            events: Mutex::new(vec![OutboxEvent {
                event_id: event_id.to_string(),
                sink: "lakecat.lineage-and-graph".to_string(),
                event_type: "credentials.vend-attempted".to_string(),
                payload: json!({
                    "audit-event-id": format!("audit-{event_id}"),
                    "event-type": "credentials.vend-attempted",
                    "table": table.clone(),
                    "payload": {
                        "authorization-receipt": {
                            "principal": principal.clone(),
                            "action": "credentials-vend",
                            "allowed": true,
                            "engine": "test",
                            "policy_hash": null,
                            "checked_at": chrono::Utc::now(),
                            "context": {
                                "lakecat:raw-credential-exception": receipt_exception
                            }
                        },
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
                        "lakecat:credential-block-reason": "fine-grained read restriction requires Sail-planned reads",
                        "lakecat:raw-credential-exception": top_exception
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
            .expect_err("raw credential exception evidence should reject extra fields");

        let message = err.to_string();
        assert!(message.contains("credentials.vend-attempted"));
        assert!(message.contains(expected_message), "{message}");
        assert!(message.contains("event-id-hash=sha256:"));
        assert!(!message.contains(event_id));
        assert!(
            store.delivered.lock().await.is_empty(),
            "extra raw credential exception replay must fail before acknowledgement"
        );
        assert!(
            graph.events.lock().await.is_empty(),
            "extra raw credential exception replay must fail before graph projection"
        );
        assert!(
            lineage.events.lock().await.is_empty(),
            "extra raw credential exception replay must fail before lineage projection"
        );
    }
}

#[tokio::test]
async fn outbox_drain_rejects_malformed_raw_credential_exception_allowed() {
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let principal = Principal {
        subject: "agent:reader".to_string(),
        kind: PrincipalKind::Agent,
    };
    let raw_exception = json!({
        "requested": true,
        "allowed": "false",
        "reason": "fine-grained read restriction requires Sail-planned reads"
    });
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-malformed-raw-credential-exception-allowed".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "credentials.vend-attempted".to_string(),
            payload: json!({
                "audit-event-id": "audit-malformed-raw-credential-exception-allowed",
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
                        "context": {
                            "lakecat:raw-credential-exception": raw_exception
                        }
                    },
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
                    "lakecat:credential-block-reason": "fine-grained read restriction requires Sail-planned reads",
                    "lakecat:raw-credential-exception": raw_exception
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
        .expect_err("raw credential exception allowed must be a boolean");

    let message = err.to_string();
    assert!(message.contains("credentials.vend-attempted"));
    assert!(message.contains("credential-vend raw-credential exception allowed must be boolean"));
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains("evt-malformed-raw-credential-exception-allowed"));
    assert!(store.delivered.lock().await.is_empty());
    assert!(graph.events.lock().await.is_empty());
    assert!(lineage.events.lock().await.is_empty());
}

#[tokio::test]
async fn outbox_drain_rejects_blocked_raw_credential_exception_missing_reason() {
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let principal = Principal {
        subject: "agent:reader".to_string(),
        kind: PrincipalKind::Agent,
    };
    let raw_exception = json!({
        "requested": true,
        "allowed": false
    });
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-blocked-raw-credential-exception-missing-reason".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "credentials.vend-attempted".to_string(),
            payload: json!({
                "audit-event-id": "audit-blocked-raw-credential-exception-missing-reason",
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
                        "context": {
                            "lakecat:raw-credential-exception": raw_exception
                        }
                    },
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
                    "lakecat:credential-block-reason": "fine-grained read restriction requires Sail-planned reads",
                    "lakecat:raw-credential-exception": raw_exception
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
        .expect_err("blocked raw credential exception must carry reason proof");

    let message = err.to_string();
    assert!(message.contains("credentials.vend-attempted"));
    assert!(
        message.contains(
            "credential-vend blocked raw-credential exception must carry non-empty reason"
        )
    );
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains("evt-blocked-raw-credential-exception-missing-reason"));
    assert!(store.delivered.lock().await.is_empty());
    assert!(graph.events.lock().await.is_empty());
    assert!(lineage.events.lock().await.is_empty());
}

#[tokio::test]
async fn outbox_drain_rejects_blocked_credential_evidence_with_credentials() {
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let principal = Principal {
        subject: "agent:reader".to_string(),
        kind: PrincipalKind::Agent,
    };
    let raw_exception = json!({
        "requested": true,
        "allowed": false,
        "reason": "fine-grained read restriction requires Sail-planned reads"
    });
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-blocked-credential-evidence-with-credentials".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "credentials.vend-attempted".to_string(),
            payload: json!({
                "audit-event-id": "audit-blocked-credential-evidence-with-credentials",
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
                        "context": {
                            "lakecat:raw-credential-exception": raw_exception
                        }
                    },
                    "credential-count": 1,
                    "credential-response-evidence": [{}],
                    "storage-profile-id": "events-local",
                    "storage-profile": {
                        "profile-id": "events-local",
                        "warehouse": "local",
                        "provider": "file",
                        "issuance-mode": "local-file-no-secret",
                        "secret-ref-present": false,
                        "location-prefix-hash": "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                    },
                    "lakecat:credential-block-reason": "fine-grained read restriction requires Sail-planned reads",
                    "lakecat:raw-credential-exception": raw_exception
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
        .expect_err("blocked credential replay must not carry credential entries");

    let message = err.to_string();
    assert!(message.contains("credentials.vend-attempted"));
    assert!(
        message.contains("credential-vend blocked credential evidence must not carry credentials")
    );
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains("evt-blocked-credential-evidence-with-credentials"));
    assert!(store.delivered.lock().await.is_empty());
    assert!(graph.events.lock().await.is_empty());
    assert!(lineage.events.lock().await.is_empty());
}

#[tokio::test]
async fn outbox_drain_rejects_missing_credential_block_reason() {
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let principal = Principal {
        subject: "agent:reader".to_string(),
        kind: PrincipalKind::Agent,
    };
    let raw_exception = json!({
        "requested": true,
        "allowed": false,
        "reason": "fine-grained read restriction requires Sail-planned reads"
    });
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-missing-credential-block-reason".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "credentials.vend-attempted".to_string(),
            payload: json!({
                "audit-event-id": "audit-missing-credential-block-reason",
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
                        "context": {
                            "lakecat:raw-credential-exception": raw_exception
                        }
                    },
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
                    "lakecat:raw-credential-exception": raw_exception
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
        .expect_err("blocked credential replay must carry block reason");

    let message = err.to_string();
    assert!(message.contains("credentials.vend-attempted"));
    assert!(
        message.contains("credential-vend blocked credential evidence must contain block reason")
    );
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains("evt-missing-credential-block-reason"));
    assert!(store.delivered.lock().await.is_empty());
    assert!(graph.events.lock().await.is_empty());
    assert!(lineage.events.lock().await.is_empty());
}

#[tokio::test]
async fn outbox_drain_rejects_blank_credential_block_reason() {
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let principal = Principal {
        subject: "agent:reader".to_string(),
        kind: PrincipalKind::Agent,
    };
    let raw_exception = json!({
        "requested": true,
        "allowed": false,
        "reason": "fine-grained read restriction requires Sail-planned reads"
    });
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-blank-credential-block-reason".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "credentials.vend-attempted".to_string(),
            payload: json!({
                "audit-event-id": "audit-blank-credential-block-reason",
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
                        "context": {
                            "lakecat:raw-credential-exception": raw_exception
                        }
                    },
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
                    "lakecat:credential-block-reason": " ",
                    "lakecat:raw-credential-exception": raw_exception
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
        .expect_err("blocked credential replay must reject blank block reason");

    let message = err.to_string();
    assert!(message.contains("credentials.vend-attempted"));
    assert!(
        message.contains("credential-vend blocked credential evidence must contain block reason")
    );
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains("evt-blank-credential-block-reason"));
    assert!(store.delivered.lock().await.is_empty());
    assert!(graph.events.lock().await.is_empty());
    assert!(lineage.events.lock().await.is_empty());
}

#[tokio::test]
async fn outbox_drain_rejects_credential_block_reason_drift() {
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let principal = Principal {
        subject: "agent:reader".to_string(),
        kind: PrincipalKind::Agent,
    };
    let raw_exception = json!({
        "requested": true,
        "allowed": false,
        "reason": "fine-grained read restriction requires Sail-planned reads"
    });
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-credential-block-reason-drift".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "credentials.vend-attempted".to_string(),
            payload: json!({
                "audit-event-id": "audit-credential-block-reason-drift",
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
                        "context": {
                            "lakecat:raw-credential-exception": raw_exception
                        }
                    },
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
                    "lakecat:credential-block-reason": "trusted human principal may use audited raw credential vending",
                    "lakecat:raw-credential-exception": raw_exception
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
        .expect_err("blocked credential replay reason must match receipt context");

    let message = err.to_string();
    assert!(message.contains("credentials.vend-attempted"));
    assert!(
        message.contains("credential-vend block reason must match raw-credential exception reason")
    );
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains("evt-credential-block-reason-drift"));
    assert!(store.delivered.lock().await.is_empty());
    assert!(graph.events.lock().await.is_empty());
    assert!(lineage.events.lock().await.is_empty());
}

#[tokio::test]
async fn outbox_drain_rejects_credential_block_reason_when_raw_credentials_allowed() {
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let principal = Principal {
        subject: "human:operator".to_string(),
        kind: PrincipalKind::Human,
    };
    let raw_exception = json!({
        "requested": true,
        "allowed": true,
        "reason": "trusted human principal may use audited raw credential vending"
    });
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-credential-block-reason-raw-allowed".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "credentials.vend-attempted".to_string(),
            payload: json!({
                "audit-event-id": "audit-credential-block-reason-raw-allowed",
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
                        "context": {
                            "lakecat:raw-credential-exception": raw_exception
                        }
                    },
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
                    "lakecat:credential-block-reason": "fine-grained read restriction requires Sail-planned reads",
                    "lakecat:raw-credential-exception": raw_exception
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
        .expect_err("allowed raw credential replay must not carry block reason");

    let message = err.to_string();
    assert!(message.contains("credentials.vend-attempted"));
    assert!(
        message.contains(
            "credential-vend block reason must be absent when raw credentials are allowed"
        )
    );
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains("evt-credential-block-reason-raw-allowed"));
    assert!(store.delivered.lock().await.is_empty());
    assert!(graph.events.lock().await.is_empty());
    assert!(lineage.events.lock().await.is_empty());
}

#[tokio::test]
async fn outbox_drain_rejects_raw_storage_profile_secret_ref_evidence() {
    let principal = Principal::new("agent:operator", PrincipalKind::Agent).unwrap();
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-storage-profile-secret-ref".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "storage-profile.upserted".to_string(),
            payload: json!({
                "audit-event-id": "audit-storage-profile-secret-ref",
                "event-type": "storage-profile.upserted",
                "payload": {
                    "authorization-receipt": {
                        "principal": principal,
                        "action": "storage-profile-manage",
                        "allowed": true,
                        "engine": "test",
                        "policy_hash": null,
                        "checked_at": chrono::Utc::now(),
                    },
                    "warehouse": "local",
                    "storage-profile": {
                        "profile-id": "s3-events",
                        "warehouse": "local",
                        "location-prefix-hash": content_hash_json(&json!({
                            "location-prefix": "s3://lakecat/events"
                        })).unwrap(),
                        "provider": "s3",
                        "issuance-mode": "secret-ref",
                        "secret-ref": "vault://kv/lakecat/events",
                        "secret-ref-present": true,
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
        .expect_err("raw storage-profile secret-ref evidence should fail before delivery");
    let message = err.to_string();
    assert!(message.contains("storage-profile.upserted"));
    assert!(message.contains("raw secret-ref"));
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(store.delivered.lock().await.is_empty());
    assert!(graph.events.lock().await.is_empty());
    assert!(lineage.events.lock().await.is_empty());
}

#[tokio::test]
async fn outbox_drain_rejects_storage_profile_secret_ref_mode_missing_ref() {
    let principal = Principal::new("agent:operator", PrincipalKind::Agent).unwrap();
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-storage-profile-secret-ref-mode-missing-ref".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "storage-profile.upserted".to_string(),
            payload: json!({
                "audit-event-id": "audit-storage-profile-secret-ref-mode-missing-ref",
                "event-type": "storage-profile.upserted",
                "payload": {
                    "authorization-receipt": {
                        "principal": principal,
                        "action": "storage-profile-manage",
                        "allowed": true,
                        "engine": "test",
                        "policy_hash": null,
                        "checked_at": chrono::Utc::now(),
                    },
                    "warehouse": "local",
                    "storage-profile": {
                        "profile-id": "s3-events",
                        "warehouse": "local",
                        "location-prefix-hash": content_hash_json(&json!({
                            "location-prefix": "s3://lakecat/events"
                        })).unwrap(),
                        "provider": "s3",
                        "issuance-mode": "short-lived-secret-ref",
                        "secret-ref-present": false,
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
        .expect_err("storage-profile secret-ref mode without proof should fail before delivery");
    let message = err.to_string();
    assert!(message.contains("storage-profile.upserted"));
    assert!(message.contains("secret-ref-present must match issuance-mode"));
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains("evt-storage-profile-secret-ref-mode-missing-ref"));
    assert!(store.delivered.lock().await.is_empty());
    assert!(graph.events.lock().await.is_empty());
    assert!(lineage.events.lock().await.is_empty());
}

#[tokio::test]
async fn outbox_drain_rejects_storage_profile_blank_secret_ref_provider() {
    let principal = Principal::new("agent:operator", PrincipalKind::Agent).unwrap();
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-storage-profile-blank-secret-provider".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "storage-profile.upserted".to_string(),
            payload: json!({
                "audit-event-id": "audit-storage-profile-blank-secret-provider",
                "event-type": "storage-profile.upserted",
                "payload": {
                    "authorization-receipt": {
                        "principal": principal,
                        "action": "storage-profile-manage",
                        "allowed": true,
                        "engine": "test",
                        "policy_hash": null,
                        "checked_at": chrono::Utc::now(),
                    },
                    "warehouse": "local",
                    "storage-profile": {
                        "profile-id": "s3-events",
                        "warehouse": "local",
                        "location-prefix-hash": content_hash_json(&json!({
                            "location-prefix": "s3://lakecat/events"
                        })).unwrap(),
                        "provider": "s3",
                        "issuance-mode": "short-lived-secret-ref",
                        "secret-ref-present": true,
                        "secret-ref-provider": " ",
                        "secret-ref-hash": content_hash_json(&json!({
                            "secret-ref": "typesec://env/LAKECAT_S3_EVENTS"
                        })).unwrap(),
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
        .expect_err("blank storage-profile secret-ref provider should fail before delivery");
    let message = err.to_string();
    assert!(message.contains("storage-profile.upserted"));
    assert!(message.contains("secret-ref-present requires secret-ref-provider"));
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains("evt-storage-profile-blank-secret-provider"));
    assert!(store.delivered.lock().await.is_empty());
    assert!(graph.events.lock().await.is_empty());
    assert!(lineage.events.lock().await.is_empty());
}

#[tokio::test]
async fn outbox_drain_rejects_storage_profile_secret_ref_mode_unexpected_ref() {
    let principal = Principal::new("agent:operator", PrincipalKind::Agent).unwrap();
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-storage-profile-secret-ref-mode-unexpected-ref".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "storage-profile.upserted".to_string(),
            payload: json!({
                "audit-event-id": "audit-storage-profile-secret-ref-mode-unexpected-ref",
                "event-type": "storage-profile.upserted",
                "payload": {
                    "authorization-receipt": {
                        "principal": principal,
                        "action": "storage-profile-manage",
                        "allowed": true,
                        "engine": "test",
                        "policy_hash": null,
                        "checked_at": chrono::Utc::now(),
                    },
                    "warehouse": "local",
                    "storage-profile": {
                        "profile-id": "file-events",
                        "warehouse": "local",
                        "location-prefix-hash": content_hash_json(&json!({
                            "location-prefix": "file:///tmp/lakecat/events"
                        })).unwrap(),
                        "provider": "file",
                        "issuance-mode": "local-file-no-secret",
                        "secret-ref-present": true,
                        "secret-ref-provider": "typesec",
                        "secret-ref-hash": content_hash_json(&json!({
                            "secret-ref": "typesec://env/LAKECAT_FILE_EVENTS"
                        })).unwrap(),
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

    let err = drain_outbox_once(&state, 10).await.expect_err(
        "storage-profile no-secret mode with secret-ref proof should fail before delivery",
    );
    let message = err.to_string();
    assert!(message.contains("storage-profile.upserted"));
    assert!(message.contains("secret-ref-present must match issuance-mode"));
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains("evt-storage-profile-secret-ref-mode-unexpected-ref"));
    assert!(store.delivered.lock().await.is_empty());
    assert!(graph.events.lock().await.is_empty());
    assert!(lineage.events.lock().await.is_empty());
}

#[tokio::test]
async fn outbox_drain_rejects_storage_profile_absent_secret_ref_provider_object() {
    let principal = Principal::new("agent:operator", PrincipalKind::Agent).unwrap();
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-storage-profile-absent-secret-provider-object".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "storage-profile.upserted".to_string(),
            payload: json!({
                "audit-event-id": "audit-storage-profile-absent-secret-provider-object",
                "event-type": "storage-profile.upserted",
                "payload": {
                    "authorization-receipt": {
                        "principal": principal,
                        "action": "storage-profile-manage",
                        "allowed": true,
                        "engine": "test",
                        "policy_hash": null,
                        "checked_at": chrono::Utc::now(),
                    },
                    "warehouse": "local",
                    "storage-profile": {
                        "profile-id": "file-events",
                        "warehouse": "local",
                        "location-prefix-hash": content_hash_json(&json!({
                            "location-prefix": "file:///tmp/lakecat/events"
                        })).unwrap(),
                        "provider": "file",
                        "issuance-mode": "local-file-no-secret",
                        "secret-ref-present": false,
                        "secret-ref-provider": {
                            "unexpected": "typesec"
                        },
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

    let err = drain_outbox_once(&state, 10).await.expect_err(
        "storage-profile absent secret-ref provider object should fail before delivery",
    );
    let message = err.to_string();
    assert!(message.contains("storage-profile.upserted"));
    assert!(
        message.contains(
            "storage-profile upsert cannot carry secret-ref evidence when secret-ref-present is false"
        ),
        "{message}"
    );
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains("evt-storage-profile-absent-secret-provider-object"));
    assert!(store.delivered.lock().await.is_empty());
    assert!(graph.events.lock().await.is_empty());
    assert!(lineage.events.lock().await.is_empty());
}

#[tokio::test]
async fn outbox_drain_rejects_storage_profile_absent_secret_ref_hash_object() {
    let principal = Principal::new("agent:operator", PrincipalKind::Agent).unwrap();
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-storage-profile-absent-secret-hash-object".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "storage-profile.upserted".to_string(),
            payload: json!({
                "audit-event-id": "audit-storage-profile-absent-secret-hash-object",
                "event-type": "storage-profile.upserted",
                "payload": {
                    "authorization-receipt": {
                        "principal": principal,
                        "action": "storage-profile-manage",
                        "allowed": true,
                        "engine": "test",
                        "policy_hash": null,
                        "checked_at": chrono::Utc::now(),
                    },
                    "warehouse": "local",
                    "storage-profile": {
                        "profile-id": "file-events",
                        "warehouse": "local",
                        "location-prefix-hash": content_hash_json(&json!({
                            "location-prefix": "file:///tmp/lakecat/events"
                        })).unwrap(),
                        "provider": "file",
                        "issuance-mode": "local-file-no-secret",
                        "secret-ref-present": false,
                        "secret-ref-hash": {
                            "unexpected": "sha256:dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd"
                        },
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
        .expect_err("storage-profile absent secret-ref hash object should fail before delivery");
    let message = err.to_string();
    assert!(message.contains("storage-profile.upserted"));
    assert!(
        message.contains(
            "storage-profile upsert cannot carry secret-ref evidence when secret-ref-present is false"
        ),
        "{message}"
    );
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains("evt-storage-profile-absent-secret-hash-object"));
    assert!(store.delivered.lock().await.is_empty());
    assert!(graph.events.lock().await.is_empty());
    assert!(lineage.events.lock().await.is_empty());
}
