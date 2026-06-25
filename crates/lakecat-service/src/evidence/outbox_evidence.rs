use std::collections::BTreeSet;

use lakecat_api::LineageDrainResponse;
use lakecat_core::{LakeCatError, content_hash_json};
#[cfg(feature = "sail-local")]
use lakecat_sail::catalog_provider::{
    LakeCatCatalogProvider, ProviderFetchScanTasksRequest, ProviderScanPlanningRequest,
};
use lakecat_store::OutboxEvent;
use serde_json::Value;

use crate::*;

pub(crate) fn validate_lineage_drain_response_manifest(
    response: &LineageDrainResponse,
) -> Result<(), LakeCatError> {
    if response.delivered != response.events.len() {
        return Err(LakeCatError::Conflict(format!(
            "lineage drain response delivered count {} did not match replay summary count {}",
            response.delivered,
            response.events.len()
        )));
    }
    if response.event_types.len() != response.events.len() {
        return Err(LakeCatError::Conflict(format!(
            "lineage drain response event_types count {} did not match replay summary count {}",
            response.event_types.len(),
            response.events.len()
        )));
    }
    let mut event_ids = BTreeSet::new();
    for (index, summary) in response.events.iter().enumerate() {
        if summary.event_id.trim().is_empty() {
            return Err(LakeCatError::Conflict(format!(
                "lineage drain response events[{index}].event_id must be non-empty"
            )));
        }
        if !event_ids.insert(summary.event_id.as_str()) {
            return Err(LakeCatError::Conflict(format!(
                "lineage drain response events[{index}].event_id must be duplicate-free"
            )));
        }
    }
    for (index, (event_type, summary)) in response
        .event_types
        .iter()
        .zip(response.events.iter())
        .enumerate()
    {
        if event_type != &summary.event_type {
            return Err(LakeCatError::Conflict(format!(
                "lineage drain response event_types[{index}] {event_type} did not match events[{index}].event_type {}",
                summary.event_type
            )));
        }
    }
    let graph_events: usize = response.events.iter().map(|event| event.graph_events).sum();
    if response.graph_events != graph_events {
        return Err(LakeCatError::Conflict(format!(
            "lineage drain response graph_events {} did not match replay summary graph_events {}",
            response.graph_events, graph_events
        )));
    }
    let lineage_events: usize = response
        .events
        .iter()
        .map(|event| event.lineage_events)
        .sum();
    if response.lineage_events != lineage_events {
        return Err(LakeCatError::Conflict(format!(
            "lineage drain response lineage_events {} did not match replay summary lineage_events {}",
            response.lineage_events, lineage_events
        )));
    }
    Ok(())
}

pub(crate) fn validate_projection_receipt_evidence(
    event: &OutboxEvent,
    receipt: &OutboxProjectionReceipt,
) -> Result<(), LakeCatError> {
    if receipt.lineage_event_hashes.len() != receipt.lineage_events
        || receipt.open_lineage_hashes.len() != receipt.lineage_events
    {
        return Err(outbox_evidence_error(
            event,
            "projection receipt hash counts must match lineage event count",
        ));
    }
    validate_projection_receipt_hash_array(
        event,
        &receipt.lineage_event_hashes,
        "replay event hashes",
    )?;
    validate_projection_receipt_hash_array(
        event,
        &receipt.open_lineage_hashes,
        "OpenLineage hashes",
    )?;
    Ok(())
}

pub(crate) fn validate_projection_receipt_hash_array(
    event: &OutboxEvent,
    hashes: &[String],
    label: &str,
) -> Result<(), LakeCatError> {
    let mut seen = BTreeSet::new();
    for hash in hashes {
        if !is_full_sha256_digest_evidence(hash) {
            return Err(outbox_evidence_error(
                event,
                &format!("projection receipt {label} must contain full SHA-256 hashes"),
            ));
        }
        if !seen.insert(hash.as_str()) {
            return Err(outbox_evidence_error(
                event,
                &format!("projection receipt {label} must be duplicate-free"),
            ));
        }
    }
    Ok(())
}

pub(crate) fn validate_outbox_event_evidence(event: &OutboxEvent) -> Result<(), LakeCatError> {
    let payload = event.payload.get("payload").unwrap_or(&event.payload);
    if !is_known_outbox_event_type(event.event_type.as_str()) {
        return Err(outbox_evidence_error(
            event,
            "outbox event type is not supported for projection",
        ));
    }
    validate_outbox_event_type_binding(event, &event.payload, "outbox payload", true)?;
    if !std::ptr::eq(payload, &event.payload) {
        validate_outbox_event_type_binding(
            event,
            payload,
            "outbox inner payload",
            outbox_event_id_matches_payload_hash(event),
        )?;
    }
    validate_read_restriction_policy_hashes(
        event,
        payload.get("read-restriction"),
        "read restriction",
    )?;
    validate_read_restriction_policy_hashes(
        event,
        authorization_receipt_read_restriction(payload),
        "authorization receipt read restriction",
    )?;
    if event.event_type == "table.commit" {
        validate_table_commit_hash_evidence(event)?;
    }
    if event.event_type == "table.commits-listed" {
        validate_table_commit_history_event_evidence(event, payload)?;
    }
    if event.event_type == "table.scan-planned" {
        validate_scan_planned_event_evidence(event, payload)?;
    }
    if event.event_type == "table.scan-tasks-fetched" {
        validate_scan_tasks_fetched_event_evidence(event, payload)?;
    }
    if matches!(
        event.event_type.as_str(),
        "table.created" | "table.loaded" | "table.deleted" | "table.restored"
    ) {
        validate_table_lifecycle_event_evidence(event, payload)?;
    }
    if event.event_type == "credentials.vend-attempted" {
        validate_credential_vend_event_evidence(event, payload)?;
    }
    if event.event_type == "storage-profile.upserted" {
        validate_storage_profile_upsert_event_evidence(event, payload)?;
    }
    if event.event_type == "policy-binding.upserted" {
        validate_policy_binding_upsert_event_evidence(event, payload)?;
    }
    if event.event_type == "project.upserted" {
        validate_project_upsert_event_evidence(event, payload)?;
    }
    if event.event_type == "server.upserted" {
        validate_server_upsert_event_evidence(event, payload)?;
    }
    if event.event_type == "warehouse.upserted" {
        validate_warehouse_upsert_event_evidence(event, payload)?;
    }
    if event.event_type == "catalog.config-read" {
        validate_catalog_config_read_event_evidence(event, payload)?;
    }
    if event.event_type == "namespace.listed" {
        validate_namespace_list_event_evidence(event, payload)?;
    }
    if event.event_type == "view.listed" {
        validate_view_list_event_evidence(event, payload)?;
    }
    if matches!(
        event.event_type.as_str(),
        "view.upserted" | "view.loaded" | "view.dropped"
    ) {
        validate_view_lifecycle_event_evidence(event, payload)?;
    }
    if matches!(
        event.event_type.as_str(),
        "policy-binding.listed"
            | "project.listed"
            | "server.listed"
            | "storage-profile.listed"
            | "warehouse.listed"
    ) {
        validate_management_list_event_evidence(event, payload)?;
    }
    if matches!(
        event.event_type.as_str(),
        "namespace.created" | "namespace.dropped" | "namespace.loaded"
    ) {
        validate_namespace_lifecycle_event_evidence(event, payload)?;
    }
    if event.event_type == "view.version-receipts-listed" {
        validate_view_receipt_list_event_evidence(event, payload)?;
    }
    if event.event_type == "view.version-receipt-chains-listed" {
        validate_view_receipt_chain_event_evidence(event, payload)?;
    }
    if event.event_type == "querygraph.bootstrap" {
        validate_querygraph_bootstrap_event_evidence(event, payload)?;
    }
    Ok(())
}

pub(crate) fn validate_outbox_event_type_binding(
    event: &OutboxEvent,
    payload: &Value,
    label: &str,
    required: bool,
) -> Result<(), LakeCatError> {
    let Some(event_type) = payload.get("event-type") else {
        if required {
            return Err(outbox_evidence_error(
                event,
                &format!("{label} missing event-type"),
            ));
        }
        return Ok(());
    };
    let Some(event_type) = event_type.as_str().filter(|value| !value.trim().is_empty()) else {
        return Err(outbox_evidence_error(
            event,
            &format!("{label} event-type must be a non-empty string when present"),
        ));
    };
    if event_type != event.event_type {
        return Err(outbox_evidence_error(
            event,
            &format!("{label} event-type must match outbox event type"),
        ));
    }
    Ok(())
}

pub(crate) fn outbox_event_id_matches_payload_hash(event: &OutboxEvent) -> bool {
    content_hash_json(&event.payload).is_ok_and(|hash| hash == event.event_id)
}

pub(crate) fn is_known_outbox_event_type(event_type: &str) -> bool {
    matches!(
        event_type,
        "catalog.config-read"
            | "credentials.vend-attempted"
            | "namespace.created"
            | "namespace.dropped"
            | "namespace.listed"
            | "namespace.loaded"
            | "policy-binding.listed"
            | "policy-binding.upserted"
            | "project.listed"
            | "project.upserted"
            | "querygraph.bootstrap"
            | "server.listed"
            | "server.upserted"
            | "storage-profile.listed"
            | "storage-profile.upserted"
            | "table.commit"
            | "table.commits-listed"
            | "table.created"
            | "table.deleted"
            | "table.loaded"
            | "table.restored"
            | "table.scan-planned"
            | "table.scan-tasks-fetched"
            | "view.dropped"
            | "view.listed"
            | "view.loaded"
            | "view.upserted"
            | "view.version-receipt-chains-listed"
            | "view.version-receipts-listed"
            | "warehouse.listed"
            | "warehouse.upserted"
    )
}

pub(crate) fn validate_read_restriction_policy_hashes(
    event: &OutboxEvent,
    restriction: Option<&Value>,
    evidence_label: &str,
) -> Result<(), LakeCatError> {
    let Some(policy_hashes) = restriction.and_then(|restriction| restriction.get("policy-hashes"))
    else {
        return Ok(());
    };
    let Some(policy_hashes) = policy_hashes.as_array() else {
        return Err(outbox_evidence_error(
            event,
            &format!("{evidence_label} policy-hashes must be an array"),
        ));
    };
    if policy_hashes.is_empty() {
        return Err(outbox_evidence_error(
            event,
            &format!("{evidence_label} policy-hashes must not be empty"),
        ));
    }
    for policy_hash in policy_hashes {
        if !policy_hash
            .as_str()
            .is_some_and(is_full_sha256_digest_evidence)
        {
            return Err(outbox_evidence_error(
                event,
                &format!(
                    "{evidence_label} policy-hashes must contain full SHA-256 digest evidence"
                ),
            ));
        }
    }
    let mut seen = BTreeSet::new();
    if policy_hashes
        .iter()
        .filter_map(Value::as_str)
        .any(|policy_hash| !seen.insert(policy_hash))
    {
        return Err(outbox_evidence_error(
            event,
            &format!("{evidence_label} policy-hashes must be duplicate-free"),
        ));
    }
    Ok(())
}
