use std::collections::BTreeSet;

use axum::Json;
use axum::extract::State;
use axum::http::HeaderMap;
use lakecat_api::LineageDrainResponse;
use lakecat_core::{
    LakeCatError, Namespace, Principal, PrincipalKind, TableIdent, WarehouseName,
    content_hash_bytes,
};
use lakecat_graph::{GraphAction, GraphEvent};
use lakecat_lineage::{LineageEvent, LineageEventType, LineageReceipt};
#[cfg(feature = "sail-local")]
use lakecat_sail::catalog_provider::{
    LakeCatCatalogProvider, ProviderFetchScanTasksRequest, ProviderScanPlanningRequest,
};
use lakecat_store::OutboxEvent;
use serde_json::{Value, json};

use crate::*;

#[derive(Debug, Default)]
pub(crate) struct OutboxProjectionReceipt {
    pub(crate) graph_events: usize,
    pub(crate) lineage_events: usize,
    pub(crate) lineage_event_hashes: Vec<String>,
    pub(crate) open_lineage_hashes: Vec<String>,
}

impl OutboxProjectionReceipt {
    fn record_lineage(&mut self, receipt: LineageReceipt) {
        self.lineage_events += 1;
        self.lineage_event_hashes.push(receipt.event_hash);
        self.open_lineage_hashes.push(receipt.open_lineage_hash);
    }
}

pub async fn drain_outbox_once(
    state: &LakeCatState,
    limit: usize,
) -> Result<LineageDrainResponse, LakeCatError> {
    let mut events = state
        .store
        .pending_outbox_events(Some("lakecat.lineage-and-graph"), limit)
        .await?;
    events.sort_by(|left, right| {
        left.created_at
            .cmp(&right.created_at)
            .then_with(|| left.event_id.cmp(&right.event_id))
    });
    let mut seen_event_ids = BTreeSet::new();
    for event in &events {
        if !seen_event_ids.insert(event.event_id.as_str()) {
            return Err(LakeCatError::Conflict(format!(
                "outbox pending batch contained duplicate event id hash {}",
                content_hash_bytes(event.event_id.as_bytes())
            )));
        }
    }
    let mut delivered = Vec::with_capacity(events.len());
    let mut event_types = Vec::with_capacity(events.len());
    let mut summaries = Vec::with_capacity(events.len());
    let mut graph_events = 0usize;
    let mut lineage_events = 0usize;
    for event in events {
        validate_outbox_event_evidence(&event)?;
        let receipt = project_outbox_event(state, &event).await?;
        validate_projection_receipt_evidence(&event, &receipt)?;
        graph_events += receipt.graph_events;
        lineage_events += receipt.lineage_events;
        summaries.push(lineage_drain_event_summary(&event, &receipt)?);
        event_types.push(event.event_type.clone());
        delivered.push(event.event_id.clone());
    }
    // Acknowledgement is all-or-retry: if any projection fails above, no pending
    // event is marked delivered and the outbox remains the recovery source.
    let projected = delivered.len();
    let delivered = state.store.mark_outbox_delivered(&delivered).await?;
    if delivered != projected {
        return Err(LakeCatError::Conflict(format!(
            "outbox drain acknowledgement mismatch: projected {projected} event(s) but marked {delivered} delivered"
        )));
    }
    let response = LineageDrainResponse {
        delivered,
        event_types,
        graph_events,
        lineage_events,
        principal_subject: None,
        principal_kind: None,
        authorization_receipt_hash: None,
        authorization_receipt_action: None,
        request_identity_state: None,
        request_identity_source: None,
        typedid_envelope_hash: None,
        typedid_proof_hash: None,
        events: summaries,
    };
    validate_lineage_drain_response_manifest(&response)?;
    Ok(response)
}
pub(crate) async fn drain_lineage_outbox(
    State(state): State<LakeCatState>,
    headers: HeaderMap,
) -> Result<Json<LineageDrainResponse>, LakeCatHttpError> {
    let capability = authorize_lineage_read(&state, request_identity(&headers)?).await?;
    let mut response = drain_outbox_once(&state, 100).await?;
    attach_lineage_drain_authorization(&mut response, capability.receipt())?;
    Ok(Json(response))
}

pub(crate) async fn project_outbox_event(
    state: &LakeCatState,
    event: &OutboxEvent,
) -> Result<OutboxProjectionReceipt, LakeCatError> {
    let event_payload = event
        .payload
        .get("payload")
        .unwrap_or(&event.payload)
        .clone();
    let table = outbox_table(event)?;
    let principal = outbox_principal(event)?;
    let mut receipt = OutboxProjectionReceipt::default();
    if principal.kind != PrincipalKind::Anonymous {
        state
            .graph
            .emit(
                GraphEvent::principal(
                    GraphAction::Loaded,
                    &principal,
                    json!({
                        "event-type": event.event_type,
                        "principal": principal,
                    }),
                )
                .with_event_id(format!("{}:principal", event.event_id)),
            )
            .await?;
        receipt.graph_events += 1;
    }
    if let Some((graph_action, lineage_type)) = outbox_table_projection(event.event_type.as_str()) {
        if let Some(table) = table.clone() {
            state
                .graph
                .emit(
                    GraphEvent::table(graph_action.clone(), table.clone(), event_payload.clone())
                        .with_event_id(event.event_id.clone()),
                )
                .await?;
            receipt.graph_events += 1;
            project_table_metadata_graph_events(
                state,
                event,
                graph_action.clone(),
                &table,
                &event_payload,
                &mut receipt,
            )
            .await?;
            if outbox_is_scan_projection(event.event_type.as_str()) {
                state
                    .graph
                    .emit(
                        GraphEvent::scan_plan(
                            GraphAction::PlannedScan,
                            event.event_id.clone(),
                            event_payload.clone(),
                        )
                        .with_event_id(format!("{}:scan-plan", event.event_id)),
                    )
                    .await?;
                receipt.graph_events += 1;
            }
            if event.event_type == "table.commit" {
                let sequence_number = outbox_commit_sequence_number(event)?;
                state
                    .graph
                    .emit(
                        GraphEvent::commit(
                            GraphAction::Committed,
                            &table,
                            sequence_number,
                            event_payload.clone(),
                        )
                        .with_event_id(format!("{}:commit", event.event_id)),
                    )
                    .await?;
                receipt.graph_events += 1;
            }
            let lineage_receipt = state
                .lineage
                .emit(LineageEvent::new(
                    lineage_type,
                    principal,
                    Some(table),
                    event_payload,
                ))
                .await?;
            receipt.record_lineage(lineage_receipt);
        }
    } else if event.event_type == "catalog.config-read" {
        let warehouse = outbox_warehouse(event, &state.warehouse)?;
        let event_payload = redact_warehouse_event_payload(event_payload);
        state
            .graph
            .emit(
                GraphEvent::warehouse(GraphAction::Loaded, warehouse, event_payload.clone())
                    .with_event_id(event.event_id.clone()),
            )
            .await?;
        receipt.graph_events += 1;
        let lineage_receipt = state
            .lineage
            .emit(LineageEvent::new(
                LineageEventType::CatalogConfigRead,
                principal,
                None,
                event_payload,
            ))
            .await?;
        receipt.record_lineage(lineage_receipt);
    } else if matches!(
        event.event_type.as_str(),
        "namespace.created" | "namespace.dropped" | "namespace.loaded"
    ) {
        let (warehouse, namespace) = outbox_namespace(event, &state.warehouse)?;
        let (graph_action, lineage_type) = match event.event_type.as_str() {
            "namespace.dropped" => (GraphAction::Deleted, LineageEventType::NamespaceDropped),
            "namespace.loaded" => (GraphAction::Loaded, LineageEventType::NamespaceLoaded),
            _ => (GraphAction::Created, LineageEventType::NamespaceCreated),
        };
        state
            .graph
            .emit(
                GraphEvent::namespace(graph_action, warehouse, namespace, event_payload.clone())
                    .with_event_id(event.event_id.clone()),
            )
            .await?;
        receipt.graph_events += 1;
        let lineage_receipt = state
            .lineage
            .emit(LineageEvent::new(
                lineage_type,
                principal,
                None,
                event_payload,
            ))
            .await?;
        receipt.record_lineage(lineage_receipt);
    } else if event.event_type == "namespace.listed" {
        let warehouse = outbox_warehouse(event, &state.warehouse)?;
        state
            .graph
            .emit(
                GraphEvent::warehouse(GraphAction::Loaded, warehouse, event_payload.clone())
                    .with_event_id(event.event_id.clone()),
            )
            .await?;
        receipt.graph_events += 1;
        let lineage_receipt = state
            .lineage
            .emit(LineageEvent::new(
                LineageEventType::NamespaceListed,
                principal,
                None,
                event_payload,
            ))
            .await?;
        receipt.record_lineage(lineage_receipt);
    } else if matches!(
        event.event_type.as_str(),
        "policy-binding.listed"
            | "project.listed"
            | "server.listed"
            | "storage-profile.listed"
            | "warehouse.listed"
    ) {
        let lineage_type = match event.event_type.as_str() {
            "policy-binding.listed" => LineageEventType::PolicyBindingListed,
            "project.listed" => LineageEventType::ProjectListed,
            "server.listed" => LineageEventType::ServerListed,
            "storage-profile.listed" => LineageEventType::StorageProfileListed,
            _ => LineageEventType::WarehouseListed,
        };
        let lineage_receipt = state
            .lineage
            .emit(LineageEvent::new(
                lineage_type,
                principal,
                None,
                event_payload,
            ))
            .await?;
        receipt.record_lineage(lineage_receipt);
    } else if event.event_type == "view.listed" {
        let (warehouse, namespace) = outbox_namespace(event, &state.warehouse)?;
        state
            .graph
            .emit(
                GraphEvent::namespace(
                    GraphAction::Loaded,
                    warehouse,
                    namespace,
                    event_payload.clone(),
                )
                .with_event_id(event.event_id.clone()),
            )
            .await?;
        receipt.graph_events += 1;
        let lineage_receipt = state
            .lineage
            .emit(LineageEvent::new(
                LineageEventType::ViewListed,
                principal,
                None,
                event_payload,
            ))
            .await?;
        receipt.record_lineage(lineage_receipt);
    } else if event.event_type == "view.version-receipts-listed" {
        let lineage_receipt = state
            .lineage
            .emit(LineageEvent::new(
                LineageEventType::ViewVersionReceiptsListed,
                principal,
                None,
                event_payload,
            ))
            .await?;
        receipt.record_lineage(lineage_receipt);
    } else if event.event_type == "table.commits-listed" {
        if let Some(table) = table.clone() {
            project_table_commit_history_graph_events(
                state,
                event,
                &table,
                &event_payload,
                &mut receipt,
            )
            .await?;
        }
        let lineage_receipt = state
            .lineage
            .emit(LineageEvent::new(
                LineageEventType::TableCommitRecordsListed,
                principal,
                None,
                event_payload,
            ))
            .await?;
        receipt.record_lineage(lineage_receipt);
    } else if event.event_type == "view.version-receipt-chains-listed" {
        let lineage_receipt = state
            .lineage
            .emit(LineageEvent::new(
                LineageEventType::ViewVersionReceiptChainsListed,
                principal,
                None,
                event_payload,
            ))
            .await?;
        receipt.record_lineage(lineage_receipt);
    } else if event.event_type == "policy-binding.upserted" {
        let (warehouse, policy_id) = outbox_policy_binding(event, &state.warehouse)?;
        state
            .graph
            .emit(
                GraphEvent::policy(
                    GraphAction::Upserted,
                    warehouse,
                    policy_id,
                    event_payload.clone(),
                )
                .with_event_id(event.event_id.clone()),
            )
            .await?;
        receipt.graph_events += 1;
        let lineage_receipt = state
            .lineage
            .emit(LineageEvent::new(
                LineageEventType::PolicyBindingUpserted,
                principal,
                None,
                event_payload,
            ))
            .await?;
        receipt.record_lineage(lineage_receipt);
    } else if event.event_type == "project.upserted" {
        let project_id = outbox_project(event)?;
        state
            .graph
            .emit(
                GraphEvent::project(GraphAction::Upserted, project_id, event_payload.clone())
                    .with_event_id(event.event_id.clone()),
            )
            .await?;
        receipt.graph_events += 1;
        let lineage_receipt = state
            .lineage
            .emit(LineageEvent::new(
                LineageEventType::ProjectUpserted,
                principal,
                None,
                event_payload,
            ))
            .await?;
        receipt.record_lineage(lineage_receipt);
    } else if event.event_type == "server.upserted" {
        let server_id = outbox_server(event)?;
        let event_payload = redact_server_event_payload(event_payload);
        state
            .graph
            .emit(
                GraphEvent::server(GraphAction::Upserted, server_id, event_payload.clone())
                    .with_event_id(event.event_id.clone()),
            )
            .await?;
        receipt.graph_events += 1;
        let lineage_receipt = state
            .lineage
            .emit(LineageEvent::new(
                LineageEventType::ServerUpserted,
                principal,
                None,
                event_payload,
            ))
            .await?;
        receipt.record_lineage(lineage_receipt);
    } else if event.event_type == "storage-profile.upserted" {
        let (warehouse, profile_id) = outbox_storage_profile(event, &state.warehouse)?;
        let event_payload = redact_storage_profile_event_payload(event_payload);
        state
            .graph
            .emit(
                GraphEvent::storage_profile(
                    GraphAction::Upserted,
                    warehouse,
                    profile_id,
                    event_payload.clone(),
                )
                .with_event_id(event.event_id.clone()),
            )
            .await?;
        receipt.graph_events += 1;
        let lineage_receipt = state
            .lineage
            .emit(LineageEvent::new(
                LineageEventType::StorageProfileUpserted,
                principal,
                None,
                event_payload,
            ))
            .await?;
        receipt.record_lineage(lineage_receipt);
    } else if matches!(
        event.event_type.as_str(),
        "view.upserted" | "view.loaded" | "view.dropped"
    ) {
        let (warehouse, namespace, view_name) = outbox_view(event, &state.warehouse)?;
        let (graph_action, lineage_type) = match event.event_type.as_str() {
            "view.dropped" => (GraphAction::Deleted, LineageEventType::ViewDropped),
            "view.loaded" => (GraphAction::Loaded, LineageEventType::ViewLoaded),
            _ => (GraphAction::Upserted, LineageEventType::ViewUpserted),
        };
        state
            .graph
            .emit(
                GraphEvent::view(
                    graph_action,
                    warehouse,
                    namespace,
                    view_name.as_str(),
                    event_payload.clone(),
                )
                .with_event_id(event.event_id.clone()),
            )
            .await?;
        receipt.graph_events += 1;
        let lineage_receipt = state
            .lineage
            .emit(LineageEvent::new(
                lineage_type,
                principal,
                None,
                event_payload,
            ))
            .await?;
        receipt.record_lineage(lineage_receipt);
    } else if event.event_type == "warehouse.upserted" {
        let warehouse = outbox_warehouse(event, &state.warehouse)?;
        let event_payload = redact_warehouse_event_payload(event_payload);
        state
            .graph
            .emit(
                GraphEvent::warehouse(GraphAction::Upserted, warehouse, event_payload.clone())
                    .with_event_id(event.event_id.clone()),
            )
            .await?;
        receipt.graph_events += 1;
        let lineage_receipt = state
            .lineage
            .emit(LineageEvent::new(
                LineageEventType::WarehouseUpserted,
                principal,
                None,
                event_payload,
            ))
            .await?;
        receipt.record_lineage(lineage_receipt);
    } else if event.event_type == "table.restored" {
        if let Some(table) = table {
            state
                .graph
                .emit(
                    GraphEvent::table(GraphAction::Loaded, table.clone(), event_payload.clone())
                        .with_event_id(event.event_id.clone()),
                )
                .await?;
            receipt.graph_events += 1;
            let lineage_receipt = state
                .lineage
                .emit(LineageEvent::new(
                    LineageEventType::TableRestored,
                    principal,
                    Some(table),
                    event_payload,
                ))
                .await?;
            receipt.record_lineage(lineage_receipt);
        }
    } else if event.event_type == "credentials.vend-attempted" {
        if let Some((warehouse, profile_id)) =
            outbox_optional_storage_profile(event, &state.warehouse)?
        {
            let credential_payload = redact_storage_profile_event_payload(event_payload.clone());
            state
                .graph
                .emit(
                    GraphEvent::storage_profile(
                        GraphAction::Loaded,
                        warehouse,
                        profile_id,
                        credential_payload,
                    )
                    .with_event_id(format!("{}:storage-profile", event.event_id)),
                )
                .await?;
            receipt.graph_events += 1;
        }
        let lineage_receipt = state
            .lineage
            .emit(LineageEvent::new(
                LineageEventType::CredentialsVendAttempted,
                principal,
                table,
                event_payload,
            ))
            .await?;
        receipt.record_lineage(lineage_receipt);
    } else if event.event_type == "querygraph.bootstrap" {
        let lineage_receipt = state
            .lineage
            .emit(LineageEvent::new(
                LineageEventType::QueryGraphBootstrap,
                principal,
                None,
                event_payload,
            ))
            .await?;
        receipt.record_lineage(lineage_receipt);
    }
    Ok(receipt)
}

pub(crate) async fn project_table_commit_history_graph_events(
    state: &LakeCatState,
    event: &OutboxEvent,
    table: &TableIdent,
    event_payload: &Value,
    receipt: &mut OutboxProjectionReceipt,
) -> Result<(), LakeCatError> {
    for (sequence_number, commit_hash) in outbox_commit_history_entries(event)? {
        state
            .graph
            .emit(
                GraphEvent::commit(
                    GraphAction::Loaded,
                    table,
                    sequence_number,
                    json!({
                        "event-type": event.event_type,
                        "table": table,
                        "sequence-number": sequence_number,
                        "commit-hash": commit_hash,
                        "commit-history-read": event_payload,
                    }),
                )
                .with_event_id(format!(
                    "{}:commit-history:{sequence_number}",
                    event.event_id
                )),
            )
            .await?;
        receipt.graph_events += 1;
    }
    Ok(())
}

pub(crate) async fn project_table_metadata_graph_events(
    state: &LakeCatState,
    event: &OutboxEvent,
    action: GraphAction,
    table: &TableIdent,
    event_payload: &Value,
    receipt: &mut OutboxProjectionReceipt,
) -> Result<(), LakeCatError> {
    let Some(metadata_graph) = event_payload
        .get("metadata-graph")
        .or_else(|| event_payload.get("metadata"))
    else {
        return Ok(());
    };
    for field in metadata_graph_fields(metadata_graph) {
        let Some(column_id) = metadata_field_id(&field) else {
            continue;
        };
        state
            .graph
            .emit(
                GraphEvent::column(
                    action.clone(),
                    table,
                    column_id.clone(),
                    json!({
                        "event-type": event.event_type,
                        "table": table,
                        "current-schema-id": metadata_graph.get("current-schema-id"),
                        "field": field,
                    }),
                )
                .with_event_id(format!("{}:column:{column_id}", event.event_id)),
            )
            .await?;
        receipt.graph_events += 1;
    }
    if let Some((snapshot_id, snapshot)) = metadata_graph_current_snapshot(metadata_graph) {
        state
            .graph
            .emit(
                GraphEvent::snapshot(
                    action,
                    table,
                    snapshot_id.clone(),
                    json!({
                        "event-type": event.event_type,
                        "table": table,
                        "current-snapshot-id": metadata_graph.get("current-snapshot-id"),
                        "snapshot": snapshot,
                    }),
                )
                .with_event_id(format!("{}:snapshot:{snapshot_id}", event.event_id)),
            )
            .await?;
        receipt.graph_events += 1;
    }
    Ok(())
}

pub(crate) fn metadata_graph_fields(metadata_graph: &Value) -> Vec<Value> {
    metadata_graph
        .get("fields")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_else(|| metadata_current_schema_fields(metadata_graph))
}

pub(crate) fn metadata_field_id(field: &Value) -> Option<String> {
    field
        .get("id")
        .and_then(value_to_stable_part)
        .or_else(|| field.get("name").and_then(value_to_stable_part))
}

pub(crate) fn metadata_graph_current_snapshot(metadata_graph: &Value) -> Option<(String, Value)> {
    let snapshot = metadata_graph
        .get("current-snapshot")
        .filter(|snapshot| !snapshot.is_null())
        .cloned()
        .or_else(|| metadata_current_snapshot(metadata_graph).cloned())?;
    let snapshot_id = snapshot
        .get("snapshot-id")
        .and_then(value_to_stable_part)
        .or_else(|| {
            metadata_graph
                .get("current-snapshot-id")
                .and_then(value_to_stable_part)
        })?;
    Some((snapshot_id, snapshot))
}

pub(crate) fn value_to_stable_part(value: &Value) -> Option<String> {
    value
        .as_i64()
        .map(|value| value.to_string())
        .or_else(|| value.as_str().map(ToString::to_string))
}

pub(crate) fn outbox_event_hash(event: &OutboxEvent) -> String {
    content_hash_bytes(event.event_id.as_bytes())
}

pub(crate) fn outbox_table(event: &OutboxEvent) -> Result<Option<TableIdent>, LakeCatError> {
    event
        .payload
        .get("table")
        .filter(|table| !table.is_null())
        .map(|table| {
            serde_json::from_value(table.clone()).map_err(|err| {
                LakeCatError::Internal(format!(
                    "failed to decode outbox table for event hash {}: {err}",
                    outbox_event_hash(event)
                ))
            })
        })
        .transpose()
}

pub(crate) fn outbox_namespace(
    event: &OutboxEvent,
    default_warehouse: &WarehouseName,
) -> Result<(WarehouseName, Namespace), LakeCatError> {
    let warehouse = event
        .payload
        .get("payload")
        .and_then(|payload| payload.get("warehouse"))
        .or_else(|| event.payload.get("warehouse"))
        .and_then(serde_json::Value::as_str)
        .map(WarehouseName::new)
        .transpose()?
        .unwrap_or_else(|| default_warehouse.clone());
    let namespace = event
        .payload
        .get("payload")
        .and_then(|payload| payload.get("namespace"))
        .or_else(|| event.payload.get("namespace"))
        .ok_or_else(|| {
            LakeCatError::Internal(format!(
                "outbox event hash {} is missing namespace payload",
                outbox_event_hash(event)
            ))
        })?;
    let namespace = match namespace {
        serde_json::Value::Array(parts) => Namespace::new(
            parts
                .iter()
                .map(|part| {
                    part.as_str().map(ToString::to_string).ok_or_else(|| {
                        LakeCatError::Internal(format!(
                            "outbox event hash {} namespace components must be strings",
                            outbox_event_hash(event)
                        ))
                    })
                })
                .collect::<Result<Vec<_>, _>>()?,
        )?,
        serde_json::Value::String(path) => path.parse::<Namespace>()?,
        _ => {
            return Err(LakeCatError::Internal(format!(
                "outbox event hash {} namespace payload must be an array or string",
                outbox_event_hash(event)
            )));
        }
    };
    Ok((warehouse, namespace))
}

pub(crate) fn outbox_policy_binding(
    event: &OutboxEvent,
    default_warehouse: &WarehouseName,
) -> Result<(WarehouseName, String), LakeCatError> {
    let payload = event.payload.get("payload").unwrap_or(&event.payload);
    let policy = payload.get("policy").ok_or_else(|| {
        LakeCatError::Internal(format!(
            "outbox event hash {} is missing policy payload",
            outbox_event_hash(event)
        ))
    })?;
    let warehouse = policy
        .get("warehouse")
        .or_else(|| payload.get("warehouse"))
        .and_then(serde_json::Value::as_str)
        .map(WarehouseName::new)
        .transpose()?
        .unwrap_or_else(|| default_warehouse.clone());
    let policy_id = policy
        .get("policy-id")
        .and_then(serde_json::Value::as_str)
        .filter(|policy_id| !policy_id.is_empty())
        .ok_or_else(|| {
            LakeCatError::Internal(format!(
                "outbox event hash {} policy payload is missing policy-id",
                outbox_event_hash(event)
            ))
        })?
        .to_string();
    Ok((warehouse, policy_id))
}

pub(crate) fn outbox_project(event: &OutboxEvent) -> Result<String, LakeCatError> {
    let payload = event.payload.get("payload").unwrap_or(&event.payload);
    payload
        .get("project-id")
        .or_else(|| {
            payload
                .get("project-record")
                .and_then(|record| record.get("project-id"))
        })
        .and_then(Value::as_str)
        .filter(|project_id| !project_id.is_empty())
        .map(ToString::to_string)
        .ok_or_else(|| {
            LakeCatError::Internal(format!(
                "outbox event hash {} is missing project payload",
                outbox_event_hash(event)
            ))
        })
}

pub(crate) fn outbox_server(event: &OutboxEvent) -> Result<String, LakeCatError> {
    let payload = event.payload.get("payload").unwrap_or(&event.payload);
    payload
        .get("server-id")
        .or_else(|| {
            payload
                .get("server-record")
                .and_then(|record| record.get("server-id"))
        })
        .and_then(Value::as_str)
        .filter(|server_id| !server_id.is_empty())
        .map(ToString::to_string)
        .ok_or_else(|| {
            LakeCatError::Internal(format!(
                "outbox event hash {} is missing server payload",
                outbox_event_hash(event)
            ))
        })
}

pub(crate) fn outbox_storage_profile(
    event: &OutboxEvent,
    default_warehouse: &WarehouseName,
) -> Result<(WarehouseName, String), LakeCatError> {
    let payload = event.payload.get("payload").unwrap_or(&event.payload);
    let storage_profile = payload.get("storage-profile").ok_or_else(|| {
        LakeCatError::Internal(format!(
            "outbox event hash {} is missing storage profile payload",
            outbox_event_hash(event)
        ))
    })?;
    let warehouse = storage_profile
        .get("warehouse")
        .or_else(|| payload.get("warehouse"))
        .and_then(Value::as_str)
        .map(WarehouseName::new)
        .transpose()?
        .unwrap_or_else(|| default_warehouse.clone());
    let profile_id = storage_profile
        .get("profile-id")
        .and_then(Value::as_str)
        .filter(|profile_id| !profile_id.is_empty())
        .map(ToString::to_string)
        .ok_or_else(|| {
            LakeCatError::Internal(format!(
                "outbox event hash {} is missing storage profile payload",
                outbox_event_hash(event)
            ))
        })?;
    Ok((warehouse, profile_id))
}

pub(crate) fn outbox_optional_storage_profile(
    event: &OutboxEvent,
    default_warehouse: &WarehouseName,
) -> Result<Option<(WarehouseName, String)>, LakeCatError> {
    let payload = event.payload.get("payload").unwrap_or(&event.payload);
    let Some(profile_id) = payload
        .get("storage-profile")
        .and_then(|storage_profile| storage_profile.get("profile-id"))
        .or_else(|| payload.get("storage-profile-id"))
        .and_then(Value::as_str)
        .filter(|profile_id| !profile_id.is_empty())
    else {
        return Ok(None);
    };
    let warehouse = payload
        .get("storage-profile")
        .and_then(|storage_profile| storage_profile.get("warehouse"))
        .or_else(|| payload.get("warehouse"))
        .or_else(|| {
            payload
                .get("table")
                .and_then(|table| table.get("warehouse"))
        })
        .and_then(Value::as_str)
        .map(WarehouseName::new)
        .transpose()?
        .unwrap_or_else(|| default_warehouse.clone());
    Ok(Some((warehouse, profile_id.to_string())))
}

pub(crate) fn outbox_view(
    event: &OutboxEvent,
    default_warehouse: &WarehouseName,
) -> Result<(WarehouseName, Namespace, String), LakeCatError> {
    let payload = event.payload.get("payload").unwrap_or(&event.payload);
    let view = payload.get("view").ok_or_else(|| {
        LakeCatError::Internal(format!(
            "outbox event hash {} is missing view payload",
            outbox_event_hash(event)
        ))
    })?;
    let warehouse = view
        .get("warehouse")
        .or_else(|| payload.get("warehouse"))
        .and_then(Value::as_str)
        .map(WarehouseName::new)
        .transpose()?
        .unwrap_or_else(|| default_warehouse.clone());
    let namespace_value = view
        .get("namespace")
        .or_else(|| payload.get("namespace"))
        .ok_or_else(|| {
            LakeCatError::Internal(format!(
                "outbox event hash {} view payload is missing namespace",
                outbox_event_hash(event)
            ))
        })?;
    let namespace = match namespace_value {
        Value::Array(parts) => Namespace::new(
            parts
                .iter()
                .map(|part| {
                    part.as_str().map(ToString::to_string).ok_or_else(|| {
                        LakeCatError::Internal(format!(
                            "outbox event hash {} view namespace components must be strings",
                            outbox_event_hash(event)
                        ))
                    })
                })
                .collect::<Result<Vec<_>, _>>()?,
        )?,
        Value::String(path) => path.parse::<Namespace>()?,
        _ => {
            return Err(LakeCatError::Internal(format!(
                "outbox event hash {} view namespace must be an array or string",
                outbox_event_hash(event)
            )));
        }
    };
    let view_name = view
        .get("name")
        .and_then(Value::as_str)
        .filter(|name| !name.is_empty())
        .ok_or_else(|| {
            LakeCatError::Internal(format!(
                "outbox event hash {} view payload is missing name",
                outbox_event_hash(event)
            ))
        })?
        .to_string();
    Ok((warehouse, namespace, view_name))
}

pub(crate) fn outbox_warehouse(
    event: &OutboxEvent,
    default_warehouse: &WarehouseName,
) -> Result<WarehouseName, LakeCatError> {
    event
        .payload
        .get("payload")
        .and_then(|payload| payload.get("warehouse"))
        .or_else(|| event.payload.get("warehouse"))
        .and_then(Value::as_str)
        .map(WarehouseName::new)
        .transpose()
        .map(|warehouse| warehouse.unwrap_or_else(|| default_warehouse.clone()))
}

pub(crate) fn outbox_commit_sequence_number(event: &OutboxEvent) -> Result<u64, LakeCatError> {
    let commit = event
        .payload
        .get("payload")
        .and_then(|payload| payload.get("commit"))
        .or_else(|| event.payload.get("commit"))
        .ok_or_else(|| {
            LakeCatError::Internal(format!(
                "outbox event hash {} is missing commit payload",
                outbox_event_hash(event)
            ))
        })?;
    commit
        .get("sequence_number")
        .or_else(|| commit.get("sequence-number"))
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| {
            LakeCatError::Internal(format!(
                "outbox event hash {} commit payload is missing sequence number",
                outbox_event_hash(event)
            ))
        })
}

pub(crate) fn outbox_commit_history_entries(
    event: &OutboxEvent,
) -> Result<Vec<(u64, String)>, LakeCatError> {
    let payload = event.payload.get("payload").unwrap_or(&event.payload);
    let sequence_numbers = payload
        .get("sequence-numbers")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            LakeCatError::Internal(format!(
                "outbox event hash {} commit history payload is missing sequence numbers",
                outbox_event_hash(event)
            ))
        })?;
    let commit_hashes = payload
        .get("commit-hashes")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            LakeCatError::Internal(format!(
                "outbox event hash {} commit history payload is missing commit hashes",
                outbox_event_hash(event)
            ))
        })?;
    if commit_hashes.len() != sequence_numbers.len() {
        return Err(LakeCatError::Internal(format!(
            "outbox event hash {} commit history payload commit hash count does not match sequence numbers",
            outbox_event_hash(event)
        )));
    }
    sequence_numbers
        .iter()
        .enumerate()
        .map(|(index, sequence_number)| {
            let sequence_number = sequence_number.as_u64().ok_or_else(|| {
                LakeCatError::Internal(format!(
                    "outbox event hash {} commit history payload has a non-numeric sequence number",
                    outbox_event_hash(event)
                ))
            })?;
            let commit_hash = commit_hashes
                .get(index)
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    LakeCatError::Internal(format!(
                        "outbox event hash {} commit history payload has a non-string commit hash",
                        outbox_event_hash(event)
                    ))
                })?
                .to_string();
            Ok((sequence_number, commit_hash))
        })
        .collect()
}

pub(crate) fn outbox_principal(event: &OutboxEvent) -> Result<Principal, LakeCatError> {
    for pointer in [
        "/payload/authorization-receipt/principal",
        "/authorization-receipt/principal",
        "/commit/principal",
    ] {
        if let Some(principal) = event.payload.pointer(pointer) {
            return serde_json::from_value(principal.clone()).map_err(|err| {
                LakeCatError::Internal(format!(
                    "failed to decode outbox principal for event hash {}: {err}",
                    outbox_event_hash(event)
                ))
            });
        }
    }
    Ok(Principal::anonymous())
}

pub(crate) fn outbox_table_projection(event_type: &str) -> Option<(GraphAction, LineageEventType)> {
    match event_type {
        "table.created" => Some((GraphAction::Created, LineageEventType::TableCreated)),
        "table.loaded" => Some((GraphAction::Loaded, LineageEventType::TableLoaded)),
        "table.scan-planned" => {
            Some((GraphAction::PlannedScan, LineageEventType::TableScanPlanned))
        }
        "table.scan-tasks-fetched" => {
            Some((GraphAction::PlannedScan, LineageEventType::TableScanPlanned))
        }
        "table.commit" => Some((GraphAction::Committed, LineageEventType::TableCommitted)),
        "table.deleted" => Some((GraphAction::Deleted, LineageEventType::TableDeleted)),
        _ => None,
    }
}

pub(crate) fn outbox_is_scan_projection(event_type: &str) -> bool {
    matches!(
        event_type,
        "table.scan-planned" | "table.scan-tasks-fetched"
    )
}
