use std::collections::BTreeMap;

use lakecat_api::{
    ConfigEntry, LoadTableResponse, PolicyBindingResponse, ProjectResponse, ServerResponse,
    StorageCredential, StorageProfileResponse, TableCommitRecordResponse, TableIdentifier,
    ViewColumnResponse, ViewResponse, ViewVersionReceiptChainResponse, ViewVersionReceiptResponse,
    WarehouseResponse,
};
use lakecat_core::{
    LakeCatError, LakeCatResult, PrincipalKind, WarehouseName, content_hash_bytes,
    content_hash_json,
};
#[cfg(feature = "sail-local")]
use lakecat_sail::catalog_provider::{
    LakeCatCatalogProvider, ProviderFetchScanTasksRequest, ProviderScanPlanningRequest,
};
use lakecat_security::{AuthorizationReceipt, ReadRestriction};
use lakecat_store::{
    PolicyBinding, ProjectRecord, ServerRecord, StorageProfile, TableCommitRecord, TableRecord,
    ViewColumnRecord, ViewRecord, ViewVersionOperation, ViewVersionReceipt, WarehouseRecord,
};
use serde_json::{Value, json};

use crate::*;

pub(crate) fn load_table_response(table: TableRecord) -> LoadTableResponse {
    LoadTableResponse {
        identifier: TableIdentifier::from_ident(&table.ident),
        metadata_location: table.metadata_location,
        metadata: table.metadata,
        config: vec![],
    }
}

pub(crate) fn table_commit_record_response(
    record: &TableCommitRecord,
) -> LakeCatResult<TableCommitRecordResponse> {
    Ok(TableCommitRecordResponse {
        warehouse: record.table.warehouse.as_str().to_string(),
        namespace: record.table.namespace.parts().to_vec(),
        table: record.table.name.as_str().to_string(),
        previous_metadata_location: record.previous_metadata_location.clone(),
        new_metadata_location: record.new_metadata_location.clone(),
        sequence_number: record.sequence_number,
        format_version: record.format_version,
        snapshot_id: record.snapshot_id,
        policy_hash: record.policy_hash.clone(),
        request_hash: record.request_hash.clone(),
        response_hash: record.response_hash.clone(),
        idempotency_key_sha256: record.idempotency_key_sha256.clone(),
        commit_hash: content_hash_json(&serde_json::to_value(record).map_err(|err| {
            LakeCatError::Internal(format!("failed to serialize table commit record: {err}"))
        })?)?,
        principal_subject: record.principal.subject.clone(),
        principal_kind: principal_kind_name(&record.principal.kind).to_string(),
        committed_at: record.committed_at.to_rfc3339(),
    })
}

pub(crate) fn public_storage_credentials_for_profile(
    profile: &StorageProfile,
) -> Vec<StorageCredential> {
    let mut config = vec![ConfigEntry::new(
        "lakecat.storage-profile-id",
        profile.profile_id.clone(),
    )];
    config.extend(
        profile
            .public_config
            .iter()
            .map(|(key, value)| ConfigEntry::new(key.clone(), value.clone())),
    );
    vec![StorageCredential {
        prefix: profile.location_prefix.clone(),
        config,
    }]
}

pub(crate) fn apply_credential_ttl_cap(
    mut credentials: Vec<StorageCredential>,
    max_credential_ttl_seconds: Option<u64>,
) -> Vec<StorageCredential> {
    let Some(max_credential_ttl_seconds) = max_credential_ttl_seconds else {
        return credentials;
    };
    for credential in &mut credentials {
        let mut effective = max_credential_ttl_seconds;
        credential.config.retain(|entry| {
            if entry.key == "lakecat.max-credential-ttl-seconds" {
                if let Ok(existing) = entry.value.parse::<u64>() {
                    effective = effective.min(existing);
                }
                false
            } else {
                true
            }
        });
        credential.config.push(ConfigEntry::new(
            "lakecat.max-credential-ttl-seconds",
            effective.to_string(),
        ));
    }
    credentials
}

pub(crate) fn apply_secret_ref_provider_evidence(
    mut credentials: Vec<StorageCredential>,
    profile: &StorageProfile,
) -> Vec<StorageCredential> {
    let Some(secret_ref) = profile.secret_ref.as_deref() else {
        return credentials;
    };
    let Some(secret_ref_provider) = secret_ref_provider_label(secret_ref).map(str::to_string)
    else {
        return credentials;
    };
    let secret_ref_hash = content_hash_bytes(secret_ref.as_bytes());
    for credential in &mut credentials {
        credential.config.retain(|entry| {
            !matches!(
                entry.key.to_ascii_lowercase().as_str(),
                "lakecat.secret-ref-provider" | "lakecat.secret-ref-hash"
            )
        });
        credential.config.push(ConfigEntry::new(
            "lakecat.secret-ref-provider",
            secret_ref_provider.clone(),
        ));
        credential.config.push(ConfigEntry::new(
            "lakecat.secret-ref-hash",
            secret_ref_hash.clone(),
        ));
    }
    credentials
}

pub(crate) fn canonicalize_credential_response_evidence(
    mut credentials: Vec<StorageCredential>,
    profile: &StorageProfile,
    receipt: &AuthorizationReceipt,
    governed_read_required: bool,
    max_credential_ttl_seconds: Option<u64>,
) -> Vec<StorageCredential> {
    for credential in &mut credentials {
        let mut effective_ttl = max_credential_ttl_seconds;
        credential.config.retain(|entry| {
            let normalized = entry.key.to_ascii_lowercase();
            if normalized == "lakecat.max-credential-ttl-seconds" {
                if let Ok(existing) = entry.value.parse::<u64>() {
                    effective_ttl =
                        Some(effective_ttl.map_or(existing, |current| current.min(existing)));
                }
                return false;
            }
            !LAKECAT_CREDENTIAL_RESPONSE_EVIDENCE_KEYS.contains(&normalized.as_str())
        });
        credential.config.extend([
            ConfigEntry::new("lakecat.storage-profile-id", profile.profile_id.clone()),
            ConfigEntry::new("lakecat.storage-provider", profile.provider.as_str()),
            ConfigEntry::new("lakecat.credential-mode", profile.issuance_mode.as_str()),
            ConfigEntry::new(
                "lakecat.authorization-principal",
                receipt.principal.subject.clone(),
            ),
            ConfigEntry::new(
                "lakecat.governed-read-required",
                governed_read_required.to_string(),
            ),
        ]);
        if let Some(effective_ttl) = effective_ttl {
            credential.config.push(ConfigEntry::new(
                "lakecat.max-credential-ttl-seconds",
                effective_ttl.to_string(),
            ));
        }
    }
    apply_secret_ref_provider_evidence(credentials, profile)
}

pub(crate) const LAKECAT_CREDENTIAL_RESPONSE_EVIDENCE_KEYS: &[&str] = &[
    "lakecat.storage-profile-id",
    "lakecat.storage-provider",
    "lakecat.credential-mode",
    "lakecat.authorization-principal",
    "lakecat.governed-read-required",
    "lakecat.secret-ref-provider",
    "lakecat.secret-ref-hash",
];

pub(crate) fn issued_credentials_for_profile(
    credentials: Vec<StorageCredential>,
    profile: &StorageProfile,
    max_credential_ttl_seconds: Option<u64>,
) -> LakeCatResult<Vec<StorageCredential>> {
    for credential in &credentials {
        if !location_is_within_prefix(&credential.prefix, &profile.location_prefix) {
            return Err(LakeCatError::InvalidArgument(format!(
                "issued credential prefix is outside storage profile scope; \
                 credential-prefix-hash={}; storage-profile-prefix-hash={}",
                content_hash_json(&json!({"credential-prefix": &credential.prefix}))?,
                content_hash_json(&json!({"location-prefix": &profile.location_prefix}))?
            )));
        }
    }
    Ok(apply_secret_ref_provider_evidence(
        apply_credential_ttl_cap(credentials, max_credential_ttl_seconds),
        profile,
    ))
}

pub(crate) fn storage_profile_event_payload(profile: &StorageProfile) -> Value {
    let mut payload = json!({
        "profile-id": profile.profile_id.clone(),
        "warehouse": profile.warehouse.as_str(),
        "location-prefix-hash": content_hash_json(&json!({
            "location-prefix": &profile.location_prefix
        })).expect("storage profile location prefix hash should serialize"),
        "provider": profile.provider.as_str(),
        "issuance-mode": profile.issuance_mode.as_str(),
        "secret-ref-present": profile.secret_ref.is_some(),
        "public-config": profile.public_config.clone(),
    });
    if let Some(provider) = profile
        .secret_ref
        .as_deref()
        .and_then(secret_ref_provider_label)
    {
        payload["secret-ref-provider"] = json!(provider);
    }
    if let Some(secret_ref) = profile.secret_ref.as_deref() {
        payload["secret-ref-hash"] = json!(content_hash_bytes(secret_ref.as_bytes()));
    }
    payload
}

pub(crate) fn redact_storage_profile_event_payload(mut payload: Value) -> Value {
    let Some(profile) = payload
        .get_mut("storage-profile")
        .and_then(Value::as_object_mut)
    else {
        return payload;
    };
    if let Some(location_prefix) = profile.remove("location-prefix").and_then(|value| {
        value
            .as_str()
            .map(|location_prefix| location_prefix.to_string())
    }) && let Ok(hash) = content_hash_json(&json!({"location-prefix": location_prefix}))
    {
        profile.insert("location-prefix-hash".to_string(), json!(hash));
    }
    let raw_secret_ref = profile
        .remove("secret-ref")
        .and_then(|value| value.as_str().map(str::to_string));
    let provider = raw_secret_ref
        .as_deref()
        .and_then(secret_ref_provider_label)
        .map(str::to_string);
    if let Some(secret_ref) = raw_secret_ref.as_deref() {
        profile.insert(
            "secret-ref-hash".to_string(),
            json!(content_hash_bytes(secret_ref.as_bytes())),
        );
    }
    let secret_ref_present = raw_secret_ref.is_some()
        || profile
            .get("secret-ref-present")
            .and_then(Value::as_bool)
            .unwrap_or(false);
    profile.insert("secret-ref-present".to_string(), json!(secret_ref_present));
    if let Some(provider) = provider {
        profile.insert("secret-ref-provider".to_string(), json!(provider));
    }
    payload
}

pub(crate) fn redact_warehouse_event_payload(mut payload: Value) -> Value {
    let Some(warehouse) = payload
        .get_mut("warehouse-record")
        .and_then(Value::as_object_mut)
    else {
        return payload;
    };
    if let Some(storage_root) = warehouse
        .remove("storage-root")
        .and_then(|value| value.as_str().map(str::to_string))
    {
        warehouse.insert(
            "storage-root-hash".to_string(),
            json!(
                content_hash_json(&json!({"storage-root": storage_root}))
                    .unwrap_or_else(|_| content_hash_bytes(storage_root.as_bytes()))
            ),
        );
    }
    payload
}

pub(crate) fn redact_server_event_payload(mut payload: Value) -> Value {
    let Some(server) = payload
        .get_mut("server-record")
        .and_then(Value::as_object_mut)
    else {
        return payload;
    };
    if let Some(endpoint_url) = server
        .remove("endpoint-url")
        .and_then(|value| value.as_str().map(str::to_string))
    {
        server.insert(
            "endpoint-url-hash".to_string(),
            json!(
                content_hash_json(&json!({"endpoint-url": endpoint_url}))
                    .unwrap_or_else(|_| content_hash_bytes(endpoint_url.as_bytes()))
            ),
        );
    }
    payload
}

pub(crate) fn secret_ref_provider_label(secret_ref: &str) -> Option<&str> {
    secret_ref.split_once("://").map(|(scheme, _)| scheme)
}

pub(crate) fn storage_profile_response(profile: &StorageProfile) -> StorageProfileResponse {
    let secret_ref = profile.secret_ref.as_deref();
    StorageProfileResponse {
        profile_id: profile.profile_id.clone(),
        warehouse: profile.warehouse.as_str().to_string(),
        location_prefix: profile.location_prefix.clone(),
        provider: profile.provider.as_str().to_string(),
        issuance_mode: profile.issuance_mode.as_str().to_string(),
        secret_ref: None,
        secret_ref_present: secret_ref.is_some(),
        secret_ref_provider: secret_ref
            .and_then(secret_ref_provider_label)
            .map(str::to_string),
        secret_ref_hash: secret_ref.map(|secret_ref| content_hash_bytes(secret_ref.as_bytes())),
        public_config: profile.public_config.clone(),
    }
}

pub(crate) fn warehouse_response(record: &WarehouseRecord) -> WarehouseResponse {
    WarehouseResponse {
        warehouse: record.warehouse.as_str().to_string(),
        project_id: record.project_id.clone(),
        storage_root: record.storage_root.clone(),
        properties: record.properties.clone(),
    }
}

pub(crate) fn server_response(record: &ServerRecord) -> ServerResponse {
    ServerResponse {
        server_id: record.server_id.clone(),
        display_name: record.display_name.clone(),
        endpoint_url: record.endpoint_url.clone(),
        properties: record.properties.clone(),
    }
}

pub(crate) fn project_response(record: &ProjectRecord) -> ProjectResponse {
    ProjectResponse {
        project_id: record.project_id.clone(),
        server_id: record.server_id.clone(),
        display_name: record.display_name.clone(),
        properties: record.properties.clone(),
    }
}

pub(crate) fn view_response(record: &ViewRecord) -> ViewResponse {
    ViewResponse {
        warehouse: record.warehouse.as_str().to_string(),
        namespace: record.namespace.parts().to_vec(),
        name: record.name.as_str().to_string(),
        view_version: record.view_version,
        sql: record.sql.clone(),
        dialect: record.dialect.clone(),
        schema_version: record.schema_version,
        columns: record
            .columns
            .iter()
            .map(|column| ViewColumnResponse {
                name: column.name.clone(),
                data_type: column.data_type.clone(),
                nullable: column.nullable,
                comment: column.comment.clone(),
            })
            .collect(),
        properties: record.properties.clone(),
    }
}

pub(crate) fn view_version_receipt_response(
    receipt: &ViewVersionReceipt,
) -> LakeCatResult<ViewVersionReceiptResponse> {
    Ok(ViewVersionReceiptResponse {
        stable_id: receipt.stable_id.clone(),
        warehouse: receipt.warehouse.as_str().to_string(),
        namespace: receipt.namespace.parts().to_vec(),
        name: receipt.name.as_str().to_string(),
        view_version: receipt.view_version,
        previous_view_version: receipt.previous_view_version,
        previous_receipt_hash: receipt.previous_receipt_hash.clone(),
        operation: view_version_operation(&receipt.operation).to_string(),
        view_hash: receipt.view_hash.clone(),
        receipt_hash: content_hash_json(&serde_json::to_value(receipt).map_err(|err| {
            LakeCatError::Internal(format!("failed to serialize view receipt: {err}"))
        })?)?,
        principal_subject: receipt.principal.subject.clone(),
        principal_kind: principal_kind_name(&receipt.principal.kind).to_string(),
        recorded_at: receipt.recorded_at.to_rfc3339(),
    })
}

pub(crate) fn view_version_receipt_chains(
    receipts: &[ViewVersionReceipt],
) -> LakeCatResult<Vec<ViewVersionReceiptChainResponse>> {
    let mut grouped = BTreeMap::<String, Vec<&ViewVersionReceipt>>::new();
    for receipt in receipts {
        grouped
            .entry(receipt.stable_id.clone())
            .or_default()
            .push(receipt);
    }

    grouped
        .into_values()
        .filter_map(|mut receipts| {
            receipts.sort_by(|left, right| {
                left.view_version
                    .cmp(&right.view_version)
                    .then_with(|| left.recorded_at.cmp(&right.recorded_at))
                    .then_with(|| {
                        view_version_operation(&left.operation)
                            .cmp(view_version_operation(&right.operation))
                    })
            });
            let latest = receipts.last().copied()?;
            let response_receipts = receipts
                .iter()
                .map(|receipt| view_version_receipt_response(receipt))
                .collect::<LakeCatResult<Vec<_>>>();
            Some(response_receipts.and_then(|response_receipts| {
                let chain_hash = view_version_receipt_chain_hash(&response_receipts)?;
                let chain_verified = view_version_receipt_chain_verified(&response_receipts);
                Ok(ViewVersionReceiptChainResponse {
                    stable_id: latest.stable_id.clone(),
                    warehouse: latest.warehouse.as_str().to_string(),
                    namespace: latest.namespace.parts().to_vec(),
                    name: latest.name.as_str().to_string(),
                    chain_hash,
                    chain_verified,
                    latest_view_version: latest.view_version,
                    latest_operation: view_version_operation(&latest.operation).to_string(),
                    tombstoned: latest.operation == ViewVersionOperation::Drop,
                    receipt_count: response_receipts.len(),
                    receipts: response_receipts,
                })
            }))
        })
        .collect()
}

pub(crate) fn view_version_receipt_chain_hash(
    receipts: &[ViewVersionReceiptResponse],
) -> LakeCatResult<String> {
    let latest = receipts.last().ok_or_else(|| {
        LakeCatError::InvalidArgument(
            "view receipt-chain hash requires at least one receipt".to_string(),
        )
    })?;
    content_hash_json(&json!({
        "stable-id": latest.stable_id,
        "warehouse": latest.warehouse,
        "namespace": latest.namespace,
        "name": latest.name,
        "latest-view-version": latest.view_version,
        "latest-operation": latest.operation,
        "tombstoned": latest.operation == "drop",
        "receipt-hashes": receipts
            .iter()
            .map(|receipt| receipt.receipt_hash.clone())
            .collect::<Vec<_>>(),
    }))
}

pub(crate) fn view_version_receipt_chain_verified(receipts: &[ViewVersionReceiptResponse]) -> bool {
    if receipts.is_empty() {
        return false;
    }

    receipts.iter().enumerate().all(|(index, receipt)| {
        if receipt.view_version == 0 {
            return false;
        }
        let Some(previous) = index.checked_sub(1).and_then(|index| receipts.get(index)) else {
            return receipt.operation == "upsert"
                && receipt.view_version == 1
                && receipt.previous_view_version.is_none()
                && receipt.previous_receipt_hash.is_none();
        };
        if receipt.previous_receipt_hash.as_deref() != Some(previous.receipt_hash.as_str()) {
            return false;
        }
        match receipt.operation.as_str() {
            "upsert" => {
                receipt.previous_view_version == Some(previous.view_version)
                    && receipt.view_version == previous.view_version.saturating_add(1)
            }
            "drop" => {
                receipt.previous_view_version == Some(previous.view_version)
                    && receipt.view_version == previous.view_version
            }
            _ => false,
        }
    })
}

pub(crate) fn principal_kind_name(kind: &PrincipalKind) -> &'static str {
    match kind {
        PrincipalKind::Anonymous => "anonymous",
        PrincipalKind::Human => "human",
        PrincipalKind::Service => "service",
        PrincipalKind::Agent => "agent",
    }
}

pub(crate) fn view_version_operation(operation: &ViewVersionOperation) -> &'static str {
    match operation {
        ViewVersionOperation::Upsert => "upsert",
        ViewVersionOperation::Drop => "drop",
    }
}

pub(crate) fn view_columns_from_request(
    columns: Vec<lakecat_api::ViewColumnRequest>,
) -> LakeCatResult<Vec<ViewColumnRecord>> {
    columns
        .into_iter()
        .map(|column| {
            let record = ViewColumnRecord {
                name: column.name,
                data_type: column.data_type,
                nullable: column.nullable,
                comment: column.comment,
            };
            record.validate()?;
            Ok(record)
        })
        .collect()
}

pub(crate) fn policy_binding_response(binding: &PolicyBinding) -> PolicyBindingResponse {
    PolicyBindingResponse {
        policy_id: binding.policy_id.clone(),
        warehouse: binding.warehouse.as_str().to_string(),
        namespace: binding
            .namespace
            .as_ref()
            .map(|namespace| namespace.parts().to_vec()),
        table: binding
            .table
            .as_ref()
            .map(|table| table.as_str().to_string()),
        enforced: binding.enforced,
        odrl: binding.odrl.clone(),
    }
}

pub(crate) fn read_restriction_from_policy_bindings(
    bindings: &[PolicyBinding],
) -> Result<ReadRestriction, LakeCatError> {
    ReadRestriction::from_odrl_policies(bindings.iter().map(|binding| &binding.odrl))
}

pub(crate) fn management_warehouse(
    _state: &LakeCatState,
    warehouse: String,
) -> Result<WarehouseName, LakeCatHttpError> {
    let warehouse = WarehouseName::new(warehouse)?;
    Ok(warehouse)
}

pub(crate) async fn prefixed_catalog_warehouse(
    state: &LakeCatState,
    warehouse: String,
) -> Result<WarehouseName, LakeCatHttpError> {
    let warehouse = WarehouseName::new(warehouse)?;
    state.store.load_warehouse(&warehouse).await?;
    Ok(warehouse)
}
