use std::collections::BTreeSet;

use lakecat_core::{LakeCatError, Namespace, TableName, WarehouseName, content_hash_json};
#[cfg(feature = "sail-local")]
use lakecat_sail::catalog_provider::{
    LakeCatCatalogProvider, ProviderFetchScanTasksRequest, ProviderScanPlanningRequest,
};
use lakecat_store::{CredentialIssuanceMode, OutboxEvent, StorageProvider};
use serde_json::{Value, json};

use crate::*;

pub(crate) fn validate_credential_vend_event_evidence(
    event: &OutboxEvent,
    payload: &Value,
) -> Result<(), LakeCatError> {
    if event.payload.get("payload").is_some() {
        validate_object_evidence_schema(
            event,
            &event.payload,
            "credential-vend outbox payload",
            CREDENTIAL_VEND_OUTBOX_PAYLOAD_FIELDS,
        )?;
    }
    validate_object_evidence_schema(
        event,
        payload,
        "credential-vend",
        CREDENTIAL_VEND_EVIDENCE_FIELDS,
    )?;
    validate_read_restriction_evidence_schema(
        event,
        payload.get("read-restriction"),
        "credential-vend read-restriction",
    )?;
    validate_read_restriction_evidence_schema(
        event,
        authorization_receipt_read_restriction(payload),
        "credential-vend authorization receipt read-restriction",
    )?;
    validate_raw_credential_exception_evidence_schema(
        event,
        payload.get("lakecat:raw-credential-exception"),
        "credential-vend raw-credential exception",
    )?;
    validate_raw_credential_exception_evidence_schema(
        event,
        authorization_receipt_raw_credential_exception(payload),
        "credential-vend authorization receipt raw-credential exception",
    )?;
    validate_read_restriction_receipt_match(event, payload, "credential-vend")?;
    validate_read_restriction_purpose_and_ttl(event, payload, "credential-vend")?;
    validate_raw_credential_exception_receipt_match(event, payload)?;
    validate_authorization_receipt_principal(event, payload, "credential-vend")?;
    let table = validate_required_outbox_table_identity(event, "credential-vend")?;
    if let Some(payload_table) = payload.get("table") {
        validate_table_lifecycle_table_hint(
            event,
            payload_table,
            &table,
            "credential-vend payload table",
        )?;
    }

    let Some(credential_count) = payload.get("credential-count").and_then(Value::as_u64) else {
        return Err(outbox_evidence_error(
            event,
            "credential-vend evidence must contain credential-count",
        ));
    };
    let evidence = payload
        .get("credential-response-evidence")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            outbox_evidence_error(
                event,
                "credential-vend evidence must contain credential-response-evidence",
            )
        })?;
    if evidence.len() as u64 != credential_count {
        return Err(outbox_evidence_error(
            event,
            "credential-vend credential-count does not match credential-response-evidence",
        ));
    }
    validate_credential_block_reason_evidence(event, payload, credential_count)?;
    let Some(storage_profile) = payload.get("storage-profile") else {
        return Err(outbox_evidence_error(
            event,
            "credential-vend evidence must contain storage-profile",
        ));
    };
    validate_storage_profile_evidence_schema(
        event,
        storage_profile,
        "credential-vend storage-profile",
    )?;
    validate_storage_profile_public_config_evidence(
        event,
        storage_profile,
        "credential-vend storage-profile",
    )?;
    validate_required_full_hash_field(event, storage_profile, "location-prefix-hash")?;
    validate_secret_ref_evidence(event, storage_profile, "credential-vend storage-profile")?;
    validate_storage_profile_provider_mode_evidence(
        event,
        storage_profile,
        "credential-vend storage-profile",
    )?;
    validate_optional_location_evidence(
        event,
        payload.get("storage-location"),
        "credential-vend storage-location",
    )?;
    let profile_id = required_string_field(
        event,
        storage_profile,
        "profile-id",
        "credential-vend storage-profile",
    )?;
    validate_storage_profile_id_evidence(event, profile_id, "credential-vend storage-profile")?;
    validate_string_field_equals(
        event,
        storage_profile,
        "warehouse",
        table.warehouse.as_str(),
        "credential-vend storage-profile",
    )?;
    validate_string_field_equals(
        event,
        payload,
        "storage-profile-id",
        profile_id,
        "credential-vend evidence",
    )?;
    if payload.get("mode").is_some() {
        let issuance_mode = required_string_field(
            event,
            storage_profile,
            "issuance-mode",
            "credential-vend storage-profile",
        )?;
        validate_string_field_equals(
            event,
            payload,
            "mode",
            issuance_mode,
            "credential-vend evidence",
        )?;
    }
    let secret_ref_present = storage_profile
        .get("secret-ref-present")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    let mut response_prefix_hashes = BTreeSet::new();
    for entry in evidence {
        let prefix_hash =
            validate_credential_response_entry_evidence(event, payload, storage_profile, entry)?;
        if !response_prefix_hashes.insert(prefix_hash) {
            return Err(outbox_evidence_error(
                event,
                "credential-vend credential-response-evidence must not contain duplicate prefix-hash values",
            ));
        }
    }
    validate_required_bool_field_equals(
        event,
        payload,
        "secret-ref-present",
        secret_ref_present,
        "credential-vend evidence",
    )?;
    Ok(())
}

pub(crate) fn validate_credential_block_reason_evidence(
    event: &OutboxEvent,
    payload: &Value,
    credential_count: u64,
) -> Result<(), LakeCatError> {
    let block_reason = payload.get("lakecat:credential-block-reason");
    if let Some(raw_exception) = payload.get("lakecat:raw-credential-exception") {
        let Some(raw_exception_allowed) = raw_exception.get("allowed").and_then(Value::as_bool)
        else {
            return Err(outbox_evidence_error(
                event,
                "credential-vend raw-credential exception allowed must be boolean",
            ));
        };
        if !raw_exception_allowed {
            if credential_count != 0 {
                return Err(outbox_evidence_error(
                    event,
                    "credential-vend blocked credential evidence must not carry credentials",
                ));
            }
            let Some(expected_reason) = raw_exception
                .get("reason")
                .and_then(Value::as_str)
                .filter(|reason| !reason.trim().is_empty())
            else {
                return Err(outbox_evidence_error(
                    event,
                    "credential-vend blocked raw-credential exception must carry non-empty reason",
                ));
            };
            let Some(actual_reason) = block_reason
                .and_then(Value::as_str)
                .filter(|reason| !reason.trim().is_empty())
            else {
                return Err(outbox_evidence_error(
                    event,
                    "credential-vend blocked credential evidence must contain block reason",
                ));
            };
            if actual_reason != expected_reason {
                return Err(outbox_evidence_error(
                    event,
                    "credential-vend block reason must match raw-credential exception reason",
                ));
            }
            return Ok(());
        }
        if !matches!(block_reason, None | Some(Value::Null)) {
            return Err(outbox_evidence_error(
                event,
                "credential-vend block reason must be absent when raw credentials are allowed",
            ));
        }
        return Ok(());
    }
    if let Some(reason) = block_reason {
        if credential_count != 0 {
            return Err(outbox_evidence_error(
                event,
                "credential-vend blocked credential evidence must not carry credentials",
            ));
        }
        if !reason
            .as_str()
            .is_some_and(|reason| !reason.trim().is_empty())
        {
            return Err(outbox_evidence_error(
                event,
                "credential-vend blocked credential evidence must contain block reason",
            ));
        }
    }
    Ok(())
}

pub(crate) fn validate_credential_response_entry_evidence(
    event: &OutboxEvent,
    payload: &Value,
    storage_profile: &Value,
    entry: &Value,
) -> Result<String, LakeCatError> {
    validate_credential_response_entry_schema(event, entry)?;
    validate_required_full_hash_field(event, entry, "prefix-hash")?;
    validate_required_full_hash_field(event, entry, "issuer-config-hash")?;
    let prefix_hash = required_string_field(
        event,
        entry,
        "prefix-hash",
        "credential-vend credential-response",
    )?;

    let profile_id = required_string_field(
        event,
        storage_profile,
        "profile-id",
        "credential-vend storage-profile",
    )?;
    let provider = required_string_field(
        event,
        storage_profile,
        "provider",
        "credential-vend storage-profile",
    )?;
    let issuance_mode = required_string_field(
        event,
        storage_profile,
        "issuance-mode",
        "credential-vend storage-profile",
    )?;
    let receipt_principal = required_pointer_string(
        event,
        payload,
        "/authorization-receipt/principal/subject",
        "credential-vend authorization receipt principal subject",
    )?;

    validate_string_field_equals(
        event,
        entry,
        "storage-profile-id",
        profile_id,
        "credential-vend credential-response",
    )?;
    validate_string_field_equals(
        event,
        entry,
        "catalog-profile-id",
        profile_id,
        "credential-vend credential-response",
    )?;
    validate_string_field_equals(
        event,
        entry,
        "storage-provider",
        provider,
        "credential-vend credential-response",
    )?;
    validate_string_field_equals(
        event,
        entry,
        "credential-mode",
        issuance_mode,
        "credential-vend credential-response",
    )?;
    validate_string_field_equals(
        event,
        entry,
        "authorization-principal",
        receipt_principal,
        "credential-vend credential-response",
    )?;
    validate_string_field_equals(
        event,
        entry,
        "receipt-principal",
        receipt_principal,
        "credential-vend credential-response",
    )?;
    validate_string_field_equals(
        event,
        entry,
        "governed-read-required",
        if payload.get("read-restriction").is_some() {
            "true"
        } else {
            "false"
        },
        "credential-vend credential-response",
    )?;
    let secret_ref_present = storage_profile
        .get("secret-ref-present")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if secret_ref_present {
        let secret_ref_provider = required_string_field(
            event,
            storage_profile,
            "secret-ref-provider",
            "credential-vend storage-profile",
        )?;
        let secret_ref_hash = required_string_field(
            event,
            storage_profile,
            "secret-ref-hash",
            "credential-vend storage-profile",
        )?;
        validate_string_field_equals(
            event,
            entry,
            "secret-ref-provider",
            secret_ref_provider,
            "credential-vend credential-response",
        )?;
        validate_string_field_equals(
            event,
            entry,
            "secret-ref-hash",
            secret_ref_hash,
            "credential-vend credential-response",
        )?;
    } else {
        validate_null_or_absent_field(
            event,
            entry,
            "secret-ref-provider",
            "credential-vend credential-response",
        )?;
        validate_null_or_absent_field(
            event,
            entry,
            "secret-ref-hash",
            "credential-vend credential-response",
        )?;
    }

    let expected_ttl = payload
        .get("read-restriction")
        .and_then(|restriction| restriction.get("max-credential-ttl-seconds"))
        .and_then(Value::as_u64)
        .map(|ttl| ttl.to_string());
    match expected_ttl {
        Some(ttl) => validate_string_field_equals(
            event,
            entry,
            "max-credential-ttl-seconds",
            &ttl,
            "credential-vend credential-response",
        )?,
        None => validate_null_or_absent_field(
            event,
            entry,
            "max-credential-ttl-seconds",
            "credential-vend credential-response",
        )?,
    }

    let Some(issuer_config_entry_count) = entry
        .get("issuer-config-entry-count")
        .and_then(Value::as_u64)
    else {
        return Err(outbox_evidence_error(
            event,
            "credential-vend credential-response issuer-config-entry-count must be unsigned",
        ));
    };
    if issuer_config_entry_count == 0 {
        let expected_empty_hash = content_hash_json(&json!([]))?;
        validate_string_field_equals(
            event,
            entry,
            "issuer-config-hash",
            &expected_empty_hash,
            "credential-vend credential-response",
        )?;
    }

    Ok(prefix_hash.to_string())
}

pub(crate) fn validate_credential_response_entry_schema(
    event: &OutboxEvent,
    entry: &Value,
) -> Result<(), LakeCatError> {
    let Some(entry) = entry.as_object() else {
        return Err(outbox_evidence_error(
            event,
            "credential-vend credential-response must be an object",
        ));
    };
    for field in entry.keys() {
        if !CREDENTIAL_RESPONSE_EVIDENCE_FIELDS.contains(&field.as_str()) {
            return Err(outbox_evidence_error(
                event,
                &format!("credential-vend credential-response contains unexpected field {field}"),
            ));
        }
    }
    Ok(())
}

pub(crate) fn validate_raw_credential_exception_evidence_schema(
    event: &OutboxEvent,
    raw_exception: Option<&Value>,
    evidence_label: &str,
) -> Result<(), LakeCatError> {
    let Some(raw_exception) = raw_exception else {
        return Ok(());
    };
    validate_object_evidence_schema(
        event,
        raw_exception,
        evidence_label,
        RAW_CREDENTIAL_EXCEPTION_EVIDENCE_FIELDS,
    )
}

pub(crate) fn validate_raw_credential_exception_receipt_match(
    event: &OutboxEvent,
    payload: &Value,
) -> Result<(), LakeCatError> {
    match (
        payload.get("lakecat:raw-credential-exception"),
        authorization_receipt_raw_credential_exception(payload),
    ) {
        (None, None) => Ok(()),
        (Some(exception), Some(receipt_exception)) if exception == receipt_exception => Ok(()),
        (Some(_), None) => Err(outbox_evidence_error(
            event,
            "credential-vend raw-credential exception must be captured in authorization receipt context",
        )),
        (None, Some(_)) => Err(outbox_evidence_error(
            event,
            "credential-vend authorization receipt raw-credential exception must match top-level evidence",
        )),
        (Some(_), Some(_)) => Err(outbox_evidence_error(
            event,
            "credential-vend raw-credential exception must match authorization receipt context",
        )),
    }
}

pub(crate) fn validate_secret_ref_evidence(
    event: &OutboxEvent,
    object: &Value,
    label: &str,
) -> Result<(), LakeCatError> {
    let secret_ref_present = object
        .get("secret-ref-present")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let secret_ref_provider_value = object.get("secret-ref-provider");
    let secret_ref_provider = secret_ref_provider_value
        .and_then(Value::as_str)
        .filter(|provider| !provider.trim().is_empty());
    let secret_ref_hash_value = object.get("secret-ref-hash");
    let secret_ref_hash = secret_ref_hash_value.and_then(Value::as_str);

    if secret_ref_present {
        if secret_ref_provider.is_none() {
            return Err(outbox_evidence_error(
                event,
                &format!("{label} secret-ref-present requires secret-ref-provider"),
            ));
        }
        if !secret_ref_hash.is_some_and(is_full_sha256_digest_evidence) {
            return Err(outbox_evidence_error(
                event,
                &format!("{label} secret-ref-hash must contain full SHA-256 digest evidence"),
            ));
        }
    } else if secret_ref_provider_value.is_some() || secret_ref_hash_value.is_some() {
        return Err(outbox_evidence_error(
            event,
            &format!("{label} cannot carry secret-ref evidence when secret-ref-present is false"),
        ));
    }
    Ok(())
}

pub(crate) fn validate_storage_profile_provider_mode_evidence(
    event: &OutboxEvent,
    storage_profile: &Value,
    label: &str,
) -> Result<(), LakeCatError> {
    let provider = required_string_field(event, storage_profile, "provider", label)?
        .parse::<StorageProvider>()
        .map_err(|_| {
            outbox_evidence_error(event, &format!("{label} evidence has invalid provider"))
        })?;
    let issuance_mode = required_string_field(event, storage_profile, "issuance-mode", label)?
        .parse::<CredentialIssuanceMode>()
        .map_err(|_| {
            outbox_evidence_error(
                event,
                &format!("{label} evidence has invalid issuance-mode"),
            )
        })?;
    let secret_ref_present = storage_profile
        .get("secret-ref-present")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if matches!(issuance_mode, CredentialIssuanceMode::ShortLivedSecretRef) != secret_ref_present {
        return Err(outbox_evidence_error(
            event,
            &format!("{label} secret-ref-present must match issuance-mode"),
        ));
    }
    if matches!(issuance_mode, CredentialIssuanceMode::LocalFileNoSecret)
        && provider != StorageProvider::File
    {
        return Err(outbox_evidence_error(
            event,
            &format!("{label} local-file-no-secret issuance mode requires file provider"),
        ));
    }
    if matches!(issuance_mode, CredentialIssuanceMode::ShortLivedSecretRef)
        && !matches!(
            provider,
            StorageProvider::S3 | StorageProvider::Gcs | StorageProvider::Azure
        )
    {
        return Err(outbox_evidence_error(
            event,
            &format!("{label} short-lived-secret-ref issuance mode requires cloud object provider"),
        ));
    }
    Ok(())
}

pub(crate) fn validate_view_receipt_list_event_evidence(
    event: &OutboxEvent,
    payload: &Value,
) -> Result<(), LakeCatError> {
    if event.payload.get("payload").is_some() {
        validate_object_evidence_schema(
            event,
            &event.payload,
            "view receipt-read outbox payload",
            VIEW_RECEIPT_READ_OUTBOX_PAYLOAD_FIELDS,
        )?;
    }
    validate_object_evidence_schema(
        event,
        payload,
        "view receipt-list",
        VIEW_RECEIPT_LIST_EVIDENCE_FIELDS,
    )?;
    validate_required_warehouse_field(event, payload, "view receipt-list")?;
    let Some(namespace) = payload.get("namespace") else {
        return Err(outbox_evidence_error(
            event,
            "view receipt-list evidence must contain namespace",
        ));
    };
    validate_namespace_value(event, namespace, "view receipt-list")?;
    let Some(view) = payload
        .get("view")
        .and_then(Value::as_str)
        .filter(|view| !view.trim().is_empty())
    else {
        return Err(outbox_evidence_error(
            event,
            "view receipt-list evidence must contain view",
        ));
    };
    TableName::new(view)
        .map_err(|_| outbox_evidence_error(event, "view receipt-list evidence has invalid view"))?;
    validate_authorization_receipt_principal(event, payload, "view receipt-list")?;

    let receipt_hashes = validate_required_full_hash_array_field(event, payload, "receipt-hashes")?;
    let drop_receipt_hashes =
        validate_required_full_hash_array_field(event, payload, "drop-receipt-hashes")?;
    validate_unique_hash_array(event, &receipt_hashes, "view receipt-list receipt-hashes")?;
    validate_unique_hash_array(
        event,
        &drop_receipt_hashes,
        "view receipt-list drop-receipt-hashes",
    )?;
    let Some(expected_receipt_count) = payload.get("receipt-count").and_then(Value::as_u64) else {
        return Err(outbox_evidence_error(
            event,
            "view receipt-list evidence must contain receipt-count",
        ));
    };
    if receipt_hashes.len() as u64 != expected_receipt_count {
        return Err(outbox_evidence_error(
            event,
            "view receipt-list receipt-count does not match receipt-hashes",
        ));
    }

    let receipt_hashes = receipt_hashes.into_iter().collect::<BTreeSet<_>>();
    for drop_receipt_hash in drop_receipt_hashes {
        if !receipt_hashes.contains(drop_receipt_hash) {
            return Err(outbox_evidence_error(
                event,
                "view receipt-list drop-receipt-hashes must be included in receipt-hashes",
            ));
        }
    }
    Ok(())
}

pub(crate) fn validate_view_receipt_chain_event_evidence(
    event: &OutboxEvent,
    payload: &Value,
) -> Result<(), LakeCatError> {
    if event.payload.get("payload").is_some() {
        validate_object_evidence_schema(
            event,
            &event.payload,
            "view receipt-read outbox payload",
            VIEW_RECEIPT_READ_OUTBOX_PAYLOAD_FIELDS,
        )?;
    }
    validate_object_evidence_schema(
        event,
        payload,
        "view receipt-chain",
        VIEW_RECEIPT_CHAIN_LIST_EVIDENCE_FIELDS,
    )?;
    let warehouse = validate_required_warehouse_field(event, payload, "view receipt-chain")?;
    let Some(namespace) = payload.get("namespace") else {
        return Err(outbox_evidence_error(
            event,
            "view receipt-chain evidence must contain namespace",
        ));
    };
    let namespace = decode_namespace_value(event, namespace, "view receipt-chain")?;
    validate_authorization_receipt_principal(event, payload, "view receipt-chain")?;

    let Some(chains) = payload
        .get("view-version-receipt-chains")
        .and_then(Value::as_array)
    else {
        return Err(outbox_evidence_error(
            event,
            "view receipt-chain evidence must contain view-version-receipt-chains",
        ));
    };
    let Some(expected_chain_count) = payload.get("chain-count").and_then(Value::as_u64) else {
        return Err(outbox_evidence_error(
            event,
            "view receipt-chain evidence must contain chain-count",
        ));
    };
    if chains.len() as u64 != expected_chain_count {
        return Err(outbox_evidence_error(
            event,
            "view receipt-chain chain-count does not match chains",
        ));
    }
    let Some(expected_receipt_count) = payload.get("receipt-count").and_then(Value::as_u64) else {
        return Err(outbox_evidence_error(
            event,
            "view receipt-chain evidence must contain receipt-count",
        ));
    };
    let Some(expected_tombstone_count) = payload.get("tombstone-count").and_then(Value::as_u64)
    else {
        return Err(outbox_evidence_error(
            event,
            "view receipt-chain evidence must contain tombstone-count",
        ));
    };
    let Some(expected_verified_count) = payload.get("chain-verified-count").and_then(Value::as_u64)
    else {
        return Err(outbox_evidence_error(
            event,
            "view receipt-chain evidence must contain chain-verified-count",
        ));
    };

    let mut verified_count = 0u64;
    let mut receipt_count = 0u64;
    let mut tombstone_count = 0u64;
    let mut structural_chain_hashes = BTreeSet::new();
    let mut structural_receipt_hashes = BTreeSet::new();
    let mut structural_drop_receipt_hashes = BTreeSet::new();
    for chain in chains {
        validate_object_evidence_schema(
            event,
            chain,
            "view receipt-chain chain",
            VIEW_RECEIPT_CHAIN_EVIDENCE_FIELDS,
        )?;
        validate_view_receipt_chain_scope_evidence(
            event,
            chain,
            &warehouse,
            &namespace,
            "view receipt-chain chain",
        )?;
        let chain_stable_id = validate_view_receipt_chain_identity_evidence(
            event,
            chain,
            &warehouse,
            &namespace,
            "view receipt-chain chain",
        )?;
        if let Some(receipts) = chain.get("receipts") {
            let Some(receipts) = receipts.as_array() else {
                return Err(outbox_evidence_error(
                    event,
                    "view receipt-chain receipts must be an array when present",
                ));
            };
            for receipt in receipts {
                validate_object_evidence_schema(
                    event,
                    receipt,
                    "view receipt-chain receipt",
                    VIEW_RECEIPT_CHAIN_RECEIPT_EVIDENCE_FIELDS,
                )?;
                validate_view_receipt_chain_scope_evidence(
                    event,
                    receipt,
                    &warehouse,
                    &namespace,
                    "view receipt-chain receipt",
                )?;
                let receipt_stable_id = validate_view_receipt_chain_identity_evidence(
                    event,
                    receipt,
                    &warehouse,
                    &namespace,
                    "view receipt-chain receipt",
                )?;
                if receipt_stable_id != chain_stable_id {
                    return Err(outbox_evidence_error(
                        event,
                        "view receipt-chain receipt stable-id must match chain stable-id",
                    ));
                }
            }
        }
        let Some(chain_receipt_count) = chain.get("receipt-count").and_then(Value::as_u64) else {
            return Err(outbox_evidence_error(
                event,
                "view receipt-chain chain evidence must contain receipt-count",
            ));
        };
        receipt_count = receipt_count.saturating_add(chain_receipt_count);
        if chain
            .get("tombstoned")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            tombstone_count = tombstone_count.saturating_add(1);
        }
        let chain_verified = chain
            .get("chain-verified")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if !chain_verified {
            return Err(outbox_evidence_error(
                event,
                "view receipt-chain chain must be structurally verified",
            ));
        }
        verified_count = verified_count.saturating_add(1);
        validate_view_receipt_chain_hash_evidence(event, chain)?;
        validate_view_receipt_chain_structure_evidence(event, chain)?;
        if let Some(chain_hash) = chain.get("chain-hash").and_then(Value::as_str) {
            structural_chain_hashes.insert(chain_hash.to_string());
        }
        if let Some(receipts) = chain.get("receipts").and_then(Value::as_array) {
            for receipt in receipts {
                if let Some(receipt_hash) = receipt.get("receipt-hash").and_then(Value::as_str) {
                    structural_receipt_hashes.insert(receipt_hash.to_string());
                    if receipt
                        .get("operation")
                        .and_then(Value::as_str)
                        .is_some_and(|operation| operation == "drop")
                    {
                        structural_drop_receipt_hashes.insert(receipt_hash.to_string());
                    }
                }
            }
        }
    }

    if verified_count != expected_verified_count {
        return Err(outbox_evidence_error(
            event,
            "view receipt-chain verified count does not match verified chains",
        ));
    }
    if receipt_count != expected_receipt_count {
        return Err(outbox_evidence_error(
            event,
            "view receipt-chain receipt-count does not match chains",
        ));
    }
    if tombstone_count != expected_tombstone_count {
        return Err(outbox_evidence_error(
            event,
            "view receipt-chain tombstone-count does not match chains",
        ));
    }

    let chain_hashes = validate_optional_full_hash_array_field(event, payload, "chain-hashes")?;
    validate_unique_hash_array(event, &chain_hashes, "view receipt-chain chain-hashes")?;
    let receipt_hashes = validate_optional_full_hash_array_field(event, payload, "receipt-hashes")?;
    validate_unique_hash_array(event, &receipt_hashes, "view receipt-chain receipt-hashes")?;
    let drop_receipt_hashes =
        validate_optional_full_hash_array_field(event, payload, "drop-receipt-hashes")?;
    validate_unique_hash_array(
        event,
        &drop_receipt_hashes,
        "view receipt-chain drop-receipt-hashes",
    )?;
    validate_hash_array_covers_structure(
        event,
        "view receipt-chain chain-hashes",
        &chain_hashes,
        &structural_chain_hashes,
    )?;
    validate_hash_array_covers_structure(
        event,
        "view receipt-chain receipt-hashes",
        &receipt_hashes,
        &structural_receipt_hashes,
    )?;
    validate_hash_array_covers_structure(
        event,
        "view receipt-chain drop-receipt-hashes",
        &drop_receipt_hashes,
        &structural_drop_receipt_hashes,
    )?;
    Ok(())
}

pub(crate) fn validate_view_receipt_chain_scope_evidence(
    event: &OutboxEvent,
    object: &Value,
    expected_warehouse: &WarehouseName,
    expected_namespace: &Namespace,
    label: &str,
) -> Result<(), LakeCatError> {
    let warehouse_name = required_string_field(event, object, "warehouse", label)?;
    let warehouse = WarehouseName::new(warehouse_name)
        .map_err(|_| outbox_evidence_error(event, &format!("{label} has invalid warehouse")))?;
    if &warehouse != expected_warehouse {
        return Err(outbox_evidence_error(
            event,
            &format!("{label} warehouse must match payload warehouse"),
        ));
    }
    let Some(namespace) = object.get("namespace") else {
        return Err(outbox_evidence_error(
            event,
            &format!("{label} evidence must contain namespace"),
        ));
    };
    let namespace = decode_namespace_value(event, namespace, label)?;
    if &namespace != expected_namespace {
        return Err(outbox_evidence_error(
            event,
            &format!("{label} namespace must match payload namespace"),
        ));
    }
    Ok(())
}

pub(crate) fn validate_view_receipt_chain_identity_evidence(
    event: &OutboxEvent,
    object: &Value,
    expected_warehouse: &WarehouseName,
    expected_namespace: &Namespace,
    label: &str,
) -> Result<String, LakeCatError> {
    let name = required_string_field(event, object, "name", label)?;
    let name = TableName::new(name)
        .map_err(|_| outbox_evidence_error(event, &format!("{label} has invalid name")))?;
    let expected_stable_id = format!(
        "lakecat:view:{}:{}:{}",
        expected_warehouse, expected_namespace, name
    );
    let stable_id = required_string_field(event, object, "stable-id", label)?;
    if stable_id != expected_stable_id {
        return Err(outbox_evidence_error(
            event,
            &format!("{label} stable-id must match warehouse, namespace, and name"),
        ));
    }
    Ok(expected_stable_id)
}

pub(crate) fn validate_hash_array_covers_structure(
    event: &OutboxEvent,
    label: &str,
    declared: &[&str],
    structural: &BTreeSet<String>,
) -> Result<(), LakeCatError> {
    let declared = declared
        .iter()
        .map(|value| (*value).to_string())
        .collect::<BTreeSet<_>>();
    if &declared != structural {
        return Err(outbox_evidence_error(
            event,
            &format!("{label} must match structural receipt-chain evidence"),
        ));
    }
    Ok(())
}

pub(crate) fn validate_querygraph_bootstrap_event_evidence(
    event: &OutboxEvent,
    payload: &Value,
) -> Result<(), LakeCatError> {
    if event.payload.get("payload").is_some() {
        validate_object_evidence_schema(
            event,
            &event.payload,
            "querygraph bootstrap outbox payload",
            LIST_OUTBOX_PAYLOAD_FIELDS,
        )?;
    }
    validate_object_evidence_schema(
        event,
        payload,
        "querygraph bootstrap",
        QUERYGRAPH_BOOTSTRAP_EVIDENCE_FIELDS,
    )?;
    validate_required_warehouse_field(event, payload, "querygraph bootstrap")?;
    validate_authorization_receipt_principal(event, payload, "querygraph bootstrap")?;
    for field in [
        "bundle-hash",
        "graph-hash",
        "open-lineage-hash",
        "querygraph-import-hash",
    ] {
        validate_required_full_hash_field(event, payload, field)?;
    }

    let verified_tables = validate_required_string_array_field(
        event,
        payload,
        "verified-tables",
        "querygraph bootstrap",
    )?;
    let verified_views = validate_required_string_array_field(
        event,
        payload,
        "verified-views",
        "querygraph bootstrap",
    )?;
    let table_count = validate_required_unsigned_count_field(
        event,
        payload,
        "table-count",
        "querygraph bootstrap",
    )?;
    let view_count = validate_required_unsigned_count_field(
        event,
        payload,
        "view-count",
        "querygraph bootstrap",
    )?;
    validate_required_unsigned_count_field(
        event,
        payload,
        "policy-binding-count",
        "querygraph bootstrap",
    )?;
    if verified_tables.len() as u64 != table_count {
        return Err(outbox_evidence_error(
            event,
            "querygraph bootstrap table-count does not match verified-tables",
        ));
    }
    if verified_views.len() as u64 != view_count {
        return Err(outbox_evidence_error(
            event,
            "querygraph bootstrap view-count does not match verified-views",
        ));
    }
    validate_unique_string_array(
        event,
        &verified_tables,
        "querygraph bootstrap verified-tables",
    )?;
    validate_unique_string_array(
        event,
        &verified_views,
        "querygraph bootstrap verified-views",
    )?;

    let table_artifact_ids = validate_querygraph_artifacts(
        event,
        payload,
        "table-artifacts",
        table_count,
        &[
            "croissant-hash",
            "cdif-hash",
            "osi-hash",
            "odrl-hash",
            "policy-bindings-hash",
        ],
    )?;
    let view_artifact_ids =
        validate_querygraph_artifacts(event, payload, "view-artifacts", view_count, &["osi-hash"])?;
    validate_stable_id_set_matches_manifest(
        event,
        "querygraph bootstrap table-artifacts",
        &table_artifact_ids,
        &verified_tables,
    )?;
    validate_stable_id_set_matches_manifest(
        event,
        "querygraph bootstrap view-artifacts",
        &view_artifact_ids,
        &verified_views,
    )?;
    let view_receipt_ids = validate_querygraph_view_receipts(event, payload, view_count)?;
    validate_stable_id_set_matches_manifest(
        event,
        "querygraph bootstrap view-version-receipts",
        &view_receipt_ids,
        &verified_views,
    )?;
    validate_querygraph_bootstrap_standards(event, payload)?;
    validate_querygraph_bootstrap_request_identity(event, payload)?;
    Ok(())
}

pub(crate) fn validate_querygraph_artifacts(
    event: &OutboxEvent,
    payload: &Value,
    field: &str,
    expected_count: u64,
    hash_fields: &[&str],
) -> Result<BTreeSet<String>, LakeCatError> {
    let Some(artifacts) = payload.get(field).and_then(Value::as_array) else {
        return Err(outbox_evidence_error(
            event,
            &format!("querygraph bootstrap evidence must contain {field}"),
        ));
    };
    if artifacts.len() as u64 != expected_count {
        return Err(outbox_evidence_error(
            event,
            &format!("querygraph bootstrap {field} count does not match manifest count"),
        ));
    }
    let mut stable_ids = BTreeSet::new();
    for artifact in artifacts {
        validate_querygraph_artifact_evidence_schema(event, artifact, field, hash_fields)?;
        let Some(stable_id) = artifact
            .get("stable-id")
            .and_then(Value::as_str)
            .filter(|stable_id| !stable_id.is_empty())
        else {
            return Err(outbox_evidence_error(
                event,
                &format!("querygraph bootstrap {field} entries must contain stable-id"),
            ));
        };
        if stable_id.contains(char::is_whitespace) {
            return Err(outbox_evidence_error(
                event,
                &format!("querygraph bootstrap {field} stable-id must not contain whitespace"),
            ));
        }
        if !stable_ids.insert(stable_id.to_string()) {
            return Err(outbox_evidence_error(
                event,
                &format!(
                    "querygraph bootstrap {field} must not contain duplicate stable-id values"
                ),
            ));
        }
        for hash_field in hash_fields {
            validate_required_full_hash_field(event, artifact, hash_field)?;
        }
    }
    Ok(stable_ids)
}

pub(crate) fn validate_querygraph_view_receipts(
    event: &OutboxEvent,
    payload: &Value,
    view_count: u64,
) -> Result<BTreeSet<String>, LakeCatError> {
    let Some(receipts) = payload.get("view-version-receipts") else {
        if view_count == 0 {
            return Ok(BTreeSet::new());
        }
        return Err(outbox_evidence_error(
            event,
            "querygraph bootstrap evidence must contain view-version-receipts",
        ));
    };
    let Some(receipts) = receipts.as_array() else {
        return Err(outbox_evidence_error(
            event,
            "querygraph bootstrap view-version-receipts must be an array",
        ));
    };
    if receipts.len() as u64 != view_count {
        return Err(outbox_evidence_error(
            event,
            "querygraph bootstrap view-version-receipts count does not match view-count",
        ));
    }
    let mut stable_ids = BTreeSet::new();
    for receipt in receipts {
        validate_object_evidence_schema(
            event,
            receipt,
            "querygraph bootstrap view-version receipt",
            QUERYGRAPH_VIEW_RECEIPT_EVIDENCE_FIELDS,
        )?;
        let Some(stable_id) = receipt
            .get("stable-id")
            .and_then(Value::as_str)
            .filter(|stable_id| !stable_id.is_empty())
        else {
            return Err(outbox_evidence_error(
                event,
                "querygraph bootstrap view-version receipt must contain stable-id",
            ));
        };
        if stable_id.contains(char::is_whitespace) {
            return Err(outbox_evidence_error(
                event,
                "querygraph bootstrap view-version receipt stable-id must not contain whitespace",
            ));
        }
        if !stable_ids.insert(stable_id.to_string()) {
            return Err(outbox_evidence_error(
                event,
                "querygraph bootstrap view-version-receipts must not contain duplicate stable-id values",
            ));
        }
        let Some(view_version) = receipt.get("view-version").and_then(Value::as_u64) else {
            return Err(outbox_evidence_error(
                event,
                "querygraph bootstrap view-version receipt must contain view-version",
            ));
        };
        if view_version == 0 {
            return Err(outbox_evidence_error(
                event,
                "querygraph bootstrap view-version receipt version must be positive",
            ));
        }
        validate_required_full_hash_field(event, receipt, "receipt-hash")?;
        validate_required_full_hash_field(event, receipt, "receipt-chain-hash")?;
    }
    Ok(stable_ids)
}

pub(crate) fn validate_querygraph_artifact_evidence_schema(
    event: &OutboxEvent,
    artifact: &Value,
    field: &str,
    hash_fields: &[&str],
) -> Result<(), LakeCatError> {
    let Some(artifact) = artifact.as_object() else {
        return Err(outbox_evidence_error(
            event,
            &format!("querygraph bootstrap {field} entry must be an object"),
        ));
    };
    let mut allowed_fields = BTreeSet::from(["stable-id"]);
    allowed_fields.extend(hash_fields.iter().copied());
    for entry_field in artifact.keys() {
        if !allowed_fields.contains(entry_field.as_str()) {
            return Err(outbox_evidence_error(
                event,
                &format!(
                    "querygraph bootstrap {field} entry contains unexpected field {entry_field}"
                ),
            ));
        }
    }
    Ok(())
}

pub(crate) fn validate_stable_id_set_matches_manifest(
    event: &OutboxEvent,
    label: &str,
    actual: &BTreeSet<String>,
    expected: &[String],
) -> Result<(), LakeCatError> {
    let expected = expected.iter().cloned().collect::<BTreeSet<_>>();
    if actual != &expected {
        return Err(outbox_evidence_error(
            event,
            &format!("{label} stable-id set must match verified manifest"),
        ));
    }
    Ok(())
}

pub(crate) fn validate_querygraph_bootstrap_standards(
    event: &OutboxEvent,
    payload: &Value,
) -> Result<(), LakeCatError> {
    let standards =
        validate_required_string_array_field(event, payload, "standards", "querygraph bootstrap")?;
    let standards = standards
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    for required in [
        "Iceberg REST",
        "Croissant",
        "CDIF",
        "OSI handoff",
        "ODRL",
        "Grust catalog graph",
        "OpenLineage",
    ] {
        if !standards.contains(required) {
            return Err(outbox_evidence_error(
                event,
                &format!("querygraph bootstrap standards must include {required}"),
            ));
        }
    }
    Ok(())
}

pub(crate) fn validate_querygraph_bootstrap_request_identity(
    event: &OutboxEvent,
    payload: &Value,
) -> Result<(), LakeCatError> {
    let Some(request_identity) = payload
        .pointer("/authorization-receipt/context/request-identity")
        .or_else(|| payload.pointer("/authorization-receipt/request-identity"))
    else {
        return Ok(());
    };
    validate_object_evidence_schema(
        event,
        request_identity,
        "querygraph bootstrap request-identity",
        REQUEST_IDENTITY_EVIDENCE_FIELDS,
    )?;
    for field in [
        "typedid-envelope-sha256",
        "typedid-proof-sha256",
        "agent-delegation-sha256",
        "agent-summary-signature-sha256",
    ] {
        validate_optional_full_hash_field(event, request_identity, field)?;
    }
    if request_identity
        .get("typedid-proof-sha256")
        .and_then(Value::as_str)
        .is_some()
        && request_identity
            .get("typedid-envelope-sha256")
            .and_then(Value::as_str)
            .is_none()
    {
        return Err(outbox_evidence_error(
            event,
            "querygraph bootstrap TypeDID proof hash requires envelope hash",
        ));
    }
    Ok(())
}

pub(crate) fn validate_view_receipt_chain_hash_evidence(
    event: &OutboxEvent,
    chain: &Value,
) -> Result<(), LakeCatError> {
    validate_required_full_hash_field(event, chain, "chain-hash")?;
    let Some(receipts) = chain.get("receipts").and_then(Value::as_array) else {
        return Err(outbox_evidence_error(
            event,
            "verified view receipt-chain must contain receipts",
        ));
    };
    for receipt in receipts {
        validate_required_full_hash_field(event, receipt, "receipt-hash")?;
    }
    Ok(())
}

pub(crate) fn validate_view_receipt_chain_structure_evidence(
    event: &OutboxEvent,
    chain: &Value,
) -> Result<(), LakeCatError> {
    let Some(receipts) = chain.get("receipts").and_then(Value::as_array) else {
        return Err(outbox_evidence_error(
            event,
            "verified view receipt-chain must contain receipts",
        ));
    };
    if receipts.is_empty() {
        return Err(outbox_evidence_error(
            event,
            "verified view receipt-chain must contain at least one receipt",
        ));
    }
    let Some(chain_receipt_count) = chain.get("receipt-count").and_then(Value::as_u64) else {
        return Err(outbox_evidence_error(
            event,
            "verified view receipt-chain must contain receipt-count",
        ));
    };
    if chain_receipt_count != receipts.len() as u64 {
        return Err(outbox_evidence_error(
            event,
            "verified view receipt-chain receipt-count must match receipts",
        ));
    }

    for (index, receipt) in receipts.iter().enumerate() {
        let Some(view_version) = receipt.get("view-version").and_then(Value::as_u64) else {
            return Err(outbox_evidence_error(
                event,
                "verified view receipt-chain receipt must contain view-version",
            ));
        };
        if view_version == 0 {
            return Err(outbox_evidence_error(
                event,
                "verified view receipt-chain receipt version must be positive",
            ));
        }
        let Some(operation) = receipt.get("operation").and_then(Value::as_str) else {
            return Err(outbox_evidence_error(
                event,
                "verified view receipt-chain receipt must contain operation",
            ));
        };
        let Some(receipt_hash) = receipt.get("receipt-hash").and_then(Value::as_str) else {
            return Err(outbox_evidence_error(
                event,
                "verified view receipt-chain receipt must contain receipt-hash",
            ));
        };

        let Some(previous) = index.checked_sub(1).and_then(|index| receipts.get(index)) else {
            if operation == "upsert"
                && view_version == 1
                && receipt
                    .get("previous-view-version")
                    .is_none_or(Value::is_null)
                && receipt
                    .get("previous-receipt-hash")
                    .is_none_or(Value::is_null)
            {
                continue;
            }
            return Err(outbox_evidence_error(
                event,
                "verified view receipt-chain first receipt must be a version 1 upsert without previous links",
            ));
        };

        let previous_version = previous
            .get("view-version")
            .and_then(Value::as_u64)
            .ok_or_else(|| {
                outbox_evidence_error(
                    event,
                    "verified view receipt-chain previous receipt must contain view-version",
                )
            })?;
        let previous_receipt_hash = previous
            .get("receipt-hash")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                outbox_evidence_error(
                    event,
                    "verified view receipt-chain previous receipt must contain receipt-hash",
                )
            })?;
        if receipt.get("previous-view-version").and_then(Value::as_u64) != Some(previous_version)
            || receipt.get("previous-receipt-hash").and_then(Value::as_str)
                != Some(previous_receipt_hash)
        {
            return Err(outbox_evidence_error(
                event,
                "verified view receipt-chain receipt previous links must match the prior receipt",
            ));
        }

        match operation {
            "upsert" if view_version == previous_version.saturating_add(1) => {}
            "drop" if view_version == previous_version => {}
            _ => {
                return Err(outbox_evidence_error(
                    event,
                    "verified view receipt-chain receipt transition is invalid",
                ));
            }
        }

        if !is_full_sha256_digest_evidence(receipt_hash) {
            return Err(outbox_evidence_error(
                event,
                "verified view receipt-chain receipt-hash must contain full SHA-256 digest evidence",
            ));
        }
    }

    let latest_receipt = receipts.last().expect("receipts is non-empty");
    let latest_view_version = latest_receipt
        .get("view-version")
        .and_then(Value::as_u64)
        .expect("receipt view-version was validated");
    let latest_operation = latest_receipt
        .get("operation")
        .and_then(Value::as_str)
        .expect("receipt operation was validated");
    let Some(chain_latest_view_version) = chain.get("latest-view-version").and_then(Value::as_u64)
    else {
        return Err(outbox_evidence_error(
            event,
            "verified view receipt-chain must contain latest-view-version",
        ));
    };
    if chain_latest_view_version != latest_view_version {
        return Err(outbox_evidence_error(
            event,
            "verified view receipt-chain latest-view-version must match the last receipt",
        ));
    }
    let Some(chain_latest_operation) = chain.get("latest-operation").and_then(Value::as_str) else {
        return Err(outbox_evidence_error(
            event,
            "verified view receipt-chain must contain latest-operation",
        ));
    };
    if chain_latest_operation != latest_operation {
        return Err(outbox_evidence_error(
            event,
            "verified view receipt-chain latest-operation must match the last receipt",
        ));
    }
    let Some(chain_tombstoned) = chain.get("tombstoned").and_then(Value::as_bool) else {
        return Err(outbox_evidence_error(
            event,
            "verified view receipt-chain must contain tombstoned",
        ));
    };
    if chain_tombstoned != (latest_operation == "drop") {
        return Err(outbox_evidence_error(
            event,
            "verified view receipt-chain tombstoned flag must match the last receipt operation",
        ));
    }
    let receipt_hashes = receipts
        .iter()
        .map(|receipt| {
            receipt
                .get("receipt-hash")
                .and_then(Value::as_str)
                .expect("receipt-hash was validated")
                .to_string()
        })
        .collect::<Vec<_>>();
    let expected_chain_hash = content_hash_json(&json!({
        "stable-id": latest_receipt
            .get("stable-id")
            .and_then(Value::as_str)
            .expect("stable-id was validated"),
        "warehouse": latest_receipt
            .get("warehouse")
            .and_then(Value::as_str)
            .expect("warehouse was validated"),
        "namespace": latest_receipt
            .get("namespace")
            .expect("namespace was validated"),
        "name": latest_receipt
            .get("name")
            .and_then(Value::as_str)
            .expect("name was validated"),
        "latest-view-version": latest_view_version,
        "latest-operation": latest_operation,
        "tombstoned": chain_tombstoned,
        "receipt-hashes": receipt_hashes,
    }))
    .map_err(|err| {
        outbox_evidence_error(
            event,
            &format!("verified view receipt-chain hash could not be computed: {err}"),
        )
    })?;
    if chain.get("chain-hash").and_then(Value::as_str) != Some(expected_chain_hash.as_str()) {
        return Err(outbox_evidence_error(
            event,
            "verified view receipt-chain chain-hash must match structural receipt-chain evidence",
        ));
    }

    Ok(())
}
