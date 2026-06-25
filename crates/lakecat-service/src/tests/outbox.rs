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
fn lineage_drain_response_manifest_rejects_summary_drift() {
    let namespace_summary: LineageDrainEventSummary = serde_json::from_value(json!({
        "event-id": "evt-namespace",
        "event-type": "namespace.created",
        "graph-events": 1,
        "lineage-events": 1
    }))
    .unwrap();
    let table_summary: LineageDrainEventSummary = serde_json::from_value(json!({
        "event-id": "evt-table",
        "event-type": "table.created",
        "graph-events": 2,
        "lineage-events": 1
    }))
    .unwrap();
    let response = LineageDrainResponse {
        delivered: 2,
        event_types: vec!["namespace.created".to_string(), "table.created".to_string()],
        graph_events: 3,
        lineage_events: 2,
        principal_subject: None,
        principal_kind: None,
        authorization_receipt_hash: None,
        authorization_receipt_action: None,
        request_identity_state: None,
        request_identity_source: None,
        typedid_envelope_hash: None,
        typedid_proof_hash: None,
        events: vec![namespace_summary, table_summary],
    };
    validate_lineage_drain_response_manifest(&response).unwrap();

    let mut delivered_drift = response.clone();
    delivered_drift.delivered = 1;
    let message = validate_lineage_drain_response_manifest(&delivered_drift)
        .expect_err("delivered count drift should fail")
        .to_string();
    assert!(message.contains("delivered count 1 did not match replay summary count 2"));

    let mut event_type_count_drift = response.clone();
    event_type_count_drift.event_types.pop();
    let message = validate_lineage_drain_response_manifest(&event_type_count_drift)
        .expect_err("event type count drift should fail")
        .to_string();
    assert!(message.contains("event_types count 1 did not match replay summary count 2"));

    let mut event_type_order_drift = response.clone();
    event_type_order_drift.event_types.swap(0, 1);
    let message = validate_lineage_drain_response_manifest(&event_type_order_drift)
        .expect_err("event type order drift should fail")
        .to_string();
    assert!(message.contains(
        "event_types[0] table.created did not match events[0].event_type namespace.created"
    ));

    let mut blank_event_id = response.clone();
    blank_event_id.events[0].event_id = "   ".to_string();
    let message = validate_lineage_drain_response_manifest(&blank_event_id)
        .expect_err("blank replay summary event ids should fail")
        .to_string();
    assert!(message.contains("events[0].event_id must be non-empty"));

    let mut duplicate_event_id = response.clone();
    duplicate_event_id.events[1].event_id = "evt-namespace".to_string();
    let message = validate_lineage_drain_response_manifest(&duplicate_event_id)
        .expect_err("duplicate replay summary event ids should fail")
        .to_string();
    assert!(message.contains("events[1].event_id must be duplicate-free"));

    let mut graph_count_drift = response.clone();
    graph_count_drift.graph_events = 4;
    let message = validate_lineage_drain_response_manifest(&graph_count_drift)
        .expect_err("graph event aggregate drift should fail")
        .to_string();
    assert!(message.contains("graph_events 4 did not match replay summary graph_events 3"));

    let mut lineage_count_drift = response;
    lineage_count_drift.lineage_events = 3;
    let message = validate_lineage_drain_response_manifest(&lineage_count_drift)
        .expect_err("lineage event aggregate drift should fail")
        .to_string();
    assert!(message.contains("lineage_events 3 did not match replay summary lineage_events 2"));
}

#[tokio::test]
async fn lineage_drain_endpoint_replays_querygraph_bootstrap_outbox() {
    let principal = Principal {
        subject: "did:example:agent".to_string(),
        kind: PrincipalKind::Agent,
    };
    let bootstrap_bundle_hash =
        content_hash_json(&json!({"querygraph-bootstrap": "bundle"})).unwrap();
    let bootstrap_graph_hash =
        content_hash_json(&json!({"querygraph-bootstrap": "graph"})).unwrap();
    let bootstrap_open_lineage_hash =
        content_hash_json(&json!({"querygraph-bootstrap": "open-lineage"})).unwrap();
    let bootstrap_import_hash =
        content_hash_json(&json!({"querygraph-bootstrap": "querygraph-import"})).unwrap();
    let bootstrap_croissant_hash =
        content_hash_json(&json!({"querygraph-bootstrap": "croissant"})).unwrap();
    let bootstrap_cdif_hash = content_hash_json(&json!({"querygraph-bootstrap": "cdif"})).unwrap();
    let bootstrap_osi_hash = content_hash_json(&json!({"querygraph-bootstrap": "osi"})).unwrap();
    let bootstrap_odrl_hash = content_hash_json(&json!({"querygraph-bootstrap": "odrl"})).unwrap();
    let bootstrap_policy_hash =
        content_hash_json(&json!({"querygraph-bootstrap": "policies"})).unwrap();
    let bootstrap_view_osi_hash =
        content_hash_json(&json!({"querygraph-bootstrap": "view-osi"})).unwrap();
    let bootstrap_view_receipt_hash =
        content_hash_json(&json!({"querygraph-bootstrap": "view-receipt"})).unwrap();
    let bootstrap_view_chain_hash =
        content_hash_json(&json!({"querygraph-bootstrap": "view-chain"})).unwrap();
    let bootstrap_typedid_envelope_hash =
        content_hash_json(&json!({"querygraph-bootstrap": "typedid-envelope"})).unwrap();
    let bootstrap_typedid_proof_hash =
        content_hash_json(&json!({"querygraph-bootstrap": "typedid-proof"})).unwrap();
    let bootstrap_agent_delegation_hash =
        content_hash_json(&json!({"querygraph-bootstrap": "agent-delegation"})).unwrap();
    let bootstrap_agent_summary_hash =
        content_hash_json(&json!({"querygraph-bootstrap": "agent-summary"})).unwrap();
    let bootstrap_standards = vec![
        "Iceberg REST",
        "Croissant",
        "CDIF",
        "OSI handoff",
        "ODRL",
        "Grust catalog graph",
        "OpenLineage",
    ];
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-bootstrap".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "querygraph.bootstrap".to_string(),
            payload: json!({
                "audit-event-id": "audit-bootstrap",
                "event-type": "querygraph.bootstrap",
                "payload": {
                    "authorization-receipt": {
                        "principal": principal,
                        "action": "graph-read",
                        "allowed": true,
                        "engine": "test",
                        "policy_hash": null,
                        "checked_at": chrono::Utc::now(),
                        "context": {
                            "request-identity": {
                                "attestation-state": "verified",
                                "source": "x-lakecat-typedid-envelope",
                                "typedid-envelope-sha256": bootstrap_typedid_envelope_hash,
                                "typedid-proof-sha256": bootstrap_typedid_proof_hash,
                                "agent-delegation-sha256": bootstrap_agent_delegation_hash,
                                "agent-summary-signature-sha256": bootstrap_agent_summary_hash,
                                "typedid": "did:example:agent"
                            }
                        }
                    },
                    "warehouse": "local",
                    "table-count": 1,
                    "view-count": 1,
                    "policy-binding-count": 1,
                    "verified-tables": ["local.default.events"],
                    "verified-views": ["lakecat:view:local:default:active_customers"],
                    "bundle-hash": bootstrap_bundle_hash,
                    "graph-hash": bootstrap_graph_hash,
                    "open-lineage-hash": bootstrap_open_lineage_hash,
                    "querygraph-import-hash": bootstrap_import_hash,
                    "table-artifacts": [{
                        "stable-id": "local.default.events",
                        "croissant-hash": bootstrap_croissant_hash,
                        "cdif-hash": bootstrap_cdif_hash,
                        "osi-hash": bootstrap_osi_hash,
                        "odrl-hash": bootstrap_odrl_hash,
                        "policy-bindings-hash": bootstrap_policy_hash
                    }],
                    "view-artifacts": [{
                        "stable-id": "lakecat:view:local:default:active_customers",
                        "osi-hash": bootstrap_view_osi_hash
                    }],
                    "view-version-receipts": [{
                        "stable-id": "lakecat:view:local:default:active_customers",
                        "view-version": 1,
                        "receipt-hash": bootstrap_view_receipt_hash,
                        "receipt-chain-hash": bootstrap_view_chain_hash
                    }],
                    "standards": bootstrap_standards
                }
            }),
            created_at: chrono::Utc::now(),
            delivered_at: None,
        }]),
        delivered: Mutex::default(),
    });
    let graph = Arc::new(RecordingGraph::default());
    let lineage = Arc::new(RecordingLineage::default());
    let app = app(
        LakeCatState::new(WarehouseName::new("local").unwrap(), store.clone()).with_integrations(
            default_sail_engine(),
            AllowAllGovernanceEngine::new(),
            graph.clone(),
            lineage.clone(),
        ),
    );

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/management/v1/lineage/drain")
                .header("x-lakecat-agent-did", "did:example:agent")
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
    assert_eq!(payload["delivered"], serde_json::json!(1));
    assert_eq!(
        payload["event-types"],
        serde_json::json!(["querygraph.bootstrap"])
    );
    assert_eq!(payload["graph-events"], serde_json::json!(1));
    assert_eq!(payload["lineage-events"], serde_json::json!(1));
    assert_eq!(
        payload["principal-subject"],
        serde_json::json!("did:example:agent")
    );
    assert_eq!(payload["principal-kind"], serde_json::json!("agent"));
    assert!(
        payload["authorization-receipt-hash"]
            .as_str()
            .is_some_and(|hash| hash.starts_with("sha256:"))
    );
    assert_eq!(
        payload["request-identity-state"],
        serde_json::json!("unverified")
    );
    assert_eq!(
        payload["request-identity-source"],
        serde_json::json!("x-lakecat-agent-did")
    );
    assert_eq!(
        payload["events"][0]["event-id"],
        serde_json::json!("evt-bootstrap")
    );
    assert_eq!(
        payload["events"][0]["event-type"],
        serde_json::json!("querygraph.bootstrap")
    );
    assert_eq!(
        payload["events"][0]["principal-subject"],
        serde_json::json!("did:example:agent")
    );
    assert_eq!(
        payload["events"][0]["principal-kind"],
        serde_json::json!("agent")
    );
    assert!(
        payload["events"][0]["authorization-receipt-hash"]
            .as_str()
            .is_some_and(|hash| hash.starts_with("sha256:"))
    );
    assert_eq!(
        payload["events"][0]["request-identity-state"],
        serde_json::json!("verified")
    );
    assert_eq!(
        payload["events"][0]["request-identity-source"],
        serde_json::json!("x-lakecat-typedid-envelope")
    );
    assert_eq!(
        payload["events"][0]["typedid-envelope-hash"],
        serde_json::json!(bootstrap_typedid_envelope_hash)
    );
    assert_eq!(
        payload["events"][0]["typedid-proof-hash"],
        serde_json::json!(bootstrap_typedid_proof_hash)
    );
    assert!(
        payload["events"][0]["agent-delegation-hash"]
            .as_str()
            .is_some_and(|hash| hash.starts_with("sha256:"))
    );
    assert!(
        payload["events"][0]["agent-summary-signature-hash"]
            .as_str()
            .is_some_and(|hash| hash.starts_with("sha256:"))
    );
    assert_eq!(payload["events"][0]["graph-events"], serde_json::json!(1));
    assert_eq!(payload["events"][0]["lineage-events"], serde_json::json!(1));
    assert_eq!(
        payload["events"][0]["bundle-hash"],
        serde_json::json!(bootstrap_bundle_hash)
    );
    assert_eq!(
        payload["events"][0]["graph-hash"],
        serde_json::json!(bootstrap_graph_hash)
    );
    assert_eq!(
        payload["events"][0]["open-lineage-hash"],
        serde_json::json!(bootstrap_open_lineage_hash)
    );
    assert_eq!(
        payload["events"][0]["querygraph-import-hash"],
        serde_json::json!(bootstrap_import_hash)
    );
    assert_eq!(
        payload["events"][0]["table-artifact-count"],
        serde_json::json!(1)
    );
    assert_eq!(
        payload["events"][0]["view-artifact-count"],
        serde_json::json!(1)
    );
    assert_eq!(
        payload["events"][0]["view-version-receipt-hashes"],
        serde_json::json!([bootstrap_view_receipt_hash])
    );
    assert_eq!(
        payload["events"][0]["policy-binding-count"],
        serde_json::json!(1)
    );
    assert_eq!(
        payload["events"][0]["standards"],
        serde_json::json!(bootstrap_standards)
    );
    assert!(
        payload["events"][0]["replay-event-hashes"]
            .as_array()
            .is_some_and(|hashes| hashes
                .iter()
                .all(|hash| hash.as_str().is_some_and(is_full_sha256_hash)))
    );
    assert!(
        payload["events"][0]["replay-open-lineage-hashes"]
            .as_array()
            .is_some_and(|hashes| hashes
                .iter()
                .all(|hash| hash.as_str().is_some_and(is_full_sha256_hash)))
    );
    assert_eq!(
        store.delivered.lock().await.as_slice(),
        &["evt-bootstrap".to_string()]
    );
    let graph_events = graph.events.lock().await;
    assert_eq!(graph_events.len(), 1);
    assert_eq!(graph_events[0].label, GraphNodeLabel::Principal);
    assert_eq!(
        graph_events[0].subject,
        "lakecat:principal:did:example:agent"
    );
    assert_eq!(
        graph_events[0].event_id.as_deref(),
        Some("evt-bootstrap:principal")
    );
    drop(graph_events);
    let lineage_events = lineage.events.lock().await;
    assert_eq!(lineage_events.len(), 1);
    assert_eq!(
        lineage_events[0].event_type,
        LineageEventType::QueryGraphBootstrap
    );
    assert_eq!(lineage_events[0].principal.subject, "did:example:agent");
    assert_eq!(
        lineage_events[0].payload["bundle-hash"],
        serde_json::json!(bootstrap_bundle_hash)
    );
    assert_eq!(
        lineage_events[0].payload["graph-hash"],
        serde_json::json!(bootstrap_graph_hash)
    );
    assert_eq!(
        lineage_events[0].payload["querygraph-import-hash"],
        serde_json::json!(bootstrap_import_hash)
    );
    assert_eq!(
        lineage_events[0].payload["table-artifacts"][0]["croissant-hash"],
        serde_json::json!(bootstrap_croissant_hash)
    );
    assert_eq!(
        lineage_events[0].payload["table-artifacts"][0]["policy-bindings-hash"],
        serde_json::json!(bootstrap_policy_hash)
    );
    assert_eq!(
        lineage_events[0].payload["view-artifacts"][0]["osi-hash"],
        serde_json::json!(bootstrap_view_osi_hash)
    );
    assert_eq!(
        lineage_events[0].payload["standards"],
        serde_json::json!(bootstrap_standards)
    );
}

#[test]
fn lineage_drain_summary_rejects_malformed_scan_projection_fields() {
    let receipt = OutboxProjectionReceipt::default();
    let malformed_requested_projection = OutboxEvent {
        event_id: "evt-bad-summary-requested-projection".to_string(),
        sink: "lakecat.lineage-and-graph".to_string(),
        event_type: "table.scan-planned".to_string(),
        payload: json!({
            "payload": {
                "requested-projection": ["event_id", 42],
                "effective-projection": ["event_id"],
                "requested-stats-fields": ["event_id"],
                "effective-stats-fields": ["event_id"],
                "scan-task-count": 1
            }
        }),
        created_at: chrono::Utc::now(),
        delivered_at: None,
    };
    let err = lineage_drain_event_summary(&malformed_requested_projection, &receipt)
        .unwrap_err()
        .to_string();
    assert!(err.contains("requested-projection must contain strings"));

    let duplicate_effective_projection = OutboxEvent {
        event_id: "evt-duplicate-summary-effective-projection".to_string(),
        sink: "lakecat.lineage-and-graph".to_string(),
        event_type: "table.scan-planned".to_string(),
        payload: json!({
            "payload": {
                "requested-projection": ["event_id"],
                "effective-projection": ["event_id", "event_id"],
                "requested-stats-fields": ["event_id"],
                "effective-stats-fields": ["event_id"],
                "scan-task-count": 1
            }
        }),
        created_at: chrono::Utc::now(),
        delivered_at: None,
    };
    let err = lineage_drain_event_summary(&duplicate_effective_projection, &receipt)
        .unwrap_err()
        .to_string();
    assert!(err.contains("effective-projection must not contain duplicate values"));
}

#[test]
fn lineage_drain_summary_rejects_malformed_scan_stats_fields() {
    let receipt = OutboxProjectionReceipt::default();
    let blank_requested_stats_field = OutboxEvent {
        event_id: "evt-blank-summary-requested-stats".to_string(),
        sink: "lakecat.lineage-and-graph".to_string(),
        event_type: "table.scan-planned".to_string(),
        payload: json!({
            "payload": {
                "requested-projection": ["event_id"],
                "effective-projection": ["event_id"],
                "requested-stats-fields": ["event_id", " "],
                "effective-stats-fields": ["event_id"],
                "scan-task-count": 1
            }
        }),
        created_at: chrono::Utc::now(),
        delivered_at: None,
    };
    let err = lineage_drain_event_summary(&blank_requested_stats_field, &receipt)
        .unwrap_err()
        .to_string();
    assert!(err.contains("requested-stats-fields must not contain blank strings"));

    let malformed_effective_stats_field = OutboxEvent {
        event_id: "evt-bad-summary-effective-stats".to_string(),
        sink: "lakecat.lineage-and-graph".to_string(),
        event_type: "table.scan-planned".to_string(),
        payload: json!({
            "payload": {
                "requested-projection": ["event_id"],
                "effective-projection": ["event_id"],
                "requested-stats-fields": ["event_id"],
                "effective-stats-fields": [false],
                "scan-task-count": 1
            }
        }),
        created_at: chrono::Utc::now(),
        delivered_at: None,
    };
    let err = lineage_drain_event_summary(&malformed_effective_stats_field, &receipt)
        .unwrap_err()
        .to_string();
    assert!(err.contains("effective-stats-fields must contain strings"));

    let empty_stats_fields = OutboxEvent {
        event_id: "evt-empty-summary-stats-fields".to_string(),
        sink: "lakecat.lineage-and-graph".to_string(),
        event_type: "table.scan-tasks-fetched".to_string(),
        payload: json!({
            "payload": {
                "requested-projection": ["event_id"],
                "effective-projection": ["event_id"],
                "requested-stats-fields": ["event_id"],
                "effective-stats-fields": ["event_id"],
                "stats-fields": [],
                "file-scan-task-count": 1,
                "delete-file-count": 0,
                "child-plan-task-count": 0
            }
        }),
        created_at: chrono::Utc::now(),
        delivered_at: None,
    };
    let err = lineage_drain_event_summary(&empty_stats_fields, &receipt)
        .unwrap_err()
        .to_string();
    assert!(err.contains("stats-fields must not be empty when present"));

    let duplicate_stats_fields = OutboxEvent {
        event_id: "evt-duplicate-summary-stats-fields".to_string(),
        sink: "lakecat.lineage-and-graph".to_string(),
        event_type: "table.scan-tasks-fetched".to_string(),
        payload: json!({
            "payload": {
                "requested-projection": ["event_id"],
                "effective-projection": ["event_id"],
                "requested-stats-fields": ["event_id"],
                "effective-stats-fields": ["event_id"],
                "stats-fields": ["event_id", "event_id"],
                "file-scan-task-count": 1,
                "delete-file-count": 0,
                "child-plan-task-count": 0
            }
        }),
        created_at: chrono::Utc::now(),
        delivered_at: None,
    };
    let err = lineage_drain_event_summary(&duplicate_stats_fields, &receipt)
        .unwrap_err()
        .to_string();
    assert!(err.contains("stats-fields must not contain duplicate values"));

    let drifted_stats_fields = OutboxEvent {
        event_id: "evt-drifted-summary-stats-fields".to_string(),
        sink: "lakecat.lineage-and-graph".to_string(),
        event_type: "table.scan-tasks-fetched".to_string(),
        payload: json!({
            "payload": {
                "requested-projection": ["event_id", "payload"],
                "effective-projection": ["event_id"],
                "requested-stats-fields": ["event_id", "payload"],
                "effective-stats-fields": ["event_id"],
                "stats-fields": ["event_id", "payload"],
                "file-scan-task-count": 1,
                "delete-file-count": 0,
                "child-plan-task-count": 0
            }
        }),
        created_at: chrono::Utc::now(),
        delivered_at: None,
    };
    let err = lineage_drain_event_summary(&drifted_stats_fields, &receipt)
        .unwrap_err()
        .to_string();
    assert!(
        err.contains(
            "lineage drain summary stats-fields must be a subset of effective-stats-fields"
        ) || err.contains("lineage drain summary stats-fields must match effective-stats-fields"),
        "{err}"
    );

    let narrowed_stats_fields = OutboxEvent {
        event_id: "evt-narrowed-summary-stats-fields".to_string(),
        sink: "lakecat.lineage-and-graph".to_string(),
        event_type: "table.scan-tasks-fetched".to_string(),
        payload: json!({
            "payload": {
                "requested-projection": ["event_id", "payload"],
                "effective-projection": ["event_id", "payload"],
                "requested-stats-fields": ["event_id", "payload"],
                "effective-stats-fields": ["event_id", "payload"],
                "stats-fields": ["event_id"],
                "file-scan-task-count": 1,
                "delete-file-count": 0,
                "child-plan-task-count": 0
            }
        }),
        created_at: chrono::Utc::now(),
        delivered_at: None,
    };
    let err = lineage_drain_event_summary(&narrowed_stats_fields, &receipt)
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("lineage drain summary stats-fields must match effective-stats-fields"),
        "{err}"
    );
}

#[test]
fn lineage_drain_summary_rejects_malformed_operational_counts() {
    let receipt = OutboxProjectionReceipt::default();
    let cases = [
        (
            "table.commits-listed",
            "commit-count",
            json!("2"),
            "commit-count must be an unsigned integer when present",
        ),
        (
            "table.scan-planned",
            "scan-task-count",
            json!("1"),
            "scan-task-count must be an unsigned integer when present",
        ),
        (
            "table.scan-tasks-fetched",
            "file-scan-task-count",
            json!("1"),
            "file-scan-task-count must be an unsigned integer when present",
        ),
        (
            "table.scan-tasks-fetched",
            "delete-file-count",
            json!(-1),
            "delete-file-count must be an unsigned integer when present",
        ),
        (
            "table.scan-tasks-fetched",
            "child-plan-task-count",
            json!("1"),
            "child-plan-task-count must be an unsigned integer when present",
        ),
        (
            "credentials.vend-attempted",
            "credential-count",
            json!("0"),
            "credential-vend evidence must contain credential-count",
        ),
    ];

    for (event_type, field, value, expected_message) in cases {
        let mut payload = serde_json::Map::new();
        payload.insert(field.to_string(), value);
        let event = if event_type == "credentials.vend-attempted" {
            let mut event =
                valid_lineage_summary_credential_event(&format!("evt-malformed-summary-{field}"));
            for (key, value) in payload {
                event.payload["payload"][key] = value;
            }
            event
        } else {
            OutboxEvent {
                event_id: format!("evt-malformed-summary-{field}"),
                sink: "lakecat.lineage-and-graph".to_string(),
                event_type: event_type.to_string(),
                payload: json!({
                    "payload": Value::Object(payload)
                }),
                created_at: chrono::Utc::now(),
                delivered_at: None,
            }
        };
        let err = lineage_drain_event_summary(&event, &receipt)
            .unwrap_err()
            .to_string();
        assert!(err.contains(expected_message), "{err}");
    }
}

#[test]
fn lineage_drain_summary_rejects_malformed_table_commit_evidence() {
    let receipt = OutboxProjectionReceipt::default();
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let principal = Principal::new("agent:writer", PrincipalKind::Agent).unwrap();
    let base_commit = json!({
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

    for (case, mutate, expected_message) in [
        (
            "missing-committed-at",
            "remove-committed-at",
            "table commit evidence committed_at timestamp must be present",
        ),
        (
            "malformed-committed-at",
            "malformed-committed-at",
            "table commit evidence committed_at timestamp must be RFC3339",
        ),
        (
            "short-request-hash",
            "short-request-hash",
            "request_hash/request-hash must contain full SHA-256 digest evidence",
        ),
        (
            "short-response-hash",
            "short-response-hash",
            "response_hash/response-hash must contain full SHA-256 digest evidence",
        ),
        (
            "short-idempotency-hash",
            "short-idempotency-hash",
            "idempotency_key_sha256/idempotency-key-sha256 must contain full SHA-256 digest evidence",
        ),
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
        (
            "missing-new-metadata",
            "remove-new-metadata",
            "table commit evidence must contain non-empty new metadata location",
        ),
        (
            "blank-previous-metadata",
            "blank-previous-metadata",
            "table commit evidence previous metadata location must be non-empty when present",
        ),
        (
            "duplicate-new-metadata-alias",
            "duplicate-new-metadata-alias",
            "table commit must not carry both new_metadata_location and new-metadata-location",
        ),
        (
            "extra-commit-claim",
            "extra-commit-claim",
            "table commit contains unexpected field unverified-commit-claim",
        ),
    ] {
        let mut commit = base_commit.clone();
        match mutate {
            "remove-committed-at" => {
                commit.as_object_mut().unwrap().remove("committed_at");
            }
            "malformed-committed-at" => {
                commit["committed_at"] = json!("not-a-timestamp");
            }
            "short-request-hash" => {
                commit["request_hash"] = json!("sha256:short");
            }
            "short-response-hash" => {
                commit["response_hash"] = json!("sha256:short");
            }
            "short-idempotency-hash" => {
                commit["idempotency_key_sha256"] = json!("sha256:short");
            }
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
            "remove-new-metadata" => {
                commit
                    .as_object_mut()
                    .unwrap()
                    .remove("new_metadata_location");
            }
            "blank-previous-metadata" => {
                commit["previous_metadata_location"] = json!(" ");
            }
            "duplicate-new-metadata-alias" => {
                commit["new-metadata-location"] = json!("file:///tmp/events/metadata/00002.json");
            }
            "extra-commit-claim" => {
                commit["unverified-commit-claim"] = json!("already-authorized");
            }
            _ => unreachable!("unknown table commit summary mutation"),
        }
        let event = OutboxEvent {
            event_id: format!("evt-summary-table-commit-{case}"),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "table.commit".to_string(),
            payload: json!({
                "audit-event-id": format!("audit-summary-table-commit-{case}"),
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
        };

        let err = lineage_drain_event_summary(&event, &receipt)
            .unwrap_err()
            .to_string();
        assert!(err.contains(expected_message), "{case}: {err}");
        assert!(err.contains("event-id-hash=sha256:"), "{case}: {err}");
        assert!(!err.contains(event.event_id.as_str()), "{case}: {err}");
    }
}

#[test]
fn lineage_drain_summary_rejects_extra_table_commit_payload_fields() {
    let receipt = OutboxProjectionReceipt::default();
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
    let inner_payload = json!({
        "audit-event-id": "audit-summary-extra-commit-payload",
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
    });

    for (case, payload, expected_message) in [
        (
            "extra-top-level",
            {
                let mut payload = inner_payload.clone();
                payload["unverified-commit-claim"] = json!("already-authorized");
                payload
            },
            "table commit payload contains unexpected field unverified-commit-claim",
        ),
        (
            "extra-wrapper",
            json!({
                "audit-event-id": "audit-summary-extra-commit-wrapper",
                "event-type": "table.commit",
                "table": table,
                "payload": inner_payload,
                "unverified-commit-wrapper-claim": "already-authorized",
            }),
            "table commit outbox payload contains unexpected field unverified-commit-wrapper-claim",
        ),
    ] {
        let event = OutboxEvent {
            event_id: format!("evt-summary-table-commit-{case}"),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "table.commit".to_string(),
            payload,
            created_at: chrono::Utc::now(),
            delivered_at: None,
        };

        let err = lineage_drain_event_summary(&event, &receipt)
            .unwrap_err()
            .to_string();
        assert!(err.contains(expected_message), "{case}: {err}");
        assert!(err.contains("event-id-hash=sha256:"), "{case}: {err}");
        assert!(!err.contains(event.event_id.as_str()), "{case}: {err}");
    }
}

#[test]
fn lineage_drain_summary_rejects_malformed_scan_required_filters() {
    let receipt = OutboxProjectionReceipt::default();
    let missing_required_filters = OutboxEvent {
        event_id: "evt-missing-summary-required-filters".to_string(),
        sink: "lakecat.lineage-and-graph".to_string(),
        event_type: "table.scan-planned".to_string(),
        payload: json!({
            "payload": {
                "read-restriction": {
                    "allowed-columns": ["event_id"],
                    "row-predicate": {
                        "type": "eq",
                        "term": "event_id",
                        "value": "evt-1"
                    },
                    "policy-hashes": [content_hash_bytes(b"policy")]
                }
            }
        }),
        created_at: chrono::Utc::now(),
        delivered_at: None,
    };
    let err = lineage_drain_event_summary(&missing_required_filters, &receipt)
        .unwrap_err()
        .to_string();
    assert!(err.contains("lineage drain table.scan-planned required-filters must be an array"));

    let missing_fetched_required_filters = OutboxEvent {
        event_id: "evt-missing-summary-fetched-required-filters".to_string(),
        sink: "lakecat.lineage-and-graph".to_string(),
        event_type: "table.scan-tasks-fetched".to_string(),
        payload: json!({
            "payload": {
                "read-restriction": {
                    "allowed-columns": ["event_id"],
                    "row-predicate": {
                        "type": "eq",
                        "term": "event_id",
                        "value": "evt-1"
                    },
                    "policy-hashes": [content_hash_bytes(b"policy")]
                }
            }
        }),
        created_at: chrono::Utc::now(),
        delivered_at: None,
    };
    let err = lineage_drain_event_summary(&missing_fetched_required_filters, &receipt)
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("lineage drain table.scan-tasks-fetched required-filters must be an array")
    );

    let row_predicate = json!({
        "type": "eq",
        "term": "event_id",
        "value": "evt-1"
    });
    let non_array_required_filters = OutboxEvent {
        event_id: "evt-bad-summary-required-filters".to_string(),
        sink: "lakecat.lineage-and-graph".to_string(),
        event_type: "table.scan-tasks-fetched".to_string(),
        payload: json!({
            "payload": {
                "read-restriction": {
                    "allowed-columns": ["event_id"],
                    "row-predicate": row_predicate,
                    "policy-hashes": [content_hash_bytes(b"policy")]
                },
                "required-filters": row_predicate
            }
        }),
        created_at: chrono::Utc::now(),
        delivered_at: None,
    };
    let err = lineage_drain_event_summary(&non_array_required_filters, &receipt)
        .unwrap_err()
        .to_string();
    assert!(err.contains("required-filters must be an array when present"));

    let unsourced_required_filters = OutboxEvent {
        event_id: "evt-unsourced-summary-required-filters".to_string(),
        sink: "lakecat.lineage-and-graph".to_string(),
        event_type: "table.scan-planned".to_string(),
        payload: json!({
            "payload": {
                "required-filters": [{
                    "type": "eq",
                    "term": "event_id",
                    "value": "evt-1"
                }]
            }
        }),
        created_at: chrono::Utc::now(),
        delivered_at: None,
    };
    let err = lineage_drain_event_summary(&unsourced_required_filters, &receipt)
        .unwrap_err()
        .to_string();
    assert!(
        err.contains(
            "lineage drain table.scan-planned required-filters must be empty without read-restriction row-predicate"
        ),
        "{err}"
    );

    let unsourced_fetched_required_filters = OutboxEvent {
        event_id: "evt-unsourced-summary-fetched-required-filters".to_string(),
        sink: "lakecat.lineage-and-graph".to_string(),
        event_type: "table.scan-tasks-fetched".to_string(),
        payload: json!({
            "payload": {
                "required-filters": [{
                    "type": "eq",
                    "term": "event_id",
                    "value": "evt-1"
                }]
            }
        }),
        created_at: chrono::Utc::now(),
        delivered_at: None,
    };
    let err = lineage_drain_event_summary(&unsourced_fetched_required_filters, &receipt)
        .unwrap_err()
        .to_string();
    assert!(
        err.contains(
            "lineage drain table.scan-tasks-fetched required-filters must be empty without read-restriction row-predicate"
        ),
        "{err}"
    );

    let expected_row_predicate = json!({
        "type": "eq",
        "term": "event_id",
        "value": "evt-1"
    });
    let drifted_required_filters = OutboxEvent {
        event_id: "evt-drifted-summary-required-filters".to_string(),
        sink: "lakecat.lineage-and-graph".to_string(),
        event_type: "table.scan-tasks-fetched".to_string(),
        payload: json!({
            "payload": {
                "read-restriction": {
                    "allowed-columns": ["event_id"],
                    "row-predicate": expected_row_predicate,
                    "policy-hashes": [content_hash_bytes(b"policy")]
                },
                "required-filters": [{
                    "type": "eq",
                    "term": "event_id",
                    "value": "evt-2"
                }]
            }
        }),
        created_at: chrono::Utc::now(),
        delivered_at: None,
    };
    let err = lineage_drain_event_summary(&drifted_required_filters, &receipt)
        .unwrap_err()
        .to_string();
    assert!(
        err.contains(
            "lineage drain table.scan-tasks-fetched required-filters must exactly preserve read-restriction row-predicate"
        ),
        "{err}"
    );

    let expected_row_predicate = json!({
        "type": "eq",
        "term": "event_id",
        "value": "evt-1"
    });
    let drifted_planned_required_filters = OutboxEvent {
        event_id: "evt-drifted-summary-planned-required-filters".to_string(),
        sink: "lakecat.lineage-and-graph".to_string(),
        event_type: "table.scan-planned".to_string(),
        payload: json!({
            "payload": {
                "read-restriction": {
                    "allowed-columns": ["event_id"],
                    "row-predicate": expected_row_predicate,
                    "policy-hashes": [content_hash_bytes(b"policy")]
                },
                "required-filters": [{
                    "type": "eq",
                    "term": "event_id",
                    "value": "evt-2"
                }]
            }
        }),
        created_at: chrono::Utc::now(),
        delivered_at: None,
    };
    let err = lineage_drain_event_summary(&drifted_planned_required_filters, &receipt)
        .unwrap_err()
        .to_string();
    assert!(
        err.contains(
            "lineage drain table.scan-planned required-filters must exactly preserve read-restriction row-predicate"
        ),
        "{err}"
    );
}

#[test]
fn lineage_drain_summary_rejects_malformed_scan_plan_task() {
    let receipt = OutboxProjectionReceipt::default();
    let decorated_plan_task = OutboxEvent {
        event_id: "evt-bad-summary-decorated-plan-task".to_string(),
        sink: "lakecat.lineage-and-graph".to_string(),
        event_type: "table.scan-tasks-fetched".to_string(),
        payload: json!({
            "payload": {
                "plan-task": "lakecat:plan:abc?token=secret",
                "file-scan-task-count": 1,
                "delete-file-count": 0,
                "child-plan-task-count": 0
            }
        }),
        created_at: chrono::Utc::now(),
        delivered_at: None,
    };
    let err = lineage_drain_event_summary(&decorated_plan_task, &receipt)
        .unwrap_err()
        .to_string();
    assert!(
        err.contains(
            "lineage drain summary plan-task must not contain decorated location material"
        ),
        "{err}"
    );
    assert!(!err.contains("lakecat:plan:abc?token=secret"));

    let credential_bearing_plan_task = OutboxEvent {
        event_id: "evt-bad-summary-credential-plan-task".to_string(),
        sink: "lakecat.lineage-and-graph".to_string(),
        event_type: "table.scan-tasks-fetched".to_string(),
        payload: json!({
            "payload": {
                "plan-task": "lakecat:plan:abc:credential=secret",
                "file-scan-task-count": 1,
                "delete-file-count": 0,
                "child-plan-task-count": 0
            }
        }),
        created_at: chrono::Utc::now(),
        delivered_at: None,
    };
    let err = lineage_drain_event_summary(&credential_bearing_plan_task, &receipt)
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("lineage drain summary plan-task must not contain credential material"),
        "{err}"
    );
    assert!(!err.contains("credential=secret"));

    let foreign_plan_task = OutboxEvent {
        event_id: "evt-bad-summary-foreign-plan-task".to_string(),
        sink: "lakecat.lineage-and-graph".to_string(),
        event_type: "table.scan-tasks-fetched".to_string(),
        payload: json!({
            "payload": {
                "plan-task": "spark:plan:abc",
                "file-scan-task-count": 1,
                "delete-file-count": 0,
                "child-plan-task-count": 0
            }
        }),
        created_at: chrono::Utc::now(),
        delivered_at: None,
    };
    let err = lineage_drain_event_summary(&foreign_plan_task, &receipt)
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("lineage drain summary plan-task must be LakeCat-issued evidence"),
        "{err}"
    );
    assert!(!err.contains("spark:plan:abc"));
}

#[test]
fn lineage_drain_summary_rejects_malformed_view_receipt_hashes() {
    let receipt = OutboxProjectionReceipt::default();
    let malformed_top_level_receipt_hash = OutboxEvent {
        event_id: "evt-bad-receipt-hashes".to_string(),
        sink: "lakecat.lineage-and-graph".to_string(),
        event_type: "view.version-receipts-listed".to_string(),
        payload: json!({
            "payload": {
                "warehouse": "local",
                "namespace": ["default"],
                "view": "events_view",
                "receipt-hashes": [42],
            }
        }),
        created_at: chrono::Utc::now(),
        delivered_at: None,
    };
    let err = lineage_drain_event_summary(&malformed_top_level_receipt_hash, &receipt)
        .unwrap_err()
        .to_string();
    assert!(err.contains("receipt-hashes must contain full SHA-256 digest evidence"));

    let malformed_nested_receipt_hash = OutboxEvent {
        event_id: "evt-bad-nested-receipt-hash".to_string(),
        sink: "lakecat.lineage-and-graph".to_string(),
        event_type: "view.version-receipt-chains-listed".to_string(),
        payload: json!({
            "payload": {
                "warehouse": "local",
                "namespace": ["default"],
                "view-version-receipt-chains": [{
                    "chain-hash": content_hash_bytes(b"chain"),
                    "receipts": [{
                        "receipt-hash": "not-a-full-hash"
                    }]
                }]
            }
        }),
        created_at: chrono::Utc::now(),
        delivered_at: None,
    };
    let err = lineage_drain_event_summary(&malformed_nested_receipt_hash, &receipt)
        .unwrap_err()
        .to_string();
    assert!(err.contains("receipt-hash must contain full SHA-256 digest evidence"));
}

#[test]
fn lineage_drain_summary_rejects_malformed_view_chain_hashes() {
    let receipt = OutboxProjectionReceipt::default();
    let malformed_nested_chain_hash = OutboxEvent {
        event_id: "evt-bad-nested-chain-hash".to_string(),
        sink: "lakecat.lineage-and-graph".to_string(),
        event_type: "view.version-receipt-chains-listed".to_string(),
        payload: json!({
            "payload": {
                "warehouse": "local",
                "namespace": ["default"],
                "view-version-receipt-chains": [{
                    "chain-hash": 42,
                    "receipts": [{
                        "receipt-hash": content_hash_bytes(b"receipt")
                    }]
                }]
            }
        }),
        created_at: chrono::Utc::now(),
        delivered_at: None,
    };
    let err = lineage_drain_event_summary(&malformed_nested_chain_hash, &receipt)
        .unwrap_err()
        .to_string();
    assert!(err.contains("chain-hash must contain full SHA-256 digest evidence"));

    let malformed_top_level_chain_hash = OutboxEvent {
        event_id: "evt-bad-chain-hashes".to_string(),
        sink: "lakecat.lineage-and-graph".to_string(),
        event_type: "view.version-receipt-chains-listed".to_string(),
        payload: json!({
            "payload": {
                "warehouse": "local",
                "namespace": ["default"],
                "chain-hashes": ["sha256:not-full"]
            }
        }),
        created_at: chrono::Utc::now(),
        delivered_at: None,
    };
    let err = lineage_drain_event_summary(&malformed_top_level_chain_hash, &receipt)
        .unwrap_err()
        .to_string();
    assert!(err.contains("chain-hashes must contain full SHA-256 digest evidence"));
}

#[test]
fn lineage_drain_summary_rejects_malformed_view_receipt_chain_objects() {
    let receipt = OutboxProjectionReceipt::default();
    let chain_hash = content_hash_bytes(b"chain");
    let missing_stable_id = OutboxEvent {
        event_id: "evt-bad-summary-view-chain-shape".to_string(),
        sink: "lakecat.lineage-and-graph".to_string(),
        event_type: "view.version-receipt-chains-listed".to_string(),
        payload: json!({
            "payload": {
                "warehouse": "local",
                "namespace": ["default"],
                "view-version-receipt-chains": [{
                    "warehouse": "local",
                    "namespace": ["default"],
                    "name": "events_view",
                    "chain-hash": chain_hash,
                    "chain-verified": true,
                    "latest-view-version": 1,
                    "latest-operation": "upsert",
                    "tombstoned": false,
                    "receipt-count": 0,
                    "receipts": []
                }]
            }
        }),
        created_at: chrono::Utc::now(),
        delivered_at: None,
    };
    let err = lineage_drain_event_summary(&missing_stable_id, &receipt)
        .unwrap_err()
        .to_string();
    assert!(
        err.contains(
            "lineage drain view-version-receipt-chains entry 0 must match ViewVersionReceiptChainResponse JSON shape"
        ),
        "{err}"
    );

    let malformed_verified_count = OutboxEvent {
        event_id: "evt-bad-summary-chain-verified-count".to_string(),
        sink: "lakecat.lineage-and-graph".to_string(),
        event_type: "view.version-receipt-chains-listed".to_string(),
        payload: json!({
            "payload": {
                "warehouse": "local",
                "namespace": ["default"],
                "chain-verified-count": "1",
                "view-version-receipt-chains": []
            }
        }),
        created_at: chrono::Utc::now(),
        delivered_at: None,
    };
    let err = lineage_drain_event_summary(&malformed_verified_count, &receipt)
        .unwrap_err()
        .to_string();
    assert!(err.contains("chain-verified-count must be an unsigned integer when present"));

    let chain_hash = content_hash_bytes(b"verified-chain");
    let receipt_hash = content_hash_bytes(b"verified-chain-receipt");
    let drifted_verified_count = OutboxEvent {
        event_id: "evt-drifted-summary-chain-verified-count".to_string(),
        sink: "lakecat.lineage-and-graph".to_string(),
        event_type: "view.version-receipt-chains-listed".to_string(),
        payload: json!({
            "payload": {
                "warehouse": "local",
                "namespace": ["default"],
                "chain-verified-count": 2,
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
                        "view-hash": content_hash_bytes(b"view-v1"),
                        "receipt-hash": receipt_hash,
                        "principal-subject": "agent:operator",
                        "principal-kind": "agent",
                        "recorded-at": "2026-06-20T00:00:00Z"
                    }]
                }]
            }
        }),
        created_at: chrono::Utc::now(),
        delivered_at: None,
    };
    let err = lineage_drain_event_summary(&drifted_verified_count, &receipt)
        .unwrap_err()
        .to_string();
    assert!(err.contains("chain-verified-count must match verified view receipt chains"));

    let unverified_chain = OutboxEvent {
        event_id: "evt-unverified-summary-view-chain".to_string(),
        sink: "lakecat.lineage-and-graph".to_string(),
        event_type: "view.version-receipt-chains-listed".to_string(),
        payload: json!({
            "payload": {
                "warehouse": "local",
                "namespace": ["default"],
                "view-version-receipt-chains": [{
                    "stable-id": "lakecat:view:local:default:events_view",
                    "warehouse": "local",
                    "namespace": ["default"],
                    "name": "events_view",
                    "chain-hash": chain_hash,
                    "chain-verified": false,
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
                        "view-hash": content_hash_bytes(b"view-v1"),
                        "receipt-hash": receipt_hash,
                        "principal-subject": "agent:operator",
                        "principal-kind": "agent",
                        "recorded-at": "2026-06-20T00:00:00Z"
                    }]
                }]
            }
        }),
        created_at: chrono::Utc::now(),
        delivered_at: None,
    };
    let err = lineage_drain_event_summary(&unverified_chain, &receipt)
        .unwrap_err()
        .to_string();
    assert!(
        err.contains(
            "lineage drain view-version-receipt-chains entry 0 must be structurally verified"
        ),
        "{err}"
    );
}

#[test]
fn lineage_drain_summary_rejects_unverified_view_receipt_fields() {
    let receipt = OutboxProjectionReceipt::default();
    let principal = Principal::new("agent:reader", PrincipalKind::Agent).unwrap();
    let view_hash = content_hash_bytes(b"events-view-v1");
    let receipt_hash = content_hash_bytes(b"events-view-receipt-v1");
    let drop_receipt_hash = content_hash_bytes(b"events-view-drop-receipt-v1");
    let chain_hash = content_hash_bytes(b"events-view-chain-v1");

    let cases = [
        (
            "view.version-receipts-listed",
            "view receipt-list contains unexpected field unverified-receipt-list-claim",
            json!({
                "event-type": "view.version-receipts-listed",
                "warehouse": "local",
                "namespace": ["default"],
                "view": "events_view",
                "receipt-count": 2,
                "receipt-hashes": [receipt_hash, drop_receipt_hash],
                "drop-receipt-hashes": [drop_receipt_hash],
                "authorization-receipt": {
                    "principal": principal,
                    "action": "view-load",
                    "allowed": true,
                    "engine": "test",
                    "policy_hash": null,
                    "checked_at": chrono::Utc::now(),
                },
                "unverified-receipt-list-claim": true,
            }),
        ),
        (
            "view.version-receipt-chains-listed",
            "view receipt-chain contains unexpected field unverified-receipt-chain-claim",
            json!({
                "event-type": "view.version-receipt-chains-listed",
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
                        "view-hash": view_hash,
                        "receipt-hash": receipt_hash,
                        "principal-subject": "agent:operator",
                        "principal-kind": "agent",
                        "recorded-at": "2026-06-20T00:00:00Z"
                    }]
                }],
                "chain-hashes": [chain_hash],
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
                "unverified-receipt-chain-claim": true,
            }),
        ),
    ];

    for (event_type, expected_message, payload) in cases {
        let event = OutboxEvent {
            event_id: format!("evt-summary-{event_type}-extra-field"),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: event_type.to_string(),
            payload: json!({
                "audit-event-id": format!("audit-summary-{event_type}-extra-field"),
                "event-type": event_type,
                "payload": payload,
            }),
            created_at: chrono::Utc::now(),
            delivered_at: None,
        };

        let err = lineage_drain_event_summary(&event, &receipt)
            .unwrap_err()
            .to_string();
        assert!(err.contains(expected_message), "{event_type}: {err}");
        assert!(err.contains("event-id-hash=sha256:"), "{event_type}: {err}");
        assert!(
            !err.contains(event.event_id.as_str()),
            "{event_type}: {err}"
        );
    }
}

#[test]
fn lineage_drain_summary_rejects_malformed_view_versions() {
    let receipt = OutboxProjectionReceipt::default();
    let mut malformed_view_version =
        valid_lineage_summary_view_event("evt-bad-summary-view-version", "view.upserted");
    malformed_view_version.payload["payload"]["view"]["view-version"] = json!("1");
    let err = lineage_drain_event_summary(&malformed_view_version, &receipt)
        .unwrap_err()
        .to_string();
    assert!(err.contains("view lifecycle evidence must contain positive view-version"));

    let mut zero_view_version =
        valid_lineage_summary_view_event("evt-zero-summary-view-version", "view.loaded");
    zero_view_version.payload["payload"]["view"]["view-version"] = json!(0);
    let err = lineage_drain_event_summary(&zero_view_version, &receipt)
        .unwrap_err()
        .to_string();
    assert!(err.contains("view lifecycle evidence must contain positive view-version"));

    let mut malformed_expected_view_version =
        valid_lineage_summary_view_event("evt-bad-summary-expected-view-version", "view.dropped");
    malformed_expected_view_version.payload["payload"]["expected-view-version"] = json!("1");
    let err = lineage_drain_event_summary(&malformed_expected_view_version, &receipt)
        .unwrap_err()
        .to_string();
    assert!(err.contains("view lifecycle expected-view-version must be positive when present"));

    let mut zero_expected_view_version =
        valid_lineage_summary_view_event("evt-zero-summary-expected-view-version", "view.dropped");
    zero_expected_view_version.payload["payload"]["expected-view-version"] = json!(0);
    let err = lineage_drain_event_summary(&zero_expected_view_version, &receipt)
        .unwrap_err()
        .to_string();
    assert!(err.contains("view lifecycle expected-view-version must be positive when present"));
}

#[test]
fn lineage_drain_summary_rejects_malformed_commit_history_fields() {
    let receipt = OutboxProjectionReceipt::default();
    let malformed_sequence = OutboxEvent {
        event_id: "evt-bad-summary-sequence".to_string(),
        sink: "lakecat.lineage-and-graph".to_string(),
        event_type: "table.commits-listed".to_string(),
        payload: json!({
            "payload": {
                "commit-count": 1,
                "sequence-numbers": [1, "two"],
                "commit-hashes": [content_hash_bytes(b"commit-one")]
            }
        }),
        created_at: chrono::Utc::now(),
        delivered_at: None,
    };
    let err = lineage_drain_event_summary(&malformed_sequence, &receipt)
        .unwrap_err()
        .to_string();
    assert!(err.contains("sequence-numbers must contain unsigned integers"));

    let malformed_commit_hash = OutboxEvent {
        event_id: "evt-bad-summary-commit-hash".to_string(),
        sink: "lakecat.lineage-and-graph".to_string(),
        event_type: "table.commits-listed".to_string(),
        payload: json!({
            "payload": {
                "commit-count": 1,
                "sequence-numbers": [1],
                "commit-hashes": ["sha256:not-full"]
            }
        }),
        created_at: chrono::Utc::now(),
        delivered_at: None,
    };
    let err = lineage_drain_event_summary(&malformed_commit_hash, &receipt)
        .unwrap_err()
        .to_string();
    assert!(err.contains("commit-hashes must contain full SHA-256 digest evidence"));

    let count_hash_drift = OutboxEvent {
        event_id: "evt-drifted-summary-commit-count-hashes".to_string(),
        sink: "lakecat.lineage-and-graph".to_string(),
        event_type: "table.commits-listed".to_string(),
        payload: json!({
            "payload": {
                "commit-count": 2,
                "sequence-numbers": [1, 2],
                "commit-hashes": [content_hash_bytes(b"commit-one")]
            }
        }),
        created_at: chrono::Utc::now(),
        delivered_at: None,
    };
    let err = lineage_drain_event_summary(&count_hash_drift, &receipt)
        .unwrap_err()
        .to_string();
    assert!(err.contains("commit-count must match commit-hashes in lineage drain summary"));

    let sequence_hash_drift = OutboxEvent {
        event_id: "evt-drifted-summary-sequence-hashes".to_string(),
        sink: "lakecat.lineage-and-graph".to_string(),
        event_type: "table.commits-listed".to_string(),
        payload: json!({
            "payload": {
                "sequence-numbers": [1, 2],
                "commit-hashes": [content_hash_bytes(b"commit-one")]
            }
        }),
        created_at: chrono::Utc::now(),
        delivered_at: None,
    };
    let err = lineage_drain_event_summary(&sequence_hash_drift, &receipt)
        .unwrap_err()
        .to_string();
    assert!(err.contains("sequence-numbers must match commit-hashes in lineage drain summary"));

    let zero_sequence_number = OutboxEvent {
        event_id: "evt-zero-summary-commit-sequence".to_string(),
        sink: "lakecat.lineage-and-graph".to_string(),
        event_type: "table.commits-listed".to_string(),
        payload: json!({
            "payload": {
                "commit-count": 1,
                "sequence-numbers": [0],
                "commit-hashes": [content_hash_bytes(b"commit-one")]
            }
        }),
        created_at: chrono::Utc::now(),
        delivered_at: None,
    };
    let err = lineage_drain_event_summary(&zero_sequence_number, &receipt)
        .unwrap_err()
        .to_string();
    assert!(err.contains("sequence-numbers must be positive in lineage drain summary"));

    let repeated_sequence_number = OutboxEvent {
        event_id: "evt-repeated-summary-commit-sequence".to_string(),
        sink: "lakecat.lineage-and-graph".to_string(),
        event_type: "table.commits-listed".to_string(),
        payload: json!({
            "payload": {
                "commit-count": 2,
                "sequence-numbers": [1, 1],
                "commit-hashes": [
                    content_hash_bytes(b"commit-one"),
                    content_hash_bytes(b"commit-two")
                ]
            }
        }),
        created_at: chrono::Utc::now(),
        delivered_at: None,
    };
    let err = lineage_drain_event_summary(&repeated_sequence_number, &receipt)
        .unwrap_err()
        .to_string();
    assert!(err.contains("sequence-numbers must be strictly increasing in lineage drain summary"));

    let principal = Principal::new("agent:reader", PrincipalKind::Agent).unwrap();
    for (event_id, field, value, expected_message) in [
        (
            "evt-missing-principal-summary-commit-history",
            "principal",
            Value::Null,
            "table commit-history evidence must contain authorization receipt principal",
        ),
        (
            "evt-action-drift-summary-commit-history",
            "action",
            json!("table-commit"),
            "table commit-history authorization receipt action does not match outbox event type",
        ),
        (
            "evt-denied-summary-commit-history",
            "allowed",
            json!(false),
            "table commit-history authorization receipt must allow replay projection",
        ),
        (
            "evt-blank-summary-commit-history-engine",
            "engine",
            json!(" "),
            "table commit-history authorization receipt engine must be non-empty",
        ),
        (
            "evt-malformed-summary-commit-history-checked-at",
            "checked_at",
            json!("not-a-timestamp"),
            "table commit-history authorization receipt checked_at timestamp must be RFC3339",
        ),
    ] {
        let mut event = OutboxEvent {
            event_id: event_id.to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "table.commits-listed".to_string(),
            payload: json!({
                "payload": {
                    "event-type": "table.commits-listed",
                    "authorization-receipt": {
                        "principal": principal,
                        "action": "table-load",
                        "allowed": true,
                        "engine": "test",
                        "policy_hash": null,
                        "checked_at": chrono::Utc::now().to_rfc3339(),
                    },
                    "warehouse": "local",
                    "namespace": ["default"],
                    "table": "events",
                    "commit-count": 1,
                    "commit-hashes": [content_hash_bytes(b"commit-one")],
                    "sequence-numbers": [1],
                    "principal-subject": "agent:reader",
                    "principal-kind": "agent",
                }
            }),
            created_at: chrono::Utc::now(),
            delivered_at: None,
        };
        if field == "principal" && value.is_null() {
            event.payload["payload"]["authorization-receipt"]
                .as_object_mut()
                .unwrap()
                .remove(field);
        } else {
            event.payload["payload"]["authorization-receipt"][field] = value;
        }
        let err = lineage_drain_event_summary(&event, &receipt)
            .unwrap_err()
            .to_string();
        assert!(err.contains(expected_message), "{event_id}: {err}");
        assert!(err.contains("event-id-hash=sha256:"), "{event_id}: {err}");
        assert!(!err.contains(event_id), "{event_id}: {err}");
    }
}

#[test]
fn lineage_drain_summary_rejects_unverified_table_operation_fields() {
    let receipt = OutboxProjectionReceipt::default();
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let principal = Principal::new("agent:reader", PrincipalKind::Agent).unwrap();
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
            "table.commits-listed",
            "table commit-history contains unexpected field unverified-history-claim",
            json!({
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
                "commit-hashes": [content_hash_bytes(b"commit-one")],
                "sequence-numbers": [1],
                "principal-subject": "agent:reader",
                "principal-kind": "agent",
                "unverified-history-claim": true,
            }),
        ),
        (
            "table.scan-planned",
            "scan-planned contains unexpected field unverified-scan-plan-claim",
            json!({
                "event-type": "table.scan-planned",
                "table": table,
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
                "scan-task-count": 1,
                "storage-location": "s3://bucket/events",
                "metadata-location": "s3://bucket/events/metadata/v1.json",
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
                "unverified-scan-plan-claim": true,
            }),
        ),
        (
            "table.scan-tasks-fetched",
            "scan-tasks-fetched contains unexpected field unverified-scan-fetch-claim",
            json!({
                "event-type": "table.scan-tasks-fetched",
                "table": table,
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
                "file-scan-task-count": 1,
                "delete-file-count": 0,
                "child-plan-task-count": 0,
                "storage-location": "s3://bucket/events",
                "metadata-location": "s3://bucket/events/metadata/v1.json",
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
                "unverified-scan-fetch-claim": true,
            }),
        ),
    ];

    for (event_type, expected_message, payload) in cases {
        let event = OutboxEvent {
            event_id: format!("evt-summary-{event_type}-extra-field"),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: event_type.to_string(),
            payload: json!({
                "audit-event-id": format!("audit-summary-{event_type}-extra-field"),
                "event-type": event_type,
                "table": table,
                "payload": payload,
            }),
            created_at: chrono::Utc::now(),
            delivered_at: None,
        };

        let err = lineage_drain_event_summary(&event, &receipt)
            .unwrap_err()
            .to_string();
        assert!(err.contains(expected_message), "{event_type}: {err}");
        assert!(err.contains("event-id-hash=sha256:"), "{event_type}: {err}");
        assert!(
            !err.contains(event.event_id.as_str()),
            "{event_type}: {err}"
        );
    }
}

#[test]
fn lineage_drain_summary_rejects_malformed_table_lifecycle_evidence() {
    let receipt = OutboxProjectionReceipt::default();
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let principal = Principal::new("agent:writer", PrincipalKind::Agent).unwrap();
    let authorization_receipt = |action: &str| {
        json!({
            "principal": principal,
            "action": action,
            "allowed": true,
            "engine": "lakecat-test",
            "policy_hash": null,
            "checked_at": chrono::Utc::now().to_rfc3339()
        })
    };
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
            "evt-summary-created-extra-metadata-graph",
            "table.created",
            "table lifecycle metadata-graph contains unexpected field unverified-metadata-graph-claim",
            json!({
                "event-type": "table.created",
                "table": &table,
                "authorization-receipt": authorization_receipt("table-create"),
                "metadata-location": "file:///tmp/events/metadata/00000.json",
                "format-version": 3,
                "version": 1,
                "metadata-graph": metadata_graph_with_extra,
            }),
        ),
        (
            "evt-summary-deleted-duplicate-soft-delete-format-version",
            "table.deleted",
            "table lifecycle soft-delete must not carry both format-version and format_version evidence fields",
            json!({
                "event-type": "table.deleted",
                "table": &table,
                "authorization-receipt": authorization_receipt("table-drop"),
                "soft-delete": {
                    "table": &table,
                    "metadata-location": "file:///tmp/events/metadata/00000.json",
                    "version": 1,
                    "format-version": 3,
                    "format_version": 3,
                }
            }),
        ),
    ];

    for (event_id, event_type, expected_message, payload) in cases {
        let event = OutboxEvent {
            event_id: event_id.to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: event_type.to_string(),
            payload: json!({
                "audit-event-id": format!("audit-envelope-{event_id}"),
                "event-type": event_type,
                "table": &table,
                "payload": payload
            }),
            created_at: chrono::Utc::now(),
            delivered_at: None,
        };
        let err = lineage_drain_event_summary(&event, &receipt)
            .unwrap_err()
            .to_string();
        assert!(err.contains(event_type), "{event_id}: {err}");
        assert!(err.contains(expected_message), "{event_id}: {err}");
        assert!(err.contains("event-id-hash=sha256:"), "{event_id}: {err}");
        assert!(!err.contains(event_id), "{event_id}: {err}");
    }
}

#[test]
fn lineage_drain_summary_rejects_malformed_credential_prefix_hashes() {
    let receipt = OutboxProjectionReceipt::default();
    let mut non_array_credential_evidence =
        valid_lineage_summary_credential_event("evt-bad-summary-credential-evidence");
    non_array_credential_evidence.payload["payload"]["credential-response-evidence"] =
        json!({ "prefix-hash": content_hash_bytes(b"credential") });
    let err = lineage_drain_event_summary(&non_array_credential_evidence, &receipt)
        .unwrap_err()
        .to_string();
    assert!(err.contains("credential-response-evidence"), "{err}");

    let mut extra_credential_response_field =
        valid_lineage_summary_credential_event("evt-extra-summary-credential-response-field");
    extra_credential_response_field.payload["payload"]["credential-response-evidence"][0]["unverified-credential-scope"] =
        json!("all-objects");
    let err = lineage_drain_event_summary(&extra_credential_response_field, &receipt)
        .unwrap_err()
        .to_string();
    assert!(
        err.contains(
            "credential-vend credential-response contains unexpected field unverified-credential-scope"
        ),
        "{err}"
    );
    assert!(
        err.contains("event-id-hash=sha256:"),
        "operator-facing summary errors should keep event identity redacted: {err}"
    );
    assert!(
        !err.contains("evt-extra-summary-credential-response-field"),
        "operator-facing summary errors must not expose raw event ids: {err}"
    );

    let mut missing_prefix_hash =
        valid_lineage_summary_credential_event("evt-missing-summary-prefix-hash");
    missing_prefix_hash.payload["payload"]["credential-response-evidence"][0]
        .as_object_mut()
        .unwrap()
        .remove("prefix-hash");
    let err = lineage_drain_event_summary(&missing_prefix_hash, &receipt)
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("prefix-hash must contain full SHA-256 digest evidence"),
        "{err}"
    );

    let mut malformed_prefix_hash =
        valid_lineage_summary_credential_event("evt-bad-summary-prefix-hash");
    malformed_prefix_hash.payload["payload"]["credential-response-evidence"][0]["prefix-hash"] =
        json!("sha256:not-full");
    let err = lineage_drain_event_summary(&malformed_prefix_hash, &receipt)
        .unwrap_err()
        .to_string();
    assert!(err.contains("prefix-hash must contain full SHA-256 digest evidence"));

    let mut zero_issuer_config_hash_drift =
        valid_lineage_summary_credential_event("evt-bad-summary-zero-issuer-config-hash");
    zero_issuer_config_hash_drift.event_type = "credentials.summary-only".to_string();
    zero_issuer_config_hash_drift.payload["event-type"] = json!("credentials.summary-only");
    zero_issuer_config_hash_drift.payload["payload"]["event-type"] =
        json!("credentials.summary-only");
    zero_issuer_config_hash_drift.payload["payload"]["credential-response-evidence"][0]["issuer-config-entry-count"] =
        json!(0);
    zero_issuer_config_hash_drift.payload["payload"]["credential-response-evidence"][0]["issuer-config-hash"] =
        json!(content_hash_bytes(b"not-empty-issuer-config"));
    let err = lineage_drain_event_summary(&zero_issuer_config_hash_drift, &receipt)
        .unwrap_err()
        .to_string();
    assert!(
        err.contains(
            "credential-vend credential-response issuer-config-hash must match catalog evidence"
        ),
        "{err}"
    );

    let drift_cases = [
        (
            "evt-summary-credential-response-profile-drift",
            "storage-profile-id",
            json!("forged-profile"),
            "credential-vend credential-response storage-profile-id must match catalog evidence",
        ),
        (
            "evt-summary-credential-response-principal-drift",
            "authorization-principal",
            json!("agent:forged"),
            "credential-vend credential-response authorization-principal must match catalog evidence",
        ),
        (
            "evt-summary-credential-response-governed-drift",
            "governed-read-required",
            json!("true"),
            "credential-vend credential-response governed-read-required must match catalog evidence",
        ),
        (
            "evt-summary-credential-response-ttl-drift",
            "max-credential-ttl-seconds",
            json!("300"),
            "credential-vend credential-response max-credential-ttl-seconds must be absent when not authorized by receipt evidence",
        ),
    ];
    for (event_id, field, value, expected_message) in drift_cases {
        let mut event = valid_lineage_summary_credential_event(event_id);
        event.event_type = "credentials.summary-only".to_string();
        event.payload["event-type"] = json!("credentials.summary-only");
        event.payload["payload"]["event-type"] = json!("credentials.summary-only");
        event.payload["payload"]["credential-response-evidence"][0][field] = value;
        let err = lineage_drain_event_summary(&event, &receipt)
            .unwrap_err()
            .to_string();
        assert!(err.contains(expected_message), "{err}");
    }

    let duplicate_prefix_hash = content_hash_bytes(b"duplicate-credential-prefix");
    let mut duplicate_prefix_hashes =
        valid_lineage_summary_credential_event("evt-duplicate-summary-prefix-hash");
    let mut duplicate_entry =
        duplicate_prefix_hashes.payload["payload"]["credential-response-evidence"][0].clone();
    duplicate_entry["prefix-hash"] = json!(duplicate_prefix_hash);
    duplicate_prefix_hashes.payload["payload"]["credential-response-evidence"][0]["prefix-hash"] =
        json!(duplicate_prefix_hash);
    duplicate_prefix_hashes.payload["payload"]["credential-response-evidence"] =
        json!([duplicate_entry.clone(), duplicate_entry]);
    duplicate_prefix_hashes.payload["payload"]["credential-count"] = json!(2);
    let err = lineage_drain_event_summary(&duplicate_prefix_hashes, &receipt)
        .unwrap_err()
        .to_string();
    assert!(
        err.contains(
            "credential-vend credential-response-evidence must not contain duplicate prefix-hash values"
        ),
        "{err}"
    );

    let mut count_without_prefix_hashes =
        valid_lineage_summary_credential_event("evt-summary-credential-count-without-prefixes");
    count_without_prefix_hashes.payload["payload"]["credential-response-evidence"] = json!([]);
    let err = lineage_drain_event_summary(&count_without_prefix_hashes, &receipt)
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("credential-count does not match credential-response-evidence"),
        "{err}"
    );

    let mut count_drift_prefix_hashes =
        valid_lineage_summary_credential_event("evt-summary-credential-count-drift-prefixes");
    count_drift_prefix_hashes.payload["payload"]["credential-count"] = json!(2);
    let err = lineage_drain_event_summary(&count_drift_prefix_hashes, &receipt)
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("credential-count does not match credential-response-evidence"),
        "{err}"
    );

    let mut zero_count_with_prefix_hash =
        valid_lineage_summary_credential_event("evt-summary-zero-credential-count-with-prefix");
    zero_count_with_prefix_hash.payload["payload"]["credential-count"] = json!(0);
    let err = lineage_drain_event_summary(&zero_count_with_prefix_hash, &receipt)
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("credential-count does not match credential-response-evidence"),
        "{err}"
    );
}

#[test]
fn lineage_drain_summary_rejects_malformed_raw_credential_exception() {
    let receipt = OutboxProjectionReceipt::default();
    let cases = [
        (
            "evt-bad-summary-raw-exception-shape",
            json!({
                "credential-count": 0,
                "lakecat:raw-credential-exception": true
            }),
            "raw-credential exception must be an object",
        ),
        (
            "evt-bad-summary-raw-exception-allowed",
            json!({
                "credential-count": 0,
                "lakecat:credential-block-reason": "fine-grained read restriction requires Sail-planned reads",
                "lakecat:raw-credential-exception": {
                    "allowed": "false",
                    "reason": "fine-grained read restriction requires Sail-planned reads"
                }
            }),
            "raw-credential exception allowed must be boolean",
        ),
        (
            "evt-extra-summary-raw-exception-field",
            json!({
                "credential-count": 0,
                "lakecat:credential-block-reason": "fine-grained read restriction requires Sail-planned reads",
                "lakecat:raw-credential-exception": {
                    "allowed": false,
                    "reason": "fine-grained read restriction requires Sail-planned reads",
                    "unverified-raw-credential-claim": "credential-safe"
                }
            }),
            "lakecat:raw-credential-exception contains unexpected field unverified-raw-credential-claim",
        ),
        (
            "evt-bad-summary-raw-exception-missing-reason",
            json!({
                "credential-count": 0,
                "lakecat:credential-block-reason": "fine-grained read restriction requires Sail-planned reads",
                "lakecat:raw-credential-exception": {
                    "allowed": false
                }
            }),
            "blocked raw-credential exception must carry non-empty reason",
        ),
        (
            "evt-bad-summary-raw-exception-blank-reason",
            json!({
                "credential-count": 0,
                "lakecat:credential-block-reason": "fine-grained read restriction requires Sail-planned reads",
                "lakecat:raw-credential-exception": {
                    "allowed": false,
                    "reason": " "
                }
            }),
            "blocked raw-credential exception must carry non-empty reason",
        ),
        (
            "evt-bad-summary-credential-block-reason",
            json!({
                "credential-count": 0,
                "lakecat:credential-block-reason": " ",
                "lakecat:raw-credential-exception": {
                    "allowed": false,
                    "reason": "fine-grained read restriction requires Sail-planned reads"
                }
            }),
            "blocked credential evidence must contain block reason",
        ),
        (
            "evt-bad-summary-block-reason-drift",
            json!({
                "credential-count": 0,
                "lakecat:credential-block-reason": "another reason",
                "lakecat:raw-credential-exception": {
                    "allowed": false,
                    "reason": "fine-grained read restriction requires Sail-planned reads"
                }
            }),
            "block reason must match raw-credential exception reason",
        ),
        (
            "evt-bad-summary-blocked-with-credentials",
            json!({
                "credential-count": 1,
                "lakecat:credential-block-reason": "fine-grained read restriction requires Sail-planned reads",
                "lakecat:raw-credential-exception": {
                    "allowed": false,
                    "reason": "fine-grained read restriction requires Sail-planned reads"
                }
            }),
            "blocked credential evidence must not carry credentials",
        ),
        (
            "evt-bad-summary-allowed-with-block-reason",
            json!({
                "credential-count": 1,
                "lakecat:credential-block-reason": "fine-grained read restriction requires Sail-planned reads",
                "lakecat:raw-credential-exception": {
                    "allowed": true,
                    "reason": "trusted human principal may use audited raw credential vending"
                }
            }),
            "block reason must be absent when raw credentials are allowed",
        ),
    ];

    for (event_id, payload, expected_message) in cases {
        let mut event = valid_lineage_summary_credential_event(event_id);
        if payload.get("credential-count").and_then(Value::as_u64) == Some(0) {
            event.payload["payload"]["credential-response-evidence"] = json!([]);
        }
        for (key, value) in payload.as_object().unwrap() {
            event.payload["payload"][key] = value.clone();
        }
        if let Some(raw_exception) = payload.get("lakecat:raw-credential-exception") {
            event.payload["payload"]["authorization-receipt"]["context"]["lakecat:raw-credential-exception"] =
                raw_exception.clone();
        }
        if event_id == "evt-extra-summary-raw-exception-field" {
            event.event_type = "credentials.summary-only".to_string();
            event.payload["event-type"] = json!("credentials.summary-only");
            event.payload["payload"]["event-type"] = json!("credentials.summary-only");
        }
        let err = lineage_drain_event_summary(&event, &receipt)
            .unwrap_err()
            .to_string();
        assert!(err.contains(expected_message), "{err}");
    }

    let raw_exception = json!({
        "allowed": false,
        "reason": "fine-grained read restriction requires Sail-planned reads"
    });
    let drifted_raw_exception = json!({
        "allowed": true,
        "reason": "trusted human principal may use audited raw credential vending"
    });
    let receipt_cases = [
        (
            "evt-summary-raw-exception-missing-receipt-context",
            Some(raw_exception.clone()),
            None,
            "raw-credential exception must be captured in authorization receipt context",
        ),
        (
            "evt-summary-raw-exception-receipt-only",
            None,
            Some(raw_exception.clone()),
            "authorization receipt raw-credential exception must match top-level evidence",
        ),
        (
            "evt-summary-raw-exception-receipt-drift",
            Some(raw_exception.clone()),
            Some(drifted_raw_exception),
            "raw-credential exception must match authorization receipt context",
        ),
    ];

    for (event_id, top_level_exception, receipt_exception, expected_message) in receipt_cases {
        let mut event = valid_lineage_summary_credential_event(event_id);
        event.event_type = "credentials.summary-only".to_string();
        event.payload["event-type"] = json!("credentials.summary-only");
        event.payload["payload"]["event-type"] = json!("credentials.summary-only");
        event.payload["payload"]["credential-count"] = json!(0);
        event.payload["payload"]["credential-response-evidence"] = json!([]);
        event.payload["payload"]["lakecat:credential-block-reason"] =
            json!("fine-grained read restriction requires Sail-planned reads");
        event.payload["payload"]["authorization-receipt"]["context"] = json!({});
        if let Some(raw_exception) = top_level_exception {
            event.payload["payload"]["lakecat:raw-credential-exception"] = raw_exception.clone();
        } else {
            event.payload["payload"]
                .as_object_mut()
                .unwrap()
                .remove("lakecat:raw-credential-exception");
        }
        if let Some(raw_exception) = receipt_exception {
            event.payload["payload"]["authorization-receipt"]["context"]["lakecat:raw-credential-exception"] =
                raw_exception;
        } else {
            event.payload["payload"]["authorization-receipt"]["context"]
                .as_object_mut()
                .unwrap()
                .remove("lakecat:raw-credential-exception");
        }

        let err = lineage_drain_event_summary(&event, &receipt)
            .unwrap_err()
            .to_string();
        assert!(err.contains(expected_message), "{err}");
    }
}

#[test]
fn lineage_drain_summary_rejects_malformed_storage_profile_secret_ref_evidence() {
    let receipt = OutboxProjectionReceipt::default();
    let valid_location_hash = content_hash_bytes(b"storage-profile-prefix");
    let valid_secret_hash = content_hash_bytes(b"storage-profile-secret-ref");

    let mut raw_secret_ref =
        valid_lineage_summary_credential_event("evt-summary-storage-profile-raw-secret-ref");
    raw_secret_ref.payload["payload"]["storage-profile"] = json!({
        "profile-id": "s3-events",
        "warehouse": "local",
        "provider": "s3",
        "issuance-mode": "short-lived-secret-ref",
        "location-prefix-hash": valid_location_hash,
        "secret-ref-present": true,
        "secret-ref-provider": "typesec",
        "secret-ref-hash": valid_secret_hash,
        "secret-ref": "typesec://env/LAKECAT_S3_EVENTS"
    });
    let err = lineage_drain_event_summary(&raw_secret_ref, &receipt)
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("storage-profile contains unexpected field secret-ref"),
        "{err}"
    );

    let mut short_location_hash = valid_lineage_summary_storage_profile_upsert_event(
        "evt-summary-storage-profile-short-prefix",
    );
    short_location_hash.payload["payload"]["storage-profile"]["location-prefix-hash"] =
        json!("sha256:short");
    let err = lineage_drain_event_summary(&short_location_hash, &receipt)
        .unwrap_err()
        .to_string();
    assert!(err.contains("location-prefix-hash must contain full SHA-256 digest evidence"));

    let mut reserved_public_config_key = valid_lineage_summary_storage_profile_upsert_event(
        "evt-summary-storage-profile-reserved-public-config",
    );
    reserved_public_config_key.payload["payload"]["storage-profile"]["location-prefix-hash"] =
        json!(valid_location_hash);
    reserved_public_config_key.payload["payload"]["storage-profile"]["secret-ref-hash"] =
        json!(valid_secret_hash);
    reserved_public_config_key.payload["payload"]["storage-profile"]["public-config"] = json!({
        "lakecat.storage-profile-id": "shadow-profile"
    });
    let err = lineage_drain_event_summary(&reserved_public_config_key, &receipt)
        .unwrap_err()
        .to_string();
    assert!(
        err.contains(
            "storage-profile upsert storage-profile public-config key is reserved for LakeCat credential evidence"
        ),
        "{err}"
    );
    assert!(err.contains("public-config-key-hash=sha256:"));
    assert!(!err.contains("lakecat.storage-profile-id"));
    assert!(!err.contains("shadow-profile"));

    let mut secret_like_public_config_value =
        valid_lineage_summary_credential_event("evt-summary-storage-profile-secret-public-config");
    secret_like_public_config_value.payload["payload"]["storage-profile"] = json!({
        "profile-id": "s3-events",
        "warehouse": "local",
        "provider": "s3",
        "issuance-mode": "short-lived-secret-ref",
        "location-prefix-hash": valid_location_hash,
        "secret-ref-present": true,
        "secret-ref-provider": "typesec",
        "secret-ref-hash": valid_secret_hash,
        "public-config": {
            "region": "password=super-secret"
        }
    });
    let err = lineage_drain_event_summary(&secret_like_public_config_value, &receipt)
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("storage-profile public-config value may expose secret material"),
        "{err}"
    );
    assert!(err.contains("public-config-key-hash=sha256:"));
    assert!(!err.contains("password=super-secret"));

    let mut secret_like_public_config_key =
        valid_lineage_summary_credential_event("evt-summary-credential-secret-public-config-key");
    secret_like_public_config_key.payload["payload"]["storage-profile"]["public-config"] = json!({
        "access-token": "redacted"
    });
    let err = lineage_drain_event_summary(&secret_like_public_config_key, &receipt)
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("storage-profile public-config key may expose secret material"),
        "{err}"
    );
    assert!(err.contains("public-config-key-hash=sha256:"));
    assert!(!err.contains("access-token"));
    assert!(!err.contains("redacted"));

    let mut non_string_public_config_value = valid_lineage_summary_credential_event(
        "evt-summary-credential-non-string-public-config-value",
    );
    non_string_public_config_value.payload["payload"]["storage-profile"]["public-config"] = json!({
        "region": ["us-west-2"]
    });
    let err = lineage_drain_event_summary(&non_string_public_config_value, &receipt)
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("storage-profile public-config value must be a string"),
        "{err}"
    );
    assert!(err.contains("public-config-key-hash=sha256:"));
    assert!(!err.contains("us-west-2"));

    let mut missing_secret_ref_provider = valid_lineage_summary_credential_event(
        "evt-summary-storage-profile-missing-secret-provider",
    );
    missing_secret_ref_provider.payload["payload"]["storage-profile"] = json!({
        "profile-id": "s3-events",
        "warehouse": "local",
        "provider": "s3",
        "issuance-mode": "short-lived-secret-ref",
        "location-prefix-hash": valid_location_hash,
        "secret-ref-present": true,
        "secret-ref-hash": valid_secret_hash
    });
    let err = lineage_drain_event_summary(&missing_secret_ref_provider, &receipt)
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("storage-profile secret-ref-present requires secret-ref-provider"),
        "{err}"
    );

    let mut unexpected_secret_ref_hash = valid_lineage_summary_credential_event(
        "evt-summary-storage-profile-unexpected-secret-hash",
    );
    unexpected_secret_ref_hash.payload["payload"]["storage-profile"] = json!({
        "profile-id": "local-events",
        "warehouse": "local",
        "provider": "file",
        "issuance-mode": "local-file-no-secret",
        "location-prefix-hash": valid_location_hash,
        "secret-ref-present": false,
        "secret-ref-hash": valid_secret_hash
    });
    let err = lineage_drain_event_summary(&unexpected_secret_ref_hash, &receipt)
        .unwrap_err()
        .to_string();
    assert!(
        err.contains(
            "storage-profile cannot carry secret-ref evidence when secret-ref-present is false"
        ),
        "{err}"
    );

    let mut invalid_secret_ref_mode = valid_lineage_summary_storage_profile_upsert_event(
        "evt-summary-storage-profile-invalid-secret-mode",
    );
    invalid_secret_ref_mode.payload["payload"]["storage-profile"]["profile-id"] =
        json!("file-secret");
    invalid_secret_ref_mode.payload["payload"]["storage-profile"]["provider"] = json!("file");
    invalid_secret_ref_mode.payload["payload"]["storage-profile"]["location-prefix-hash"] =
        json!(valid_location_hash);
    invalid_secret_ref_mode.payload["payload"]["storage-profile"]["secret-ref-hash"] =
        json!(valid_secret_hash);
    let err = lineage_drain_event_summary(&invalid_secret_ref_mode, &receipt)
        .unwrap_err()
        .to_string();
    assert!(
        err.contains(
            "storage-profile upsert short-lived-secret-ref issuance mode requires cloud object provider"
        ),
        "{err}"
    );

    let secret_ref_presence_cases = [
        (
            "evt-summary-credential-missing-secret-ref-present",
            None,
            "lineage drain credential summary must contain secret-ref-present",
        ),
        (
            "evt-summary-credential-malformed-secret-ref-present",
            Some(json!("false")),
            "lineage drain credential summary secret-ref-present must be a boolean",
        ),
        (
            "evt-summary-credential-secret-ref-present-drift",
            Some(json!(false)),
            "lineage drain credential summary secret-ref-present must match catalog evidence",
        ),
    ];
    for (event_id, top_level_secret_ref_present, expected_message) in secret_ref_presence_cases {
        let mut event = valid_lineage_summary_credential_event(event_id);
        event.event_type = "credentials.summary-only".to_string();
        event.payload["event-type"] = json!("credentials.summary-only");
        event.payload["payload"]["event-type"] = json!("credentials.summary-only");
        event.payload["payload"]["storage-profile"] = json!({
            "profile-id": "s3-events",
            "warehouse": "local",
            "provider": "s3",
            "issuance-mode": "short-lived-secret-ref",
            "location-prefix-hash": valid_location_hash,
            "secret-ref-present": true,
            "secret-ref-provider": "typesec",
            "secret-ref-hash": valid_secret_hash
        });
        event.payload["payload"]["storage-profile-id"] = json!("s3-events");
        event.payload["payload"]["mode"] = json!("short-lived-secret-ref");
        event.payload["payload"]["credential-response-evidence"][0]["storage-profile-id"] =
            json!("s3-events");
        event.payload["payload"]["credential-response-evidence"][0]["catalog-profile-id"] =
            json!("s3-events");
        event.payload["payload"]["credential-response-evidence"][0]["storage-provider"] =
            json!("s3");
        event.payload["payload"]["credential-response-evidence"][0]["credential-mode"] =
            json!("short-lived-secret-ref");
        event.payload["payload"]["credential-response-evidence"][0]["secret-ref-provider"] =
            json!("typesec");
        event.payload["payload"]["credential-response-evidence"][0]["secret-ref-hash"] =
            json!(valid_secret_hash);
        match top_level_secret_ref_present {
            Some(value) => event.payload["payload"]["secret-ref-present"] = value,
            None => {
                event.payload["payload"]
                    .as_object_mut()
                    .unwrap()
                    .remove("secret-ref-present");
            }
        }
        let err = lineage_drain_event_summary(&event, &receipt)
            .unwrap_err()
            .to_string();
        assert!(err.contains(expected_message), "{err}");
    }
}

#[test]
fn lineage_drain_summary_rejects_malformed_querygraph_standards() {
    let receipt = OutboxProjectionReceipt::default();
    let mut malformed_standards =
        valid_lineage_summary_querygraph_bootstrap_event("evt-bad-summary-standards");
    malformed_standards.payload["payload"]["standards"] = json!(["Iceberg REST", 42]);
    let err = lineage_drain_event_summary(&malformed_standards, &receipt)
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("querygraph bootstrap standards must contain non-empty strings"),
        "{err}"
    );

    let mut blank_standard =
        valid_lineage_summary_querygraph_bootstrap_event("evt-blank-summary-standard");
    blank_standard.payload["payload"]["standards"] = json!(["Iceberg REST", " "]);
    let err = lineage_drain_event_summary(&blank_standard, &receipt)
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("querygraph bootstrap standards must contain non-empty strings"),
        "{err}"
    );

    let mut duplicate_standard =
        valid_lineage_summary_querygraph_bootstrap_event("evt-duplicate-summary-standard");
    duplicate_standard.payload["payload"]["standards"] = json!(["Iceberg REST", "Iceberg REST"]);
    let err = lineage_drain_event_summary(&duplicate_standard, &receipt)
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("querygraph bootstrap standards must be duplicate-free"),
        "{err}"
    );
}

#[test]
fn lineage_drain_summary_rejects_malformed_querygraph_hashes() {
    let receipt = OutboxProjectionReceipt::default();
    let cases = [
        (
            "evt-bad-summary-bundle-hash",
            json!({ "bundle-hash": "sha256:not-full" }),
            "bundle-hash must contain full SHA-256 digest evidence",
        ),
        (
            "evt-bad-summary-graph-hash",
            json!({ "graph-hash": 42 }),
            "graph-hash must contain full SHA-256 digest evidence",
        ),
        (
            "evt-bad-summary-open-lineage-hash",
            json!({ "open-lineage-hash": "sha256:not-full" }),
            "open-lineage-hash must contain full SHA-256 digest evidence",
        ),
        (
            "evt-bad-summary-querygraph-import-hash",
            json!({ "querygraph-import-hash": "sha256:not-full" }),
            "querygraph-import-hash must contain full SHA-256 digest evidence",
        ),
    ];

    for (event_id, payload, expected_message) in cases {
        let mut event = valid_lineage_summary_querygraph_bootstrap_event(event_id);
        let field = payload
            .as_object()
            .and_then(|fields| fields.iter().next())
            .expect("single malformed hash field");
        event.payload["payload"][field.0] = field.1.clone();
        let err = lineage_drain_event_summary(&event, &receipt)
            .unwrap_err()
            .to_string();
        assert!(err.contains(expected_message), "{err}");
    }
}

#[test]
fn lineage_drain_summary_rejects_malformed_querygraph_artifact_arrays() {
    let receipt = OutboxProjectionReceipt::default();
    let mut malformed_table_artifacts =
        valid_lineage_summary_querygraph_bootstrap_event("evt-bad-summary-table-artifacts");
    malformed_table_artifacts.payload["payload"]["table-artifacts"] = json!({
        "stable-id": "lakecat:table:local:default.events"
    });
    let err = lineage_drain_event_summary(&malformed_table_artifacts, &receipt)
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("querygraph bootstrap evidence must contain table-artifacts"),
        "{err}"
    );

    let mut malformed_view_artifacts =
        valid_lineage_summary_querygraph_bootstrap_event("evt-bad-summary-view-artifacts");
    malformed_view_artifacts.payload["payload"]["view-artifacts"] = json!({
        "stable-id": "lakecat:view:local:default.events_view"
    });
    let err = lineage_drain_event_summary(&malformed_view_artifacts, &receipt)
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("querygraph bootstrap evidence must contain view-artifacts"),
        "{err}"
    );
}

#[test]
fn lineage_drain_summary_rejects_malformed_management_ids() {
    let receipt = OutboxProjectionReceipt::default();
    let cases = [
        (
            "evt-bad-summary-project-ids",
            "project.listed",
            "project-ids",
            json!([42]),
            "project list project-ids must contain non-empty strings",
        ),
        (
            "evt-blank-summary-server-id",
            "server.listed",
            "server-ids",
            json!([" "]),
            "server list server-ids must contain non-empty strings",
        ),
        (
            "evt-duplicate-summary-warehouse-name",
            "warehouse.listed",
            "warehouse-names",
            json!(["local", "local"]),
            "warehouse list warehouse-names must not contain duplicate identifiers",
        ),
        (
            "evt-duplicate-summary-policy-id",
            "policy-binding.listed",
            "policy-ids",
            json!(["agent-read", "agent-read"]),
            "policy-binding list policy-ids must not contain duplicate identifiers",
        ),
        (
            "evt-bad-summary-storage-profile-ids",
            "storage-profile.listed",
            "storage-profile-ids",
            json!({"profile-id": "local-file"}),
            "storage-profile list storage-profile-ids must be an array when present",
        ),
        (
            "evt-count-drift-summary-project-ids",
            "project.listed",
            "project-ids",
            json!(["analytics", "experiments"]),
            "project list project-ids count must match project list count",
        ),
        (
            "evt-count-drift-summary-server-ids",
            "server.listed",
            "server-ids",
            json!(["primary", "backup"]),
            "server list server-ids count must match server list count",
        ),
        (
            "evt-count-drift-summary-warehouse-names",
            "warehouse.listed",
            "warehouse-names",
            json!(["local", "warehouse2"]),
            "warehouse list warehouse-names count must match warehouse list count",
        ),
        (
            "evt-count-drift-summary-storage-profile-ids",
            "storage-profile.listed",
            "storage-profile-ids",
            json!(["local-file", "s3-events"]),
            "storage-profile list storage-profile-ids count must match storage-profile list count",
        ),
        (
            "evt-count-drift-summary-policy-ids",
            "policy-binding.listed",
            "policy-ids",
            json!(["agent-read", "agent-write"]),
            "policy-binding list policy-ids count must match policy-binding list count",
        ),
        (
            "evt-malformed-summary-project-count",
            "project.listed",
            "project-count",
            json!("2"),
            "project list evidence must contain unsigned project-count",
        ),
        (
            "evt-malformed-summary-server-count",
            "server.listed",
            "server-count",
            json!(-1),
            "server list evidence must contain unsigned server-count",
        ),
        (
            "evt-malformed-summary-warehouse-count",
            "warehouse.listed",
            "warehouse-count",
            json!("3"),
            "warehouse list evidence must contain unsigned warehouse-count",
        ),
        (
            "evt-malformed-summary-storage-profile-count",
            "storage-profile.listed",
            "storage-profile-count",
            json!("1"),
            "storage-profile list evidence must contain unsigned storage-profile-count",
        ),
        (
            "evt-malformed-summary-policy-count",
            "policy-binding.listed",
            "policy-count",
            json!("1"),
            "policy-binding list evidence must contain unsigned policy-count",
        ),
    ];

    for (event_id, event_type, field, value, expected_message) in cases {
        let mut event = valid_lineage_summary_management_list_event(event_id, event_type);
        event.payload["payload"][field] = value;
        if event_id.contains("duplicate-summary-warehouse-name") {
            event.payload["payload"]["warehouse-count"] = json!(2);
        }
        if event_id.contains("duplicate-summary-policy-id") {
            event.payload["payload"]["policy-count"] = json!(2);
        }
        let err = lineage_drain_event_summary(&event, &receipt)
            .unwrap_err()
            .to_string();
        assert!(err.contains(expected_message), "{err}");
    }

    let mut missing_ids = valid_lineage_summary_management_list_event(
        "evt-missing-storage-profile-ids-summary-list",
        "storage-profile.listed",
    );
    missing_ids.payload["payload"]
        .as_object_mut()
        .unwrap()
        .remove("storage-profile-ids");
    let err = lineage_drain_event_summary(&missing_ids, &receipt)
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("storage-profile list evidence must contain storage-profile-ids"),
        "{err}"
    );
    assert!(err.contains("event-id-hash=sha256:"), "{err}");
    assert!(
        !err.contains("evt-missing-storage-profile-ids-summary-list"),
        "{err}"
    );

    let mut invalid_id = valid_lineage_summary_management_list_event(
        "evt-invalid-project-id-summary-list",
        "project.listed",
    );
    invalid_id.payload["payload"]["project-ids"] = json!(["analytics/../../secret"]);
    let err = lineage_drain_event_summary(&invalid_id, &receipt)
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("project list project-ids contains an invalid identifier"),
        "{err}"
    );
    assert!(err.contains("project-id-hash=sha256:"), "{err}");
    assert!(err.contains("event-id-hash=sha256:"), "{err}");
    assert!(
        !err.contains("evt-invalid-project-id-summary-list"),
        "{err}"
    );
    assert!(!err.contains("analytics/../../secret"), "{err}");

    let mut extra_field = valid_lineage_summary_management_list_event(
        "evt-extra-field-summary-server-list",
        "server.listed",
    );
    extra_field.payload["payload"]["querygraph"] = json!({"claim": "shadow"});
    let err = lineage_drain_event_summary(&extra_field, &receipt)
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("server list contains unexpected field querygraph"),
        "{err}"
    );
    assert!(err.contains("event-id-hash=sha256:"), "{err}");
    assert!(
        !err.contains("evt-extra-field-summary-server-list"),
        "{err}"
    );

    let mut action_drift = valid_lineage_summary_management_list_event(
        "evt-action-drift-summary-server-list",
        "server.listed",
    );
    action_drift.payload["payload"]["authorization-receipt"]["action"] = json!("table-load");
    let err = lineage_drain_event_summary(&action_drift, &receipt)
        .unwrap_err()
        .to_string();
    assert!(
        err.contains(
            "management-list authorization receipt action does not match outbox event type"
        ),
        "{err}"
    );
    assert!(err.contains("event-id-hash=sha256:"), "{err}");
    assert!(
        !err.contains("evt-action-drift-summary-server-list"),
        "{err}"
    );

    let mut blank_engine = valid_lineage_summary_management_list_event(
        "evt-blank-engine-summary-server-list",
        "server.listed",
    );
    blank_engine.payload["payload"]["authorization-receipt"]["engine"] = json!(" ");
    let err = lineage_drain_event_summary(&blank_engine, &receipt)
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("management-list authorization receipt engine must be non-empty"),
        "{err}"
    );
    assert!(err.contains("event-id-hash=sha256:"), "{err}");
    assert!(
        !err.contains("evt-blank-engine-summary-server-list"),
        "{err}"
    );

    let mut malformed_checked_at = valid_lineage_summary_management_list_event(
        "evt-malformed-checked-at-summary-server-list",
        "server.listed",
    );
    malformed_checked_at.payload["payload"]["authorization-receipt"]["checked_at"] =
        json!("not-a-timestamp");
    let err = lineage_drain_event_summary(&malformed_checked_at, &receipt)
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("management-list authorization receipt checked_at timestamp must be RFC3339"),
        "{err}"
    );
    assert!(err.contains("event-id-hash=sha256:"), "{err}");
    assert!(
        !err.contains("evt-malformed-checked-at-summary-server-list"),
        "{err}"
    );

    let mut missing_principal = valid_lineage_summary_management_list_event(
        "evt-missing-principal-summary-server-list",
        "server.listed",
    );
    missing_principal.payload["payload"]["authorization-receipt"]
        .as_object_mut()
        .unwrap()
        .remove("principal");
    let err = lineage_drain_event_summary(&missing_principal, &receipt)
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("management-list evidence must contain authorization receipt principal"),
        "{err}"
    );
    assert!(err.contains("event-id-hash=sha256:"), "{err}");
    assert!(
        !err.contains("evt-missing-principal-summary-server-list"),
        "{err}"
    );

    let mut malformed_principal = valid_lineage_summary_management_list_event(
        "evt-malformed-principal-summary-server-list",
        "server.listed",
    );
    malformed_principal.payload["payload"]["authorization-receipt"]["principal"] = json!({
        "subject": "agent:operator",
        "kind": "unknown"
    });
    let err = lineage_drain_event_summary(&malformed_principal, &receipt)
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("management-list authorization receipt principal"),
        "{err}"
    );
    assert!(err.contains("must be a valid principal"), "{err}");
    assert!(err.contains("event-id-hash=sha256:"), "{err}");
    assert!(
        !err.contains("evt-malformed-principal-summary-server-list"),
        "{err}"
    );

    let mut missing_allowed = valid_lineage_summary_management_list_event(
        "evt-missing-allowed-summary-server-list",
        "server.listed",
    );
    missing_allowed.payload["payload"]["authorization-receipt"]
        .as_object_mut()
        .unwrap()
        .remove("allowed");
    let err = lineage_drain_event_summary(&missing_allowed, &receipt)
        .unwrap_err()
        .to_string();
    assert!(
        err.contains(
            "management-list evidence must contain authorization receipt allowed decision"
        ),
        "{err}"
    );
    assert!(err.contains("event-id-hash=sha256:"), "{err}");
    assert!(
        !err.contains("evt-missing-allowed-summary-server-list"),
        "{err}"
    );

    let mut denied = valid_lineage_summary_management_list_event(
        "evt-denied-summary-server-list",
        "server.listed",
    );
    denied.payload["payload"]["authorization-receipt"]["allowed"] = json!(false);
    let err = lineage_drain_event_summary(&denied, &receipt)
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("management-list authorization receipt must allow replay projection"),
        "{err}"
    );
    assert!(err.contains("event-id-hash=sha256:"), "{err}");
    assert!(!err.contains("evt-denied-summary-server-list"), "{err}");
}

#[test]
fn lineage_drain_summary_rejects_malformed_scope_scalars() {
    let receipt = OutboxProjectionReceipt::default();
    let cases = [
        (
            "evt-bad-summary-view-warehouse",
            "view.loaded",
            json!({
                "view": {
                    "warehouse": 42,
                    "namespace": ["default"],
                    "name": "events_view"
                }
            }),
            "view warehouse must be a string when present",
        ),
        (
            "evt-bad-summary-view-namespace",
            "view.loaded",
            json!({
                "view": {
                    "warehouse": "local",
                    "namespace": ["default", 42],
                    "name": "events_view"
                }
            }),
            "view lifecycle namespace components must be non-empty strings",
        ),
        (
            "evt-blank-summary-view-name",
            "view.loaded",
            json!({
                "view": {
                    "warehouse": "local",
                    "namespace": ["default"],
                    "name": " "
                }
            }),
            "view lifecycle evidence has invalid view name",
        ),
        (
            "evt-blank-summary-policy-id",
            "policy-binding.upserted",
            json!({
                "policy": {
                    "policy-id": " "
                }
            }),
            "policy-binding upsert policy-id contains unsupported characters",
        ),
        (
            "evt-bad-summary-policy-odrl-hash",
            "policy-binding.upserted",
            json!({
                "policy": {
                    "policy-id": "agent-read",
                    "odrl-hash": "sha256:short"
                }
            }),
            "odrl-hash must contain full SHA-256 digest evidence",
        ),
        (
            "evt-bad-summary-management-project",
            "warehouse.listed",
            json!({
                "project-id": 42
            }),
            "project-id must be a string when present",
        ),
        (
            "evt-blank-summary-management-warehouse",
            "table.commits-listed",
            json!({
                "warehouse": " "
            }),
            "warehouse must not be blank",
        ),
    ];

    for (event_id, event_type, payload, expected_message) in cases {
        let event = if event_type == "policy-binding.upserted" {
            let mut event = valid_lineage_summary_management_upsert_event(event_id, event_type);
            merge_json_object(&mut event.payload["payload"], payload);
            event
        } else if matches!(
            event_type,
            "view.listed" | "view.upserted" | "view.loaded" | "view.dropped"
        ) {
            let mut event = valid_lineage_summary_view_event(event_id, event_type);
            merge_json_object(&mut event.payload["payload"], payload);
            event
        } else if event_type == "warehouse.listed" {
            let mut event = valid_lineage_summary_management_list_event(event_id, event_type);
            merge_json_object(&mut event.payload["payload"], payload);
            event
        } else {
            OutboxEvent {
                event_id: event_id.to_string(),
                sink: "lakecat.lineage-and-graph".to_string(),
                event_type: event_type.to_string(),
                payload: json!({
                    "payload": payload
                }),
                created_at: chrono::Utc::now(),
                delivered_at: None,
            }
        };
        let err = lineage_drain_event_summary(&event, &receipt)
            .unwrap_err()
            .to_string();
        assert!(err.contains(expected_message), "{err}");
    }
}

#[test]
fn lineage_drain_summary_rejects_malformed_namespace_events() {
    let receipt = OutboxProjectionReceipt::default();
    let cases = [
        (
            "evt-bad-summary-namespace-count",
            "namespace.listed",
            json!({
                "namespace-count": "1"
            }),
            "namespace list evidence must contain unsigned namespace-count",
        ),
        (
            "evt-bad-summary-namespace-paths",
            "namespace.listed",
            json!({
                "namespace-count": 2,
                "namespace-paths": ["default", 42]
            }),
            "namespace list namespace-paths must contain non-empty strings",
        ),
        (
            "evt-bad-summary-namespace-count-mismatch",
            "namespace.listed",
            json!({
                "namespace-count": 2,
                "namespace-paths": ["default"]
            }),
            "namespace list namespace-paths count must match namespace list count",
        ),
        (
            "evt-duplicate-summary-namespace-paths",
            "namespace.listed",
            json!({
                "namespace-count": 2,
                "namespace-paths": ["default", "default"]
            }),
            "namespace list namespace-paths must not contain duplicate namespaces",
        ),
        (
            "evt-bad-summary-namespace-lifecycle-namespace",
            "namespace.created",
            json!({
                "namespace": ["default", 42]
            }),
            "namespace lifecycle namespace components must be non-empty strings",
        ),
        (
            "evt-bad-summary-namespace-lifecycle-action",
            "namespace.loaded",
            json!({
                "authorization-receipt": {
                    "action": "namespace-create"
                }
            }),
            "namespace lifecycle authorization receipt action does not match outbox event type",
        ),
        (
            "evt-extra-summary-namespace-lifecycle-field",
            "namespace.dropped",
            json!({
                "querygraph": {
                    "claim": "shadow"
                }
            }),
            "namespace lifecycle contains unexpected field querygraph",
        ),
    ];

    for (event_id, event_type, payload, expected_message) in cases {
        let mut event = valid_lineage_summary_namespace_event(event_id, event_type);
        merge_json_object(&mut event.payload["payload"], payload);
        let err = lineage_drain_event_summary(&event, &receipt)
            .unwrap_err()
            .to_string();
        assert!(err.contains(expected_message), "{event_id}: {err}");
        assert!(err.contains("event-id-hash=sha256:"), "{event_id}: {err}");
        assert!(!err.contains(event_id), "{event_id}: {err}");
    }
}

#[test]
fn lineage_drain_summary_rejects_malformed_view_events() {
    let receipt = OutboxProjectionReceipt::default();
    let cases = [
        (
            "evt-bad-summary-view-count",
            "view.listed",
            json!({
                "view-count": "1"
            }),
            "view list evidence must contain unsigned view-count",
        ),
        (
            "evt-bad-summary-view-name",
            "view.listed",
            json!({
                "view-count": 2,
                "view-names": ["events_view", "bad/view"]
            }),
            "view list view-names contains an invalid view name",
        ),
        (
            "evt-bad-summary-view-count-mismatch",
            "view.listed",
            json!({
                "view-count": 2,
                "view-names": ["events_view"]
            }),
            "view list view-names count must match view list count",
        ),
        (
            "evt-duplicate-summary-view-name",
            "view.listed",
            json!({
                "view-count": 2,
                "view-names": ["events_view", "events_view"]
            }),
            "view list view-names must not contain duplicate view names",
        ),
        (
            "evt-bad-summary-view-list-action",
            "view.listed",
            json!({
                "authorization-receipt": {
                    "action": "view-manage"
                }
            }),
            "view list authorization receipt action does not match outbox event type",
        ),
        (
            "evt-bad-summary-view-action",
            "view.upserted",
            json!({
                "authorization-receipt": {
                    "action": "view-load"
                }
            }),
            "view lifecycle authorization receipt action does not match outbox event type",
        ),
        (
            "evt-bad-summary-view-scope-drift",
            "view.loaded",
            json!({
                "view": {
                    "namespace": ["shadow"]
                }
            }),
            "view lifecycle view namespace must match payload namespace",
        ),
        (
            "evt-bad-summary-view-version",
            "view.dropped",
            json!({
                "view": {
                    "view-version": 0
                }
            }),
            "view lifecycle evidence must contain positive view-version",
        ),
        (
            "evt-extra-summary-view-field",
            "view.upserted",
            json!({
                "view": {
                    "querygraph": {
                        "claim": "shadow"
                    }
                }
            }),
            "view lifecycle view contains unexpected field querygraph",
        ),
    ];

    for (event_id, event_type, payload, expected_message) in cases {
        let mut event = valid_lineage_summary_view_event(event_id, event_type);
        merge_json_object(&mut event.payload["payload"], payload);
        let err = lineage_drain_event_summary(&event, &receipt)
            .unwrap_err()
            .to_string();
        assert!(err.contains(expected_message), "{event_id}: {err}");
        assert!(err.contains("event-id-hash=sha256:"), "{event_id}: {err}");
        assert!(!err.contains(event_id), "{event_id}: {err}");
    }
}

#[test]
fn lineage_drain_summary_rejects_malformed_management_upserts() {
    let receipt = OutboxProjectionReceipt::default();
    let cases = [
        (
            "evt-bad-summary-policy-odrl-full-hash",
            "policy-binding.upserted",
            json!({
                "policy": {
                    "odrl-hash": "sha256:short"
                }
            }),
            "odrl-hash must contain full SHA-256 digest evidence",
        ),
        (
            "evt-bad-summary-policy-odrl-hash-match",
            "policy-binding.upserted",
            json!({
                "policy": {
                    "odrl-hash": content_hash_json(&json!({"different": "odrl"})).unwrap()
                }
            }),
            "policy-binding upsert odrl-hash must match odrl",
        ),
        (
            "evt-bad-summary-project-record-id",
            "project.upserted",
            json!({
                "project-record": {
                    "project-id": "shadow"
                }
            }),
            "project upsert project-id must match project-record",
        ),
        (
            "evt-bad-summary-server-endpoint-hash",
            "server.upserted",
            json!({
                "server-record": {
                    "endpoint-url-hash": "sha256:short"
                }
            }),
            "endpoint-url-hash must contain full SHA-256 digest evidence",
        ),
        (
            "evt-bad-summary-server-endpoint-hash-drift",
            "server.upserted",
            json!({
                "server-record": {
                    "endpoint-url-hash": content_hash_json(&json!({
                        "endpoint-url": "https://shadow.example.com"
                    })).unwrap()
                }
            }),
            "server upsert endpoint-url-hash must match endpoint-url",
        ),
        (
            "evt-bad-summary-warehouse-root-hash",
            "warehouse.upserted",
            json!({
                "warehouse-record": {
                    "storage-root-hash": content_hash_json(&json!({
                        "storage-root": "file:///tmp/other-root"
                    })).unwrap()
                }
            }),
            "warehouse upsert storage-root-hash must match storage-root",
        ),
    ];

    for (event_id, event_type, payload, expected_message) in cases {
        let mut event = valid_lineage_summary_management_upsert_event(event_id, event_type);
        merge_json_object(&mut event.payload["payload"], payload);
        let err = lineage_drain_event_summary(&event, &receipt)
            .unwrap_err()
            .to_string();
        assert!(err.contains(expected_message), "{event_id}: {err}");
        assert!(err.contains("event-id-hash=sha256:"), "{event_id}: {err}");
        assert!(!err.contains(event_id), "{event_id}: {err}");
    }
}

#[test]
fn lineage_drain_summary_rejects_malformed_catalog_config_fields() {
    let receipt = OutboxProjectionReceipt::default();
    let mut malformed_defaults =
        valid_lineage_summary_catalog_config_event("evt-bad-summary-config-defaults");
    malformed_defaults.payload["payload"]["defaults"][0]["value"] = json!(false);
    let err = lineage_drain_event_summary(&malformed_defaults, &receipt)
        .unwrap_err()
        .to_string();
    assert!(err.contains("catalog config-read defaults must contain string values"));

    let mut duplicate_overrides =
        valid_lineage_summary_catalog_config_event("evt-duplicate-summary-config-overrides");
    duplicate_overrides.payload["payload"]["overrides"] = json!([
        {"key": "warehouse", "value": "local"},
        {"key": "warehouse", "value": "shadow"}
    ]);
    let err = lineage_drain_event_summary(&duplicate_overrides, &receipt)
        .unwrap_err()
        .to_string();
    assert!(err.contains("catalog config-read overrides must not contain duplicate keys"));

    let mut non_array_endpoints =
        valid_lineage_summary_catalog_config_event("evt-bad-summary-config-endpoints");
    non_array_endpoints.payload["payload"]["endpoints"] =
        json!({ "route": "GET /catalog/v1/config" });
    let err = lineage_drain_event_summary(&non_array_endpoints, &receipt)
        .unwrap_err()
        .to_string();
    assert!(err.contains("catalog config-read endpoints must be an array"));

    for missing_endpoint in [
        "POST /catalog/v1/namespaces/{namespace}/tables/{table}/plan",
        "POST /catalog/v1/{warehouse}/namespaces/{namespace}/tables/{table}/plan",
        "POST /catalog/v1/namespaces/{namespace}/tables/{table}/fetch-scan-tasks",
        "POST /catalog/v1/{warehouse}/namespaces/{namespace}/tables/{table}/fetch-scan-tasks",
        "GET /catalog/v1/namespaces/{namespace}/tables/{table}/credentials",
        "GET /catalog/v1/{warehouse}/namespaces/{namespace}/tables/{table}/credentials",
    ] {
        let mut missing_required_endpoint =
            valid_lineage_summary_catalog_config_event("evt-missing-summary-config-endpoint");
        let endpoints = missing_required_endpoint.payload["payload"]["endpoints"]
            .as_array_mut()
            .expect("valid summary config endpoints should be an array");
        endpoints.retain(|endpoint| endpoint.as_str() != Some(missing_endpoint));
        let err = lineage_drain_event_summary(&missing_required_endpoint, &receipt)
            .unwrap_err()
            .to_string();
        assert!(err.contains(&format!(
            "catalog config-read endpoints must include {missing_endpoint}"
        )));
        assert!(err.contains("event-id-hash=sha256:"), "{err}");
        assert!(
            !err.contains("evt-missing-summary-config-endpoint"),
            "{err}"
        );
    }

    let mut duplicate_endpoints =
        valid_lineage_summary_catalog_config_event("evt-duplicate-summary-config-endpoint");
    duplicate_endpoints.payload["payload"]["endpoints"] =
        json!(["GET /catalog/v1/config", "GET /catalog/v1/config"]);
    let err = lineage_drain_event_summary(&duplicate_endpoints, &receipt)
        .unwrap_err()
        .to_string();
    assert!(err.contains("catalog config-read endpoints must not contain duplicate entries"));
}
