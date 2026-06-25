use std::collections::BTreeSet;

use lakecat_core::{LakeCatError, Namespace, TableIdent};
#[cfg(feature = "sail-local")]
use lakecat_sail::catalog_provider::{
    LakeCatCatalogProvider, ProviderFetchScanTasksRequest, ProviderScanPlanningRequest,
};
use lakecat_store::OutboxEvent;
use serde_json::Value;

use crate::*;

pub(crate) fn validate_table_commit_history_event_evidence(
    event: &OutboxEvent,
    payload: &Value,
) -> Result<(), LakeCatError> {
    if event.payload.get("payload").is_some() {
        validate_object_evidence_schema(
            event,
            &event.payload,
            "table commit-history outbox payload",
            TABLE_COMMIT_HISTORY_OUTBOX_PAYLOAD_FIELDS,
        )?;
    }
    validate_object_evidence_schema(
        event,
        payload,
        "table commit-history",
        TABLE_COMMIT_HISTORY_EVIDENCE_FIELDS,
    )?;
    let table = validate_required_outbox_table_identity(event, "table commit-history")?;
    validate_table_lifecycle_payload_scope(event, payload, &table, "table commit-history")?;
    validate_required_payload_table_hint(event, payload, &table, "table commit-history")?;
    let Some(receipt_principal) = payload
        .get("authorization-receipt")
        .and_then(|receipt| receipt.get("principal"))
    else {
        return Err(outbox_evidence_error(
            event,
            "table commit-history evidence must contain authorization receipt principal",
        ));
    };
    let receipt_principal = decode_outbox_principal_value(
        event,
        receipt_principal,
        "table commit-history authorization receipt principal",
    )?;
    validate_authorization_receipt_action(event, payload, "table commit-history")?;
    validate_authorization_receipt_allowed(event, payload, "table commit-history")?;
    validate_authorization_receipt_engine(event, payload, "table commit-history")?;
    validate_authorization_receipt_checked_at(event, payload, "table commit-history")?;
    let commit_hashes = validate_required_full_hash_array_field(event, payload, "commit-hashes")?;
    validate_unique_hash_array(event, &commit_hashes, "table commit-history commit-hashes")?;
    let sequence_numbers = payload
        .get("sequence-numbers")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            outbox_evidence_error(
                event,
                "table commit-history evidence must contain sequence-numbers",
            )
        })?;
    let mut previous_sequence_number = None;
    for sequence_number in sequence_numbers {
        let Some(sequence_number) = sequence_number.as_u64() else {
            return Err(outbox_evidence_error(
                event,
                "table commit-history sequence-numbers must be unsigned integers",
            ));
        };
        if sequence_number == 0 {
            return Err(outbox_evidence_error(
                event,
                "table commit-history sequence-numbers must be positive",
            ));
        }
        if previous_sequence_number
            .map(|previous| sequence_number <= previous)
            .unwrap_or(false)
        {
            return Err(outbox_evidence_error(
                event,
                "table commit-history sequence-numbers must be strictly increasing",
            ));
        }
        previous_sequence_number = Some(sequence_number);
    }
    let Some(expected_commit_count) = payload.get("commit-count").and_then(Value::as_u64) else {
        return Err(outbox_evidence_error(
            event,
            "table commit-history evidence must contain commit-count",
        ));
    };
    if commit_hashes.len() as u64 != expected_commit_count {
        return Err(outbox_evidence_error(
            event,
            "table commit-history commit-count does not match commit-hashes",
        ));
    }
    if sequence_numbers.len() as u64 != expected_commit_count {
        return Err(outbox_evidence_error(
            event,
            "table commit-history commit-count does not match sequence-numbers",
        ));
    }
    validate_string_field_equals(
        event,
        payload,
        "principal-subject",
        &receipt_principal.subject,
        "table commit-history",
    )?;
    validate_string_field_equals(
        event,
        payload,
        "principal-kind",
        principal_kind_name(&receipt_principal.kind),
        "table commit-history",
    )?;
    Ok(())
}

pub(crate) fn validate_table_lifecycle_event_evidence(
    event: &OutboxEvent,
    payload: &Value,
) -> Result<(), LakeCatError> {
    if event.payload.get("payload").is_some() {
        validate_object_evidence_schema(
            event,
            &event.payload,
            "table lifecycle outbox payload",
            TABLE_LIFECYCLE_OUTBOX_PAYLOAD_FIELDS,
        )?;
    }
    validate_object_evidence_schema(
        event,
        payload,
        "table lifecycle",
        TABLE_LIFECYCLE_EVIDENCE_FIELDS,
    )?;
    let Some(table_value) = event.payload.get("table") else {
        return Err(outbox_evidence_error(
            event,
            "table lifecycle evidence must contain table identity",
        ));
    };
    let table = decode_table_lifecycle_identity(event, table_value, "table lifecycle")?;
    validate_authorization_receipt_principal(event, payload, "table lifecycle")?;
    validate_table_lifecycle_payload_scope(event, payload, &table, "table lifecycle")?;
    if matches!(
        event.event_type.as_str(),
        "table.created" | "table.loaded" | "table.restored"
    ) {
        require_positive_i64_field(event, payload, "format-version", "table lifecycle")?;
        if payload.get("version").and_then(Value::as_u64).is_none() {
            return Err(outbox_evidence_error(
                event,
                "table lifecycle evidence must contain unsigned version",
            ));
        }
    }

    if let Some(payload_table) = payload.get("table") {
        validate_table_lifecycle_table_hint(
            event,
            payload_table,
            &table,
            "table lifecycle payload table",
        )?;
    }
    let soft_delete = payload
        .get("soft-delete")
        .or_else(|| event.payload.get("soft-delete"));
    if event.event_type == "table.deleted" && soft_delete.is_none() {
        return Err(outbox_evidence_error(
            event,
            "table lifecycle delete evidence must contain soft-delete",
        ));
    }
    if let Some(soft_delete) = soft_delete {
        validate_object_evidence_schema(
            event,
            soft_delete,
            "table lifecycle soft-delete",
            TABLE_LIFECYCLE_SOFT_DELETE_EVIDENCE_FIELDS,
        )?;
        let Some(soft_delete) = soft_delete.as_object() else {
            return Err(outbox_evidence_error(
                event,
                "table lifecycle soft-delete evidence must be an object",
            ));
        };
        let Some(soft_delete_table) = soft_delete.get("table") else {
            return Err(outbox_evidence_error(
                event,
                "table lifecycle soft-delete evidence must contain table identity",
            ));
        };
        validate_table_lifecycle_table_hint(
            event,
            soft_delete_table,
            &table,
            "table lifecycle soft-delete table",
        )?;
        if soft_delete.get("version").and_then(Value::as_u64).is_none() {
            return Err(outbox_evidence_error(
                event,
                "table lifecycle soft-delete evidence must contain unsigned version",
            ));
        }
        if event.event_type == "table.deleted"
            && soft_delete.get("version").and_then(Value::as_u64) == Some(0)
        {
            return Err(outbox_evidence_error(
                event,
                "table lifecycle soft-delete version must be positive",
            ));
        }
        require_positive_i64_field(
            event,
            &Value::Object(soft_delete.clone()),
            "format-version",
            "table lifecycle soft-delete",
        )?;
        optional_non_empty_string_field(
            event,
            &Value::Object(soft_delete.clone()),
            "metadata-location",
            "table lifecycle soft-delete",
        )?;
        validate_optional_location_evidence(
            event,
            soft_delete.get("metadata-location"),
            "table lifecycle soft-delete metadata-location",
        )?;
    }

    if let Some(metadata_graph) = payload.get("metadata-graph") {
        validate_object_evidence_schema(
            event,
            metadata_graph,
            "table lifecycle metadata-graph",
            TABLE_METADATA_GRAPH_EVIDENCE_FIELDS,
        )?;
    }
    optional_non_empty_string_field(event, payload, "metadata-location", "table lifecycle")?;
    optional_non_empty_string_field(event, payload, "location", "table lifecycle")?;
    validate_optional_location_evidence(
        event,
        payload.get("metadata-location"),
        "table lifecycle metadata-location",
    )?;
    validate_optional_location_evidence(
        event,
        payload.get("location"),
        "table lifecycle location",
    )?;
    Ok(())
}

pub(crate) fn decode_table_lifecycle_identity(
    event: &OutboxEvent,
    table: &Value,
    label: &str,
) -> Result<TableIdent, LakeCatError> {
    if table.is_object() {
        validate_object_evidence_schema(
            event,
            table,
            &format!("{label} table identity"),
            TABLE_IDENTITY_EVIDENCE_FIELDS,
        )?;
    }
    serde_json::from_value(table.clone()).map_err(|_| {
        outbox_evidence_error(
            event,
            &format!("{label} evidence has invalid table identity"),
        )
    })
}

pub(crate) fn validate_table_lifecycle_payload_scope(
    event: &OutboxEvent,
    payload: &Value,
    table: &TableIdent,
    label: &str,
) -> Result<(), LakeCatError> {
    if let Some(warehouse) = payload.get("warehouse") {
        let warehouse = warehouse
            .as_str()
            .filter(|warehouse| !warehouse.is_empty())
            .ok_or_else(|| {
                outbox_evidence_error(
                    event,
                    &format!("{label} warehouse must be a non-empty string when present"),
                )
            })?;
        if warehouse != table.warehouse.as_str() {
            return Err(outbox_evidence_error(
                event,
                &format!("{label} warehouse does not match table identity"),
            ));
        }
    }
    if let Some(namespace) = payload.get("namespace") {
        let namespace = table_lifecycle_namespace_from_value(event, namespace, label)?;
        if namespace != table.namespace {
            return Err(outbox_evidence_error(
                event,
                &format!("{label} namespace does not match table identity"),
            ));
        }
    }
    if let Some(table_name) = payload.get("table") {
        if table_name.is_object() {
            return Ok(());
        }
        let table_name = table_name
            .as_str()
            .filter(|table_name| !table_name.is_empty())
            .ok_or_else(|| {
                outbox_evidence_error(
                    event,
                    &format!("{label} table must be a non-empty string when present"),
                )
            })?;
        if table_name != table.name.as_str() {
            return Err(outbox_evidence_error(
                event,
                &format!("{label} table name does not match table identity"),
            ));
        }
    }
    Ok(())
}

pub(crate) fn validate_table_lifecycle_table_hint(
    event: &OutboxEvent,
    table_hint: &Value,
    table: &TableIdent,
    label: &str,
) -> Result<(), LakeCatError> {
    if table_hint.is_object() {
        let hinted_table = decode_table_lifecycle_identity(event, table_hint, label)?;
        if hinted_table != *table {
            return Err(outbox_evidence_error(
                event,
                &format!("{label} does not match table identity"),
            ));
        }
        return Ok(());
    }
    let table_name = table_hint
        .as_str()
        .filter(|table_name| !table_name.is_empty())
        .ok_or_else(|| {
            outbox_evidence_error(
                event,
                &format!("{label} must be a table identity object or non-empty table name"),
            )
        })?;
    if table_name != table.name.as_str() {
        return Err(outbox_evidence_error(
            event,
            &format!("{label} name does not match table identity"),
        ));
    }
    Ok(())
}

pub(crate) fn validate_scan_planned_event_evidence(
    event: &OutboxEvent,
    payload: &Value,
) -> Result<(), LakeCatError> {
    if event.payload.get("payload").is_some() {
        validate_object_evidence_schema(
            event,
            &event.payload,
            "scan-planned outbox payload",
            SCAN_PLANNED_OUTBOX_PAYLOAD_FIELDS,
        )?;
    }
    validate_object_evidence_schema(event, payload, "scan-planned", SCAN_PLANNED_EVIDENCE_FIELDS)?;
    let table = validate_required_outbox_table_identity(event, "scan-planned")?;
    validate_required_payload_table_hint(event, payload, &table, "scan-planned")?;
    validate_required_unsigned_count_field(event, payload, "scan-task-count", "scan-planned")?;
    validate_authorization_receipt_principal(event, payload, "scan-planned")?;
    validate_plan_task_evidence(event, payload.get("plan-task"), "scan-planned")?;
    validate_optional_location_evidence(
        event,
        payload.get("storage-location"),
        "scan-planned storage-location",
    )?;
    validate_optional_location_evidence(
        event,
        payload.get("metadata-location"),
        "scan-planned metadata-location",
    )?;

    let requested_projection = validate_required_non_empty_string_array_field(
        event,
        payload,
        "requested-projection",
        "scan-planned",
    )?;
    let effective_projection = validate_required_non_empty_string_array_field(
        event,
        payload,
        "effective-projection",
        "scan-planned",
    )?;
    validate_effective_evidence_subset(
        event,
        "scan-planned effective-projection",
        &effective_projection,
        "requested-projection",
        &requested_projection,
        false,
    )?;
    validate_effective_projection_matches_read_restriction(
        event,
        payload,
        "scan-planned",
        &effective_projection,
    )?;
    validate_read_restriction_evidence_schema(
        event,
        payload.get("read-restriction"),
        "scan-planned read-restriction",
    )?;
    validate_read_restriction_evidence_schema(
        event,
        authorization_receipt_read_restriction(payload),
        "scan-planned authorization receipt read-restriction",
    )?;
    validate_required_read_restriction_policy_hashes(
        event,
        payload.get("read-restriction"),
        "scan-planned read-restriction",
    )?;
    validate_required_read_restriction_policy_hashes(
        event,
        authorization_receipt_read_restriction(payload),
        "scan-planned authorization receipt read-restriction",
    )?;
    validate_read_restriction_receipt_match(event, payload, "scan-planned")?;
    validate_scan_read_restriction_row_predicate(event, payload, "scan-planned")?;

    let requested_stats = validate_required_non_empty_string_array_field(
        event,
        payload,
        "requested-stats-fields",
        "scan-planned",
    )?;
    let effective_stats = validate_required_non_empty_string_array_field(
        event,
        payload,
        "effective-stats-fields",
        "scan-planned",
    )?;
    validate_effective_evidence_subset(
        event,
        "scan-planned effective-stats-fields",
        &effective_stats,
        "requested-stats-fields",
        &requested_stats,
        false,
    )?;
    validate_effective_stats_matches_read_restriction(
        event,
        payload,
        "scan-planned",
        &effective_stats,
    )?;
    validate_read_restriction_purpose_and_ttl(event, payload, "scan-planned")?;
    if payload.get("read-restriction").is_some() {
        validate_scan_required_filters_match_row_predicate(event, payload, "scan-planned")?;
    } else if payload.get("required-filters").is_some() {
        validate_scan_required_filters_match_row_predicate(event, payload, "scan-planned")?;
    }
    Ok(())
}

pub(crate) fn validate_scan_tasks_fetched_event_evidence(
    event: &OutboxEvent,
    payload: &Value,
) -> Result<(), LakeCatError> {
    if event.payload.get("payload").is_some() {
        validate_object_evidence_schema(
            event,
            &event.payload,
            "scan-tasks-fetched outbox payload",
            SCAN_TASKS_FETCHED_OUTBOX_PAYLOAD_FIELDS,
        )?;
    }
    validate_object_evidence_schema(
        event,
        payload,
        "scan-tasks-fetched",
        SCAN_TASKS_FETCHED_EVIDENCE_FIELDS,
    )?;
    let table = validate_required_outbox_table_identity(event, "scan-tasks-fetched")?;
    validate_required_payload_table_hint(event, payload, &table, "scan-tasks-fetched")?;
    validate_authorization_receipt_principal(event, payload, "scan-tasks-fetched")?;
    for field in [
        "file-scan-task-count",
        "delete-file-count",
        "child-plan-task-count",
    ] {
        validate_required_unsigned_count_field(event, payload, field, "scan-tasks-fetched")?;
    }
    validate_plan_task_evidence(event, payload.get("plan-task"), "scan-tasks-fetched")?;
    validate_optional_location_evidence(
        event,
        payload.get("storage-location"),
        "scan-tasks-fetched storage-location",
    )?;
    validate_optional_location_evidence(
        event,
        payload.get("metadata-location"),
        "scan-tasks-fetched metadata-location",
    )?;

    let required_projection = validate_required_non_empty_string_array_field(
        event,
        payload,
        "required-projection",
        "scan-tasks-fetched",
    )?;
    let effective_projection = validate_required_non_empty_string_array_field(
        event,
        payload,
        "effective-projection",
        "scan-tasks-fetched",
    )?;
    validate_effective_evidence_subset(
        event,
        "scan-tasks-fetched effective-projection",
        &effective_projection,
        "required-projection",
        &required_projection,
        true,
    )?;
    validate_effective_projection_matches_read_restriction(
        event,
        payload,
        "scan-tasks-fetched",
        &effective_projection,
    )?;
    let requested_stats = validate_required_non_empty_string_array_field(
        event,
        payload,
        "requested-stats-fields",
        "scan-tasks-fetched",
    )?;
    let effective_stats = validate_required_non_empty_string_array_field(
        event,
        payload,
        "effective-stats-fields",
        "scan-tasks-fetched",
    )?;
    validate_effective_evidence_subset(
        event,
        "scan-tasks-fetched effective-stats-fields",
        &effective_stats,
        "requested-stats-fields",
        &requested_stats,
        true,
    )?;
    validate_effective_stats_matches_read_restriction(
        event,
        payload,
        "scan-tasks-fetched",
        &effective_stats,
    )?;
    if payload.get("stats-fields").is_some() {
        let stats_fields = validate_required_non_empty_string_array_field(
            event,
            payload,
            "stats-fields",
            "scan-tasks-fetched",
        )?;
        validate_effective_evidence_subset(
            event,
            "scan-tasks-fetched stats-fields",
            &stats_fields,
            "effective-stats-fields",
            &effective_stats,
            true,
        )?;
    }
    validate_read_restriction_evidence_schema(
        event,
        payload.get("read-restriction"),
        "scan-tasks-fetched read-restriction",
    )?;
    validate_read_restriction_evidence_schema(
        event,
        authorization_receipt_read_restriction(payload),
        "scan-tasks-fetched authorization receipt read-restriction",
    )?;
    validate_required_read_restriction_policy_hashes(
        event,
        payload.get("read-restriction"),
        "scan-tasks-fetched read-restriction",
    )?;
    validate_required_read_restriction_policy_hashes(
        event,
        authorization_receipt_read_restriction(payload),
        "scan-tasks-fetched authorization receipt read-restriction",
    )?;
    validate_read_restriction_receipt_match(event, payload, "scan-tasks-fetched")?;
    validate_scan_read_restriction_row_predicate(event, payload, "scan-tasks-fetched")?;
    let Some(required_filters) = payload.get("required-filters") else {
        return Err(outbox_evidence_error(
            event,
            "scan-tasks-fetched evidence must contain required-filters",
        ));
    };
    if !required_filters.is_array() {
        return Err(outbox_evidence_error(
            event,
            "scan-tasks-fetched required-filters must be an array",
        ));
    }
    validate_scan_required_filters_match_row_predicate(event, payload, "scan-tasks-fetched")?;
    validate_read_restriction_purpose_and_ttl(event, payload, "scan-tasks-fetched")?;
    Ok(())
}

pub(crate) fn validate_plan_task_evidence(
    event: &OutboxEvent,
    plan_task: Option<&Value>,
    label: &str,
) -> Result<(), LakeCatError> {
    let Some(plan_task) = plan_task else {
        return Ok(());
    };
    let Some(plan_task) = plan_task
        .as_str()
        .filter(|plan_task| !plan_task.trim().is_empty())
    else {
        return Err(outbox_evidence_error(
            event,
            &format!("{label} plan-task must be a non-empty string"),
        ));
    };
    if !plan_task.starts_with("lakecat:") {
        return Err(outbox_evidence_error(
            event,
            &format!("{label} plan-task must be LakeCat-issued evidence"),
        ));
    }
    if plan_task.contains(['?', '#']) || plan_task.contains("://") {
        return Err(outbox_evidence_error(
            event,
            &format!("{label} plan-task must not contain decorated location material"),
        ));
    }
    let normalized = plan_task.to_ascii_lowercase();
    for marker in [
        "token=",
        "secret=",
        "credential=",
        "password=",
        "access_key=",
        "session_token=",
    ] {
        if normalized.contains(marker) {
            return Err(outbox_evidence_error(
                event,
                &format!("{label} plan-task must not contain credential material"),
            ));
        }
    }
    Ok(())
}

pub(crate) fn validate_optional_location_evidence(
    event: &OutboxEvent,
    location: Option<&Value>,
    label: &str,
) -> Result<(), LakeCatError> {
    let Some(location) = location else {
        return Ok(());
    };
    let Some(location) = location
        .as_str()
        .filter(|location| !location.trim().is_empty())
    else {
        return Err(outbox_evidence_error(
            event,
            &format!("{label} must be a non-empty string"),
        ));
    };
    if location.contains(['?', '#']) {
        return Err(outbox_evidence_error(
            event,
            &format!("{label} must not contain decorated location material"),
        ));
    }
    if location_has_userinfo(location) {
        return Err(outbox_evidence_error(
            event,
            &format!("{label} must not include userinfo"),
        ));
    }
    let normalized = location.to_ascii_lowercase();
    for marker in [
        "token=",
        "secret=",
        "credential=",
        "password=",
        "access_key=",
        "session_token=",
    ] {
        if normalized.contains(marker) {
            return Err(outbox_evidence_error(
                event,
                &format!("{label} must not contain credential material"),
            ));
        }
    }
    Ok(())
}

pub(crate) fn validate_scan_required_filters_match_row_predicate(
    event: &OutboxEvent,
    payload: &Value,
    label: &str,
) -> Result<(), LakeCatError> {
    let required_filters = match payload.get("required-filters") {
        Some(required_filters) => required_filters.as_array().ok_or_else(|| {
            outbox_evidence_error(event, &format!("{label} required-filters must be an array"))
        })?,
        None => {
            return Err(outbox_evidence_error(
                event,
                &format!("{label} required-filters must be an array"),
            ));
        }
    };
    let Some(read_restriction) = payload.get("read-restriction") else {
        if !required_filters.is_empty() {
            return Err(outbox_evidence_error(
                event,
                &format!(
                    "{label} required-filters must be empty without read-restriction row-predicate"
                ),
            ));
        }
        return Ok(());
    };
    let Some(row_predicate) = read_restriction.get("row-predicate") else {
        return Err(outbox_evidence_error(
            event,
            &format!("{label} read-restriction must contain row-predicate"),
        ));
    };
    let expected_filters = vec![row_predicate.clone()];
    if required_filters.as_slice() != expected_filters.as_slice() {
        return Err(outbox_evidence_error(
            event,
            &format!(
                "{label} required-filters must exactly preserve read-restriction row-predicate"
            ),
        ));
    }
    Ok(())
}

pub(crate) fn validate_read_restriction_purpose_and_ttl(
    event: &OutboxEvent,
    payload: &Value,
    label: &str,
) -> Result<(), LakeCatError> {
    let Some(read_restriction) = payload.get("read-restriction") else {
        return Ok(());
    };
    if read_restriction
        .get("purpose")
        .and_then(Value::as_str)
        .is_none_or(|purpose| purpose.trim().is_empty())
    {
        return Err(outbox_evidence_error(
            event,
            &format!("{label} read-restriction purpose must not be blank"),
        ));
    }
    let Some(ttl_seconds) = read_restriction
        .get("max-credential-ttl-seconds")
        .and_then(Value::as_u64)
    else {
        return Err(outbox_evidence_error(
            event,
            &format!(
                "{label} read-restriction max-credential-ttl-seconds must be a positive integer"
            ),
        ));
    };
    if ttl_seconds == 0 {
        return Err(outbox_evidence_error(
            event,
            &format!("{label} read-restriction max-credential-ttl-seconds must be positive"),
        ));
    }
    Ok(())
}

pub(crate) fn validate_scan_read_restriction_row_predicate(
    event: &OutboxEvent,
    payload: &Value,
    label: &str,
) -> Result<(), LakeCatError> {
    let Some(read_restriction) = payload.get("read-restriction") else {
        return Ok(());
    };
    let Some(row_predicate) = read_restriction.get("row-predicate") else {
        return Err(outbox_evidence_error(
            event,
            &format!("{label} read-restriction must contain row-predicate"),
        ));
    };
    let Some(row_predicate) = row_predicate.as_object() else {
        return Err(outbox_evidence_error(
            event,
            &format!("{label} read-restriction row-predicate must be an object"),
        ));
    };
    if row_predicate.is_empty() {
        return Err(outbox_evidence_error(
            event,
            &format!("{label} read-restriction row-predicate must contain predicate evidence"),
        ));
    }
    let Some(predicate_type) = row_predicate
        .get("type")
        .and_then(Value::as_str)
        .filter(|predicate_type| !predicate_type.trim().is_empty())
    else {
        return Err(outbox_evidence_error(
            event,
            &format!("{label} read-restriction row-predicate.type must not be blank"),
        ));
    };
    if predicate_type == "always-true" {
        return Ok(());
    }
    if row_predicate
        .get("term")
        .and_then(Value::as_str)
        .is_none_or(|term| term.trim().is_empty())
    {
        return Err(outbox_evidence_error(
            event,
            &format!("{label} read-restriction row-predicate.term must not be blank"),
        ));
    }
    if matches!(predicate_type, "eq" | "not-eq") && !row_predicate.contains_key("value") {
        return Err(outbox_evidence_error(
            event,
            &format!(
                "{label} read-restriction row-predicate.value is required for {predicate_type} predicate evidence"
            ),
        ));
    }
    Ok(())
}

pub(crate) fn validate_read_restriction_receipt_match(
    event: &OutboxEvent,
    payload: &Value,
    label: &str,
) -> Result<(), LakeCatError> {
    match (
        payload.get("read-restriction"),
        authorization_receipt_read_restriction(payload),
    ) {
        (None, None) => Ok(()),
        (Some(read_restriction), Some(receipt_read_restriction))
            if read_restriction == receipt_read_restriction =>
        {
            Ok(())
        }
        (Some(_), None) => Err(outbox_evidence_error(
            event,
            &format!("{label} read-restriction must be captured in authorization receipt context"),
        )),
        (None, Some(_)) => Err(outbox_evidence_error(
            event,
            &format!(
                "{label} authorization receipt read-restriction must match top-level read-restriction"
            ),
        )),
        (Some(_), Some(_)) => Err(outbox_evidence_error(
            event,
            &format!("{label} read-restriction must match authorization receipt context"),
        )),
    }
}

pub(crate) fn validate_required_outbox_table_identity(
    event: &OutboxEvent,
    label: &str,
) -> Result<TableIdent, LakeCatError> {
    let Some(table) = event.payload.get("table") else {
        return Err(outbox_evidence_error(
            event,
            &format!("{label} evidence must contain table identity"),
        ));
    };
    decode_table_lifecycle_identity(event, table, label)
}

pub(crate) fn validate_required_payload_table_hint(
    event: &OutboxEvent,
    payload: &Value,
    table: &TableIdent,
    label: &str,
) -> Result<(), LakeCatError> {
    let Some(payload_table) = payload.get("table") else {
        return Err(outbox_evidence_error(
            event,
            &format!("{label} evidence must contain payload table identity"),
        ));
    };
    validate_table_lifecycle_table_hint(
        event,
        payload_table,
        table,
        &format!("{label} payload table"),
    )
}

pub(crate) fn validate_required_string_array_field(
    event: &OutboxEvent,
    payload: &Value,
    field: &str,
    label: &str,
) -> Result<Vec<String>, LakeCatError> {
    let Some(values) = payload.get(field) else {
        return Err(outbox_evidence_error(
            event,
            &format!("{label} evidence must contain {field}"),
        ));
    };
    let Some(values) = values.as_array() else {
        return Err(outbox_evidence_error(
            event,
            &format!("{label} {field} must be an array"),
        ));
    };
    let values = values
        .iter()
        .map(|value| {
            value
                .as_str()
                .filter(|value| !value.trim().is_empty())
                .map(ToString::to_string)
                .ok_or_else(|| {
                    outbox_evidence_error(
                        event,
                        &format!("{label} {field} must contain non-empty strings"),
                    )
                })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut seen = BTreeSet::new();
    if values.iter().any(|value| !seen.insert(value.as_str())) {
        return Err(outbox_evidence_error(
            event,
            &format!("{label} {field} must be duplicate-free"),
        ));
    }
    Ok(values)
}

pub(crate) fn validate_required_non_empty_string_array_field(
    event: &OutboxEvent,
    payload: &Value,
    field: &str,
    label: &str,
) -> Result<Vec<String>, LakeCatError> {
    let values = validate_required_string_array_field(event, payload, field, label)?;
    if values.is_empty() {
        return Err(outbox_evidence_error(
            event,
            &format!("{label} {field} must not be empty"),
        ));
    }
    Ok(values)
}

pub(crate) fn validate_effective_evidence_subset(
    event: &OutboxEvent,
    effective_label: &str,
    effective: &[String],
    requested_field: &str,
    requested: &[String],
    require_exact: bool,
) -> Result<(), LakeCatError> {
    if requested.is_empty() {
        if require_exact && !effective.is_empty() {
            return Err(outbox_evidence_error(
                event,
                &format!("{effective_label} must match empty {requested_field}"),
            ));
        }
        return Ok(());
    }
    let requested = requested.iter().collect::<BTreeSet<_>>();
    let effective = effective.iter().collect::<BTreeSet<_>>();
    if !effective.iter().all(|field| requested.contains(*field)) {
        return Err(outbox_evidence_error(
            event,
            &format!("{effective_label} must be a subset of {requested_field}"),
        ));
    }
    if require_exact && effective != requested {
        return Err(outbox_evidence_error(
            event,
            &format!("{effective_label} must match {requested_field}"),
        ));
    }
    Ok(())
}

pub(crate) fn validate_effective_projection_matches_read_restriction(
    event: &OutboxEvent,
    payload: &Value,
    label: &str,
    effective_projection: &[String],
) -> Result<(), LakeCatError> {
    let Some(read_restriction) = payload.get("read-restriction") else {
        return Ok(());
    };
    if read_restriction.get("allowed-columns").is_none() {
        return Ok(());
    }
    let allowed_columns =
        validate_read_restriction_allowed_columns(event, read_restriction, label)?;
    let allowed_columns = allowed_columns.iter().collect::<BTreeSet<_>>();
    if !effective_projection
        .iter()
        .all(|field| allowed_columns.contains(field))
    {
        return Err(outbox_evidence_error(
            event,
            &format!("{label} effective-projection must be allowed by read-restriction"),
        ));
    }
    Ok(())
}

pub(crate) fn validate_effective_stats_matches_read_restriction(
    event: &OutboxEvent,
    payload: &Value,
    label: &str,
    effective_stats: &[String],
) -> Result<(), LakeCatError> {
    let Some(read_restriction) = payload.get("read-restriction") else {
        return Ok(());
    };
    if read_restriction.get("allowed-columns").is_none() {
        return Ok(());
    }
    let allowed_columns =
        validate_read_restriction_allowed_columns(event, read_restriction, label)?;
    let allowed_columns = allowed_columns.iter().collect::<BTreeSet<_>>();
    if !effective_stats
        .iter()
        .all(|field| allowed_columns.contains(field))
    {
        return Err(outbox_evidence_error(
            event,
            &format!("{label} effective-stats-fields must be allowed by read-restriction"),
        ));
    }
    Ok(())
}

pub(crate) fn validate_read_restriction_allowed_columns(
    event: &OutboxEvent,
    read_restriction: &Value,
    label: &str,
) -> Result<Vec<String>, LakeCatError> {
    let allowed_columns = validate_required_string_array_field(
        event,
        read_restriction,
        "allowed-columns",
        &format!("{label} read-restriction"),
    )?;
    if allowed_columns.is_empty() {
        return Err(outbox_evidence_error(
            event,
            &format!("{label} read-restriction allowed-columns must not be empty"),
        ));
    }
    Ok(allowed_columns)
}

pub(crate) fn table_lifecycle_namespace_from_value(
    event: &OutboxEvent,
    namespace: &Value,
    label: &str,
) -> Result<Namespace, LakeCatError> {
    match namespace {
        Value::Array(parts) => {
            let parts = parts
                .iter()
                .map(|part| {
                    part.as_str()
                        .filter(|part| !part.is_empty())
                        .map(ToString::to_string)
                        .ok_or_else(|| {
                            outbox_evidence_error(
                                event,
                                &format!("{label} namespace components must be non-empty strings"),
                            )
                        })
                })
                .collect::<Result<Vec<_>, _>>()?;
            Namespace::new(parts).map_err(|_| {
                outbox_evidence_error(event, &format!("{label} evidence has invalid namespace"))
            })
        }
        Value::String(path) if !path.is_empty() => path.parse::<Namespace>().map_err(|_| {
            outbox_evidence_error(event, &format!("{label} evidence has invalid namespace"))
        }),
        _ => Err(outbox_evidence_error(
            event,
            &format!("{label} namespace must be a non-empty string or array"),
        )),
    }
}
