use lakecat_core::{LakeCatError, TableIdent};
use lakecat_store::OutboxEvent;
use serde_json::Value;

use crate::*;

pub(crate) fn validate_table_rename_event_evidence(
    event: &OutboxEvent,
    payload: &Value,
) -> Result<(), LakeCatError> {
    if event.payload.get("payload").is_some() {
        validate_object_evidence_schema(
            event,
            &event.payload,
            "table rename outbox payload",
            TABLE_RENAME_OUTBOX_PAYLOAD_FIELDS,
        )?;
    }
    validate_object_evidence_schema(event, payload, "table rename", TABLE_RENAME_EVIDENCE_FIELDS)?;
    let outer_table = required_table_identity(
        event,
        event.payload.get("table"),
        "table rename outbox table",
    )?;
    let payload_table =
        required_table_identity(event, payload.get("table"), "table rename payload table")?;
    let source = required_table_identity(event, payload.get("source"), "table rename source")?;
    let destination = required_table_identity(
        event,
        payload.get("destination"),
        "table rename destination",
    )?;
    if outer_table != destination || payload_table != destination {
        return Err(outbox_evidence_error(
            event,
            "table rename table scope must match destination",
        ));
    }
    if source == destination {
        return Err(outbox_evidence_error(
            event,
            "table rename source and destination must differ",
        ));
    }
    if source.warehouse != destination.warehouse {
        return Err(outbox_evidence_error(
            event,
            "table rename source and destination warehouses must match",
        ));
    }

    validate_authorization_receipt_principal(event, payload, "table rename")?;
    let receipt = payload
        .get("authorization-receipt")
        .ok_or_else(|| outbox_evidence_error(event, "table rename receipt is required"))?;
    let receipt_source =
        required_table_identity(event, receipt.get("table"), "table rename receipt source")?;
    if receipt_source != source {
        return Err(outbox_evidence_error(
            event,
            "table rename receipt source must match event source",
        ));
    }
    let receipt_destination = required_table_identity(
        event,
        receipt.pointer("/context/destination-table"),
        "table rename receipt destination",
    )?;
    if receipt_destination != destination {
        return Err(outbox_evidence_error(
            event,
            "table rename receipt destination must match event destination",
        ));
    }
    require_positive_i64_field(event, payload, "format-version", "table rename")?;
    if payload.get("version").and_then(Value::as_u64).is_none() {
        return Err(outbox_evidence_error(
            event,
            "table rename version must be unsigned",
        ));
    }
    optional_non_empty_string_field(event, payload, "metadata-location", "table rename")?;
    validate_optional_location_evidence(
        event,
        payload.get("metadata-location"),
        "table rename metadata-location",
    )?;
    Ok(())
}

fn required_table_identity(
    event: &OutboxEvent,
    value: Option<&Value>,
    label: &str,
) -> Result<TableIdent, LakeCatError> {
    let value = value.ok_or_else(|| {
        outbox_evidence_error(event, &format!("{label} must contain table identity"))
    })?;
    decode_table_lifecycle_identity(event, value, label)
}
