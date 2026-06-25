use std::collections::{BTreeMap, BTreeSet};

use lakecat_core::{LakeCatError, content_hash_bytes, content_hash_json};
#[cfg(feature = "sail-local")]
use lakecat_sail::catalog_provider::{
    LakeCatCatalogProvider, ProviderFetchScanTasksRequest, ProviderScanPlanningRequest,
};
use lakecat_store::OutboxEvent;
use serde_json::{Value, json};

pub(crate) fn validate_required_full_hash_field(
    event: &OutboxEvent,
    object: &Value,
    field: &str,
) -> Result<(), LakeCatError> {
    if object
        .get(field)
        .and_then(Value::as_str)
        .is_some_and(is_full_sha256_digest_evidence)
    {
        return Ok(());
    }
    Err(outbox_evidence_error(
        event,
        &format!("{field} must contain full SHA-256 digest evidence"),
    ))
}

pub(crate) fn required_string_field<'a>(
    event: &OutboxEvent,
    object: &'a Value,
    field: &str,
    label: &str,
) -> Result<&'a str, LakeCatError> {
    object
        .get(field)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            outbox_evidence_error(
                event,
                &format!("{label} {field} must be a non-empty string"),
            )
        })
}

pub(crate) fn validate_storage_profile_id_evidence(
    event: &OutboxEvent,
    profile_id: &str,
    label: &str,
) -> Result<(), LakeCatError> {
    if profile_id
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.'))
    {
        return Ok(());
    }
    Err(outbox_evidence_error(
        event,
        &format!(
            "{label} profile-id contains unsupported characters; storage-profile-id-hash={}",
            content_hash_bytes(profile_id.as_bytes())
        ),
    ))
}

pub(crate) fn validate_management_id_evidence(
    event: &OutboxEvent,
    identifier: &str,
    label: &str,
    field: &str,
) -> Result<(), LakeCatError> {
    if identifier
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.'))
    {
        return Ok(());
    }
    Err(outbox_evidence_error(
        event,
        &format!(
            "{label} {field} contains unsupported characters; {field}-hash={}",
            content_hash_bytes(identifier.as_bytes())
        ),
    ))
}

pub(crate) fn validate_string_content_hash_matches_field(
    event: &OutboxEvent,
    object: &Value,
    value_field: &str,
    hash_field: &str,
    label: &str,
) -> Result<(), LakeCatError> {
    let value = required_string_field(event, object, value_field, label)?;
    let recorded_hash = required_string_field(event, object, hash_field, label)?;
    let computed_hash = content_hash_json(&json!({ value_field: value })).map_err(|_| {
        outbox_evidence_error(
            event,
            &format!("{label} {hash_field} could not be recomputed"),
        )
    })?;
    if recorded_hash != computed_hash {
        return Err(outbox_evidence_error(
            event,
            &format!("{label} {hash_field} must match {value_field}"),
        ));
    }
    Ok(())
}

pub(crate) fn required_pointer_string<'a>(
    event: &OutboxEvent,
    object: &'a Value,
    pointer: &str,
    label: &str,
) -> Result<&'a str, LakeCatError> {
    object
        .pointer(pointer)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| outbox_evidence_error(event, &format!("{label} must be a non-empty string")))
}

pub(crate) fn validate_string_field_equals(
    event: &OutboxEvent,
    object: &Value,
    field: &str,
    expected: &str,
    label: &str,
) -> Result<(), LakeCatError> {
    let actual = required_string_field(event, object, field, label)?;
    if actual == expected {
        return Ok(());
    }
    Err(outbox_evidence_error(
        event,
        &format!("{label} {field} must match catalog evidence"),
    ))
}

pub(crate) fn validate_required_bool_field_equals(
    event: &OutboxEvent,
    object: &Value,
    field: &str,
    expected: bool,
    label: &str,
) -> Result<(), LakeCatError> {
    let Some(value) = object.get(field) else {
        return Err(outbox_evidence_error(
            event,
            &format!("{label} must contain {field}"),
        ));
    };
    let Some(actual) = value.as_bool() else {
        return Err(outbox_evidence_error(
            event,
            &format!("{label} {field} must be a boolean"),
        ));
    };
    if actual == expected {
        return Ok(());
    }
    Err(outbox_evidence_error(
        event,
        &format!("{label} {field} must match catalog evidence"),
    ))
}

pub(crate) fn validate_null_or_absent_field(
    event: &OutboxEvent,
    object: &Value,
    field: &str,
    label: &str,
) -> Result<(), LakeCatError> {
    match object.get(field) {
        None | Some(Value::Null) => Ok(()),
        _ => Err(outbox_evidence_error(
            event,
            &format!("{label} {field} must be absent when not authorized by receipt evidence"),
        )),
    }
}

pub(crate) fn validate_required_full_hash_array_field<'a>(
    event: &OutboxEvent,
    object: &'a Value,
    field: &str,
) -> Result<Vec<&'a str>, LakeCatError> {
    let Some(values) = object.get(field) else {
        return Err(outbox_evidence_error(
            event,
            &format!("{field} must be present"),
        ));
    };
    let Some(values) = values.as_array() else {
        return Err(outbox_evidence_error(
            event,
            &format!("{field} must be an array"),
        ));
    };
    let mut hashes = Vec::with_capacity(values.len());
    for value in values {
        let Some(hash) = value.as_str() else {
            return Err(outbox_evidence_error(
                event,
                &format!("{field} must contain full SHA-256 digest evidence"),
            ));
        };
        if !is_full_sha256_digest_evidence(hash) {
            return Err(outbox_evidence_error(
                event,
                &format!("{field} must contain full SHA-256 digest evidence"),
            ));
        }
        hashes.push(hash);
    }
    Ok(hashes)
}

pub(crate) fn validate_optional_full_hash_array_field<'a>(
    event: &OutboxEvent,
    object: &'a Value,
    field: &str,
) -> Result<Vec<&'a str>, LakeCatError> {
    let Some(values) = object.get(field) else {
        return Ok(Vec::new());
    };
    let Some(values) = values.as_array() else {
        return Err(outbox_evidence_error(
            event,
            &format!("{field} must be an array"),
        ));
    };
    let mut hashes = Vec::with_capacity(values.len());
    for value in values {
        let Some(hash) = value.as_str() else {
            return Err(outbox_evidence_error(
                event,
                &format!("{field} must contain full SHA-256 digest evidence"),
            ));
        };
        if !is_full_sha256_digest_evidence(hash) {
            return Err(outbox_evidence_error(
                event,
                &format!("{field} must contain full SHA-256 digest evidence"),
            ));
        }
        hashes.push(hash);
    }
    Ok(hashes)
}

pub(crate) fn validate_unique_hash_array(
    event: &OutboxEvent,
    hashes: &[&str],
    label: &str,
) -> Result<(), LakeCatError> {
    let mut unique = BTreeSet::new();
    for hash in hashes {
        if !unique.insert(*hash) {
            return Err(outbox_evidence_error(
                event,
                &format!("{label} must not contain duplicate hashes"),
            ));
        }
    }
    Ok(())
}

pub(crate) fn validate_unique_string_array(
    event: &OutboxEvent,
    values: &[String],
    label: &str,
) -> Result<(), LakeCatError> {
    let mut unique = BTreeSet::new();
    for value in values {
        if !unique.insert(value.as_str()) {
            return Err(outbox_evidence_error(
                event,
                &format!("{label} must not contain duplicate values"),
            ));
        }
    }
    Ok(())
}

pub(crate) fn validate_optional_full_hash_field(
    event: &OutboxEvent,
    object: &Value,
    field: &str,
) -> Result<(), LakeCatError> {
    let Some(value) = object.get(field) else {
        return Ok(());
    };
    if value.is_null() {
        return Ok(());
    }
    if value.as_str().is_some_and(is_full_sha256_digest_evidence) {
        return Ok(());
    }
    Err(outbox_evidence_error(
        event,
        &format!("{field} must contain full SHA-256 digest evidence"),
    ))
}

pub(crate) fn optional_string_field(
    event: &OutboxEvent,
    object: &Value,
    field: &str,
    label: &str,
) -> Result<Option<String>, LakeCatError> {
    match object.get(field) {
        Some(Value::String(value)) => Ok(Some(value.clone())),
        Some(Value::Null) | None => Ok(None),
        _ => Err(outbox_evidence_error(
            event,
            &format!("{label} {field} must be a string when present"),
        )),
    }
}

pub(crate) fn optional_non_empty_string_field(
    event: &OutboxEvent,
    object: &Value,
    field: &str,
    label: &str,
) -> Result<Option<String>, LakeCatError> {
    match optional_string_field(event, object, field, label)? {
        Some(value) if value.trim().is_empty() => Err(outbox_evidence_error(
            event,
            &format!("{label} {field} must be non-empty when present"),
        )),
        value => Ok(value),
    }
}

pub(crate) fn require_positive_i64_field(
    event: &OutboxEvent,
    object: &Value,
    field: &str,
    label: &str,
) -> Result<i64, LakeCatError> {
    let alternate_field = field.replace('-', "_");
    let primary_value = object.get(field);
    let alternate_value = object.get(alternate_field.as_str());
    if primary_value.is_some() && alternate_value.is_some() {
        return Err(outbox_evidence_error(
            event,
            &format!("{label} must not carry both {field} and {alternate_field} evidence fields"),
        ));
    }
    let value = primary_value.or(alternate_value);
    let Some(value) = value else {
        return Err(outbox_evidence_error(
            event,
            &format!("{label} evidence must contain positive {field}"),
        ));
    };
    let Some(value) = value.as_i64() else {
        return Err(outbox_evidence_error(
            event,
            &format!("{label} {field} must be a positive integer"),
        ));
    };
    if value <= 0 {
        return Err(outbox_evidence_error(
            event,
            &format!("{label} {field} must be positive"),
        ));
    }
    Ok(value)
}

pub(crate) fn optional_string_map_field(
    event: &OutboxEvent,
    object: &Value,
    field: &str,
    label: &str,
) -> Result<BTreeMap<String, String>, LakeCatError> {
    let Some(value) = object.get(field) else {
        return Ok(BTreeMap::new());
    };
    if value.is_null() {
        return Ok(BTreeMap::new());
    }
    serde_json::from_value(value.clone()).map_err(|_| {
        outbox_evidence_error(
            event,
            &format!("{label} {field} must be a string map when present"),
        )
    })
}

pub(crate) fn outbox_evidence_error(event: &OutboxEvent, message: &str) -> LakeCatError {
    LakeCatError::InvalidArgument(format!(
        "outbox event {} ({}) has invalid {message}; event-id-hash={}",
        event.event_type,
        event.sink,
        content_hash_bytes(event.event_id.as_bytes())
    ))
}

pub(crate) fn is_full_sha256_digest_evidence(value: &str) -> bool {
    let Some(digest) = value.strip_prefix("sha256:") else {
        return false;
    };
    digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit())
}
