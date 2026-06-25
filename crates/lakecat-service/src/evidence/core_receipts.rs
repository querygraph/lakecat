use lakecat_core::{LakeCatError, Principal, content_hash_bytes};
#[cfg(feature = "sail-local")]
use lakecat_sail::catalog_provider::{
    LakeCatCatalogProvider, ProviderFetchScanTasksRequest, ProviderScanPlanningRequest,
};
use lakecat_security::CatalogAction;
use lakecat_store::OutboxEvent;
use serde_json::Value;

use crate::*;

pub(crate) fn validate_read_restriction_evidence_schema(
    event: &OutboxEvent,
    restriction: Option<&Value>,
    evidence_label: &str,
) -> Result<(), LakeCatError> {
    let Some(restriction) = restriction else {
        return Ok(());
    };
    let Some(restriction) = restriction.as_object() else {
        return Err(outbox_evidence_error(
            event,
            &format!("{evidence_label} must be an object"),
        ));
    };
    for field in restriction.keys() {
        if !READ_RESTRICTION_EVIDENCE_FIELDS.contains(&field.as_str()) {
            return Err(outbox_evidence_error(
                event,
                &format!("{evidence_label} contains unexpected field {field}"),
            ));
        }
    }
    if let Some(row_predicate) = restriction.get("row-predicate") {
        validate_row_predicate_evidence_schema(
            event,
            row_predicate,
            &format!("{evidence_label} row-predicate"),
        )?;
    }
    Ok(())
}

pub(crate) fn validate_row_predicate_evidence_schema(
    event: &OutboxEvent,
    row_predicate: &Value,
    evidence_label: &str,
) -> Result<(), LakeCatError> {
    let Some(row_predicate) = row_predicate.as_object() else {
        return Err(outbox_evidence_error(
            event,
            &format!("{evidence_label} must be an object"),
        ));
    };
    for field in row_predicate.keys() {
        if !ROW_PREDICATE_EVIDENCE_FIELDS.contains(&field.as_str()) {
            return Err(outbox_evidence_error(
                event,
                &format!("{evidence_label} contains unexpected field {field}"),
            ));
        }
    }
    Ok(())
}

pub(crate) fn validate_storage_profile_evidence_schema(
    event: &OutboxEvent,
    storage_profile: &Value,
    evidence_label: &str,
) -> Result<(), LakeCatError> {
    let Some(storage_profile) = storage_profile.as_object() else {
        return Err(outbox_evidence_error(
            event,
            &format!("{evidence_label} must be an object"),
        ));
    };
    for field in storage_profile.keys() {
        if !STORAGE_PROFILE_EVIDENCE_FIELDS.contains(&field.as_str()) {
            return Err(outbox_evidence_error(
                event,
                &format!("{evidence_label} contains unexpected field {field}"),
            ));
        }
    }
    Ok(())
}

pub(crate) fn validate_storage_profile_public_config_evidence(
    event: &OutboxEvent,
    storage_profile: &Value,
    evidence_label: &str,
) -> Result<(), LakeCatError> {
    let Some(public_config) = storage_profile.get("public-config") else {
        return Ok(());
    };
    let Some(public_config) = public_config.as_object() else {
        return Err(outbox_evidence_error(
            event,
            &format!("{evidence_label} public-config must be an object"),
        ));
    };
    for (key, value) in public_config {
        let key_hash = content_hash_bytes(key.as_bytes());
        if key.trim().is_empty() {
            return Err(outbox_evidence_error(
                event,
                &format!(
                    "{evidence_label} public-config key must be non-empty; public-config-key-hash={key_hash}"
                ),
            ));
        }
        let normalized = key.to_ascii_lowercase();
        if normalized.contains("secret")
            || normalized.contains("token")
            || normalized.contains("password")
            || normalized.contains("credential")
        {
            return Err(outbox_evidence_error(
                event,
                &format!(
                    "{evidence_label} public-config key may expose secret material; public-config-key-hash={key_hash}"
                ),
            ));
        }
        if RESERVED_STORAGE_PROFILE_PUBLIC_CONFIG_KEYS.contains(&normalized.as_str()) {
            return Err(outbox_evidence_error(
                event,
                &format!(
                    "{evidence_label} public-config key is reserved for LakeCat credential evidence; public-config-key-hash={key_hash}"
                ),
            ));
        }
        let Some(value) = value.as_str() else {
            return Err(outbox_evidence_error(
                event,
                &format!(
                    "{evidence_label} public-config value must be a string; public-config-key-hash={key_hash}"
                ),
            ));
        };
        if embeds_raw_secret_material(value) {
            return Err(outbox_evidence_error(
                event,
                &format!(
                    "{evidence_label} public-config value may expose secret material; public-config-key-hash={key_hash}"
                ),
            ));
        }
    }
    Ok(())
}

pub(crate) fn embeds_raw_secret_material(value: &str) -> bool {
    let normalized = value.to_ascii_lowercase();
    [
        "password=",
        "secret=",
        "token=",
        "credential=",
        "api_key=",
        "apikey=",
        "access_key=",
        "private_key=",
        "pass=",
    ]
    .iter()
    .any(|pattern| normalized.contains(pattern))
}

pub(crate) fn validate_policy_binding_evidence_schema(
    event: &OutboxEvent,
    policy: &Value,
) -> Result<(), LakeCatError> {
    let Some(policy) = policy.as_object() else {
        return Err(outbox_evidence_error(
            event,
            "policy-binding upsert policy must be an object",
        ));
    };
    for field in policy.keys() {
        if !POLICY_BINDING_EVIDENCE_FIELDS.contains(&field.as_str()) {
            return Err(outbox_evidence_error(
                event,
                &format!("policy-binding upsert policy contains unexpected field {field}"),
            ));
        }
    }
    Ok(())
}

pub(crate) fn validate_object_evidence_schema(
    event: &OutboxEvent,
    object: &Value,
    label: &str,
    allowed_fields: &[&str],
) -> Result<(), LakeCatError> {
    let Some(object) = object.as_object() else {
        return Err(outbox_evidence_error(
            event,
            &format!("{label} must be an object"),
        ));
    };
    for field in object.keys() {
        if !allowed_fields.contains(&field.as_str()) {
            return Err(outbox_evidence_error(
                event,
                &format!("{label} contains unexpected field {field}"),
            ));
        }
    }
    Ok(())
}

pub(crate) fn validate_required_read_restriction_policy_hashes(
    event: &OutboxEvent,
    restriction: Option<&Value>,
    evidence_label: &str,
) -> Result<(), LakeCatError> {
    let Some(restriction) = restriction else {
        return Ok(());
    };
    if restriction.get("policy-hashes").is_none() {
        return Err(outbox_evidence_error(
            event,
            &format!("{evidence_label} must contain policy-hashes"),
        ));
    }
    validate_read_restriction_policy_hashes(event, Some(restriction), evidence_label)
}

pub(crate) fn authorization_receipt_read_restriction(payload: &Value) -> Option<&Value> {
    payload
        .get("authorization-receipt")?
        .get("context")?
        .get("read-restriction")
}

pub(crate) fn authorization_receipt_raw_credential_exception(payload: &Value) -> Option<&Value> {
    payload
        .get("authorization-receipt")?
        .get("context")?
        .get("lakecat:raw-credential-exception")
}

pub(crate) fn validate_table_commit_hash_evidence(event: &OutboxEvent) -> Result<(), LakeCatError> {
    let payload = event.payload.get("payload").unwrap_or(&event.payload);
    if event.payload.get("payload").is_some() {
        validate_object_evidence_schema(
            event,
            &event.payload,
            "table commit outbox payload",
            TABLE_COMMIT_OUTBOX_PAYLOAD_FIELDS,
        )?;
    }
    validate_object_evidence_schema(
        event,
        payload,
        "table commit payload",
        TABLE_COMMIT_PAYLOAD_EVIDENCE_FIELDS,
    )?;
    let Some(commit) = payload
        .get("commit")
        .or_else(|| event.payload.get("commit"))
    else {
        return Err(outbox_evidence_error(
            event,
            "table commit evidence must contain commit",
        ));
    };
    validate_object_evidence_schema(event, commit, "table commit", TABLE_COMMIT_EVIDENCE_FIELDS)?;
    let Some(root_table) = event.payload.get("table") else {
        return Err(outbox_evidence_error(
            event,
            "table commit evidence must contain table identity",
        ));
    };
    let root_table = decode_table_lifecycle_identity(event, root_table, "table commit")?;
    validate_table_lifecycle_payload_scope(event, payload, &root_table, "table commit")?;
    if let Some(commit_table) = commit.get("table") {
        validate_table_lifecycle_table_hint(
            event,
            commit_table,
            &root_table,
            "table commit table",
        )?;
    }
    let Some(commit_principal) = commit.get("principal") else {
        return Err(outbox_evidence_error(
            event,
            "table commit evidence must contain commit principal",
        ));
    };
    let commit_principal =
        decode_outbox_principal_value(event, commit_principal, "table commit principal")?;
    let Some(receipt_principal) = payload
        .get("authorization-receipt")
        .and_then(|receipt| receipt.get("principal"))
    else {
        return Err(outbox_evidence_error(
            event,
            "table commit evidence must contain authorization receipt principal",
        ));
    };
    let receipt_principal = decode_outbox_principal_value(
        event,
        receipt_principal,
        "table commit authorization receipt principal",
    )?;
    validate_authorization_receipt_action(event, payload, "table commit")?;
    validate_authorization_receipt_allowed(event, payload, "table commit")?;
    validate_authorization_receipt_engine(event, payload, "table commit")?;
    validate_authorization_receipt_checked_at(event, payload, "table commit")?;
    if commit_principal != receipt_principal {
        return Err(outbox_evidence_error(
            event,
            "table commit principal does not match authorization receipt principal",
        ));
    }
    let Some(sequence_number) = aliased_evidence_field(
        event,
        commit,
        "sequence_number",
        "sequence-number",
        "table commit",
    )?
    .and_then(Value::as_u64) else {
        return Err(outbox_evidence_error(
            event,
            "table commit evidence must contain unsigned sequence number",
        ));
    };
    if sequence_number == 0 {
        return Err(outbox_evidence_error(
            event,
            "table commit evidence sequence number must be positive",
        ));
    }
    let Some(format_version) = aliased_evidence_field(
        event,
        commit,
        "format_version",
        "format-version",
        "table commit",
    )?
    .and_then(Value::as_u64) else {
        return Err(outbox_evidence_error(
            event,
            "table commit evidence must contain unsigned format version",
        ));
    };
    if format_version == 0 {
        return Err(outbox_evidence_error(
            event,
            "table commit evidence format version must be positive",
        ));
    }
    let Some(snapshot_id) =
        aliased_evidence_field(event, commit, "snapshot_id", "snapshot-id", "table commit")?
            .and_then(Value::as_i64)
    else {
        return Err(outbox_evidence_error(
            event,
            "table commit evidence must contain signed snapshot id",
        ));
    };
    if snapshot_id < 0 {
        return Err(outbox_evidence_error(
            event,
            "table commit evidence snapshot id must be non-negative",
        ));
    }
    validate_required_aliased_rfc3339_string(
        event,
        commit,
        "committed_at",
        "committed-at",
        "table commit evidence committed_at timestamp",
    )?;
    if !aliased_evidence_field(
        event,
        commit,
        "new_metadata_location",
        "new-metadata-location",
        "table commit",
    )?
    .and_then(Value::as_str)
    .is_some_and(|location| !location.trim().is_empty())
    {
        return Err(outbox_evidence_error(
            event,
            "table commit evidence must contain non-empty new metadata location",
        ));
    }
    validate_optional_location_evidence(
        event,
        aliased_evidence_field(
            event,
            commit,
            "new_metadata_location",
            "new-metadata-location",
            "table commit",
        )?,
        "table commit new metadata location",
    )?;
    if aliased_evidence_field(
        event,
        commit,
        "previous_metadata_location",
        "previous-metadata-location",
        "table commit",
    )?
    .is_some_and(|location| {
        !location
            .as_str()
            .is_some_and(|location| !location.trim().is_empty())
    }) {
        return Err(outbox_evidence_error(
            event,
            "table commit evidence previous metadata location must be non-empty when present",
        ));
    }
    validate_optional_location_evidence(
        event,
        aliased_evidence_field(
            event,
            commit,
            "previous_metadata_location",
            "previous-metadata-location",
            "table commit",
        )?,
        "table commit previous metadata location",
    )?;
    validate_required_aliased_full_hash_field(event, commit, "request_hash", "request-hash")?;
    validate_required_aliased_full_hash_field(event, commit, "response_hash", "response-hash")?;
    validate_optional_aliased_full_hash_field(
        event,
        commit,
        "idempotency_key_sha256",
        "idempotency-key-sha256",
    )?;
    validate_optional_aliased_full_hash_field(event, commit, "policy_hash", "policy-hash")?;
    Ok(())
}

pub(crate) fn aliased_evidence_field<'a>(
    event: &OutboxEvent,
    object: &'a Value,
    snake_case: &str,
    kebab_case: &str,
    label: &str,
) -> Result<Option<&'a Value>, LakeCatError> {
    let snake_value = object.get(snake_case);
    let kebab_value = object.get(kebab_case);
    match (snake_value, kebab_value) {
        (Some(_), Some(_)) => Err(outbox_evidence_error(
            event,
            &format!("{label} must not carry both {snake_case} and {kebab_case} evidence fields"),
        )),
        (Some(value), None) | (None, Some(value)) => Ok(Some(value)),
        (None, None) => Ok(None),
    }
}

pub(crate) fn validate_required_aliased_full_hash_field(
    event: &OutboxEvent,
    object: &Value,
    snake_case: &str,
    kebab_case: &str,
) -> Result<(), LakeCatError> {
    if aliased_evidence_field(event, object, snake_case, kebab_case, "table commit")?
        .and_then(Value::as_str)
        .is_some_and(is_full_sha256_digest_evidence)
    {
        return Ok(());
    }
    Err(outbox_evidence_error(
        event,
        &format!("{snake_case}/{kebab_case} must contain full SHA-256 digest evidence"),
    ))
}

pub(crate) fn validate_optional_aliased_full_hash_field(
    event: &OutboxEvent,
    object: &Value,
    snake_case: &str,
    kebab_case: &str,
) -> Result<(), LakeCatError> {
    let Some(value) =
        aliased_evidence_field(event, object, snake_case, kebab_case, "table commit")?
    else {
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
        &format!("{snake_case}/{kebab_case} must contain full SHA-256 digest evidence"),
    ))
}

pub(crate) fn validate_required_aliased_rfc3339_string(
    event: &OutboxEvent,
    object: &Value,
    snake_case: &str,
    kebab_case: &str,
    label: &str,
) -> Result<(), LakeCatError> {
    let Some(timestamp) =
        aliased_evidence_field(event, object, snake_case, kebab_case, "table commit")?
            .and_then(Value::as_str)
    else {
        return Err(outbox_evidence_error(
            event,
            &format!("{label} must be present"),
        ));
    };
    if timestamp.trim().is_empty() {
        return Err(outbox_evidence_error(
            event,
            &format!("{label} must be non-empty"),
        ));
    }
    chrono::DateTime::parse_from_rfc3339(timestamp)
        .map_err(|err| outbox_evidence_error(event, &format!("{label} must be RFC3339: {err}")))?;
    Ok(())
}

pub(crate) fn decode_outbox_principal_value(
    event: &OutboxEvent,
    principal: &Value,
    label: &str,
) -> Result<Principal, LakeCatError> {
    if principal.is_object() {
        validate_object_evidence_schema(event, principal, label, PRINCIPAL_EVIDENCE_FIELDS)?;
    }
    serde_json::from_value(principal.clone()).map_err(|err| {
        outbox_evidence_error(event, &format!("{label} must be a valid principal: {err}"))
    })
}

pub(crate) fn validate_authorization_receipt_evidence_schema(
    event: &OutboxEvent,
    payload: &Value,
    label: &str,
) -> Result<(), LakeCatError> {
    if let Some(receipt) = payload
        .get("authorization-receipt")
        .filter(|receipt| receipt.is_object())
    {
        validate_object_evidence_schema(
            event,
            receipt,
            &format!("{label} authorization receipt"),
            AUTHORIZATION_RECEIPT_EVIDENCE_FIELDS,
        )?;
        if let Some(context) = receipt.get("context") {
            validate_object_evidence_schema(
                event,
                context,
                &format!("{label} authorization receipt context"),
                AUTHORIZATION_RECEIPT_CONTEXT_EVIDENCE_FIELDS,
            )?;
            validate_authorization_receipt_context_policy_bindings(
                event,
                context,
                &format!("{label} authorization receipt context policy-bindings"),
            )?;
        }
    }
    Ok(())
}

pub(crate) fn validate_authorization_receipt_context_policy_bindings(
    event: &OutboxEvent,
    context: &Value,
    label: &str,
) -> Result<(), LakeCatError> {
    let Some(policy_bindings) = context.get("policy-bindings") else {
        return Ok(());
    };
    let Some(policy_bindings) = policy_bindings.as_array() else {
        return Err(outbox_evidence_error(
            event,
            &format!("{label} must be an array"),
        ));
    };
    for policy_binding in policy_bindings {
        validate_object_evidence_schema(
            event,
            policy_binding,
            label,
            AUTHORIZATION_RECEIPT_CONTEXT_POLICY_BINDING_FIELDS,
        )?;
    }
    Ok(())
}

pub(crate) fn validate_authorization_receipt_principal(
    event: &OutboxEvent,
    payload: &Value,
    label: &str,
) -> Result<(), LakeCatError> {
    validate_authorization_receipt_evidence_schema(event, payload, label)?;
    let Some(receipt_principal) = payload
        .get("authorization-receipt")
        .and_then(|receipt| receipt.get("principal"))
    else {
        return Err(outbox_evidence_error(
            event,
            &format!("{label} evidence must contain authorization receipt principal"),
        ));
    };
    decode_outbox_principal_value(
        event,
        receipt_principal,
        &format!("{label} authorization receipt principal"),
    )?;
    validate_authorization_receipt_action(event, payload, label)?;
    validate_authorization_receipt_allowed(event, payload, label)?;
    validate_authorization_receipt_engine(event, payload, label)?;
    validate_authorization_receipt_checked_at(event, payload, label)?;
    Ok(())
}

pub(crate) fn validate_authorization_receipt_action(
    event: &OutboxEvent,
    payload: &Value,
    label: &str,
) -> Result<(), LakeCatError> {
    let Some(action) = payload
        .get("authorization-receipt")
        .and_then(|receipt| receipt.get("action"))
        .and_then(Value::as_str)
    else {
        return Err(outbox_evidence_error(
            event,
            &format!("{label} evidence must contain authorization receipt action"),
        ));
    };
    if action.trim().is_empty() {
        return Err(outbox_evidence_error(
            event,
            &format!("{label} authorization receipt action must be non-empty"),
        ));
    }
    let action = serde_json::from_value::<CatalogAction>(Value::String(action.to_string()))
        .map_err(|err| {
            outbox_evidence_error(
                event,
                &format!(
                    "{label} authorization receipt action must be a known catalog action: {err}"
                ),
            )
        })?;
    if !authorization_receipt_action_matches_event(event.event_type.as_str(), &action) {
        return Err(outbox_evidence_error(
            event,
            &format!("{label} authorization receipt action does not match outbox event type"),
        ));
    }
    Ok(())
}

pub(crate) fn authorization_receipt_action_matches_event(
    event_type: &str,
    action: &CatalogAction,
) -> bool {
    match event_type {
        "catalog.config-read" => matches!(action, CatalogAction::CatalogConfig),
        "credentials.vend-attempted" => matches!(action, CatalogAction::CredentialsVend),
        "namespace.created" => matches!(action, CatalogAction::NamespaceCreate),
        "namespace.dropped" => matches!(action, CatalogAction::NamespaceDrop),
        "namespace.listed" => matches!(action, CatalogAction::NamespaceList),
        "namespace.loaded" => matches!(action, CatalogAction::NamespaceLoad),
        "policy-binding.listed" | "policy-binding.upserted" => {
            matches!(action, CatalogAction::PolicyManage)
        }
        "project.listed" | "project.upserted" => matches!(action, CatalogAction::ProjectManage),
        "querygraph.bootstrap" => matches!(action, CatalogAction::GraphRead),
        "server.listed" | "server.upserted" => matches!(action, CatalogAction::ServerManage),
        "storage-profile.listed" | "storage-profile.upserted" => {
            matches!(action, CatalogAction::StorageProfileManage)
        }
        "table.commit" => matches!(action, CatalogAction::TableCommit),
        "table.commits-listed" | "table.loaded" => matches!(action, CatalogAction::TableLoad),
        "table.created" => matches!(action, CatalogAction::TableCreate),
        "table.deleted" => matches!(action, CatalogAction::TableDrop),
        "table.restored" => matches!(action, CatalogAction::TableRestore),
        "table.scan-planned" | "table.scan-tasks-fetched" => {
            matches!(action, CatalogAction::TablePlanScan)
        }
        "view.dropped" => matches!(action, CatalogAction::ViewDrop),
        "view.listed" => matches!(action, CatalogAction::ViewLoad),
        "view.loaded" | "view.version-receipts-listed" | "view.version-receipt-chains-listed" => {
            matches!(action, CatalogAction::ViewLoad)
        }
        "view.upserted" => matches!(action, CatalogAction::ViewManage),
        "warehouse.listed" | "warehouse.upserted" => {
            matches!(action, CatalogAction::WarehouseManage)
        }
        _ => false,
    }
}

pub(crate) fn validate_authorization_receipt_allowed(
    event: &OutboxEvent,
    payload: &Value,
    label: &str,
) -> Result<(), LakeCatError> {
    let Some(allowed) = payload
        .get("authorization-receipt")
        .and_then(|receipt| receipt.get("allowed"))
        .and_then(Value::as_bool)
    else {
        return Err(outbox_evidence_error(
            event,
            &format!("{label} evidence must contain authorization receipt allowed decision"),
        ));
    };
    if !allowed {
        return Err(outbox_evidence_error(
            event,
            &format!("{label} authorization receipt must allow replay projection"),
        ));
    }
    Ok(())
}

pub(crate) fn validate_authorization_receipt_engine(
    event: &OutboxEvent,
    payload: &Value,
    label: &str,
) -> Result<(), LakeCatError> {
    let Some(engine) = payload
        .get("authorization-receipt")
        .and_then(|receipt| receipt.get("engine"))
        .and_then(Value::as_str)
    else {
        return Err(outbox_evidence_error(
            event,
            &format!("{label} evidence must contain authorization receipt engine"),
        ));
    };
    if engine.trim().is_empty() {
        return Err(outbox_evidence_error(
            event,
            &format!("{label} authorization receipt engine must be non-empty"),
        ));
    }
    Ok(())
}

pub(crate) fn validate_authorization_receipt_checked_at(
    event: &OutboxEvent,
    payload: &Value,
    label: &str,
) -> Result<(), LakeCatError> {
    let Some(checked_at) = payload
        .get("authorization-receipt")
        .and_then(|receipt| receipt.get("checked_at"))
        .and_then(Value::as_str)
    else {
        return Err(outbox_evidence_error(
            event,
            &format!("{label} evidence must contain authorization receipt checked_at timestamp"),
        ));
    };
    if checked_at.trim().is_empty() {
        return Err(outbox_evidence_error(
            event,
            &format!("{label} authorization receipt checked_at timestamp must be non-empty"),
        ));
    }
    chrono::DateTime::parse_from_rfc3339(checked_at).map_err(|err| {
        outbox_evidence_error(
            event,
            &format!("{label} authorization receipt checked_at timestamp must be RFC3339: {err}"),
        )
    })?;
    Ok(())
}
