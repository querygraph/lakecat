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
async fn outbox_drain_projects_table_events_to_sinks() {
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let principal = Principal {
        subject: "agent:writer".to_string(),
        kind: PrincipalKind::Agent,
    };
    let full_policy_hash =
        content_hash_json(&json!({"policy-id": "agent-read", "scope": "default.events"})).unwrap();
    let commit_request_hash = content_hash_json(&json!({"request": "commit"})).unwrap();
    let commit_response_hash = content_hash_json(&json!({"response": "commit"})).unwrap();
    let commit_idempotency_hash = content_hash_bytes("commit:events:0001".as_bytes());
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
        events: Mutex::new(vec![
            OutboxEvent {
                event_id: "evt-namespace".to_string(),
                sink: "lakecat.lineage-and-graph".to_string(),
                event_type: "namespace.created".to_string(),
                payload: json!({
                    "audit-event-id": "audit-namespace",
                    "event-type": "namespace.created",
                    "payload": {
                        "authorization-receipt": {
                            "principal": principal,
                            "action": "namespace-create",
                            "allowed": true,
                            "engine": "test",
                            "policy_hash": null,
                            "checked_at": chrono::Utc::now(),
                        },
                        "warehouse": "local",
                        "namespace": ["default"],
                    }
                }),
                created_at: chrono::Utc::now(),
                delivered_at: None,
            },
            OutboxEvent {
                event_id: "evt-policy".to_string(),
                sink: "lakecat.lineage-and-graph".to_string(),
                event_type: "policy-binding.upserted".to_string(),
                payload: json!({
                    "audit-event-id": "audit-policy",
                    "event-type": "policy-binding.upserted",
                    "payload": {
                        "authorization-receipt": {
                            "principal": principal,
                            "action": "policy-manage",
                            "allowed": true,
                            "engine": "test",
                            "policy_hash": null,
                            "checked_at": chrono::Utc::now(),
                        },
                        "warehouse": "local",
                        "policy": {
                            "policy-id": "agent-read",
                            "warehouse": "local",
                            "namespace": ["default"],
                            "table": "events",
                            "enforced": true,
                            "odrl-hash": content_hash_json(&json!({
                                "uid": "policy:agent-read",
                                "lakecat:read-restriction": {
                                    "allowed-columns": ["event_id"]
                                }
                            })).unwrap(),
                            "odrl": {
                                "uid": "policy:agent-read",
                                "lakecat:read-restriction": {
                                    "allowed-columns": ["event_id"]
                                }
                            }
                        }
                    }
                }),
                created_at: chrono::Utc::now(),
                delivered_at: None,
            },
            OutboxEvent {
                event_id: "evt-1".to_string(),
                sink: "lakecat.lineage-and-graph".to_string(),
                event_type: "table.created".to_string(),
                payload: json!({
                    "audit-event-id": "audit-1",
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
                        "metadata-graph": {
                            "current-schema-id": 1,
                            "fields": [
                                {"id": 1, "name": "event_id", "type": "string", "required": true},
                                {"id": 2, "name": "payload", "type": "string", "required": false}
                            ],
                            "current-snapshot-id": 42,
                            "current-snapshot": {
                                "snapshot-id": 42,
                                "sequence-number": 7,
                                "timestamp-ms": 1710000000000_i64,
                                "manifest-list": "file:///tmp/events/metadata/snap-42.avro",
                                "summary": {"operation": "append"},
                                "schema-id": 1
                            }
                        },
                    }
                }),
                created_at: chrono::Utc::now(),
                delivered_at: None,
            },
            OutboxEvent {
                event_id: "evt-2".to_string(),
                sink: "lakecat.lineage-and-graph".to_string(),
                event_type: "table.scan-tasks-fetched".to_string(),
                payload: json!({
                    "audit-event-id": "audit-2",
                    "event-type": "table.scan-tasks-fetched",
                    "table": table,
                    "payload": {
                        "table": table,
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
                                    "purpose": "qglake-agent-demo",
                                    "max-credential-ttl-seconds": 300,
                                    "policy-hashes": [full_policy_hash.clone()]
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
                            "purpose": "qglake-agent-demo",
                            "max-credential-ttl-seconds": 300,
                            "policy-hashes": [full_policy_hash.clone()]
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
                        "storage-location": "file:///tmp/events",
                        "metadata-location": "file:///tmp/events/metadata/00000.json",
                        "file-scan-task-count": 1,
                        "delete-file-count": 1,
                        "child-plan-task-count": 1,
                    },
                }),
                created_at: chrono::Utc::now(),
                delivered_at: None,
            },
            OutboxEvent {
                event_id: "evt-commit".to_string(),
                sink: "lakecat.lineage-and-graph".to_string(),
                event_type: "table.commit".to_string(),
                payload: json!({
                    "audit-event-id": "audit-commit",
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
                        "policy_hash": full_policy_hash,
                        "request_hash": commit_request_hash,
                        "response_hash": commit_response_hash,
                        "idempotency_key_sha256": commit_idempotency_hash,
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
            },
            OutboxEvent {
                event_id: "evt-credentials".to_string(),
                sink: "lakecat.lineage-and-graph".to_string(),
                event_type: "credentials.vend-attempted".to_string(),
                payload: json!({
                    "audit-event-id": "audit-credentials",
                    "event-type": "credentials.vend-attempted",
                    "table": table,
                    "payload": {
                        "authorization-receipt": {
                            "principal": principal,
                            "action": "credentials-vend",
                            "allowed": true,
                            "engine": "test",
                            "policy_hash": "sha256:policy",
                            "checked_at": chrono::Utc::now(),
                            "context": {
                                "lakecat:raw-credential-exception": {
                                    "requested": true,
                                    "allowed": false,
                                    "reason": "fine-grained read restriction requires Sail-planned reads"
                                }
                            }
                        },
                        "credential-count": 0,
                        "storage-profile-id": "events-local",
                        "secret-ref-present": false,
                        "storage-profile": {
                            "profile-id": "events-local",
                            "warehouse": "local",
                            "provider": "file",
                            "issuance-mode": "local-file-no-secret",
                            "secret-ref-present": false,
                            "location-prefix-hash": content_hash_json(&json!({
                                "location-prefix": "file:///tmp/events"
                            })).unwrap()
                        },
                        "credential-response-evidence": [],
                        "lakecat:credential-block-reason": "fine-grained read restriction requires Sail-planned reads",
                        "lakecat:raw-credential-exception": {
                            "requested": true,
                            "allowed": false,
                            "reason": "fine-grained read restriction requires Sail-planned reads"
                        }
                    }
                }),
                created_at: chrono::Utc::now(),
                delivered_at: None,
            },
            OutboxEvent {
                event_id: "evt-3".to_string(),
                sink: "lakecat.lineage-and-graph".to_string(),
                event_type: "querygraph.bootstrap".to_string(),
                payload: json!({
                    "audit-event-id": "audit-3",
                    "event-type": "querygraph.bootstrap",
                    "payload": {
                        "authorization-receipt": {
                            "principal": principal,
                            "action": "graph-read",
                            "allowed": true,
                            "engine": "test",
                            "policy_hash": null,
                            "checked_at": chrono::Utc::now(),
                            "request-identity": {
                                "attestation-state": "verified",
                                "source": "x-lakecat-typedid-envelope",
                                "typedid-envelope-sha256": bootstrap_typedid_envelope_hash,
                                "typedid-proof-sha256": bootstrap_typedid_proof_hash,
                                "agent-delegation-sha256": bootstrap_agent_delegation_hash,
                                "agent-summary-signature-sha256": bootstrap_agent_summary_hash,
                                "typedid": "did:example:agent"
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
            },
            OutboxEvent {
                event_id: "evt-namespace-drop".to_string(),
                sink: "lakecat.lineage-and-graph".to_string(),
                event_type: "namespace.dropped".to_string(),
                payload: json!({
                    "audit-event-id": "audit-namespace-drop",
                    "event-type": "namespace.dropped",
                    "payload": {
                        "authorization-receipt": {
                            "principal": principal,
                            "action": "namespace-drop",
                            "allowed": true,
                            "engine": "test",
                            "policy_hash": null,
                            "checked_at": chrono::Utc::now(),
                        },
                        "warehouse": "local",
                        "namespace": ["archived"],
                    }
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
    assert_eq!(drain.delivered, 8);
    assert_eq!(
        drain.event_types,
        vec![
            "namespace.created".to_string(),
            "policy-binding.upserted".to_string(),
            "table.created".to_string(),
            "table.scan-tasks-fetched".to_string(),
            "table.commit".to_string(),
            "credentials.vend-attempted".to_string(),
            "querygraph.bootstrap".to_string(),
            "namespace.dropped".to_string()
        ]
    );
    assert_eq!(drain.graph_events, 20);
    assert_eq!(drain.lineage_events, 8);
    let credential_summary = drain
        .events
        .iter()
        .find(|event| event.event_type == "credentials.vend-attempted")
        .expect("credential replay summary should be exposed");
    assert_eq!(
        credential_summary.principal_subject.as_deref(),
        Some("agent:writer")
    );
    assert_eq!(credential_summary.principal_kind.as_deref(), Some("agent"));
    assert_eq!(credential_summary.graph_events, 2);
    assert_eq!(credential_summary.lineage_events, 1);
    assert_eq!(credential_summary.credential_count, Some(0));
    assert!(credential_summary.credential_prefix_hashes.is_empty());
    assert_eq!(
        credential_summary.credential_block_reason.as_deref(),
        Some("fine-grained read restriction requires Sail-planned reads")
    );
    assert_eq!(
        credential_summary.raw_credential_exception_allowed,
        Some(false)
    );
    assert_eq!(
        credential_summary
            .raw_credential_exception_reason
            .as_deref(),
        None
    );
    assert!(
        credential_summary
            .replay_event_hashes
            .iter()
            .all(|hash| is_full_sha256_hash(hash))
    );
    assert!(
        credential_summary
            .replay_open_lineage_hashes
            .iter()
            .all(|hash| is_full_sha256_hash(hash))
    );
    let graph_events = graph.events.lock().await;
    let credential_profile_event = graph_events
        .iter()
        .find(|event| event.event_id.as_deref() == Some("evt-credentials:storage-profile"))
        .expect("credential replay should project storage profile graph anchor");
    assert_eq!(
        credential_profile_event.label,
        GraphNodeLabel::StorageProfile
    );
    assert_eq!(credential_profile_event.action, GraphAction::Loaded);
    assert_eq!(
        credential_profile_event.subject,
        "lakecat:warehouse:local:storage-profile:events-local"
    );
    assert_eq!(
        credential_profile_event.properties["storage-profile"]["secret-ref-present"],
        serde_json::json!(false)
    );
    drop(graph_events);
    let scan_fetch_summary = drain
        .events
        .iter()
        .find(|event| event.event_type == "table.scan-tasks-fetched")
        .expect("scan task fetch replay summary should be exposed");
    assert_eq!(scan_fetch_summary.file_scan_task_count, Some(1));
    assert_eq!(scan_fetch_summary.delete_file_count, Some(1));
    assert_eq!(scan_fetch_summary.child_plan_task_count, Some(1));
    assert_eq!(scan_fetch_summary.scan_task_count, None);
    assert_eq!(
        scan_fetch_summary.read_restriction.as_ref().unwrap()["allowed-columns"],
        serde_json::json!(["event_id"])
    );
    assert_eq!(
        scan_fetch_summary.read_restriction.as_ref().unwrap()["row-predicate"],
        serde_json::json!({
            "type": "not-eq",
            "term": "severity",
            "value": "debug"
        })
    );
    assert_eq!(
        scan_fetch_summary.required_projection,
        vec!["event_id".to_string()]
    );
    assert_eq!(
        scan_fetch_summary.effective_projection,
        vec!["event_id".to_string()]
    );
    assert_eq!(
        scan_fetch_summary.required_filters,
        vec![serde_json::json!({
            "type": "not-eq",
            "term": "severity",
            "value": "debug"
        })]
    );
    let bootstrap_summary = drain
        .events
        .iter()
        .find(|event| event.event_type == "querygraph.bootstrap")
        .expect("bootstrap replay summary should be exposed");
    assert_eq!(
        bootstrap_summary.principal_subject.as_deref(),
        Some("agent:writer")
    );
    assert_eq!(bootstrap_summary.principal_kind.as_deref(), Some("agent"));
    assert!(
        bootstrap_summary
            .authorization_receipt_hash
            .as_deref()
            .is_some_and(|hash| hash.starts_with("sha256:"))
    );
    assert_eq!(
        bootstrap_summary.request_identity_state.as_deref(),
        Some("verified")
    );
    assert!(
        bootstrap_summary
            .agent_delegation_hash
            .as_deref()
            .is_some_and(|hash| hash.starts_with("sha256:"))
    );
    assert!(
        bootstrap_summary
            .agent_summary_signature_hash
            .as_deref()
            .is_some_and(|hash| hash.starts_with("sha256:"))
    );
    assert_eq!(bootstrap_summary.graph_events, 1);
    assert_eq!(bootstrap_summary.lineage_events, 1);
    assert_eq!(
        bootstrap_summary.bundle_hash.as_deref(),
        Some(bootstrap_bundle_hash.as_str())
    );
    assert_eq!(
        bootstrap_summary.graph_hash.as_deref(),
        Some(bootstrap_graph_hash.as_str())
    );
    assert_eq!(
        bootstrap_summary.open_lineage_hash.as_deref(),
        Some(bootstrap_open_lineage_hash.as_str())
    );
    assert_eq!(
        bootstrap_summary.querygraph_import_hash.as_deref(),
        Some(bootstrap_import_hash.as_str())
    );
    assert_eq!(bootstrap_summary.table_artifact_count, 1);
    assert_eq!(bootstrap_summary.view_artifact_count, 1);
    assert_eq!(bootstrap_summary.policy_binding_count, 1);
    assert_eq!(
        bootstrap_summary.standards,
        bootstrap_standards
            .iter()
            .map(|standard| standard.to_string())
            .collect::<Vec<_>>()
    );
    assert!(
        bootstrap_summary
            .replay_event_hashes
            .iter()
            .all(|hash| is_full_sha256_hash(hash))
    );
    assert!(
        bootstrap_summary
            .replay_open_lineage_hashes
            .iter()
            .all(|hash| is_full_sha256_hash(hash))
    );

    let graph_events = graph.events.lock().await;
    assert_eq!(graph_events.len(), 20);
    assert_eq!(graph_events[0].label, GraphNodeLabel::Principal);
    assert_eq!(graph_events[0].subject, "lakecat:principal:agent:writer");
    assert_eq!(
        graph_events[0].event_id.as_deref(),
        Some("evt-namespace:principal")
    );
    assert_eq!(
        graph_events[0].properties["principal"]["kind"],
        serde_json::json!("agent")
    );
    assert_eq!(graph_events[1].label, GraphNodeLabel::Namespace);
    assert_eq!(
        graph_events[1].subject,
        "lakecat:warehouse:local:namespace:default"
    );
    assert_eq!(graph_events[1].event_id.as_deref(), Some("evt-namespace"));
    assert_eq!(
        graph_events[1].properties["authorization-receipt"]["principal"]["subject"],
        serde_json::json!("agent:writer")
    );
    assert_eq!(graph_events[2].label, GraphNodeLabel::Principal);
    assert_eq!(
        graph_events[2].event_id.as_deref(),
        Some("evt-policy:principal")
    );
    assert_eq!(graph_events[3].label, GraphNodeLabel::Policy);
    assert_eq!(graph_events[3].action, GraphAction::Upserted);
    assert_eq!(
        graph_events[3].subject,
        "lakecat:warehouse:local:policy:agent-read"
    );
    assert_eq!(graph_events[3].event_id.as_deref(), Some("evt-policy"));
    assert_eq!(
        graph_events[3].properties["policy"]["odrl"]["uid"],
        serde_json::json!("policy:agent-read")
    );
    assert_eq!(graph_events[4].label, GraphNodeLabel::Principal);
    assert_eq!(graph_events[5].label, GraphNodeLabel::Table);
    assert_eq!(graph_events[5].action, GraphAction::Created);
    assert_eq!(graph_events[5].event_id.as_deref(), Some("evt-1"));
    assert_eq!(graph_events[6].label, GraphNodeLabel::Column);
    assert_eq!(
        graph_events[6].subject,
        "lakecat:column:lakecat:table:local:default:events:1"
    );
    assert_eq!(graph_events[6].event_id.as_deref(), Some("evt-1:column:1"));
    assert_eq!(
        graph_events[6].properties["field"]["name"],
        serde_json::json!("event_id")
    );
    assert_eq!(graph_events[7].label, GraphNodeLabel::Column);
    assert_eq!(
        graph_events[7].subject,
        "lakecat:column:lakecat:table:local:default:events:2"
    );
    assert_eq!(graph_events[8].label, GraphNodeLabel::Snapshot);
    assert_eq!(
        graph_events[8].subject,
        "lakecat:snapshot:lakecat:table:local:default:events:42"
    );
    assert_eq!(
        graph_events[8].event_id.as_deref(),
        Some("evt-1:snapshot:42")
    );
    assert_eq!(
        graph_events[8].properties["snapshot"]["manifest-list"],
        serde_json::json!("file:///tmp/events/metadata/snap-42.avro")
    );
    assert_eq!(graph_events[9].label, GraphNodeLabel::Principal);
    assert_eq!(graph_events[10].label, GraphNodeLabel::Table);
    assert_eq!(graph_events[10].action, GraphAction::PlannedScan);
    assert_eq!(
        graph_events[10].properties["read-restriction"]["allowed-columns"],
        serde_json::json!(["event_id"])
    );
    assert_eq!(graph_events[11].label, GraphNodeLabel::ScanPlan);
    assert_eq!(graph_events[11].subject, "lakecat:scan-plan:evt-2");
    assert_eq!(
        graph_events[11].event_id.as_deref(),
        Some("evt-2:scan-plan")
    );
    assert_eq!(
        graph_events[11].properties["read-restriction"]["allowed-columns"],
        serde_json::json!(["event_id"])
    );
    assert_eq!(graph_events[12].label, GraphNodeLabel::Principal);
    assert_eq!(graph_events[13].label, GraphNodeLabel::Table);
    assert_eq!(graph_events[13].action, GraphAction::Committed);
    assert_eq!(graph_events[13].event_id.as_deref(), Some("evt-commit"));
    assert_eq!(
        graph_events[13].properties["commit"]["new_metadata_location"],
        serde_json::json!("file:///tmp/events/metadata/00001.json")
    );
    assert_eq!(graph_events[14].label, GraphNodeLabel::Commit);
    assert_eq!(
        graph_events[14].subject,
        "lakecat:commit:lakecat:table:local:default:events:7"
    );
    assert_eq!(
        graph_events[14].event_id.as_deref(),
        Some("evt-commit:commit")
    );
    assert_eq!(
        graph_events[14].properties["commit"]["idempotency_key_sha256"],
        serde_json::json!(commit_idempotency_hash)
    );
    assert_eq!(
        graph_events[14].properties["commit"]["response_hash"],
        serde_json::json!(commit_response_hash)
    );
    assert_eq!(
        graph_events[14].properties["commit"]["format_version"],
        serde_json::json!(3)
    );
    assert_eq!(
        graph_events[14].properties["commit"]["snapshot_id"],
        serde_json::json!(42)
    );
    assert_eq!(
        graph_events[14].properties["commit"]["policy_hash"],
        serde_json::json!(full_policy_hash)
    );
    assert!(
        graph_events
            .iter()
            .any(|event| event.label == GraphNodeLabel::Principal
                && event.event_id.as_deref() == Some("evt-credentials:principal"))
    );
    let credential_profile_event = graph_events
        .iter()
        .find(|event| event.event_id.as_deref() == Some("evt-credentials:storage-profile"))
        .expect("credential replay should project storage profile graph anchor");
    assert_eq!(
        credential_profile_event.label,
        GraphNodeLabel::StorageProfile
    );
    assert_eq!(credential_profile_event.action, GraphAction::Loaded);
    assert_eq!(
        credential_profile_event.subject,
        "lakecat:warehouse:local:storage-profile:events-local"
    );
    assert!(
        graph_events
            .iter()
            .any(|event| event.label == GraphNodeLabel::Principal
                && event.event_id.as_deref() == Some("evt-3:principal"))
    );
    assert!(
        graph_events
            .iter()
            .any(|event| event.label == GraphNodeLabel::Principal
                && event.event_id.as_deref() == Some("evt-namespace-drop:principal"))
    );
    let namespace_drop_event = graph_events
        .iter()
        .find(|event| event.event_id.as_deref() == Some("evt-namespace-drop"))
        .expect("namespace drop should project a graph event");
    assert_eq!(namespace_drop_event.label, GraphNodeLabel::Namespace);
    assert_eq!(namespace_drop_event.action, GraphAction::Deleted);
    assert_eq!(
        namespace_drop_event.subject,
        "lakecat:warehouse:local:namespace:archived"
    );
    let policy_summary = drain
        .events
        .iter()
        .find(|event| event.event_type == "policy-binding.upserted")
        .expect("policy replay summary should be exposed");
    assert_eq!(policy_summary.graph_events, 2);
    assert_eq!(policy_summary.lineage_events, 1);
    assert!(
        policy_summary
            .replay_event_hashes
            .iter()
            .all(|hash| is_full_sha256_hash(hash))
    );
    assert!(
        policy_summary
            .replay_open_lineage_hashes
            .iter()
            .all(|hash| is_full_sha256_hash(hash))
    );

    let lineage_events = lineage.events.lock().await;
    assert_eq!(lineage_events.len(), 8);
    assert_eq!(
        lineage_events[0].event_type,
        LineageEventType::NamespaceCreated
    );
    assert_eq!(
        lineage_events[1].event_type,
        LineageEventType::PolicyBindingUpserted
    );
    assert_eq!(
        lineage_events[1].payload["policy"]["odrl"]["uid"],
        serde_json::json!("policy:agent-read")
    );
    assert_eq!(lineage_events[2].event_type, LineageEventType::TableCreated);
    assert_eq!(
        lineage_events[3].event_type,
        LineageEventType::TableScanPlanned
    );
    assert_eq!(
        lineage_events[3].payload["read-restriction"]["allowed-columns"],
        serde_json::json!(["event_id"])
    );
    assert_eq!(
        lineage_events[4].event_type,
        LineageEventType::TableCommitted
    );
    assert_eq!(
        lineage_events[4].payload["commit"]["new_metadata_location"],
        serde_json::json!("file:///tmp/events/metadata/00001.json")
    );
    assert_eq!(
        lineage_events[4].payload["commit"]["response_hash"],
        serde_json::json!(commit_response_hash)
    );
    assert_eq!(
        lineage_events[4].payload["commit"]["format_version"],
        serde_json::json!(3)
    );
    assert_eq!(
        lineage_events[4].payload["commit"]["snapshot_id"],
        serde_json::json!(42)
    );
    assert_eq!(
        lineage_events[4].payload["commit"]["policy_hash"],
        serde_json::json!(full_policy_hash)
    );
    assert_eq!(
        lineage_events[5].event_type,
        LineageEventType::CredentialsVendAttempted
    );
    assert_eq!(
        lineage_events[5].payload["credential-count"],
        serde_json::json!(0)
    );
    assert_eq!(
        lineage_events[5].payload["lakecat:raw-credential-exception"]["allowed"],
        serde_json::json!(false)
    );
    assert_eq!(
        lineage_events[6].event_type,
        LineageEventType::QueryGraphBootstrap
    );
    assert_eq!(
        lineage_events[6].payload["authorization-receipt"]["request-identity"]["attestation-state"],
        serde_json::json!("verified")
    );
    assert_eq!(
        lineage_events[6].payload["bundle-hash"],
        serde_json::json!(bootstrap_bundle_hash)
    );
    assert_eq!(
        lineage_events[6].payload["graph-hash"],
        serde_json::json!(bootstrap_graph_hash)
    );
    assert_eq!(
        lineage_events[6].payload["open-lineage-hash"],
        serde_json::json!(bootstrap_open_lineage_hash)
    );
    assert_eq!(
        lineage_events[6].payload["querygraph-import-hash"],
        serde_json::json!(bootstrap_import_hash)
    );
    assert_eq!(
        lineage_events[6].payload["table-artifacts"][0]["cdif-hash"],
        serde_json::json!(bootstrap_cdif_hash)
    );
    assert_eq!(
        lineage_events[6].payload["view-artifacts"][0]["stable-id"],
        serde_json::json!("lakecat:view:local:default:active_customers")
    );
    assert_eq!(
        lineage_events[7].event_type,
        LineageEventType::NamespaceDropped
    );
    assert_eq!(
        store.delivered.lock().await.as_slice(),
        &[
            "evt-namespace".to_string(),
            "evt-policy".to_string(),
            "evt-1".to_string(),
            "evt-2".to_string(),
            "evt-commit".to_string(),
            "evt-credentials".to_string(),
            "evt-3".to_string(),
            "evt-namespace-drop".to_string()
        ]
    );
}

#[tokio::test]
async fn outbox_drain_does_not_acknowledge_projection_failures() {
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
            event_id: "evt-lineage-fails".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "table.created".to_string(),
            payload: json!({
                "audit-event-id": "audit-lineage-fails",
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
    let lineage = Arc::new(FailingLineage::default());
    let state = LakeCatState::new(WarehouseName::new("local").unwrap(), store.clone())
        .with_integrations(
            default_sail_engine(),
            AllowAllGovernanceEngine::new(),
            graph.clone(),
            lineage.clone(),
        );

    let err = drain_outbox_once(&state, 10)
        .await
        .expect_err("lineage projection failure must fail the drain");
    assert!(err.to_string().contains("lineage projection failure"));
    assert!(
        store.delivered.lock().await.is_empty(),
        "failed projection must leave the event pending for retry"
    );
    assert_eq!(lineage.events.lock().await.len(), 1);
    assert!(
        !graph.events.lock().await.is_empty(),
        "graph projection may already be emitted, so retryability depends on outbox ack"
    );
}

#[tokio::test]
async fn outbox_drain_does_not_acknowledge_earlier_events_when_later_projection_fails() {
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let principal = Principal {
        subject: "agent:writer".to_string(),
        kind: PrincipalKind::Agent,
    };
    let first_created_at = chrono::Utc::now();
    let second_created_at = first_created_at + chrono::Duration::seconds(1);
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![
            OutboxEvent {
                event_id: "evt-first-projected".to_string(),
                sink: "lakecat.lineage-and-graph".to_string(),
                event_type: "table.created".to_string(),
                payload: json!({
                    "audit-event-id": "audit-first-projected",
                    "event-type": "table.created",
                    "table": table.clone(),
                    "payload": {
                        "authorization-receipt": {
                            "principal": principal.clone(),
                            "action": "table-create",
                            "allowed": true,
                            "engine": "test",
                            "policy_hash": null,
                            "checked_at": first_created_at,
                        },
                        "metadata-location": "file:///tmp/events/metadata/00000.json",
                        "format-version": 3,
                        "version": 0,
                    }
                }),
                created_at: first_created_at,
                delivered_at: None,
            },
            OutboxEvent {
                event_id: "evt-second-fails".to_string(),
                sink: "lakecat.lineage-and-graph".to_string(),
                event_type: "table.created".to_string(),
                payload: json!({
                    "audit-event-id": "audit-second-fails",
                    "event-type": "table.created",
                    "table": table,
                    "payload": {
                        "authorization-receipt": {
                            "principal": principal,
                            "action": "table-create",
                            "allowed": true,
                            "engine": "test",
                            "policy_hash": null,
                            "checked_at": second_created_at,
                        },
                        "metadata-location": "file:///tmp/events/metadata/00001.json",
                        "format-version": 3,
                        "version": 0,
                    }
                }),
                created_at: second_created_at,
                delivered_at: None,
            },
        ]),
        delivered: Mutex::default(),
    });
    let graph = Arc::new(RecordingGraph::default());
    let lineage = Arc::new(FailingLineageAfter {
        events: Mutex::default(),
        fail_after: 1,
    });
    let state = LakeCatState::new(WarehouseName::new("local").unwrap(), store.clone())
        .with_integrations(
            default_sail_engine(),
            AllowAllGovernanceEngine::new(),
            graph.clone(),
            lineage.clone(),
        );

    let err = drain_outbox_once(&state, 10)
        .await
        .expect_err("later lineage projection failure must fail the whole drain");

    assert!(err.to_string().contains("later lineage projection failure"));
    assert!(
        store.delivered.lock().await.is_empty(),
        "a later projection failure must leave earlier projected events pending too"
    );
    assert_eq!(lineage.events.lock().await.len(), 2);
    assert_eq!(
        graph.events.lock().await.len(),
        4,
        "graph projections may already have happened for both events before lineage fails"
    );
}

#[tokio::test]
async fn outbox_drain_does_not_acknowledge_graph_projection_failures() {
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
            event_id: "evt-graph-fails".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "table.created".to_string(),
            payload: json!({
                "audit-event-id": "audit-graph-fails",
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
    let graph = Arc::new(FailingGraph::default());
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
        .expect_err("graph projection failure must fail the drain");
    assert!(err.to_string().contains("graph projection failure"));
    assert!(
        store.delivered.lock().await.is_empty(),
        "failed graph projection must leave the event pending for retry"
    );
    assert_eq!(graph.events.lock().await.len(), 1);
    assert!(
        lineage.events.lock().await.is_empty(),
        "lineage projection must not run after graph projection failure"
    );
}

#[tokio::test]
async fn outbox_drain_rejects_malformed_policy_binding_upsert_evidence() {
    let principal = Principal {
        subject: "agent:writer".to_string(),
        kind: PrincipalKind::Agent,
    };
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-policy-malformed-scope".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "policy-binding.upserted".to_string(),
            payload: json!({
                "audit-event-id": "audit-policy-malformed-scope",
                "event-type": "policy-binding.upserted",
                "payload": {
                    "authorization-receipt": {
                        "principal": principal,
                        "action": "policy-manage",
                        "allowed": true,
                        "engine": "test",
                        "policy_hash": null,
                        "checked_at": chrono::Utc::now(),
                    },
                    "warehouse": "local",
                    "policy": {
                        "policy-id": "agent-read",
                        "warehouse": "local",
                        "table": "events",
                        "enforced": true,
                        "odrl-hash": content_hash_json(&json!({
                            "uid": "policy:agent-read",
                            "lakecat:read-restriction": {
                                "allowed-columns": ["event_id"]
                            }
                        })).unwrap(),
                        "odrl": {
                            "uid": "policy:agent-read",
                            "lakecat:read-restriction": {
                                "allowed-columns": ["event_id"]
                            }
                        }
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
        .expect_err("malformed policy-binding replay evidence should fail before delivery");
    let message = err.to_string();
    assert!(message.contains("policy-binding.upserted"));
    assert!(message.contains("invalid scope or identifier"));
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(store.delivered.lock().await.is_empty());
    assert!(graph.events.lock().await.is_empty());
    assert!(lineage.events.lock().await.is_empty());
}

#[tokio::test]
async fn outbox_drain_rejects_policy_binding_upsert_missing_odrl_hash() {
    let principal = Principal::new("agent:writer", PrincipalKind::Agent).unwrap();
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-policy-missing-odrl-hash".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "policy-binding.upserted".to_string(),
            payload: json!({
                "audit-event-id": "audit-policy-missing-odrl-hash",
                "event-type": "policy-binding.upserted",
                "payload": {
                    "authorization-receipt": {
                        "principal": principal,
                        "action": "policy-manage",
                        "allowed": true,
                        "engine": "test",
                        "policy_hash": null,
                        "checked_at": chrono::Utc::now(),
                    },
                    "warehouse": "local",
                    "policy": {
                        "policy-id": "agent-read",
                        "warehouse": "local",
                        "namespace": ["default"],
                        "table": "events",
                        "enforced": true,
                        "odrl": {
                            "uid": "policy:agent-read",
                            "lakecat:read-restriction": {
                                "allowed-columns": ["event_id"]
                            }
                        }
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
        .expect_err("missing policy-binding odrl-hash should fail before delivery");
    let message = err.to_string();
    assert!(message.contains("policy-binding.upserted"));
    assert!(message.contains("policy-binding upsert evidence must contain odrl-hash"));
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains("evt-policy-missing-odrl-hash"));
    assert!(
        store.delivered.lock().await.is_empty(),
        "missing ODRL hash must fail before acknowledgement"
    );
    assert!(
        graph.events.lock().await.is_empty(),
        "missing ODRL hash must fail before graph projection"
    );
    assert!(
        lineage.events.lock().await.is_empty(),
        "missing ODRL hash must fail before lineage projection"
    );
}

#[tokio::test]
async fn outbox_drain_rejects_policy_binding_upsert_mismatched_odrl_hash() {
    let principal = Principal::new("agent:writer", PrincipalKind::Agent).unwrap();
    let wrong_odrl_hash = content_hash_json(&json!({"uid": "policy:other"})).unwrap();
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-policy-mismatched-odrl-hash".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "policy-binding.upserted".to_string(),
            payload: json!({
                "audit-event-id": "audit-policy-mismatched-odrl-hash",
                "event-type": "policy-binding.upserted",
                "payload": {
                    "authorization-receipt": {
                        "principal": principal,
                        "action": "policy-manage",
                        "allowed": true,
                        "engine": "test",
                        "policy_hash": null,
                        "checked_at": chrono::Utc::now(),
                    },
                    "warehouse": "local",
                    "policy": {
                        "policy-id": "agent-read",
                        "warehouse": "local",
                        "namespace": ["default"],
                        "table": "events",
                        "enforced": true,
                        "odrl-hash": wrong_odrl_hash,
                        "odrl": {
                            "uid": "policy:agent-read",
                            "lakecat:read-restriction": {
                                "allowed-columns": ["event_id"]
                            }
                        }
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
        .expect_err("mismatched policy-binding odrl-hash should fail before delivery");
    let message = err.to_string();
    assert!(message.contains("policy-binding.upserted"));
    assert!(message.contains("policy-binding upsert odrl-hash must match odrl"));
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains("evt-policy-mismatched-odrl-hash"));
    assert!(
        store.delivered.lock().await.is_empty(),
        "mismatched ODRL hash must fail before acknowledgement"
    );
    assert!(
        graph.events.lock().await.is_empty(),
        "mismatched ODRL hash must fail before graph projection"
    );
    assert!(
        lineage.events.lock().await.is_empty(),
        "mismatched ODRL hash must fail before lineage projection"
    );
}

#[tokio::test]
async fn outbox_drain_rejects_policy_binding_upsert_invalid_policy_ids() {
    let principal = Principal::new("agent:writer", PrincipalKind::Agent).unwrap();
    let invalid_policy_id = "agent-read?token=secret";
    let odrl = json!({
        "uid": "policy:agent-read",
        "lakecat:read-restriction": {
            "allowed-columns": ["event_id"]
        }
    });
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-policy-invalid-id".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "policy-binding.upserted".to_string(),
            payload: json!({
                "audit-event-id": "audit-policy-invalid-id",
                "event-type": "policy-binding.upserted",
                "payload": {
                    "authorization-receipt": {
                        "principal": principal,
                        "action": "policy-manage",
                        "allowed": true,
                        "engine": "test",
                        "policy_hash": null,
                        "checked_at": chrono::Utc::now(),
                    },
                    "warehouse": "local",
                    "policy": {
                        "policy-id": invalid_policy_id,
                        "warehouse": "local",
                        "namespace": ["default"],
                        "table": "events",
                        "enforced": true,
                        "odrl-hash": content_hash_json(&odrl).unwrap(),
                        "odrl": odrl,
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
        .expect_err("invalid policy id should fail before delivery");
    let message = err.to_string();
    assert!(message.contains("policy-binding.upserted"));
    assert!(message.contains("policy-binding upsert policy-id contains unsupported characters"));
    assert!(message.contains("policy-id-hash=sha256:"));
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains(invalid_policy_id));
    assert!(!message.contains("token=secret"));
    assert!(store.delivered.lock().await.is_empty());
    assert!(graph.events.lock().await.is_empty());
    assert!(lineage.events.lock().await.is_empty());
}

#[tokio::test]
async fn outbox_drain_rejects_extra_policy_binding_upsert_fields() {
    let principal = Principal::new("agent:writer", PrincipalKind::Agent).unwrap();
    let odrl = json!({
        "uid": "policy:agent-read",
        "lakecat:read-restriction": {
            "allowed-columns": ["event_id"]
        }
    });
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-policy-extra-field".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "policy-binding.upserted".to_string(),
            payload: json!({
                "audit-event-id": "audit-policy-extra-field",
                "event-type": "policy-binding.upserted",
                "payload": {
                    "authorization-receipt": {
                        "principal": principal,
                        "action": "policy-manage",
                        "allowed": true,
                        "engine": "test",
                        "policy_hash": null,
                        "checked_at": chrono::Utc::now(),
                    },
                    "warehouse": "local",
                    "policy": {
                        "policy-id": "agent-read",
                        "warehouse": "local",
                        "namespace": ["default"],
                        "table": "events",
                        "enforced": true,
                        "odrl-hash": content_hash_json(&odrl).unwrap(),
                        "odrl": odrl,
                        "unverified-governance-claim": true,
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
        .expect_err("extra policy-binding fields should fail before delivery");
    let message = err.to_string();
    assert!(message.contains("policy-binding.upserted"));
    assert!(message.contains(
        "policy-binding upsert policy contains unexpected field unverified-governance-claim"
    ));
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains("evt-policy-extra-field"));
    assert!(
        store.delivered.lock().await.is_empty(),
        "extra policy fields must fail before acknowledgement"
    );
    assert!(
        graph.events.lock().await.is_empty(),
        "extra policy fields must fail before graph projection"
    );
    assert!(
        lineage.events.lock().await.is_empty(),
        "extra policy fields must fail before lineage projection"
    );
}

#[tokio::test]
async fn outbox_drain_rejects_extra_top_level_policy_binding_upsert_fields() {
    let principal = Principal::new("agent:writer", PrincipalKind::Agent).unwrap();
    let odrl = json!({
        "uid": "policy:agent-read",
        "lakecat:read-restriction": {
            "allowed-columns": ["event_id"]
        }
    });
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-policy-extra-top-field".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "policy-binding.upserted".to_string(),
            payload: json!({
                "audit-event-id": "audit-policy-extra-top-field",
                "event-type": "policy-binding.upserted",
                "payload": {
                    "event-type": "policy-binding.upserted",
                    "authorization-receipt": {
                        "principal": principal,
                        "action": "policy-manage",
                        "allowed": true,
                        "engine": "test",
                        "policy_hash": null,
                        "checked_at": chrono::Utc::now(),
                    },
                    "warehouse": "local",
                    "policy": {
                        "policy-id": "agent-read",
                        "warehouse": "local",
                        "namespace": ["default"],
                        "table": "events",
                        "enforced": true,
                        "odrl-hash": content_hash_json(&odrl).unwrap(),
                        "odrl": odrl,
                    },
                    "unverified-policy-claim": "shadow"
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
        .expect_err("extra top-level policy-binding fields should fail before delivery");
    let message = err.to_string();
    assert!(message.contains("policy-binding.upserted"));
    assert!(
        message.contains("policy-binding upsert contains unexpected field unverified-policy-claim")
    );
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains("evt-policy-extra-top-field"));
    assert!(
        store.delivered.lock().await.is_empty(),
        "extra top-level policy fields must fail before acknowledgement"
    );
    assert!(
        graph.events.lock().await.is_empty(),
        "extra top-level policy fields must fail before graph projection"
    );
    assert!(
        lineage.events.lock().await.is_empty(),
        "extra top-level policy fields must fail before lineage projection"
    );
}

#[tokio::test]
async fn outbox_drain_rejects_malformed_project_upsert_evidence() {
    let principal = Principal::new("agent:operator", PrincipalKind::Agent).unwrap();
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-project-mismatched-id".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "project.upserted".to_string(),
            payload: json!({
                "audit-event-id": "audit-project-mismatched-id",
                "event-type": "project.upserted",
                "payload": {
                    "authorization-receipt": {
                        "principal": principal,
                        "action": "project-manage",
                        "allowed": true,
                        "engine": "test",
                        "policy_hash": null,
                        "checked_at": chrono::Utc::now(),
                    },
                    "project-id": "default",
                    "project-record": {
                        "project-id": "shadow",
                        "display-name": "Default Project",
                        "properties": {"owner": "querygraph"}
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
        .expect_err("mismatched project replay evidence should fail before delivery");
    let message = err.to_string();
    assert!(message.contains("project.upserted"));
    assert!(message.contains("project-id must match"));
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(store.delivered.lock().await.is_empty());
    assert!(graph.events.lock().await.is_empty());
    assert!(lineage.events.lock().await.is_empty());
}

#[tokio::test]
async fn outbox_drain_rejects_project_upsert_invalid_project_ids() {
    let principal = Principal::new("agent:operator", PrincipalKind::Agent).unwrap();
    let invalid_project_id = "default?token=secret";
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-project-invalid-id".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "project.upserted".to_string(),
            payload: json!({
                "audit-event-id": "audit-project-invalid-id",
                "event-type": "project.upserted",
                "payload": {
                    "authorization-receipt": {
                        "principal": principal,
                        "action": "project-manage",
                        "allowed": true,
                        "engine": "test",
                        "policy_hash": null,
                        "checked_at": chrono::Utc::now(),
                    },
                    "project-id": invalid_project_id,
                    "project-record": {
                        "project-id": invalid_project_id,
                        "display-name": "Default Project",
                        "properties": {"owner": "querygraph"}
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
        .expect_err("invalid project id should fail before delivery");
    let message = err.to_string();
    assert!(message.contains("project.upserted"));
    assert!(message.contains("project upsert project-id contains unsupported characters"));
    assert!(message.contains("project-id-hash=sha256:"));
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains(invalid_project_id));
    assert!(!message.contains("token=secret"));
    assert!(store.delivered.lock().await.is_empty());
    assert!(graph.events.lock().await.is_empty());
    assert!(lineage.events.lock().await.is_empty());
}

#[tokio::test]
async fn outbox_drain_rejects_extra_management_record_fields() {
    let principal = Principal::new("agent:operator", PrincipalKind::Agent).unwrap();
    let cases = [
        (
            "project.upserted",
            "project-manage",
            "evt-project-extra-record-field",
            json!({
                "project-id": "default",
                "project-record": {
                    "project-id": "default",
                    "display-name": "Default Project",
                    "properties": {"owner": "querygraph"},
                    "unverified-tenant-claim": true,
                }
            }),
            "project upsert project-record contains unexpected field unverified-tenant-claim",
        ),
        (
            "server.upserted",
            "server-manage",
            "evt-server-extra-record-field",
            json!({
                "server-id": "prod",
                "server-record": {
                    "server-id": "prod",
                    "display-name": "Production",
                    "endpoint-url-hash": content_hash_json(&json!({
                        "endpoint-url": "https://lakecat.example"
                    })).unwrap(),
                    "properties": {"region": "global"},
                    "unverified-endpoint-claim": true,
                }
            }),
            "server upsert server-record contains unexpected field unverified-endpoint-claim",
        ),
        (
            "warehouse.upserted",
            "warehouse-manage",
            "evt-warehouse-extra-record-field",
            json!({
                "warehouse": "local",
                "warehouse-record": {
                    "warehouse": "local",
                    "project-id": "default",
                    "storage-root-hash": content_hash_json(&json!({
                        "storage-root": "file:///tmp/lakecat"
                    })).unwrap(),
                    "properties": {"environment": "demo"},
                    "unverified-storage-claim": true,
                }
            }),
            "warehouse upsert warehouse-record contains unexpected field unverified-storage-claim",
        ),
    ];

    for (event_type, action, event_id, mut payload, expected_message) in cases {
        let payload = payload.as_object_mut().expect("payload should be object");
        payload.insert(
            "authorization-receipt".to_string(),
            json!({
                "principal": principal,
                "action": action,
                "allowed": true,
                "engine": "test",
                "policy_hash": null,
                "checked_at": chrono::Utc::now(),
            }),
        );
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
            .expect_err("extra management record fields should fail before delivery");
        let message = err.to_string();
        assert!(message.contains(event_type), "{message}");
        assert!(message.contains(expected_message), "{message}");
        assert!(message.contains("event-id-hash=sha256:"), "{message}");
        assert!(!message.contains(event_id), "{message}");
        assert!(
            store.delivered.lock().await.is_empty(),
            "{event_type} extra record fields must fail before acknowledgement"
        );
        assert!(
            graph.events.lock().await.is_empty(),
            "{event_type} extra record fields must fail before graph projection"
        );
        assert!(
            lineage.events.lock().await.is_empty(),
            "{event_type} extra record fields must fail before lineage projection"
        );
    }
}

#[tokio::test]
async fn outbox_drain_rejects_extra_top_level_management_upsert_fields() {
    let principal = Principal::new("agent:operator", PrincipalKind::Agent).unwrap();
    let cases = [
        (
            "project.upserted",
            "project-manage",
            "evt-project-extra-top-field",
            json!({
                "project-id": "default",
                "project-record": {
                    "project-id": "default",
                    "display-name": "Default Project",
                    "properties": {"owner": "querygraph"}
                },
                "unverified-management-claim": "shadow",
            }),
            "project upsert contains unexpected field unverified-management-claim",
        ),
        (
            "server.upserted",
            "server-manage",
            "evt-server-extra-top-field",
            json!({
                "server-id": "prod",
                "server-record": {
                    "server-id": "prod",
                    "display-name": "Production",
                    "endpoint-url-hash": content_hash_json(&json!({
                        "endpoint-url": "https://lakecat.example"
                    })).unwrap(),
                    "properties": {"region": "global"}
                },
                "unverified-management-claim": "shadow",
            }),
            "server upsert contains unexpected field unverified-management-claim",
        ),
        (
            "warehouse.upserted",
            "warehouse-manage",
            "evt-warehouse-extra-top-field",
            json!({
                "project-id": "default",
                "warehouse": "local",
                "warehouse-record": {
                    "warehouse": "local",
                    "project-id": "default",
                    "storage-root-hash": content_hash_json(&json!({
                        "storage-root": "file:///tmp/lakecat"
                    })).unwrap(),
                    "properties": {"environment": "demo"}
                },
                "unverified-management-claim": "shadow",
            }),
            "warehouse upsert contains unexpected field unverified-management-claim",
        ),
    ];

    for (event_type, action, event_id, mut payload, expected_message) in cases {
        let payload = payload.as_object_mut().expect("payload should be object");
        payload.insert(
            "authorization-receipt".to_string(),
            json!({
                "principal": principal,
                "action": action,
                "allowed": true,
                "engine": "test",
                "policy_hash": null,
                "checked_at": chrono::Utc::now(),
            }),
        );
        payload.insert("event-type".to_string(), json!(event_type));
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
            .expect_err("extra top-level management upsert fields should fail");
        let message = err.to_string();
        assert!(message.contains(event_type), "{message}");
        assert!(message.contains(expected_message), "{message}");
        assert!(message.contains("event-id-hash=sha256:"), "{message}");
        assert!(!message.contains(event_id), "{message}");
        assert!(
            store.delivered.lock().await.is_empty(),
            "{event_type} extra top-level fields must fail before acknowledgement"
        );
        assert!(
            graph.events.lock().await.is_empty(),
            "{event_type} extra top-level fields must fail before graph projection"
        );
        assert!(
            lineage.events.lock().await.is_empty(),
            "{event_type} extra top-level fields must fail before lineage projection"
        );
    }
}

#[tokio::test]
async fn outbox_drain_rejects_extra_management_upsert_wrapper_fields() {
    let principal = Principal::new("agent:operator", PrincipalKind::Agent).unwrap();
    let odrl = json!({
        "uid": "policy:agent-read",
        "lakecat:read-restriction": {
            "allowed-columns": ["event_id"]
        }
    });
    let odrl_hash = content_hash_json(&odrl).unwrap();
    let endpoint_url_hash = content_hash_json(&json!({
        "endpoint-url": "https://lakecat.example"
    }))
    .unwrap();
    let storage_root_hash = content_hash_json(&json!({
        "storage-root": "file:///tmp/lakecat"
    }))
    .unwrap();
    let cases = [
        (
            "policy-binding.upserted",
            "policy-manage",
            "evt-policy-binding-extra-wrapper-field",
            json!({
                "authorization-receipt": {
                    "principal": principal.clone(),
                    "action": "policy-manage",
                    "allowed": true,
                    "engine": "test",
                    "policy_hash": null,
                    "checked_at": chrono::Utc::now(),
                },
                "warehouse": "local",
                "policy": {
                    "policy-id": "agent-read",
                    "warehouse": "local",
                    "namespace": ["default"],
                    "table": "events",
                    "enforced": true,
                    "odrl": odrl.clone(),
                    "odrl-hash": odrl_hash.clone(),
                }
            }),
            "policy-binding upsert outbox payload contains unexpected field unverified-management-wrapper-claim",
        ),
        (
            "project.upserted",
            "project-manage",
            "evt-project-extra-wrapper-field",
            json!({
                "authorization-receipt": {
                    "principal": principal.clone(),
                    "action": "project-manage",
                    "allowed": true,
                    "engine": "test",
                    "policy_hash": null,
                    "checked_at": chrono::Utc::now(),
                },
                "project-id": "default",
                "project-record": {
                    "project-id": "default",
                    "display-name": "Default Project",
                    "properties": {"owner": "querygraph"}
                }
            }),
            "project upsert outbox payload contains unexpected field unverified-management-wrapper-claim",
        ),
        (
            "server.upserted",
            "server-manage",
            "evt-server-extra-wrapper-field",
            json!({
                "authorization-receipt": {
                    "principal": principal.clone(),
                    "action": "server-manage",
                    "allowed": true,
                    "engine": "test",
                    "policy_hash": null,
                    "checked_at": chrono::Utc::now(),
                },
                "server-id": "prod",
                "server-record": {
                    "server-id": "prod",
                    "display-name": "Production",
                    "endpoint-url": "https://lakecat.example",
                    "endpoint-url-hash": endpoint_url_hash.clone(),
                    "properties": {"region": "global"}
                }
            }),
            "server upsert outbox payload contains unexpected field unverified-management-wrapper-claim",
        ),
        (
            "warehouse.upserted",
            "warehouse-manage",
            "evt-warehouse-extra-wrapper-field",
            json!({
                "authorization-receipt": {
                    "principal": principal.clone(),
                    "action": "warehouse-manage",
                    "allowed": true,
                    "engine": "test",
                    "policy_hash": null,
                    "checked_at": chrono::Utc::now(),
                },
                "project-id": "default",
                "warehouse": "local",
                "warehouse-record": {
                    "warehouse": "local",
                    "project-id": "default",
                    "storage-root": "file:///tmp/lakecat",
                    "storage-root-hash": storage_root_hash.clone(),
                    "properties": {"environment": "demo"}
                }
            }),
            "warehouse upsert outbox payload contains unexpected field unverified-management-wrapper-claim",
        ),
    ];

    for (event_type, _action, event_id, payload, expected_message) in cases {
        let store = Arc::new(RecordingOutboxStore {
            events: Mutex::new(vec![OutboxEvent {
                event_id: event_id.to_string(),
                sink: "lakecat.lineage-and-graph".to_string(),
                event_type: event_type.to_string(),
                payload: json!({
                    "audit-event-id": format!("audit-{event_id}"),
                    "event-type": event_type,
                    "payload": payload,
                    "unverified-management-wrapper-claim": "shadow",
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
            .expect_err("extra management upsert wrapper fields should fail");
        let message = err.to_string();
        assert!(message.contains(event_type), "{message}");
        assert!(message.contains(expected_message), "{message}");
        assert!(message.contains("event-id-hash=sha256:"), "{message}");
        assert!(!message.contains(event_id), "{message}");
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
async fn outbox_drain_rejects_malformed_server_upsert_endpoint_evidence() {
    let principal = Principal::new("agent:operator", PrincipalKind::Agent).unwrap();
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-server-decorated-endpoint".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "server.upserted".to_string(),
            payload: json!({
                "audit-event-id": "audit-server-decorated-endpoint",
                "event-type": "server.upserted",
                "payload": {
                    "authorization-receipt": {
                        "principal": principal,
                        "action": "server-manage",
                        "allowed": true,
                        "engine": "test",
                        "policy_hash": null,
                        "checked_at": chrono::Utc::now(),
                    },
                    "server-id": "prod",
                    "server-record": {
                        "server-id": "prod",
                        "display-name": "Production",
                        "endpoint-url": "https://lakecat.example?token=raw-secret",
                        "endpoint-url-hash": content_hash_json(&json!({
                            "endpoint-url": "https://lakecat.example?token=raw-secret"
                        })).unwrap(),
                        "properties": {"region": "global"}
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
        .expect_err("decorated server endpoint evidence should fail before delivery");
    let message = err.to_string();
    assert!(message.contains("server.upserted"));
    assert!(message.contains("invalid endpoint"));
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains("raw-secret"));
    assert!(store.delivered.lock().await.is_empty());
    assert!(graph.events.lock().await.is_empty());
    assert!(lineage.events.lock().await.is_empty());
}

#[tokio::test]
async fn outbox_drain_rejects_server_upsert_endpoint_hash_drift() {
    let principal = Principal::new("agent:operator", PrincipalKind::Agent).unwrap();
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-server-endpoint-hash-drift".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "server.upserted".to_string(),
            payload: json!({
                "audit-event-id": "audit-server-endpoint-hash-drift",
                "event-type": "server.upserted",
                "payload": {
                    "authorization-receipt": {
                        "principal": principal,
                        "action": "server-manage",
                        "allowed": true,
                        "engine": "test",
                        "policy_hash": null,
                        "checked_at": chrono::Utc::now(),
                    },
                    "server-id": "prod",
                    "server-record": {
                        "server-id": "prod",
                        "display-name": "Production",
                        "endpoint-url": "https://lakecat.example",
                        "endpoint-url-hash": content_hash_json(&json!({
                            "endpoint-url": "https://shadow.example"
                        })).unwrap(),
                        "properties": {"region": "global"}
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
        .expect_err("server endpoint hash drift should fail before delivery");
    let message = err.to_string();
    assert!(message.contains("server.upserted"));
    assert!(message.contains("server upsert endpoint-url-hash must match endpoint-url"));
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains("evt-server-endpoint-hash-drift"));
    assert!(store.delivered.lock().await.is_empty());
    assert!(graph.events.lock().await.is_empty());
    assert!(lineage.events.lock().await.is_empty());
}

#[tokio::test]
async fn outbox_drain_rejects_malformed_warehouse_upsert_storage_root_evidence() {
    let principal = Principal::new("agent:operator", PrincipalKind::Agent).unwrap();
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-warehouse-decorated-root".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "warehouse.upserted".to_string(),
            payload: json!({
                "audit-event-id": "audit-warehouse-decorated-root",
                "event-type": "warehouse.upserted",
                "payload": {
                    "authorization-receipt": {
                        "principal": principal,
                        "action": "warehouse-manage",
                        "allowed": true,
                        "engine": "test",
                        "policy_hash": null,
                        "checked_at": chrono::Utc::now(),
                    },
                    "warehouse": "local",
                    "warehouse-record": {
                        "warehouse": "local",
                        "project-id": "default",
                        "storage-root": "file:///tmp/lakecat?token=raw-secret",
                        "storage-root-hash": content_hash_json(&json!({
                            "storage-root": "file:///tmp/lakecat?token=raw-secret"
                        })).unwrap(),
                        "properties": {"region": "local"}
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
        .expect_err("decorated warehouse storage-root evidence should fail before delivery");
    let message = err.to_string();
    assert!(message.contains("warehouse.upserted"));
    assert!(message.contains("invalid storage root"));
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains("raw-secret"));
    assert!(store.delivered.lock().await.is_empty());
    assert!(graph.events.lock().await.is_empty());
    assert!(lineage.events.lock().await.is_empty());
}

#[tokio::test]
async fn outbox_drain_rejects_warehouse_upsert_storage_root_hash_drift() {
    let principal = Principal::new("agent:operator", PrincipalKind::Agent).unwrap();
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-warehouse-storage-root-hash-drift".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "warehouse.upserted".to_string(),
            payload: json!({
                "audit-event-id": "audit-warehouse-storage-root-hash-drift",
                "event-type": "warehouse.upserted",
                "payload": {
                    "authorization-receipt": {
                        "principal": principal,
                        "action": "warehouse-manage",
                        "allowed": true,
                        "engine": "test",
                        "policy_hash": null,
                        "checked_at": chrono::Utc::now(),
                    },
                    "warehouse": "local",
                    "warehouse-record": {
                        "warehouse": "local",
                        "project-id": "default",
                        "storage-root": "file:///tmp/lakecat",
                        "storage-root-hash": content_hash_json(&json!({
                            "storage-root": "file:///tmp/shadow"
                        })).unwrap(),
                        "properties": {"region": "local"}
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
        .expect_err("warehouse storage-root hash drift should fail before delivery");
    let message = err.to_string();
    assert!(message.contains("warehouse.upserted"));
    assert!(message.contains("warehouse upsert storage-root-hash must match storage-root"));
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains("evt-warehouse-storage-root-hash-drift"));
    assert!(store.delivered.lock().await.is_empty());
    assert!(graph.events.lock().await.is_empty());
    assert!(lineage.events.lock().await.is_empty());
}

#[tokio::test]
async fn outbox_drain_rejects_missing_management_upsert_receipt_principal() {
    let storage_profile_location_hash = content_hash_json(&json!({
        "location-prefix": "file:///tmp/lakecat/events"
    }))
    .unwrap();
    let cases = vec![
        (
            "policy-binding.upserted",
            "policy-binding upsert",
            json!({
                "authorization-receipt": {
                    "action": "policy-manage",
                    "allowed": true,
                    "engine": "test",
                    "policy_hash": null,
                    "checked_at": chrono::Utc::now(),
                },
                "warehouse": "local",
                "policy": {
                    "policy-id": "agent-read",
                    "warehouse": "local",
                    "namespace": ["default"],
                    "table": "events",
                    "enforced": true,
                    "odrl-hash": content_hash_json(&json!({
                        "uid": "policy:agent-read",
                        "lakecat:read-restriction": {
                            "allowed-columns": ["event_id"]
                        }
                    })).unwrap(),
                    "odrl": {
                        "uid": "policy:agent-read",
                        "lakecat:read-restriction": {
                            "allowed-columns": ["event_id"]
                        }
                    }
                }
            }),
        ),
        (
            "project.upserted",
            "project upsert",
            json!({
                "authorization-receipt": {
                    "action": "project-manage",
                    "allowed": true,
                    "engine": "test",
                    "policy_hash": null,
                    "checked_at": chrono::Utc::now(),
                },
                "project-id": "default",
                "project-record": {
                    "project-id": "default",
                    "display-name": "Default Project",
                    "properties": {"owner": "querygraph"}
                }
            }),
        ),
        (
            "server.upserted",
            "server upsert",
            json!({
                "authorization-receipt": {
                    "action": "server-manage",
                    "allowed": true,
                    "engine": "test",
                    "policy_hash": null,
                    "checked_at": chrono::Utc::now(),
                },
                "server-id": "prod",
                "server-record": {
                    "server-id": "prod",
                    "display-name": "Production",
                    "endpoint-url": "https://lakecat.example",
                    "endpoint-url-hash": content_hash_json(&json!({
                        "endpoint-url": "https://lakecat.example"
                    })).unwrap(),
                    "properties": {"region": "global"}
                }
            }),
        ),
        (
            "storage-profile.upserted",
            "storage-profile upsert",
            json!({
                "authorization-receipt": {
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
                    "location-prefix-hash": storage_profile_location_hash,
                    "provider": "file",
                    "issuance-mode": "local-file-no-secret",
                    "secret-ref-present": false,
                }
            }),
        ),
        (
            "warehouse.upserted",
            "warehouse upsert",
            json!({
                "authorization-receipt": {
                    "action": "warehouse-manage",
                    "allowed": true,
                    "engine": "test",
                    "policy_hash": null,
                    "checked_at": chrono::Utc::now(),
                },
                "warehouse": "local",
                "warehouse-record": {
                    "warehouse": "local",
                    "project-id": "default",
                    "storage-root": "file:///tmp/lakecat",
                    "storage-root-hash": content_hash_json(&json!({
                        "storage-root": "file:///tmp/lakecat"
                    })).unwrap(),
                    "properties": {"region": "local"}
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
            .expect_err("missing management-upsert receipt principal should fail");

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
async fn outbox_drain_rejects_malformed_management_upsert_receipt_principal() {
    let malformed_principal = json!({
        "subject": "agent:operator",
        "kind": "unknown"
    });
    let storage_profile_location_hash = content_hash_json(&json!({
        "location-prefix": "file:///tmp/lakecat/events"
    }))
    .unwrap();
    let cases = vec![
        (
            "policy-binding.upserted",
            "policy-binding upsert",
            json!({
                "authorization-receipt": {
                    "principal": malformed_principal,
                    "action": "policy-manage",
                    "allowed": true,
                    "engine": "test",
                    "policy_hash": null,
                    "checked_at": chrono::Utc::now(),
                },
                "warehouse": "local",
                "policy": {
                    "policy-id": "agent-read",
                    "warehouse": "local",
                    "namespace": ["default"],
                    "table": "events",
                    "enforced": true,
                    "odrl-hash": content_hash_json(&json!({
                        "uid": "policy:agent-read",
                        "lakecat:read-restriction": {
                            "allowed-columns": ["event_id"]
                        }
                    })).unwrap(),
                    "odrl": {
                        "uid": "policy:agent-read",
                        "lakecat:read-restriction": {
                            "allowed-columns": ["event_id"]
                        }
                    }
                }
            }),
        ),
        (
            "project.upserted",
            "project upsert",
            json!({
                "authorization-receipt": {
                    "principal": malformed_principal,
                    "action": "project-manage",
                    "allowed": true,
                    "engine": "test",
                    "policy_hash": null,
                    "checked_at": chrono::Utc::now(),
                },
                "project-id": "default",
                "project-record": {
                    "project-id": "default",
                    "display-name": "Default Project",
                    "properties": {"owner": "querygraph"}
                }
            }),
        ),
        (
            "server.upserted",
            "server upsert",
            json!({
                "authorization-receipt": {
                    "principal": malformed_principal,
                    "action": "server-manage",
                    "allowed": true,
                    "engine": "test",
                    "policy_hash": null,
                    "checked_at": chrono::Utc::now(),
                },
                "server-id": "prod",
                "server-record": {
                    "server-id": "prod",
                    "display-name": "Production",
                    "endpoint-url": "https://lakecat.example",
                    "endpoint-url-hash": content_hash_json(&json!({
                        "endpoint-url": "https://lakecat.example"
                    })).unwrap(),
                    "properties": {"region": "global"}
                }
            }),
        ),
        (
            "storage-profile.upserted",
            "storage-profile upsert",
            json!({
                "authorization-receipt": {
                    "principal": malformed_principal,
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
                    "location-prefix-hash": storage_profile_location_hash,
                    "provider": "file",
                    "issuance-mode": "local-file-no-secret",
                    "secret-ref-present": false,
                }
            }),
        ),
        (
            "warehouse.upserted",
            "warehouse upsert",
            json!({
                "authorization-receipt": {
                    "principal": malformed_principal,
                    "action": "warehouse-manage",
                    "allowed": true,
                    "engine": "test",
                    "policy_hash": null,
                    "checked_at": chrono::Utc::now(),
                },
                "warehouse": "local",
                "warehouse-record": {
                    "warehouse": "local",
                    "project-id": "default",
                    "storage-root": "file:///tmp/lakecat",
                    "storage-root-hash": content_hash_json(&json!({
                        "storage-root": "file:///tmp/lakecat"
                    })).unwrap(),
                    "properties": {"region": "local"}
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
            .expect_err("malformed management-upsert receipt principal should fail");

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
async fn outbox_drain_rejects_mismatched_management_upsert_receipt_actions() {
    let principal = Principal {
        subject: "agent:operator".to_string(),
        kind: PrincipalKind::Agent,
    };
    let storage_profile_location_hash = content_hash_json(&json!({
        "location-prefix": "file:///tmp/lakecat/events"
    }))
    .unwrap();
    let cases = vec![
        (
            "policy-binding.upserted",
            "policy-binding upsert",
            json!({
                "authorization-receipt": {
                    "principal": principal,
                    "action": "table-load",
                    "allowed": true,
                    "engine": "test",
                    "policy_hash": null,
                    "checked_at": chrono::Utc::now(),
                },
                "warehouse": "local",
                "policy": {
                    "policy-id": "agent-read",
                    "warehouse": "local",
                    "namespace": ["default"],
                    "table": "events",
                    "enforced": true,
                    "odrl-hash": content_hash_json(&json!({
                        "uid": "policy:agent-read",
                        "lakecat:read-restriction": {
                            "allowed-columns": ["event_id"]
                        }
                    })).unwrap(),
                    "odrl": {
                        "uid": "policy:agent-read",
                        "lakecat:read-restriction": {
                            "allowed-columns": ["event_id"]
                        }
                    }
                }
            }),
        ),
        (
            "project.upserted",
            "project upsert",
            json!({
                "authorization-receipt": {
                    "principal": principal,
                    "action": "table-load",
                    "allowed": true,
                    "engine": "test",
                    "policy_hash": null,
                    "checked_at": chrono::Utc::now(),
                },
                "project-id": "default",
                "project-record": {
                    "project-id": "default",
                    "display-name": "Default Project",
                    "properties": {"owner": "querygraph"}
                }
            }),
        ),
        (
            "server.upserted",
            "server upsert",
            json!({
                "authorization-receipt": {
                    "principal": principal,
                    "action": "table-load",
                    "allowed": true,
                    "engine": "test",
                    "policy_hash": null,
                    "checked_at": chrono::Utc::now(),
                },
                "server-id": "prod",
                "server-record": {
                    "server-id": "prod",
                    "display-name": "Production",
                    "endpoint-url": "https://lakecat.example",
                    "endpoint-url-hash": content_hash_json(&json!({
                        "endpoint-url": "https://lakecat.example"
                    })).unwrap(),
                    "properties": {"region": "global"}
                }
            }),
        ),
        (
            "storage-profile.upserted",
            "storage-profile upsert",
            json!({
                "authorization-receipt": {
                    "principal": principal,
                    "action": "table-load",
                    "allowed": true,
                    "engine": "test",
                    "policy_hash": null,
                    "checked_at": chrono::Utc::now(),
                },
                "warehouse": "local",
                "storage-profile": {
                    "profile-id": "file-events",
                    "warehouse": "local",
                    "location-prefix-hash": storage_profile_location_hash,
                    "provider": "file",
                    "issuance-mode": "local-file-no-secret",
                    "secret-ref-present": false,
                }
            }),
        ),
        (
            "warehouse.upserted",
            "warehouse upsert",
            json!({
                "authorization-receipt": {
                    "principal": principal,
                    "action": "table-load",
                    "allowed": true,
                    "engine": "test",
                    "policy_hash": null,
                    "checked_at": chrono::Utc::now(),
                },
                "warehouse": "local",
                "warehouse-record": {
                    "warehouse": "local",
                    "project-id": "default",
                    "storage-root": "file:///tmp/lakecat",
                    "storage-root-hash": content_hash_json(&json!({
                        "storage-root": "file:///tmp/lakecat"
                    })).unwrap(),
                    "properties": {"region": "local"}
                }
            }),
        ),
    ];

    for (event_type, label, payload) in cases {
        let event_id = format!("evt-mismatched-{}-action-token", event_type);
        let store = Arc::new(RecordingOutboxStore {
            events: Mutex::new(vec![OutboxEvent {
                event_id: event_id.clone(),
                sink: "lakecat.lineage-and-graph".to_string(),
                event_type: event_type.to_string(),
                payload: json!({
                    "audit-event-id": format!("audit-mismatched-{event_type}-action"),
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
            .expect_err("mismatched management-upsert receipt action should fail");

        let message = err.to_string();
        assert!(
            message.contains(event_type),
            "{event_type} error should include event type: {message}"
        );
        assert!(
            message.contains(&format!(
                "{label} authorization receipt action does not match outbox event type"
            )),
            "{event_type} error should describe mismatched receipt action: {message}"
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
async fn outbox_drain_rejects_missing_or_denied_management_upsert_receipt_allowed_decision() {
    let principal = Principal {
        subject: "agent:operator".to_string(),
        kind: PrincipalKind::Agent,
    };
    let storage_profile_location_hash = content_hash_json(&json!({
        "location-prefix": "file:///tmp/lakecat/events"
    }))
    .unwrap();
    let storage_root_hash = content_hash_json(&json!({
        "storage-root": "file:///tmp/lakecat"
    }))
    .unwrap();
    let endpoint_url_hash = content_hash_json(&json!({
        "endpoint-url": "https://lakecat.example"
    }))
    .unwrap();
    let odrl = json!({
        "uid": "policy:agent-read",
        "lakecat:read-restriction": {
            "allowed-columns": ["event_id"]
        }
    });
    let odrl_hash = content_hash_json(&odrl).unwrap();

    for (event_type, label, action) in [
        (
            "policy-binding.upserted",
            "policy-binding upsert",
            "policy-manage",
        ),
        ("project.upserted", "project upsert", "project-manage"),
        ("server.upserted", "server upsert", "server-manage"),
        (
            "storage-profile.upserted",
            "storage-profile upsert",
            "storage-profile-manage",
        ),
        ("warehouse.upserted", "warehouse upsert", "warehouse-manage"),
    ] {
        for (case, allowed, expected_message) in [
            (
                "missing",
                None,
                format!("{label} evidence must contain authorization receipt allowed decision"),
            ),
            (
                "denied",
                Some(false),
                format!("{label} authorization receipt must allow replay projection"),
            ),
        ] {
            let mut authorization_receipt = json!({
                "principal": principal.clone(),
                "action": action,
                "engine": "test",
                "policy_hash": null,
                "checked_at": chrono::Utc::now(),
            });
            if let Some(allowed) = allowed {
                authorization_receipt["allowed"] = json!(allowed);
            }
            let payload = match event_type {
                "policy-binding.upserted" => json!({
                    "authorization-receipt": authorization_receipt,
                    "warehouse": "local",
                    "policy": {
                        "policy-id": "agent-read",
                        "warehouse": "local",
                        "namespace": ["default"],
                        "table": "events",
                        "enforced": true,
                        "odrl-hash": odrl_hash.clone(),
                        "odrl": odrl.clone(),
                    }
                }),
                "project.upserted" => json!({
                    "authorization-receipt": authorization_receipt,
                    "project-id": "default",
                    "project-record": {
                        "project-id": "default",
                        "display-name": "Default Project",
                        "properties": {"owner": "querygraph"}
                    }
                }),
                "server.upserted" => json!({
                    "authorization-receipt": authorization_receipt,
                    "server-id": "prod",
                    "server-record": {
                        "server-id": "prod",
                        "display-name": "Production",
                        "endpoint-url": "https://lakecat.example",
                        "endpoint-url-hash": endpoint_url_hash.clone(),
                        "properties": {"region": "global"}
                    }
                }),
                "storage-profile.upserted" => json!({
                    "authorization-receipt": authorization_receipt,
                    "warehouse": "local",
                    "storage-profile": {
                        "profile-id": "file-events",
                        "warehouse": "local",
                        "location-prefix-hash": storage_profile_location_hash.clone(),
                        "provider": "file",
                        "issuance-mode": "local-file-no-secret",
                        "secret-ref-present": false,
                    }
                }),
                "warehouse.upserted" => json!({
                    "authorization-receipt": authorization_receipt,
                    "warehouse": "local",
                    "warehouse-record": {
                        "warehouse": "local",
                        "project-id": "default",
                        "storage-root": "file:///tmp/lakecat",
                        "storage-root-hash": storage_root_hash.clone(),
                        "properties": {"region": "local"}
                    }
                }),
                _ => unreachable!("covered by management upsert cases"),
            };
            let event_id = format!(
                "evt-{case}-{}-receipt-allowed",
                event_type.replace('.', "-")
            );
            let store = Arc::new(RecordingOutboxStore {
                events: Mutex::new(vec![OutboxEvent {
                    event_id: event_id.clone(),
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
                .expect_err("missing or denied management-upsert receipt decision should fail");

            let message = err.to_string();
            assert!(
                message.contains(event_type),
                "{event_type} error should include event type: {message}"
            );
            assert!(
                message.contains(&expected_message),
                "{event_type} should reject {case} allowed decision: {message}"
            );
            assert!(message.contains("event-id-hash=sha256:"));
            assert!(!message.contains(&event_id));
            assert!(
                store.delivered.lock().await.is_empty(),
                "{event_type} {case} allowed decision must fail before acknowledgement"
            );
            assert!(
                graph.events.lock().await.is_empty(),
                "{event_type} {case} allowed decision must fail before graph projection"
            );
            assert!(
                lineage.events.lock().await.is_empty(),
                "{event_type} {case} allowed decision must fail before lineage projection"
            );
        }
    }
}

#[tokio::test]
async fn outbox_drain_rejects_missing_table_commit_evidence_before_projection() {
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let principal = Principal::new("agent:writer", PrincipalKind::Agent).unwrap();
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-missing-commit-evidence".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "table.commit".to_string(),
            payload: json!({
                "audit-event-id": "audit-missing-commit-evidence",
                "event-type": "table.commit",
                "table": table,
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
        .expect_err("missing table commit evidence should fail before delivery");

    let message = err.to_string();
    assert!(message.contains("table.commit"));
    assert!(message.contains("table commit evidence must contain commit"));
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains("evt-missing-commit-evidence"));
    assert!(
        store.delivered.lock().await.is_empty(),
        "missing commit evidence must fail before acknowledgement"
    );
    assert!(
        graph.events.lock().await.is_empty(),
        "missing commit evidence must fail before graph projection"
    );
    assert!(
        lineage.events.lock().await.is_empty(),
        "missing commit evidence must fail before lineage projection"
    );
}

#[tokio::test]
async fn outbox_drain_rejects_zero_table_commit_sequence_before_projection() {
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let principal = Principal::new("agent:writer", PrincipalKind::Agent).unwrap();
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-zero-commit-sequence".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "table.commit".to_string(),
            payload: json!({
                "audit-event-id": "audit-zero-commit-sequence",
                "event-type": "table.commit",
                "table": table,
                "commit": {
                    "table": table,
                    "previous_metadata_location": "file:///tmp/events/metadata/00000.json",
                    "new_metadata_location": "file:///tmp/events/metadata/00001.json",
                    "sequence_number": 0,
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
        .expect_err("zero table commit sequence should fail before delivery");

    let message = err.to_string();
    assert!(message.contains("table.commit"));
    assert!(message.contains("table commit evidence sequence number must be positive"));
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains("evt-zero-commit-sequence"));
    assert!(
        store.delivered.lock().await.is_empty(),
        "zero commit sequence must fail before acknowledgement"
    );
    assert!(
        graph.events.lock().await.is_empty(),
        "zero commit sequence must fail before graph projection"
    );
    assert!(
        lineage.events.lock().await.is_empty(),
        "zero commit sequence must fail before lineage projection"
    );
}

#[tokio::test]
async fn outbox_drain_rejects_missing_table_commit_new_metadata_location_before_projection() {
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let principal = Principal::new("agent:writer", PrincipalKind::Agent).unwrap();
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-missing-commit-new-metadata".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "table.commit".to_string(),
            payload: json!({
                "audit-event-id": "audit-missing-commit-new-metadata",
                "event-type": "table.commit",
                "table": table,
                "commit": {
                    "table": table,
                    "previous_metadata_location": "file:///tmp/events/metadata/00000.json",
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
        .expect_err("missing table commit metadata pointer should fail before delivery");

    let message = err.to_string();
    assert!(message.contains("table.commit"));
    assert!(message.contains("table commit evidence must contain non-empty new metadata location"));
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains("evt-missing-commit-new-metadata"));
    assert!(
        store.delivered.lock().await.is_empty(),
        "missing commit metadata pointer must fail before acknowledgement"
    );
    assert!(
        graph.events.lock().await.is_empty(),
        "missing commit metadata pointer must fail before graph projection"
    );
    assert!(
        lineage.events.lock().await.is_empty(),
        "missing commit metadata pointer must fail before lineage projection"
    );
}

#[tokio::test]
async fn outbox_drain_rejects_blank_table_commit_new_metadata_location_before_projection() {
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let principal = Principal::new("agent:writer", PrincipalKind::Agent).unwrap();
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-blank-commit-new-metadata".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "table.commit".to_string(),
            payload: json!({
                "audit-event-id": "audit-blank-commit-new-metadata",
                "event-type": "table.commit",
                "table": table,
                "commit": {
                    "table": table,
                    "previous_metadata_location": "file:///tmp/events/metadata/00000.json",
                    "new_metadata_location": " ",
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
        .expect_err("blank table commit new metadata pointer should fail before delivery");

    let message = err.to_string();
    assert!(message.contains("table.commit"));
    assert!(message.contains("table commit evidence must contain non-empty new metadata location"));
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains("evt-blank-commit-new-metadata"));
    assert!(
        store.delivered.lock().await.is_empty(),
        "blank new commit metadata pointer must fail before acknowledgement"
    );
    assert!(
        graph.events.lock().await.is_empty(),
        "blank new commit metadata pointer must fail before graph projection"
    );
    assert!(
        lineage.events.lock().await.is_empty(),
        "blank new commit metadata pointer must fail before lineage projection"
    );
}

#[tokio::test]
async fn outbox_drain_rejects_blank_table_commit_previous_metadata_location_before_projection() {
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let principal = Principal::new("agent:writer", PrincipalKind::Agent).unwrap();
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-blank-commit-previous-metadata".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "table.commit".to_string(),
            payload: json!({
                "audit-event-id": "audit-blank-commit-previous-metadata",
                "event-type": "table.commit",
                "table": table,
                "commit": {
                    "table": table,
                    "previous_metadata_location": " ",
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
        .expect_err("blank table commit previous metadata pointer should fail before delivery");

    let message = err.to_string();
    assert!(message.contains("table.commit"));
    assert!(message.contains(
        "table commit evidence previous metadata location must be non-empty when present"
    ));
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains("evt-blank-commit-previous-metadata"));
    assert!(
        store.delivered.lock().await.is_empty(),
        "blank previous commit metadata pointer must fail before acknowledgement"
    );
    assert!(
        graph.events.lock().await.is_empty(),
        "blank previous commit metadata pointer must fail before graph projection"
    );
    assert!(
        lineage.events.lock().await.is_empty(),
        "blank previous commit metadata pointer must fail before lineage projection"
    );
}

#[tokio::test]
async fn outbox_drain_rejects_duplicate_pending_event_ids_before_projection() {
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let principal = Principal {
        subject: "agent:writer".to_string(),
        kind: PrincipalKind::Agent,
    };
    let payload = json!({
        "audit-event-id": "audit-duplicate-id",
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
            "version": 0,
        }
    });
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![
            OutboxEvent {
                event_id: "evt-duplicate".to_string(),
                sink: "lakecat.lineage-and-graph".to_string(),
                event_type: "table.created".to_string(),
                payload: payload.clone(),
                created_at: chrono::Utc::now(),
                delivered_at: None,
            },
            OutboxEvent {
                event_id: "evt-duplicate".to_string(),
                sink: "lakecat.lineage-and-graph".to_string(),
                event_type: "table.created".to_string(),
                payload,
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

    let err = drain_outbox_once(&state, 10)
        .await
        .expect_err("duplicate pending outbox event ids must fail before projection");

    let message = err.to_string();
    assert!(message.contains("outbox pending batch contained duplicate event id hash"));
    assert!(message.contains("sha256:"));
    assert!(
        !message.contains("evt-duplicate"),
        "duplicate event id should be redacted from the operator-facing error"
    );
    assert!(
        store.delivered.lock().await.is_empty(),
        "duplicate pending ids must fail before acknowledgement"
    );
    assert!(
        graph.events.lock().await.is_empty(),
        "duplicate pending ids must fail before graph projection"
    );
    assert!(
        lineage.events.lock().await.is_empty(),
        "duplicate pending ids must fail before lineage projection"
    );
}

#[tokio::test]
async fn outbox_drain_rejects_malformed_namespace_list_evidence() {
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-secret-namespace-list-token".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "namespace.listed".to_string(),
            payload: json!({
                "audit-event-id": "audit-corrupt-namespace-list",
                "event-type": "namespace.listed",
                "payload": {
                    "warehouse": "local",
                    "namespace-count": "two",
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
        .expect_err("malformed namespace-list evidence should fail");

    let message = err.to_string();
    assert!(
        message.contains("outbox event namespace.listed (lakecat.lineage-and-graph) has invalid")
    );
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(message.contains("namespace list evidence must contain unsigned namespace-count"));
    assert!(!message.contains("evt-secret-namespace-list-token"));
    assert!(
        store.delivered.lock().await.is_empty(),
        "malformed namespace-list evidence must fail before acknowledgement"
    );
    assert!(
        graph.events.lock().await.is_empty(),
        "malformed namespace-list evidence must fail before graph projection"
    );
    assert!(
        lineage.events.lock().await.is_empty(),
        "malformed namespace-list evidence must fail before lineage projection"
    );
}

#[tokio::test]
async fn outbox_drain_rejects_malformed_namespace_list_path_evidence() {
    let principal = Principal::new("agent:reader", PrincipalKind::Agent).unwrap();
    let cases = vec![
        (
            "missing-paths",
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
            }),
            "namespace list evidence must contain namespace-paths",
        ),
        (
            "count-mismatch",
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
                "namespace-count": 2,
                "namespace-paths": ["default"],
            }),
            "namespace list namespace-paths count must match namespace list count",
        ),
        (
            "invalid-path",
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
                "namespace-paths": ["analytics/../../secret"],
            }),
            "namespace list namespace-paths contains an invalid namespace",
        ),
        (
            "duplicate-path",
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
                "namespace-count": 2,
                "namespace-paths": ["default", "default"],
            }),
            "namespace list namespace-paths must not contain duplicate namespaces",
        ),
    ];

    for (label, payload, expected_message) in cases {
        let event_id = format!("evt-namespace-list-{label}");
        let store = Arc::new(RecordingOutboxStore {
            events: Mutex::new(vec![OutboxEvent {
                event_id: event_id.clone(),
                sink: "lakecat.lineage-and-graph".to_string(),
                event_type: "namespace.listed".to_string(),
                payload: json!({
                    "audit-event-id": format!("audit-namespace-list-{label}"),
                    "event-type": "namespace.listed",
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
            .expect_err("malformed namespace-list path evidence should fail");

        let message = err.to_string();
        assert!(message.contains("namespace.listed"));
        assert!(message.contains(expected_message), "{message}");
        assert!(message.contains("event-id-hash=sha256:"));
        assert!(!message.contains(&event_id));
        assert!(
            store.delivered.lock().await.is_empty(),
            "malformed namespace-list path evidence must fail before acknowledgement"
        );
        assert!(
            graph.events.lock().await.is_empty(),
            "malformed namespace-list path evidence must fail before graph projection"
        );
        assert!(
            lineage.events.lock().await.is_empty(),
            "malformed namespace-list path evidence must fail before lineage projection"
        );
    }
}

#[tokio::test]
async fn outbox_drain_rejects_malformed_management_list_count_evidence() {
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-secret-storage-profile-list-token".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "storage-profile.listed".to_string(),
            payload: json!({
                "audit-event-id": "audit-corrupt-storage-profile-list",
                "event-type": "storage-profile.listed",
                "payload": {
                    "warehouse": "local",
                    "storage-profile-count": -1,
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
        .expect_err("malformed management-list count evidence should fail");

    let message = err.to_string();
    assert!(
        message.contains(
            "outbox event storage-profile.listed (lakecat.lineage-and-graph) has invalid"
        )
    );
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(
        message
            .contains("storage-profile list evidence must contain unsigned storage-profile-count")
    );
    assert!(!message.contains("evt-secret-storage-profile-list-token"));
    assert!(
        store.delivered.lock().await.is_empty(),
        "malformed management-list evidence must fail before acknowledgement"
    );
    assert!(
        graph.events.lock().await.is_empty(),
        "malformed management-list evidence must fail before graph projection"
    );
    assert!(
        lineage.events.lock().await.is_empty(),
        "malformed management-list evidence must fail before lineage projection"
    );
}

#[tokio::test]
async fn outbox_drain_rejects_extra_namespace_list_fields() {
    let principal = Principal::new("agent:operator", PrincipalKind::Agent).unwrap();
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-namespace-list-extra-field".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "namespace.listed".to_string(),
            payload: json!({
                "audit-event-id": "audit-namespace-list-extra-field",
                "event-type": "namespace.listed",
                "payload": {
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
                    "unverified-namespace-claim": "shadow",
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
        .expect_err("extra namespace-list fields should fail before delivery");

    let message = err.to_string();
    assert!(
        message.contains("outbox event namespace.listed (lakecat.lineage-and-graph) has invalid")
    );
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(
        message.contains("namespace list contains unexpected field unverified-namespace-claim")
    );
    assert!(!message.contains("evt-namespace-list-extra-field"));
    assert!(
        store.delivered.lock().await.is_empty(),
        "extra namespace-list fields must fail before acknowledgement"
    );
    assert!(
        graph.events.lock().await.is_empty(),
        "extra namespace-list fields must fail before graph projection"
    );
    assert!(
        lineage.events.lock().await.is_empty(),
        "extra namespace-list fields must fail before lineage projection"
    );
}

#[tokio::test]
async fn outbox_drain_rejects_extra_management_list_fields() {
    let principal = Principal::new("agent:operator", PrincipalKind::Agent).unwrap();
    let cases = vec![
        (
            "policy-binding.listed",
            "policy-binding",
            "policy-binding list contains unexpected field unverified-management-claim",
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
                "unverified-management-claim": "shadow",
            }),
        ),
        (
            "project.listed",
            "project",
            "project list contains unexpected field unverified-management-claim",
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
                "unverified-management-claim": "shadow",
            }),
        ),
        (
            "server.listed",
            "server",
            "server list contains unexpected field unverified-management-claim",
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
                "unverified-management-claim": "shadow",
            }),
        ),
        (
            "storage-profile.listed",
            "storage-profile",
            "storage-profile list contains unexpected field unverified-management-claim",
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
                "unverified-management-claim": "shadow",
            }),
        ),
        (
            "warehouse.listed",
            "warehouse",
            "warehouse list contains unexpected field unverified-management-claim",
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
                "unverified-management-claim": "shadow",
            }),
        ),
    ];

    for (event_type, label, expected_message, payload) in cases {
        let event_id = format!("evt-{label}-list-extra-field");
        let store = Arc::new(RecordingOutboxStore {
            events: Mutex::new(vec![OutboxEvent {
                event_id: event_id.clone(),
                sink: "lakecat.lineage-and-graph".to_string(),
                event_type: event_type.to_string(),
                payload: json!({
                    "audit-event-id": format!("audit-{label}-list-extra-field"),
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
            .expect_err("extra management-list fields should fail before delivery");

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
            "extra management-list fields must fail before acknowledgement"
        );
        assert!(
            graph.events.lock().await.is_empty(),
            "extra management-list fields must fail before graph projection"
        );
        assert!(
            lineage.events.lock().await.is_empty(),
            "extra management-list fields must fail before lineage projection"
        );
    }
}

#[tokio::test]
async fn outbox_drain_rejects_malformed_management_list_scope_evidence() {
    let cases = vec![
        (
            "evt-secret-warehouse-list-token",
            json!(42),
            "warehouse list project-id must be a string when present",
        ),
        (
            "evt-blank-warehouse-list-project",
            json!(" "),
            "warehouse list project-id must be non-empty when present",
        ),
        (
            "evt-invalid-warehouse-list-project",
            json!("analytics/../../secret"),
            "warehouse list project-id contains an invalid identifier",
        ),
    ];

    for (event_id, project_id, expected_message) in cases {
        let store = Arc::new(RecordingOutboxStore {
            events: Mutex::new(vec![OutboxEvent {
                event_id: event_id.to_string(),
                sink: "lakecat.lineage-and-graph".to_string(),
                event_type: "warehouse.listed".to_string(),
                payload: json!({
                    "audit-event-id": "audit-corrupt-warehouse-list",
                    "event-type": "warehouse.listed",
                    "payload": {
                        "project-id": project_id,
                        "warehouse-count": 1,
                        "warehouse-names": ["local"],
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
            .expect_err("malformed management-list scope evidence should fail");

        let message = err.to_string();
        assert!(
            message
                .contains("outbox event warehouse.listed (lakecat.lineage-and-graph) has invalid")
        );
        assert!(message.contains("event-id-hash=sha256:"));
        assert!(message.contains(expected_message), "{message}");
        assert!(!message.contains(event_id));
        assert!(
            store.delivered.lock().await.is_empty(),
            "malformed management-list evidence must fail before acknowledgement"
        );
        assert!(
            graph.events.lock().await.is_empty(),
            "malformed management-list evidence must fail before graph projection"
        );
        assert!(
            lineage.events.lock().await.is_empty(),
            "malformed management-list evidence must fail before lineage projection"
        );
    }
}

#[tokio::test]
async fn outbox_drain_rejects_missing_management_list_ids() {
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-missing-management-list-ids".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "storage-profile.listed".to_string(),
            payload: json!({
                "audit-event-id": "audit-missing-management-list-ids",
                "event-type": "storage-profile.listed",
                "payload": {
                    "warehouse": "local",
                    "storage-profile-count": 1,
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
        .expect_err("missing management-list IDs should fail before delivery");

    let message = err.to_string();
    assert!(
        message.contains(
            "outbox event storage-profile.listed (lakecat.lineage-and-graph) has invalid"
        )
    );
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(message.contains("storage-profile list evidence must contain storage-profile-ids"));
    assert!(!message.contains("evt-missing-management-list-ids"));
    assert!(
        store.delivered.lock().await.is_empty(),
        "missing management-list IDs must fail before acknowledgement"
    );
    assert!(
        graph.events.lock().await.is_empty(),
        "missing management-list IDs must fail before graph projection"
    );
    assert!(
        lineage.events.lock().await.is_empty(),
        "missing management-list IDs must fail before lineage projection"
    );
}

#[tokio::test]
async fn outbox_drain_rejects_management_list_id_count_mismatch() {
    let principal = Principal::new("agent:operator", PrincipalKind::Agent).unwrap();
    let cases = vec![
        (
            "policy-binding.listed",
            "policy-binding",
            "policy-ids",
            json!({
                "authorization-receipt": {
                    "principal": principal,
                    "action": "server-manage",
                    "allowed": true,
                    "engine": "test",
                    "policy_hash": null,
                    "checked_at": chrono::Utc::now(),
                },
                "warehouse": "local",
                "policy-count": 2,
                "policy-ids": ["restricted-events"],
            }),
        ),
        (
            "project.listed",
            "project",
            "project-ids",
            json!({
                "authorization-receipt": {
                    "principal": principal,
                    "action": "server-manage",
                    "allowed": true,
                    "engine": "test",
                    "policy_hash": null,
                    "checked_at": chrono::Utc::now(),
                },
                "project-count": 2,
                "project-ids": ["analytics"],
            }),
        ),
        (
            "server.listed",
            "server",
            "server-ids",
            json!({
                "authorization-receipt": {
                    "principal": principal,
                    "action": "server-manage",
                    "allowed": true,
                    "engine": "test",
                    "policy_hash": null,
                    "checked_at": chrono::Utc::now(),
                },
                "server-count": 2,
                "server-ids": ["prod-us"],
            }),
        ),
        (
            "storage-profile.listed",
            "storage-profile",
            "storage-profile-ids",
            json!({
                "authorization-receipt": {
                    "principal": principal,
                    "action": "server-manage",
                    "allowed": true,
                    "engine": "test",
                    "policy_hash": null,
                    "checked_at": chrono::Utc::now(),
                },
                "warehouse": "local",
                "storage-profile-count": 2,
                "storage-profile-ids": ["events-local"],
            }),
        ),
        (
            "warehouse.listed",
            "warehouse",
            "warehouse-names",
            json!({
                "authorization-receipt": {
                    "principal": principal,
                    "action": "server-manage",
                    "allowed": true,
                    "engine": "test",
                    "policy_hash": null,
                    "checked_at": chrono::Utc::now(),
                },
                "project-id": "analytics",
                "warehouse-count": 2,
                "warehouse-names": ["local"],
            }),
        ),
    ];

    for (event_type, label, id_field, payload) in cases {
        let event_id = format!("evt-{label}-list-count-mismatch");
        let store = Arc::new(RecordingOutboxStore {
            events: Mutex::new(vec![OutboxEvent {
                event_id: event_id.clone(),
                sink: "lakecat.lineage-and-graph".to_string(),
                event_type: event_type.to_string(),
                payload: json!({
                    "audit-event-id": format!("audit-{label}-list-count-mismatch"),
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
            .expect_err("management-list ID count mismatch should fail before delivery");

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
                "{label} list {id_field} count must match {label} list count"
            )),
            "{event_type} should reject count-mismatched management IDs: {message}"
        );
        assert!(!message.contains(&event_id));
        assert!(
            store.delivered.lock().await.is_empty(),
            "management-list count mismatch must fail before acknowledgement"
        );
        assert!(
            graph.events.lock().await.is_empty(),
            "management-list count mismatch must fail before graph projection"
        );
        assert!(
            lineage.events.lock().await.is_empty(),
            "management-list count mismatch must fail before lineage projection"
        );
    }
}

#[tokio::test]
async fn outbox_drain_rejects_mismatched_management_list_receipt_actions() {
    let principal = Principal::new("agent:operator", PrincipalKind::Agent).unwrap();
    let cases = vec![
        (
            "policy-binding.listed",
            json!({
                "authorization-receipt": {
                    "principal": principal,
                    "action": "table-load",
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
            json!({
                "authorization-receipt": {
                    "principal": principal,
                    "action": "table-load",
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
            json!({
                "authorization-receipt": {
                    "principal": principal,
                    "action": "table-load",
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
            json!({
                "authorization-receipt": {
                    "principal": principal,
                    "action": "table-load",
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
            json!({
                "authorization-receipt": {
                    "principal": principal,
                    "action": "table-load",
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

    for (event_type, payload) in cases {
        let event_id = format!("evt-mismatched-{event_type}-action-token");
        let store = Arc::new(RecordingOutboxStore {
            events: Mutex::new(vec![OutboxEvent {
                event_id: event_id.clone(),
                sink: "lakecat.lineage-and-graph".to_string(),
                event_type: event_type.to_string(),
                payload: json!({
                    "audit-event-id": format!("audit-mismatched-{event_type}-action"),
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
            .expect_err("mismatched management-list receipt action should fail");

        let message = err.to_string();
        assert!(
            message.contains(&format!(
                "outbox event {event_type} (lakecat.lineage-and-graph) has invalid"
            )),
            "{event_type} should be identified in the validation error"
        );
        assert!(message.contains("event-id-hash=sha256:"));
        assert!(
            message.contains(
                "management-list authorization receipt action does not match outbox event type"
            ),
            "{event_type} should reject mismatched receipt action: {message}"
        );
        assert!(!message.contains(&event_id));
        assert!(
            store.delivered.lock().await.is_empty(),
            "management-list action mismatch must fail before acknowledgement"
        );
        assert!(
            graph.events.lock().await.is_empty(),
            "management-list action mismatch must fail before graph projection"
        );
        assert!(
            lineage.events.lock().await.is_empty(),
            "management-list action mismatch must fail before lineage projection"
        );
    }
}

#[tokio::test]
async fn outbox_drain_rejects_missing_or_denied_management_list_receipt_allowed_decision() {
    let principal = Principal::new("agent:operator", PrincipalKind::Agent).unwrap();
    for (event_type, action, payload_without_receipt) in [
        (
            "policy-binding.listed",
            "policy-manage",
            json!({
                "warehouse": "local",
                "policy-count": 1,
                "policy-ids": ["restricted-events"],
            }),
        ),
        (
            "project.listed",
            "project-manage",
            json!({
                "project-count": 1,
                "project-ids": ["analytics"],
            }),
        ),
        (
            "server.listed",
            "server-manage",
            json!({
                "server-count": 1,
                "server-ids": ["prod-us"],
            }),
        ),
        (
            "storage-profile.listed",
            "storage-profile-manage",
            json!({
                "warehouse": "local",
                "storage-profile-count": 1,
                "storage-profile-ids": ["events-local"],
            }),
        ),
        (
            "warehouse.listed",
            "warehouse-manage",
            json!({
                "project-id": "analytics",
                "warehouse-count": 1,
                "warehouse-names": ["local"],
            }),
        ),
    ] {
        for (case, allowed, expected_message) in [
            (
                "missing",
                None,
                "management-list evidence must contain authorization receipt allowed decision",
            ),
            (
                "denied",
                Some(false),
                "management-list authorization receipt must allow replay projection",
            ),
        ] {
            let mut receipt = json!({
                "principal": principal,
                "action": action,
                "engine": "test",
                "policy_hash": null,
                "checked_at": chrono::Utc::now(),
            });
            if let Some(allowed) = allowed {
                receipt["allowed"] = json!(allowed);
            }
            let mut payload = payload_without_receipt.clone();
            payload["authorization-receipt"] = receipt;
            let event_id = format!(
                "evt-{case}-{}-receipt-allowed",
                event_type.replace('.', "-")
            );
            let store = Arc::new(RecordingOutboxStore {
                events: Mutex::new(vec![OutboxEvent {
                    event_id: event_id.clone(),
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
                .expect_err("missing or denied management-list receipt decision should fail");

            let message = err.to_string();
            assert!(message.contains(&format!(
                "outbox event {event_type} (lakecat.lineage-and-graph) has invalid"
            )));
            assert!(message.contains(expected_message), "{message}");
            assert!(message.contains("event-id-hash=sha256:"));
            assert!(!message.contains(&event_id));
            assert!(
                store.delivered.lock().await.is_empty(),
                "{event_type} {case} receipt decision must fail before acknowledgement"
            );
            assert!(
                graph.events.lock().await.is_empty(),
                "{event_type} {case} receipt decision must fail before graph projection"
            );
            assert!(
                lineage.events.lock().await.is_empty(),
                "{event_type} {case} receipt decision must fail before lineage projection"
            );
        }
    }
}

#[tokio::test]
async fn outbox_drain_rejects_missing_management_list_receipt_principal() {
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-secret-management-list-principal-token".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "server.listed".to_string(),
            payload: json!({
                "audit-event-id": "audit-missing-management-list-principal",
                "event-type": "server.listed",
                "payload": {
                    "authorization-receipt": {
                        "action": "server-manage",
                        "allowed": true,
                        "engine": "test",
                        "policy_hash": null,
                        "checked_at": chrono::Utc::now(),
                    },
                    "server-count": 1,
                    "server-ids": ["prod-us"],
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
        .expect_err("missing management-list receipt principal should fail before delivery");

    let message = err.to_string();
    assert!(message.contains("outbox event server.listed (lakecat.lineage-and-graph) has invalid"));
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(
        message.contains("management-list evidence must contain authorization receipt principal")
    );
    assert!(!message.contains("evt-secret-management-list-principal-token"));
    assert!(
        store.delivered.lock().await.is_empty(),
        "missing management-list principal must fail before acknowledgement"
    );
    assert!(
        graph.events.lock().await.is_empty(),
        "missing management-list principal must fail before graph projection"
    );
    assert!(
        lineage.events.lock().await.is_empty(),
        "missing management-list principal must fail before lineage projection"
    );
}

#[tokio::test]
async fn outbox_drain_rejects_malformed_management_list_receipt_principal() {
    let cases = vec![
        (
            "policy-binding.listed",
            "policy-binding",
            json!({
                "warehouse": "local",
                "policy-count": 1,
                "policy-ids": ["restricted-events"],
            }),
        ),
        (
            "project.listed",
            "project",
            json!({
                "project-count": 1,
                "project-ids": ["analytics"],
            }),
        ),
        (
            "server.listed",
            "server",
            json!({
                "server-count": 1,
                "server-ids": ["prod-us"],
            }),
        ),
        (
            "storage-profile.listed",
            "storage-profile",
            json!({
                "warehouse": "local",
                "storage-profile-count": 1,
                "storage-profile-ids": ["s3-events"],
            }),
        ),
        (
            "warehouse.listed",
            "warehouse",
            json!({
                "project-id": "analytics",
                "warehouse-count": 1,
                "warehouse-names": ["local"],
            }),
        ),
    ];

    for (event_type, label, mut payload) in cases {
        payload["authorization-receipt"] = json!({
            "principal": {
                "subject": "agent:operator",
                "kind": "unknown",
            },
            "action": "server-manage",
            "allowed": true,
            "engine": "test",
            "policy_hash": null,
            "checked_at": chrono::Utc::now(),
        });
        let event_id = format!("evt-malformed-{label}-list-principal");
        let store = Arc::new(RecordingOutboxStore {
            events: Mutex::new(vec![OutboxEvent {
                event_id: event_id.clone(),
                sink: "lakecat.lineage-and-graph".to_string(),
                event_type: event_type.to_string(),
                payload: json!({
                    "audit-event-id": format!("audit-malformed-{label}-list-principal"),
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
            .expect_err("malformed management-list receipt principal should fail");

        let message = err.to_string();
        assert!(
            message.contains(&format!(
                "outbox event {event_type} (lakecat.lineage-and-graph) has invalid"
            )),
            "{event_type} should be identified in the validation error"
        );
        assert!(message.contains("event-id-hash=sha256:"));
        assert!(
            message.contains("management-list authorization receipt principal"),
            "{event_type} should identify malformed authorization receipt principal evidence"
        );
        assert!(message.contains("must be a valid principal"));
        assert!(!message.contains(&event_id));
        assert!(
            store.delivered.lock().await.is_empty(),
            "malformed management-list principal must fail before acknowledgement"
        );
        assert!(
            graph.events.lock().await.is_empty(),
            "malformed management-list principal must fail before graph projection"
        );
        assert!(
            lineage.events.lock().await.is_empty(),
            "malformed management-list principal must fail before lineage projection"
        );
    }
}

#[tokio::test]
async fn outbox_drain_rejects_malformed_management_list_id_evidence() {
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-secret-project-list-token".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "project.listed".to_string(),
            payload: json!({
                "audit-event-id": "audit-corrupt-project-list",
                "event-type": "project.listed",
                "payload": {
                    "project-count": 1,
                    "project-ids": ["analytics/../../secret"],
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
        .expect_err("malformed management-list ID evidence should fail");

    let message = err.to_string();
    assert!(
        message.contains("outbox event project.listed (lakecat.lineage-and-graph) has invalid")
    );
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(message.contains("project list project-ids contains an invalid identifier"));
    assert!(!message.contains("evt-secret-project-list-token"));
    assert!(
        store.delivered.lock().await.is_empty(),
        "malformed management-list ID evidence must fail before acknowledgement"
    );
    assert!(
        graph.events.lock().await.is_empty(),
        "malformed management-list ID evidence must fail before graph projection"
    );
    assert!(
        lineage.events.lock().await.is_empty(),
        "malformed management-list ID evidence must fail before lineage projection"
    );
}

#[tokio::test]
async fn outbox_drain_rejects_extra_authorization_receipt_context_policy_binding_fields() {
    let principal = Principal::new("agent:reader", PrincipalKind::Agent).unwrap();
    let event_id = "evt-config-extra-auth-policy-binding-field";
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
                            "policy-bindings": [{
                                "policy-id": "agent-read",
                                "warehouse": "local",
                                "namespace": ["default"],
                                "table": "events",
                                "enforced": true,
                                "odrl": {"uid": "policy:agent-read"},
                                "unverified-policy-context-claim": "shadow",
                            }],
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
        .expect_err("extra authorization receipt context policy-binding fields should fail");

    let message = err.to_string();
    assert!(message.contains("catalog.config-read"));
    assert!(
        message.contains(
            "catalog config-read authorization receipt context policy-bindings contains unexpected field unverified-policy-context-claim"
        ),
        "catalog config-read error should reject extra policy-binding context fields: {message}"
    );
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains(event_id));
    assert!(
        store.delivered.lock().await.is_empty(),
        "policy-binding context schema failures must fail before acknowledgement"
    );
    assert!(
        graph.events.lock().await.is_empty(),
        "policy-binding context schema failures must fail before graph projection"
    );
    assert!(
        lineage.events.lock().await.is_empty(),
        "policy-binding context schema failures must fail before lineage projection"
    );
}

#[tokio::test]
async fn outbox_drain_rejects_extra_namespace_receipt_context_policy_binding_fields() {
    let principal = Principal::new("agent:reader", PrincipalKind::Agent).unwrap();
    let event_id = "evt-namespace-extra-auth-policy-binding-field";
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: event_id.to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "namespace.listed".to_string(),
            payload: json!({
                "audit-event-id": format!("audit-{event_id}"),
                "event-type": "namespace.listed",
                "payload": {
                    "authorization-receipt": {
                        "principal": principal,
                        "action": "namespace-list",
                        "allowed": true,
                        "engine": "test",
                        "policy_hash": null,
                        "checked_at": chrono::Utc::now(),
                        "context": {
                            "warehouse": "local",
                            "policy-bindings": [{
                                "policy-id": "agent-read",
                                "warehouse": "local",
                                "namespace": ["default"],
                                "table": "events",
                                "enforced": true,
                                "odrl": {"uid": "policy:agent-read"},
                                "unverified-policy-context-claim": "shadow",
                            }],
                        },
                    },
                    "warehouse": "local",
                    "namespace-count": 1,
                    "namespace-paths": ["default"],
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
        .expect_err("extra namespace receipt context policy-binding fields should fail");

    let message = err.to_string();
    assert!(message.contains("namespace.listed"));
    assert!(
        message.contains(
            "namespace list authorization receipt context policy-bindings contains unexpected field unverified-policy-context-claim"
        ),
        "namespace-list error should reject extra policy-binding context fields: {message}"
    );
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains(event_id));
    assert!(
        store.delivered.lock().await.is_empty(),
        "namespace policy-binding context schema failures must fail before acknowledgement"
    );
    assert!(
        graph.events.lock().await.is_empty(),
        "namespace policy-binding context schema failures must fail before graph projection"
    );
    assert!(
        lineage.events.lock().await.is_empty(),
        "namespace policy-binding context schema failures must fail before lineage projection"
    );
}

#[tokio::test]
async fn outbox_drain_rejects_mismatched_namespace_receipt_actions() {
    let principal = Principal::new("agent:reader", PrincipalKind::Agent).unwrap();
    let cases = vec![
        (
            "namespace.listed",
            "namespace list",
            "namespace-create",
            json!({
                "warehouse": "local",
                "namespace-count": 1,
                "namespace-paths": ["default"],
            }),
        ),
        (
            "namespace.created",
            "namespace lifecycle",
            "namespace-list",
            json!({
                "warehouse": "local",
                "namespace": ["default"],
            }),
        ),
        (
            "namespace.loaded",
            "namespace lifecycle",
            "namespace-drop",
            json!({
                "warehouse": "local",
                "namespace": ["default"],
            }),
        ),
        (
            "namespace.dropped",
            "namespace lifecycle",
            "namespace-load",
            json!({
                "warehouse": "local",
                "namespace": ["archived"],
            }),
        ),
    ];

    for (event_type, label, mismatched_action, mut payload) in cases {
        payload["authorization-receipt"] = json!({
            "principal": principal,
            "action": mismatched_action,
            "allowed": true,
            "engine": "test",
            "policy_hash": null,
            "checked_at": chrono::Utc::now(),
        });
        let event_id = format!("evt-{}-mismatched-receipt-action", event_type);
        let store = Arc::new(RecordingOutboxStore {
            events: Mutex::new(vec![OutboxEvent {
                event_id: event_id.clone(),
                sink: "lakecat.lineage-and-graph".to_string(),
                event_type: event_type.to_string(),
                payload: json!({
                    "audit-event-id": format!("audit-{event_type}-mismatched-action"),
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
            .expect_err("mismatched namespace receipt action should fail before delivery");

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
async fn outbox_drain_rejects_unhashed_server_and_warehouse_roots() {
    let principal = Principal::new("agent:operator", PrincipalKind::Agent).unwrap();
    let cases = vec![
        (
            "server.upserted",
            "evt-server-unhashed-endpoint",
            json!({
                "authorization-receipt": {
                    "principal": principal,
                    "action": "server-manage",
                    "allowed": true,
                    "engine": "test",
                    "policy_hash": null,
                    "checked_at": chrono::Utc::now(),
                },
                "server-id": "prod",
                "server-record": {
                    "server-id": "prod",
                    "display-name": "Production",
                    "endpoint-url": "https://lakecat.example",
                    "properties": {"region": "global"}
                }
            }),
            "endpoint-url-hash must contain full SHA-256 digest evidence",
        ),
        (
            "warehouse.upserted",
            "evt-warehouse-unhashed-root",
            json!({
                "authorization-receipt": {
                    "principal": principal,
                    "action": "warehouse-manage",
                    "allowed": true,
                    "engine": "test",
                    "policy_hash": null,
                    "checked_at": chrono::Utc::now(),
                },
                "warehouse": "local",
                "warehouse-record": {
                    "warehouse": "local",
                    "project-id": "default",
                    "storage-root": "file:///tmp/lakecat",
                    "properties": {"region": "local"}
                }
            }),
            "storage-root-hash must contain full SHA-256 digest evidence",
        ),
    ];

    for (event_type, event_id, payload, expected_message) in cases {
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
            .expect_err("raw management endpoint/root evidence should require hash proof");

        let message = err.to_string();
        assert!(message.contains(event_type));
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
async fn outbox_drain_rejects_missing_inner_payload_event_type_before_projection() {
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let payload = json!({
        "audit-event-id": "audit-missing-inner-event-type",
        "event-type": "table.created",
        "table": &table,
        "payload": {
            "version": 1,
            "metadata-location": "file:///tmp/events/metadata/00000.json",
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
    });
    let event_id = content_hash_json(&payload).expect("payload hash should be stable");
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: event_id.clone(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "table.created".to_string(),
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
        .expect_err("missing inner payload event type should fail before delivery");

    let message = err.to_string();
    assert!(message.contains("outbox inner payload missing event-type"));
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains(&event_id));
    assert!(
        store.delivered.lock().await.is_empty(),
        "unbound inner payloads must fail before acknowledgement"
    );
    assert!(
        graph.events.lock().await.is_empty(),
        "unbound inner payloads must fail before graph projection"
    );
    assert!(
        lineage.events.lock().await.is_empty(),
        "unbound inner payloads must fail before lineage projection"
    );
}

#[tokio::test]
async fn outbox_drain_rejects_unknown_event_type_before_projection() {
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-unknown-side-effect".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "table.future-compaction".to_string(),
            payload: json!({
                "audit-event-id": "audit-unknown-side-effect",
                "event-type": "table.future-compaction",
                "payload": {
                    "authorization-receipt": {
                        "principal": {
                            "subject": "agent:writer",
                            "kind": "agent"
                        },
                        "action": "table-maintenance",
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
        .expect_err("unknown outbox event type should fail before delivery");

    let message = err.to_string();
    assert!(message.contains("outbox event table.future-compaction (lakecat.lineage-and-graph)"));
    assert!(message.contains("outbox event type is not supported for projection"));
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains("evt-unknown-side-effect"));
    assert!(
        store.delivered.lock().await.is_empty(),
        "unknown event types must remain pending instead of being acknowledged"
    );
    assert!(
        graph.events.lock().await.is_empty(),
        "unknown event types must fail before graph projection"
    );
    assert!(
        lineage.events.lock().await.is_empty(),
        "unknown event types must fail before lineage projection"
    );
}

#[tokio::test]
async fn outbox_drain_orders_pending_batch_before_projection() {
    let principal = Principal::new("agent:reader", PrincipalKind::Agent).unwrap();
    let make_event = |event_id: &str, warehouse: &str, created_at: &str| OutboxEvent {
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
                },
                "warehouse": warehouse,
                "defaults": catalog_config_defaults_json(),
                "endpoints": catalog_config_endpoints_json(),
            }
        }),
        created_at: created_at.parse().unwrap(),
        delivered_at: None,
    };
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![
            make_event("evt-sort-late", "analytics-late", "2026-01-01T00:00:01Z"),
            make_event("evt-sort-b", "analytics-b", "2026-01-01T00:00:00Z"),
            make_event("evt-sort-a", "analytics-a", "2026-01-01T00:00:00Z"),
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

    let drain = drain_outbox_once(&state, 2).await.unwrap();

    assert_eq!(drain.delivered, 2);
    assert_eq!(
        drain
            .events
            .iter()
            .map(|event| event.event_id.as_str())
            .collect::<Vec<_>>(),
        vec!["evt-sort-a", "evt-sort-b"]
    );
    assert_eq!(
        store.delivered.lock().await.as_slice(),
        &["evt-sort-a".to_string(), "evt-sort-b".to_string()]
    );
    assert!(
        !store
            .delivered
            .lock()
            .await
            .iter()
            .any(|event_id| event_id == "evt-sort-late")
    );
    let lineage_events = lineage.events.lock().await;
    assert_eq!(lineage_events.len(), 2);
    assert_eq!(
        lineage_events[0].payload["warehouse"],
        serde_json::json!("analytics-a")
    );
    assert_eq!(
        lineage_events[1].payload["warehouse"],
        serde_json::json!("analytics-b")
    );
}

#[tokio::test]
async fn outbox_drain_projects_catalog_config_reads_to_graph_and_lineage() {
    let principal = Principal::new("agent:reader", PrincipalKind::Agent).unwrap();
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-config-read".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "catalog.config-read".to_string(),
            payload: json!({
                "audit-event-id": "audit-config-read",
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
                        "storage-root-hash": content_hash_json(&json!({
                            "storage-root": "file:///tmp/lakecat/config"
                        })).unwrap(),
                        "properties": {"purpose": "config-read"}
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

    let drain = drain_outbox_once(&state, 10).await.unwrap();
    assert_eq!(drain.delivered, 1);
    assert_eq!(drain.event_types, vec!["catalog.config-read".to_string()]);
    assert_eq!(drain.graph_events, 2);
    assert_eq!(drain.lineage_events, 1);
    assert_eq!(
        store.delivered.lock().await.as_slice(),
        &["evt-config-read".to_string()]
    );
    assert_eq!(drain.events.len(), 1);
    assert_eq!(drain.events[0].graph_events, 2);
    assert_eq!(drain.events[0].lineage_events, 1);
    assert_config_defaults_include(
        &serde_json::to_value(&drain.events[0].catalog_config_defaults).unwrap(),
        "lakecat.format.v4.typed-sail",
        "unavailable",
    );
    assert_config_endpoints_include(
        &serde_json::to_value(&drain.events[0].catalog_config_endpoints).unwrap(),
        "GET /querygraph/v1/bootstrap",
    );

    let graph_events = graph.events.lock().await;
    assert_eq!(graph_events.len(), 2);
    assert_eq!(graph_events[0].label, GraphNodeLabel::Principal);
    assert_eq!(
        graph_events[0].event_id.as_deref(),
        Some("evt-config-read:principal")
    );
    assert_eq!(graph_events[1].label, GraphNodeLabel::Warehouse);
    assert_eq!(graph_events[1].action, GraphAction::Loaded);
    assert_eq!(graph_events[1].subject, "lakecat:warehouse:local");
    assert_eq!(graph_events[1].event_id.as_deref(), Some("evt-config-read"));
    assert_eq!(
        graph_events[1].properties["authorization-receipt"]["principal"]["subject"],
        serde_json::json!("agent:reader")
    );
    assert!(
        graph_events[1].properties["warehouse-record"]
            .get("storage-root")
            .is_none(),
        "catalog config graph projection must not expose raw storage roots"
    );
    assert_eq!(
        graph_events[1].properties["warehouse-record"]["storage-root-hash"],
        serde_json::json!(
            content_hash_json(&json!({"storage-root": "file:///tmp/lakecat/config"})).unwrap()
        )
    );
    assert_config_defaults_include(
        &graph_events[1].properties["defaults"],
        "lakecat.format.v4.bridge",
        "json-passthrough",
    );
    let graph_payload = serde_json::to_string(&graph_events[1].properties).unwrap();
    assert!(!graph_payload.contains("file:///tmp/lakecat/config"));
    drop(graph_events);

    let lineage_events = lineage.events.lock().await;
    assert_eq!(lineage_events.len(), 1);
    assert_eq!(
        lineage_events[0].event_type,
        LineageEventType::CatalogConfigRead
    );
    assert_eq!(
        lineage_events[0].payload["warehouse"],
        serde_json::json!("local")
    );
    assert!(
        lineage_events[0].payload["warehouse-record"]
            .get("storage-root")
            .is_none(),
        "catalog config lineage projection must not expose raw storage roots"
    );
    assert_eq!(
        lineage_events[0].payload["warehouse-record"]["storage-root-hash"],
        serde_json::json!(
            content_hash_json(&json!({"storage-root": "file:///tmp/lakecat/config"})).unwrap()
        )
    );
    assert_config_defaults_include(
        &lineage_events[0].payload["defaults"],
        "lakecat.format.v4.typed-sail",
        "unavailable",
    );
    let lineage_payload = serde_json::to_string(&lineage_events[0].payload).unwrap();
    assert!(!lineage_payload.contains("file:///tmp/lakecat/config"));
}

#[tokio::test]
async fn outbox_drain_projects_table_restores_to_graph_and_lineage() {
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let principal = Principal::new("agent:operator", PrincipalKind::Agent).unwrap();
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-table-restore".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "table.restored".to_string(),
            payload: json!({
                "audit-event-id": "audit-table-restore",
                "event-type": "table.restored",
                "table": table,
                "payload": {
                    "authorization-receipt": {
                        "principal": principal,
                        "action": "table-restore",
                        "allowed": true,
                        "engine": "test",
                        "policy_hash": null,
                        "checked_at": chrono::Utc::now(),
                    },
                    "warehouse": "local",
                    "namespace": ["default"],
                    "table": "events",
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

    let drain = drain_outbox_once(&state, 10).await.unwrap();
    assert_eq!(drain.delivered, 1);
    assert_eq!(drain.event_types, vec!["table.restored".to_string()]);
    assert_eq!(drain.graph_events, 2);
    assert_eq!(drain.lineage_events, 1);
    assert_eq!(
        store.delivered.lock().await.as_slice(),
        &["evt-table-restore".to_string()]
    );
    assert_eq!(drain.events.len(), 1);
    assert_eq!(drain.events[0].graph_events, 2);
    assert_eq!(drain.events[0].lineage_events, 1);

    let graph_events = graph.events.lock().await;
    assert_eq!(graph_events.len(), 2);
    assert_eq!(graph_events[0].label, GraphNodeLabel::Principal);
    assert_eq!(
        graph_events[0].event_id.as_deref(),
        Some("evt-table-restore:principal")
    );
    assert_eq!(graph_events[1].label, GraphNodeLabel::Table);
    assert_eq!(graph_events[1].action, GraphAction::Loaded);
    assert_eq!(
        graph_events[1].subject,
        "lakecat:table:local:default:events"
    );
    assert_eq!(
        graph_events[1].event_id.as_deref(),
        Some("evt-table-restore")
    );
    assert_eq!(
        graph_events[1].properties["metadata-location"],
        serde_json::json!("file:///tmp/events/metadata/00000.json")
    );
    drop(graph_events);

    let lineage_events = lineage.events.lock().await;
    assert_eq!(lineage_events.len(), 1);
    assert_eq!(
        lineage_events[0].event_type,
        LineageEventType::TableRestored
    );
    assert_eq!(
        lineage_events[0].payload["metadata-location"],
        serde_json::json!("file:///tmp/events/metadata/00000.json")
    );
}

#[tokio::test]
async fn outbox_drain_projects_management_list_reads_to_lineage() {
    let principal = Principal::new("agent:operator", PrincipalKind::Agent).unwrap();
    let authorization_receipt = |action: &str| {
        json!({
            "principal": principal,
            "action": action,
            "allowed": true,
            "engine": "test",
            "policy_hash": null,
            "checked_at": chrono::Utc::now(),
        })
    };
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![
            OutboxEvent {
                event_id: "evt-policy-list".to_string(),
                sink: "lakecat.lineage-and-graph".to_string(),
                event_type: "policy-binding.listed".to_string(),
                payload: json!({
                    "audit-event-id": "audit-policy-list",
                    "event-type": "policy-binding.listed",
                    "payload": {
                        "authorization-receipt": authorization_receipt("policy-manage"),
                        "warehouse": "local",
                        "policy-count": 2,
                        "policy-ids": ["agent-read", "human.raw"],
                    }
                }),
                created_at: chrono::Utc::now(),
                delivered_at: None,
            },
            OutboxEvent {
                event_id: "evt-project-list".to_string(),
                sink: "lakecat.lineage-and-graph".to_string(),
                event_type: "project.listed".to_string(),
                payload: json!({
                    "audit-event-id": "audit-project-list",
                    "event-type": "project.listed",
                    "payload": {
                        "authorization-receipt": authorization_receipt("project-manage"),
                        "project-count": 1,
                        "project-ids": ["analytics"],
                    }
                }),
                created_at: chrono::Utc::now(),
                delivered_at: None,
            },
            OutboxEvent {
                event_id: "evt-server-list".to_string(),
                sink: "lakecat.lineage-and-graph".to_string(),
                event_type: "server.listed".to_string(),
                payload: json!({
                    "audit-event-id": "audit-server-list",
                    "event-type": "server.listed",
                    "payload": {
                        "authorization-receipt": authorization_receipt("server-manage"),
                        "server-count": 1,
                        "server-ids": ["prod-us"],
                    }
                }),
                created_at: chrono::Utc::now(),
                delivered_at: None,
            },
            OutboxEvent {
                event_id: "evt-storage-profile-list".to_string(),
                sink: "lakecat.lineage-and-graph".to_string(),
                event_type: "storage-profile.listed".to_string(),
                payload: json!({
                    "audit-event-id": "audit-storage-profile-list",
                    "event-type": "storage-profile.listed",
                    "payload": {
                        "authorization-receipt": authorization_receipt("storage-profile-manage"),
                        "warehouse": "local",
                        "storage-profile-count": 2,
                        "storage-profile-ids": ["events-local", "audit-local"],
                    }
                }),
                created_at: chrono::Utc::now(),
                delivered_at: None,
            },
            OutboxEvent {
                event_id: "evt-warehouse-list".to_string(),
                sink: "lakecat.lineage-and-graph".to_string(),
                event_type: "warehouse.listed".to_string(),
                payload: json!({
                    "audit-event-id": "audit-warehouse-list",
                    "event-type": "warehouse.listed",
                    "payload": {
                        "authorization-receipt": authorization_receipt("warehouse-manage"),
                        "project-id": "analytics",
                        "warehouse-count": 3,
                        "warehouse-names": ["local", "sandbox", "prod"],
                    }
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
    assert_eq!(drain.delivered, 5);
    assert_eq!(
        drain.event_types,
        vec![
            "policy-binding.listed".to_string(),
            "project.listed".to_string(),
            "server.listed".to_string(),
            "storage-profile.listed".to_string(),
            "warehouse.listed".to_string(),
        ]
    );
    assert_eq!(drain.graph_events, 5);
    assert_eq!(drain.lineage_events, 5);
    assert_eq!(
        store.delivered.lock().await.as_slice(),
        &[
            "evt-policy-list".to_string(),
            "evt-project-list".to_string(),
            "evt-server-list".to_string(),
            "evt-storage-profile-list".to_string(),
            "evt-warehouse-list".to_string(),
        ]
    );
    assert_eq!(drain.events.len(), 5);
    assert_eq!(drain.events[0].policy_binding_count, 2);
    assert_eq!(
        drain.events[0].management_scope_warehouse.as_deref(),
        Some("local")
    );
    assert_eq!(drain.events[1].project_count, Some(1));
    assert_eq!(drain.events[2].server_count, Some(1));
    assert_eq!(drain.events[3].storage_profile_count, Some(2));
    assert_eq!(
        drain.events[3].management_scope_warehouse.as_deref(),
        Some("local")
    );
    assert_eq!(drain.events[4].warehouse_count, Some(3));
    assert_eq!(
        drain.events[4].management_scope_project_id.as_deref(),
        Some("analytics")
    );

    let graph_events = graph.events.lock().await;
    assert_eq!(graph_events.len(), 5);
    assert!(
        graph_events
            .iter()
            .all(|event| event.label == GraphNodeLabel::Principal)
    );
    drop(graph_events);

    let lineage_events = lineage.events.lock().await;
    assert_eq!(lineage_events.len(), 5);
    let lineage_types: Vec<_> = lineage_events
        .iter()
        .map(|event| event.event_type.clone())
        .collect();
    assert_eq!(
        lineage_types,
        vec![
            LineageEventType::PolicyBindingListed,
            LineageEventType::ProjectListed,
            LineageEventType::ServerListed,
            LineageEventType::StorageProfileListed,
            LineageEventType::WarehouseListed,
        ]
    );
    assert_eq!(
        lineage_events[0].payload["policy-count"],
        serde_json::json!(2)
    );
    assert_eq!(
        lineage_events[0].payload["policy-ids"],
        serde_json::json!(["agent-read", "human.raw"])
    );
    assert_eq!(
        lineage_events[1].payload["project-ids"],
        serde_json::json!(["analytics"])
    );
    assert_eq!(
        lineage_events[2].payload["server-ids"],
        serde_json::json!(["prod-us"])
    );
    assert_eq!(
        lineage_events[3].payload["storage-profile-count"],
        serde_json::json!(2)
    );
    assert_eq!(
        lineage_events[3].payload["storage-profile-ids"],
        serde_json::json!(["events-local", "audit-local"])
    );
    assert_eq!(
        lineage_events[4].payload["project-id"],
        serde_json::json!("analytics")
    );
    assert_eq!(
        lineage_events[4].payload["warehouse-names"],
        serde_json::json!(["local", "sandbox", "prod"])
    );
}

#[tokio::test]
async fn outbox_drain_rejects_management_list_invalid_ids_with_hashes() {
    let principal = Principal::new("agent:operator", PrincipalKind::Agent).unwrap();
    let invalid_id = "agent-read?token=secret";
    let cases = [
        (
            "policy-binding.listed",
            "policy-manage",
            json!({
                "warehouse": "local",
                "policy-count": 1,
                "policy-ids": [invalid_id],
            }),
            "policy-id-hash=sha256:",
        ),
        (
            "project.listed",
            "project-manage",
            json!({
                "project-count": 1,
                "project-ids": [invalid_id],
            }),
            "project-id-hash=sha256:",
        ),
        (
            "storage-profile.listed",
            "storage-profile-manage",
            json!({
                "warehouse": "local",
                "storage-profile-count": 1,
                "storage-profile-ids": [invalid_id],
            }),
            "storage-profile-id-hash=sha256:",
        ),
        (
            "warehouse.listed",
            "warehouse-manage",
            json!({
                "project-id": invalid_id,
                "warehouse-count": 1,
                "warehouse-names": ["local"],
            }),
            "project-id-hash=sha256:",
        ),
    ];

    for (event_type, action, mut payload, expected_hash) in cases {
        let payload_object = payload.as_object_mut().unwrap();
        payload_object.insert(
            "authorization-receipt".to_string(),
            json!({
                "principal": principal,
                "action": action,
                "allowed": true,
                "engine": "test",
                "policy_hash": null,
                "checked_at": chrono::Utc::now(),
            }),
        );
        let event_id = format!("evt-invalid-management-list-{}", event_type);
        let store = Arc::new(RecordingOutboxStore {
            events: Mutex::new(vec![OutboxEvent {
                event_id: event_id.clone(),
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
            .expect_err("invalid management list id should fail before delivery");
        let message = err.to_string();
        assert!(message.contains(event_type));
        assert!(message.contains("invalid identifier"));
        assert!(message.contains(expected_hash));
        assert!(message.contains("event-id-hash=sha256:"));
        assert!(!message.contains(invalid_id));
        assert!(!message.contains("token=secret"));
        assert!(!message.contains(&event_id));
        assert!(store.delivered.lock().await.is_empty());
        assert!(graph.events.lock().await.is_empty());
        assert!(lineage.events.lock().await.is_empty());
    }
}

#[tokio::test]
async fn outbox_drain_projects_namespace_reads_to_graph_and_lineage() {
    let principal = Principal::new("agent:reader", PrincipalKind::Agent).unwrap();
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![
            OutboxEvent {
                event_id: "evt-namespace-list".to_string(),
                sink: "lakecat.lineage-and-graph".to_string(),
                event_type: "namespace.listed".to_string(),
                payload: json!({
                    "audit-event-id": "audit-namespace-list",
                    "event-type": "namespace.listed",
                    "payload": {
                        "authorization-receipt": {
                            "principal": principal,
                            "action": "namespace-list",
                            "allowed": true,
                            "engine": "test",
                            "policy_hash": null,
                            "checked_at": chrono::Utc::now(),
                        },
                        "warehouse": "local",
                        "namespace-count": 2,
                        "namespace-paths": ["default", "analytics"],
                    }
                }),
                created_at: chrono::Utc::now(),
                delivered_at: None,
            },
            OutboxEvent {
                event_id: "evt-namespace-load".to_string(),
                sink: "lakecat.lineage-and-graph".to_string(),
                event_type: "namespace.loaded".to_string(),
                payload: json!({
                    "audit-event-id": "audit-namespace-load",
                    "event-type": "namespace.loaded",
                    "payload": {
                        "authorization-receipt": {
                            "principal": principal,
                            "action": "namespace-load",
                            "allowed": true,
                            "engine": "test",
                            "policy_hash": null,
                            "checked_at": chrono::Utc::now(),
                        },
                        "warehouse": "local",
                        "namespace": ["default"],
                    }
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
    assert_eq!(drain.delivered, 2);
    assert_eq!(
        drain.event_types,
        vec![
            "namespace.listed".to_string(),
            "namespace.loaded".to_string()
        ]
    );
    assert_eq!(drain.graph_events, 4);
    assert_eq!(drain.lineage_events, 2);
    assert_eq!(
        store.delivered.lock().await.as_slice(),
        &[
            "evt-namespace-list".to_string(),
            "evt-namespace-load".to_string()
        ]
    );
    assert_eq!(drain.events.len(), 2);
    assert_eq!(drain.events[0].graph_events, 2);
    assert_eq!(drain.events[0].lineage_events, 1);
    assert_eq!(drain.events[1].graph_events, 2);
    assert_eq!(drain.events[1].lineage_events, 1);

    let graph_events = graph.events.lock().await;
    assert_eq!(graph_events.len(), 4);
    assert_eq!(graph_events[0].label, GraphNodeLabel::Principal);
    assert_eq!(
        graph_events[0].event_id.as_deref(),
        Some("evt-namespace-list:principal")
    );
    assert_eq!(graph_events[1].label, GraphNodeLabel::Warehouse);
    assert_eq!(graph_events[1].action, GraphAction::Loaded);
    assert_eq!(graph_events[1].subject, "lakecat:warehouse:local");
    assert_eq!(
        graph_events[1].event_id.as_deref(),
        Some("evt-namespace-list")
    );
    assert_eq!(graph_events[2].label, GraphNodeLabel::Principal);
    assert_eq!(
        graph_events[2].event_id.as_deref(),
        Some("evt-namespace-load:principal")
    );
    assert_eq!(graph_events[3].label, GraphNodeLabel::Namespace);
    assert_eq!(graph_events[3].action, GraphAction::Loaded);
    assert_eq!(
        graph_events[3].subject,
        "lakecat:warehouse:local:namespace:default"
    );
    assert_eq!(
        graph_events[3].event_id.as_deref(),
        Some("evt-namespace-load")
    );
    drop(graph_events);

    let lineage_events = lineage.events.lock().await;
    assert_eq!(lineage_events.len(), 2);
    assert_eq!(
        lineage_events[0].event_type,
        LineageEventType::NamespaceListed
    );
    assert_eq!(
        lineage_events[0].payload["namespace-count"],
        serde_json::json!(2)
    );
    assert_eq!(
        lineage_events[0].payload["namespace-paths"],
        serde_json::json!(["default", "analytics"])
    );
    assert_eq!(
        lineage_events[1].event_type,
        LineageEventType::NamespaceLoaded
    );
    assert_eq!(
        lineage_events[1].payload["namespace"],
        serde_json::json!(["default"])
    );
}

#[tokio::test]
async fn outbox_drain_projects_server_upserts_to_lineage() {
    let principal = Principal::new("agent:operator", PrincipalKind::Agent).unwrap();
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-server".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "server.upserted".to_string(),
            payload: json!({
                "audit-event-id": "audit-server",
                "event-type": "server.upserted",
                "payload": {
                    "authorization-receipt": {
                        "principal": principal,
                        "action": "server-manage",
                        "allowed": true,
                        "engine": "test",
                        "policy_hash": null,
                        "checked_at": chrono::Utc::now(),
                    },
                    "server-id": "prod",
                    "server-record": {
                        "server-id": "prod",
                        "display-name": "Production",
                        "endpoint-url": "https://lakecat.example",
                        "endpoint-url-hash": content_hash_json(&json!({
                            "endpoint-url": "https://lakecat.example"
                        })).unwrap(),
                        "properties": {"region": "global"}
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

    let drain = drain_outbox_once(&state, 10).await.unwrap();
    assert_eq!(drain.delivered, 1);
    assert_eq!(drain.event_types, vec!["server.upserted".to_string()]);
    assert_eq!(drain.graph_events, 2);
    assert_eq!(drain.lineage_events, 1);
    assert_eq!(
        store.delivered.lock().await.as_slice(),
        &["evt-server".to_string()]
    );

    let graph_events = graph.events.lock().await;
    assert_eq!(graph_events.len(), 2);
    assert_eq!(graph_events[0].label, GraphNodeLabel::Principal);
    assert_eq!(graph_events[0].subject, "lakecat:principal:agent:operator");
    assert_eq!(graph_events[1].label, GraphNodeLabel::Server);
    assert_eq!(graph_events[1].subject, "lakecat:server:prod");
    assert_eq!(
        graph_events[1].properties["server-record"]["server-id"],
        serde_json::json!("prod")
    );
    assert_eq!(
        graph_events[1].properties["server-record"]["endpoint-url"],
        serde_json::Value::Null
    );
    assert!(
        graph_events[1].properties["server-record"]["endpoint-url-hash"]
            .as_str()
            .is_some_and(|hash| hash.starts_with("sha256:"))
    );
    assert!(
        !graph_events[1]
            .properties
            .to_string()
            .contains("raw-secret"),
        "server endpoint replay must not expose decorated endpoint URLs to graph sinks"
    );
    drop(graph_events);

    let lineage_events = lineage.events.lock().await;
    assert_eq!(lineage_events.len(), 1);
    assert_eq!(
        lineage_events[0].event_type,
        LineageEventType::ServerUpserted
    );
    assert_eq!(
        lineage_events[0].payload["server-record"]["display-name"],
        serde_json::json!("Production")
    );
    assert_eq!(
        lineage_events[0].payload["server-record"]["endpoint-url"],
        serde_json::Value::Null
    );
    assert!(
        lineage_events[0].payload["server-record"]["endpoint-url-hash"]
            .as_str()
            .is_some_and(|hash| hash.starts_with("sha256:"))
    );
    assert!(
        !lineage_events[0].payload.to_string().contains("raw-secret"),
        "server endpoint replay must not expose decorated endpoint URLs to lineage sinks"
    );
}

#[tokio::test]
async fn outbox_drain_projects_storage_profile_upserts_to_lineage() {
    let principal = Principal::new("agent:operator", PrincipalKind::Agent).unwrap();
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-storage-profile".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "storage-profile.upserted".to_string(),
            payload: json!({
                "audit-event-id": "audit-storage-profile",
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
                        "secret-ref-present": true,
                        "secret-ref-provider": "vault",
                        "secret-ref-hash": content_hash_bytes("vault://kv/lakecat/events".as_bytes()),
                        "public-config": {"region": "us-west-2"}
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

    let drain = drain_outbox_once(&state, 10).await.unwrap();
    assert_eq!(drain.delivered, 1);
    assert_eq!(
        drain.event_types,
        vec!["storage-profile.upserted".to_string()]
    );
    assert_eq!(drain.graph_events, 2);
    assert_eq!(drain.lineage_events, 1);
    assert_eq!(
        drain.events[0].storage_profile_id.as_deref(),
        Some("s3-events")
    );
    assert_eq!(
        drain.events[0].storage_profile_provider.as_deref(),
        Some("s3")
    );
    assert_eq!(
        drain.events[0].storage_profile_issuance_mode.as_deref(),
        Some("secret-ref")
    );
    assert_eq!(
        drain.events[0]
            .storage_profile_location_prefix_hash
            .as_deref(),
        Some(
            content_hash_json(&json!({"location-prefix": "s3://lakecat/events"}))
                .unwrap()
                .as_str()
        )
    );
    assert!(
        drain.events[0]
            .storage_profile_location_prefix_hash
            .as_deref()
            .is_some_and(is_full_sha256_hash)
    );
    assert_eq!(
        drain.events[0].storage_profile_secret_ref_present,
        Some(true)
    );
    assert_eq!(
        drain.events[0]
            .storage_profile_secret_ref_provider
            .as_deref(),
        Some("vault")
    );
    assert_eq!(
        drain.events[0].storage_profile_secret_ref_hash.as_deref(),
        Some(content_hash_bytes("vault://kv/lakecat/events".as_bytes()).as_str())
    );
    assert_eq!(
        store.delivered.lock().await.as_slice(),
        &["evt-storage-profile".to_string()]
    );

    let graph_events = graph.events.lock().await;
    assert_eq!(graph_events.len(), 2);
    assert_eq!(graph_events[0].label, GraphNodeLabel::Principal);
    assert_eq!(graph_events[0].subject, "lakecat:principal:agent:operator");
    assert_eq!(graph_events[1].label, GraphNodeLabel::StorageProfile);
    assert_eq!(
        graph_events[1].subject,
        "lakecat:warehouse:local:storage-profile:s3-events"
    );
    assert_eq!(
        graph_events[1].properties["storage-profile"]["secret-ref-present"],
        serde_json::json!(true)
    );
    assert_eq!(
        graph_events[1].properties["storage-profile"]["secret-ref-provider"],
        serde_json::json!("vault")
    );
    assert_eq!(
        graph_events[1].properties["storage-profile"]["secret-ref-hash"],
        serde_json::json!(content_hash_bytes("vault://kv/lakecat/events".as_bytes()))
    );
    assert!(
        graph_events[1].properties["storage-profile"]
            .get("location-prefix")
            .is_none(),
        "storage profile graph projection must not expose the raw location prefix"
    );
    assert_eq!(
        graph_events[1].properties["storage-profile"]["location-prefix-hash"],
        serde_json::json!(
            content_hash_json(&json!({"location-prefix": "s3://lakecat/events"})).unwrap()
        )
    );
    assert!(
        graph_events[1].properties["storage-profile"]["location-prefix-hash"]
            .as_str()
            .is_some_and(is_full_sha256_hash)
    );
    assert!(
        graph_events[1].properties["storage-profile"]
            .get("secret-ref")
            .is_none(),
        "storage profile graph projection must not expose the secret-ref URI"
    );
    drop(graph_events);

    let lineage_events = lineage.events.lock().await;
    assert_eq!(lineage_events.len(), 1);
    assert_eq!(
        lineage_events[0].event_type,
        LineageEventType::StorageProfileUpserted
    );
    assert_eq!(
        lineage_events[0].payload["storage-profile"]["profile-id"],
        serde_json::json!("s3-events")
    );
    assert_eq!(
        lineage_events[0].payload["storage-profile"]["provider"],
        serde_json::json!("s3")
    );
    assert_eq!(
        lineage_events[0].payload["storage-profile"]["issuance-mode"],
        serde_json::json!("secret-ref")
    );
    assert_eq!(
        lineage_events[0].payload["storage-profile"]["secret-ref-present"],
        serde_json::json!(true)
    );
    assert_eq!(
        lineage_events[0].payload["storage-profile"]["secret-ref-provider"],
        serde_json::json!("vault")
    );
    assert_eq!(
        lineage_events[0].payload["storage-profile"]["secret-ref-hash"],
        serde_json::json!(content_hash_bytes("vault://kv/lakecat/events".as_bytes()))
    );
    assert!(
        lineage_events[0].payload["storage-profile"]
            .get("location-prefix")
            .is_none(),
        "storage profile lineage projection must not expose the raw location prefix"
    );
    assert_eq!(
        lineage_events[0].payload["storage-profile"]["location-prefix-hash"],
        serde_json::json!(
            content_hash_json(&json!({"location-prefix": "s3://lakecat/events"})).unwrap()
        )
    );
    assert!(
        lineage_events[0].payload["storage-profile"]["location-prefix-hash"]
            .as_str()
            .is_some_and(is_full_sha256_hash)
    );
    assert!(
        lineage_events[0].payload["storage-profile"]
            .get("secret-ref")
            .is_none()
    );
}

#[tokio::test]
async fn outbox_drain_rejects_duplicate_management_list_ids() {
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-duplicate-server-list-ids".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "server.listed".to_string(),
            payload: json!({
                "audit-event-id": "audit-duplicate-server-list-ids",
                "event-type": "server.listed",
                "payload": {
                    "server-count": 2,
                    "server-ids": ["prod", "prod"],
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
        .expect_err("duplicate management-list IDs should fail before delivery");

    let message = err.to_string();
    assert!(message.contains("outbox event server.listed (lakecat.lineage-and-graph) has invalid"));
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(message.contains("server list server-ids must not contain duplicate identifiers"));
    assert!(!message.contains("evt-duplicate-server-list-ids"));
    assert!(
        store.delivered.lock().await.is_empty(),
        "duplicate management-list IDs must fail before acknowledgement"
    );
    assert!(
        graph.events.lock().await.is_empty(),
        "duplicate management-list IDs must fail before graph projection"
    );
    assert!(
        lineage.events.lock().await.is_empty(),
        "duplicate management-list IDs must fail before lineage projection"
    );
}

#[tokio::test]
async fn outbox_drain_rejects_raw_storage_profile_location_prefix_evidence() {
    let principal = Principal::new("agent:operator", PrincipalKind::Agent).unwrap();
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-storage-profile-location-prefix".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "storage-profile.upserted".to_string(),
            payload: json!({
                "audit-event-id": "audit-storage-profile-location-prefix",
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
                        "location-prefix": "s3://lakecat/events",
                        "provider": "s3",
                        "issuance-mode": "secret-ref",
                        "secret-ref-present": true,
                        "secret-ref-provider": "typesec",
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
        .expect_err("raw storage-profile location-prefix evidence should fail before delivery");
    let message = err.to_string();
    assert!(message.contains("storage-profile.upserted"));
    assert!(message.contains("raw location-prefix"));
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains("evt-storage-profile-location-prefix"));
    assert!(store.delivered.lock().await.is_empty());
    assert!(graph.events.lock().await.is_empty());
    assert!(lineage.events.lock().await.is_empty());
}

#[tokio::test]
async fn outbox_drain_rejects_storage_profile_upsert_warehouse_drift() {
    let principal = Principal::new("agent:operator", PrincipalKind::Agent).unwrap();
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-storage-profile-warehouse-drift".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "storage-profile.upserted".to_string(),
            payload: json!({
                "audit-event-id": "audit-storage-profile-warehouse-drift",
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
                        "warehouse": "shadow",
                        "location-prefix-hash": content_hash_json(&json!({
                            "location-prefix": "s3://lakecat/events"
                        })).unwrap(),
                        "provider": "s3",
                        "issuance-mode": "secret-ref",
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
        .expect_err("storage-profile warehouse drift should fail before delivery");
    let message = err.to_string();
    assert!(message.contains("storage-profile.upserted"));
    assert!(message.contains("warehouse must match storage-profile"));
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains("evt-storage-profile-warehouse-drift"));
    assert!(store.delivered.lock().await.is_empty());
    assert!(graph.events.lock().await.is_empty());
    assert!(lineage.events.lock().await.is_empty());
}

#[tokio::test]
async fn outbox_drain_rejects_storage_profile_upsert_invalid_profile_ids() {
    let principal = Principal::new("agent:operator", PrincipalKind::Agent).unwrap();
    let invalid_profile_id = "s3-events?token=secret";
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-storage-profile-invalid-profile-id".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "storage-profile.upserted".to_string(),
            payload: json!({
                "audit-event-id": "audit-storage-profile-invalid-profile-id",
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
                        "profile-id": invalid_profile_id,
                        "warehouse": "local",
                        "location-prefix-hash": content_hash_json(&json!({
                            "location-prefix": "s3://lakecat/events"
                        })).unwrap(),
                        "provider": "s3",
                        "issuance-mode": "short-lived-secret-ref",
                        "secret-ref-present": true,
                        "secret-ref-provider": "typesec",
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
        .expect_err("storage-profile invalid profile-id should fail before delivery");
    let message = err.to_string();
    assert!(message.contains("storage-profile.upserted"));
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
async fn outbox_drain_rejects_storage_profile_upsert_missing_provider() {
    let principal = Principal::new("agent:operator", PrincipalKind::Agent).unwrap();
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-storage-profile-missing-provider".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "storage-profile.upserted".to_string(),
            payload: json!({
                "audit-event-id": "audit-storage-profile-missing-provider",
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
                        "issuance-mode": "secret-ref",
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
        .expect_err("storage-profile provider evidence should fail before delivery");
    let message = err.to_string();
    assert!(message.contains("storage-profile.upserted"));
    assert!(message.contains("storage-profile upsert provider must be a non-empty string"));
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains("evt-storage-profile-missing-provider"));
    assert!(store.delivered.lock().await.is_empty());
    assert!(graph.events.lock().await.is_empty());
    assert!(lineage.events.lock().await.is_empty());
}

#[tokio::test]
async fn outbox_drain_rejects_extra_storage_profile_upsert_fields() {
    let principal = Principal::new("agent:operator", PrincipalKind::Agent).unwrap();
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-storage-profile-extra-field".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "storage-profile.upserted".to_string(),
            payload: json!({
                "audit-event-id": "audit-storage-profile-extra-field",
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
                        "unverified-storage-claim": true,
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
        .expect_err("extra storage-profile upsert fields should fail before delivery");
    let message = err.to_string();
    assert!(message.contains("storage-profile.upserted"));
    assert!(message.contains(
        "storage-profile upsert storage-profile contains unexpected field unverified-storage-claim"
    ));
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains("evt-storage-profile-extra-field"));
    assert!(store.delivered.lock().await.is_empty());
    assert!(graph.events.lock().await.is_empty());
    assert!(lineage.events.lock().await.is_empty());
}

#[tokio::test]
async fn outbox_drain_rejects_extra_top_level_storage_profile_upsert_fields() {
    let principal = Principal::new("agent:operator", PrincipalKind::Agent).unwrap();
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-storage-profile-extra-top-level-field".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "storage-profile.upserted".to_string(),
            payload: json!({
                "audit-event-id": "audit-storage-profile-extra-top-level-field",
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
                    },
                    "unverified-storage-profile-claim": "shadow",
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
        .expect_err("extra top-level storage-profile upsert fields should fail");
    let message = err.to_string();
    assert!(message.contains("storage-profile.upserted"));
    assert!(message.contains(
        "storage-profile upsert contains unexpected field unverified-storage-profile-claim"
    ));
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains("evt-storage-profile-extra-top-level-field"));
    assert!(store.delivered.lock().await.is_empty());
    assert!(graph.events.lock().await.is_empty());
    assert!(lineage.events.lock().await.is_empty());
}

#[tokio::test]
async fn outbox_drain_rejects_extra_storage_profile_upsert_outbox_payload_fields() {
    let principal = Principal::new("agent:operator", PrincipalKind::Agent).unwrap();
    let event_id = "evt-storage-profile-extra-wrapper-field";
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: event_id.to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "storage-profile.upserted".to_string(),
            payload: json!({
                "audit-event-id": "audit-storage-profile-extra-wrapper-field",
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
                    },
                },
                "unverified-storage-profile-wrapper-claim": "shadow",
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
        .expect_err("extra storage-profile wrapper fields should fail before delivery");
    let message = err.to_string();
    assert!(message.contains("storage-profile.upserted"));
    assert!(message.contains(
        "storage-profile upsert outbox payload contains unexpected field unverified-storage-profile-wrapper-claim"
    ));
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains(event_id));
    assert!(
        store.delivered.lock().await.is_empty(),
        "extra storage-profile wrapper fields must fail before acknowledgement"
    );
    assert!(
        graph.events.lock().await.is_empty(),
        "extra storage-profile wrapper fields must fail before graph projection"
    );
    assert!(
        lineage.events.lock().await.is_empty(),
        "extra storage-profile wrapper fields must fail before lineage projection"
    );
}

#[tokio::test]
async fn outbox_drain_rejects_storage_profile_upsert_reserved_public_config() {
    let principal = Principal::new("agent:operator", PrincipalKind::Agent).unwrap();
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-storage-profile-reserved-public-config".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "storage-profile.upserted".to_string(),
            payload: json!({
                "audit-event-id": "audit-storage-profile-reserved-public-config",
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
                        "public-config": {
                            "lakecat.governed-read-required": "false"
                        }
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
        .expect_err("reserved storage-profile public-config evidence should fail");
    let message = err.to_string();
    assert!(message.contains("storage-profile.upserted"));
    assert!(message.contains(
        "storage-profile upsert storage-profile public-config key is reserved for LakeCat credential evidence"
    ));
    assert!(message.contains("public-config-key-hash=sha256:"));
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains("lakecat.governed-read-required"));
    assert!(!message.contains("evt-storage-profile-reserved-public-config"));
    assert!(store.delivered.lock().await.is_empty());
    assert!(graph.events.lock().await.is_empty());
    assert!(lineage.events.lock().await.is_empty());
}

#[tokio::test]
async fn outbox_drain_rejects_storage_profile_local_no_secret_remote_provider() {
    let principal = Principal::new("agent:operator", PrincipalKind::Agent).unwrap();
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-storage-profile-local-no-secret-remote-provider".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "storage-profile.upserted".to_string(),
            payload: json!({
                "audit-event-id": "audit-storage-profile-local-no-secret-remote-provider",
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
                        "issuance-mode": "local-file-no-secret",
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
        .expect_err("storage-profile local-file-no-secret mode with remote provider should fail");
    let message = err.to_string();
    assert!(message.contains("storage-profile.upserted"));
    assert!(message.contains("local-file-no-secret issuance mode requires file provider"));
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains("evt-storage-profile-local-no-secret-remote-provider"));
    assert!(store.delivered.lock().await.is_empty());
    assert!(graph.events.lock().await.is_empty());
    assert!(lineage.events.lock().await.is_empty());
}

#[tokio::test]
async fn outbox_drain_rejects_storage_profile_short_lived_file_provider() {
    let principal = Principal::new("agent:operator", PrincipalKind::Agent).unwrap();
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-storage-profile-short-lived-file-provider".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "storage-profile.upserted".to_string(),
            payload: json!({
                "audit-event-id": "audit-storage-profile-short-lived-file-provider",
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
                        "issuance-mode": "short-lived-secret-ref",
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

    let err = drain_outbox_once(&state, 10)
        .await
        .expect_err("storage-profile short-lived-secret-ref mode with file provider should fail");
    let message = err.to_string();
    assert!(message.contains("storage-profile.upserted"));
    assert!(
        message.contains("short-lived-secret-ref issuance mode requires cloud object provider")
    );
    assert!(message.contains("event-id-hash=sha256:"));
    assert!(!message.contains("evt-storage-profile-short-lived-file-provider"));
    assert!(store.delivered.lock().await.is_empty());
    assert!(graph.events.lock().await.is_empty());
    assert!(lineage.events.lock().await.is_empty());
}

#[tokio::test]
async fn outbox_drain_projects_warehouse_upserts_to_graph() {
    let principal = Principal::new("agent:operator", PrincipalKind::Agent).unwrap();
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-warehouse".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "warehouse.upserted".to_string(),
            payload: json!({
                "audit-event-id": "audit-warehouse",
                "event-type": "warehouse.upserted",
                "payload": {
                    "authorization-receipt": {
                        "principal": principal,
                        "action": "warehouse-manage",
                        "allowed": true,
                        "engine": "test",
                        "policy_hash": null,
                        "checked_at": chrono::Utc::now(),
                    },
                    "warehouse": "local",
                    "warehouse-record": {
                        "warehouse": "local",
                        "project-id": "default",
                        "storage-root": "file:///tmp/lakecat",
                        "storage-root-hash": content_hash_json(&json!({
                            "storage-root": "file:///tmp/lakecat"
                        })).unwrap(),
                        "properties": {"region": "local"}
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

    let drain = drain_outbox_once(&state, 10).await.unwrap();
    assert_eq!(drain.delivered, 1);
    assert_eq!(drain.event_types, vec!["warehouse.upserted".to_string()]);
    assert_eq!(drain.graph_events, 2);
    assert_eq!(drain.lineage_events, 1);
    assert_eq!(
        store.delivered.lock().await.as_slice(),
        &["evt-warehouse".to_string()]
    );

    let graph_events = graph.events.lock().await;
    assert_eq!(graph_events.len(), 2);
    assert_eq!(graph_events[0].label, GraphNodeLabel::Principal);
    assert_eq!(graph_events[0].subject, "lakecat:principal:agent:operator");
    assert_eq!(graph_events[1].label, GraphNodeLabel::Warehouse);
    assert_eq!(graph_events[1].subject, "lakecat:warehouse:local");
    assert_eq!(graph_events[1].event_id.as_deref(), Some("evt-warehouse"));
    assert_eq!(
        graph_events[1].properties["warehouse-record"]["project-id"],
        serde_json::json!("default")
    );
    assert!(
        graph_events[1].properties["warehouse-record"]
            .get("storage-root")
            .is_none(),
        "warehouse graph projection must not expose raw storage roots"
    );
    assert_eq!(
        graph_events[1].properties["warehouse-record"]["storage-root-hash"],
        serde_json::json!(
            content_hash_json(&json!({"storage-root": "file:///tmp/lakecat"})).unwrap()
        )
    );
    let graph_payload = serde_json::to_string(&graph_events[1].properties).unwrap();
    assert!(!graph_payload.contains("file:///tmp/lakecat"));
    let lineage_events = lineage.events.lock().await;
    assert_eq!(lineage_events.len(), 1);
    assert_eq!(
        lineage_events[0].event_type,
        LineageEventType::WarehouseUpserted
    );
    assert!(
        lineage_events[0].payload["warehouse-record"]
            .get("storage-root")
            .is_none(),
        "warehouse lineage projection must not expose raw storage roots"
    );
    assert_eq!(
        lineage_events[0].payload["warehouse-record"]["storage-root-hash"],
        serde_json::json!(
            content_hash_json(&json!({"storage-root": "file:///tmp/lakecat"})).unwrap()
        )
    );
    let lineage_payload = serde_json::to_string(&lineage_events[0].payload).unwrap();
    assert!(!lineage_payload.contains("file:///tmp/lakecat"));
}

#[tokio::test]
async fn outbox_drain_projects_project_upserts_to_graph() {
    let principal = Principal::new("agent:operator", PrincipalKind::Agent).unwrap();
    let store = Arc::new(RecordingOutboxStore {
        events: Mutex::new(vec![OutboxEvent {
            event_id: "evt-project".to_string(),
            sink: "lakecat.lineage-and-graph".to_string(),
            event_type: "project.upserted".to_string(),
            payload: json!({
                "audit-event-id": "audit-project",
                "event-type": "project.upserted",
                "payload": {
                    "authorization-receipt": {
                        "principal": principal,
                        "action": "project-manage",
                        "allowed": true,
                        "engine": "test",
                        "policy_hash": null,
                        "checked_at": chrono::Utc::now(),
                    },
                    "project-id": "default",
                    "project-record": {
                        "project-id": "default",
                        "display-name": "Default Project",
                        "properties": {"owner": "querygraph"}
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

    let drain = drain_outbox_once(&state, 10).await.unwrap();
    assert_eq!(drain.delivered, 1);
    assert_eq!(drain.event_types, vec!["project.upserted".to_string()]);
    assert_eq!(drain.graph_events, 2);
    assert_eq!(drain.lineage_events, 1);
    assert_eq!(
        store.delivered.lock().await.as_slice(),
        &["evt-project".to_string()]
    );

    let graph_events = graph.events.lock().await;
    assert_eq!(graph_events.len(), 2);
    assert_eq!(graph_events[0].label, GraphNodeLabel::Principal);
    assert_eq!(graph_events[0].subject, "lakecat:principal:agent:operator");
    assert_eq!(graph_events[1].label, GraphNodeLabel::Project);
    assert_eq!(graph_events[1].subject, "lakecat:project:default");
    assert_eq!(graph_events[1].event_id.as_deref(), Some("evt-project"));
    assert_eq!(
        graph_events[1].properties["project-record"]["display-name"],
        serde_json::json!("Default Project")
    );
    let lineage_events = lineage.events.lock().await;
    assert_eq!(lineage_events.len(), 1);
    assert_eq!(
        lineage_events[0].event_type,
        LineageEventType::ProjectUpserted
    );
    assert_eq!(
        lineage_events[0].payload["project-record"]["display-name"],
        serde_json::json!("Default Project")
    );
}
