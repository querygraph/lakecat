use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use lakecat_core::{
    LakeCatError, LakeCatResult, Namespace, Principal, TableIdent, TableName, WarehouseName,
    content_hash_bytes, content_hash_json,
};
use serde_json::Value;
use url::Url;

use crate::*;

pub fn table_ident(
    warehouse: impl Into<String>,
    namespace: impl AsRef<str>,
    table: impl Into<String>,
) -> LakeCatResult<TableIdent> {
    Ok(TableIdent::new(
        WarehouseName::new(warehouse.into())?,
        namespace.as_ref().parse()?,
        TableName::new(table.into())?,
    ))
}

pub(crate) fn table_key(ident: &TableIdent) -> String {
    format!(
        "{}\u{1f}{}\u{1f}{}",
        ident.warehouse, ident.namespace, ident.name
    )
}

pub(crate) fn metadata_pointer_conflict(
    ident: &TableIdent,
    expected_metadata_location: Option<&str>,
    actual_metadata_location: Option<&str>,
) -> LakeCatError {
    let expected_hash = optional_location_hash(expected_metadata_location);
    let actual_hash = optional_location_hash(actual_metadata_location);
    LakeCatError::Conflict(format!(
        "metadata pointer changed for {}; expected-metadata-location-hash={expected_hash}; actual-metadata-location-hash={actual_hash}",
        ident.stable_id()
    ))
}

pub(crate) fn optional_location_hash(location: Option<&str>) -> String {
    location
        .map(|location| content_hash_bytes(location.as_bytes()))
        .unwrap_or_else(|| "null".to_string())
}

pub(crate) fn validate_table_record_identity(
    record: &TableRecord,
    ident: &TableIdent,
) -> LakeCatResult<()> {
    record.validate()?;
    if record.ident != *ident {
        return Err(LakeCatError::Internal(
            "table record row scope does not match requested table".to_string(),
        ));
    }
    Ok(())
}

pub(crate) fn validate_table_record_map_scope(
    record: &TableRecord,
    record_key: &str,
) -> LakeCatResult<()> {
    record.validate()?;
    if table_key(&record.ident) != record_key {
        return Err(LakeCatError::Internal(
            "table record row scope does not match requested table".to_string(),
        ));
    }
    Ok(())
}

pub(crate) fn validate_soft_delete_record_map_scope(
    record: &SoftDeleteRecord,
    record_key: &str,
) -> LakeCatResult<()> {
    if table_key(&record.table) != record_key {
        return Err(LakeCatError::Internal(
            "soft-delete row scope does not match record identity".to_string(),
        ));
    }
    Ok(())
}

#[cfg(feature = "turso-local")]
pub(crate) fn validate_table_record_scope(
    record: &TableRecord,
    ident: &TableIdent,
    row_table_key: &str,
    row_warehouse: &str,
    row_namespace_path: &str,
    row_table_name: &str,
) -> LakeCatResult<()> {
    validate_table_record_identity(record, ident)?;
    if row_table_key != table_key(ident)
        || row_warehouse != ident.warehouse.as_str()
        || row_namespace_path != ident.namespace.path()
        || row_table_name != ident.name.as_str()
    {
        return Err(LakeCatError::Internal(
            "table record row scope does not match requested table".to_string(),
        ));
    }
    Ok(())
}

pub(crate) fn validate_idempotency_record_table_key(
    row_table_key: &str,
    ident: &TableIdent,
) -> LakeCatResult<()> {
    if row_table_key != table_key(ident) {
        return Err(LakeCatError::Internal(
            "idempotency record row scope does not match requested table".to_string(),
        ));
    }
    Ok(())
}

pub(crate) fn validate_idempotency_record_request_hash(
    row_request_hash: &str,
) -> LakeCatResult<()> {
    validate_idempotency_request_hash_shape(row_request_hash).map_err(|_| {
        LakeCatError::Internal(
            "idempotency record request hash must be full SHA-256 evidence".to_string(),
        )
    })
}

pub(crate) fn validate_table_commit_record_memory_scope(
    commit: &MemoryCommitRecord,
    ident: &TableIdent,
) -> LakeCatResult<()> {
    if commit.table_key != table_key(ident) {
        return Err(LakeCatError::Internal(
            "table commit record row scope does not match requested table".to_string(),
        ));
    }
    commit.record.validate_for_table(ident)
}

#[cfg(feature = "turso-local")]
pub(crate) fn validate_namespace_scope(
    namespace: &Namespace,
    expected_warehouse: &WarehouseName,
    row_warehouse: &WarehouseName,
    row_namespace_path: &str,
) -> LakeCatResult<()> {
    if row_warehouse != expected_warehouse || namespace.path() != row_namespace_path {
        return Err(LakeCatError::Internal(
            "namespace row scope does not match namespace identity".to_string(),
        ));
    }
    Ok(())
}

#[cfg(feature = "turso-local")]
pub(crate) fn validate_policy_binding_scope(
    binding: &PolicyBinding,
    warehouse: &WarehouseName,
    policy_id: &str,
    row_namespace_path: Option<&str>,
    row_table_name: Option<&str>,
    row_enforced: bool,
) -> LakeCatResult<()> {
    binding.validate()?;
    let binding_namespace_path = binding.namespace.as_ref().map(Namespace::path);
    let binding_table_name = binding.table.as_ref().map(TableName::as_str);
    if binding.warehouse != *warehouse
        || binding.policy_id != policy_id
        || binding_namespace_path.as_deref() != row_namespace_path
        || binding_table_name != row_table_name
        || binding.enforced != row_enforced
    {
        return Err(LakeCatError::Internal(
            "policy binding row scope does not match binding identity".to_string(),
        ));
    }
    Ok(())
}

pub(crate) fn validate_policy_binding_map_scope(
    binding: &PolicyBinding,
    binding_key: &str,
) -> LakeCatResult<()> {
    binding.validate()?;
    if policy_binding_key(&binding.warehouse, &binding.policy_id) != binding_key {
        return Err(LakeCatError::Internal(
            "policy binding row scope does not match binding identity".to_string(),
        ));
    }
    Ok(())
}

pub(crate) fn validate_storage_profile_map_scope(
    profile: &StorageProfile,
    profile_key: &str,
) -> LakeCatResult<()> {
    profile.validate()?;
    if storage_profile_key(&profile.warehouse, &profile.profile_id) != profile_key {
        return Err(LakeCatError::Internal(
            "storage profile row scope does not match profile identity".to_string(),
        ));
    }
    Ok(())
}

#[cfg(feature = "turso-local")]
pub(crate) fn validate_storage_profile_scope(
    profile: &StorageProfile,
    warehouse: &WarehouseName,
    profile_key: &str,
    profile_id: &str,
    row_location_prefix: &str,
    row_provider: &str,
    row_issuance_mode: &str,
) -> LakeCatResult<()> {
    profile.validate()?;
    if profile.warehouse != *warehouse
        || storage_profile_key(warehouse, profile_id) != profile_key
        || profile.profile_id != profile_id
        || profile.location_prefix != row_location_prefix
        || profile.provider.as_str() != row_provider
        || profile.issuance_mode.as_str() != row_issuance_mode
    {
        return Err(LakeCatError::Internal(
            "storage profile row scope does not match profile identity".to_string(),
        ));
    }
    Ok(())
}

#[cfg(feature = "turso-local")]
pub(crate) fn validate_server_record_scope(
    server: &ServerRecord,
    server_id: &str,
) -> LakeCatResult<()> {
    server.validate()?;
    if server.server_id != server_id {
        return Err(LakeCatError::Internal(
            "server row scope does not match server identity".to_string(),
        ));
    }
    Ok(())
}

pub(crate) fn validate_server_record_map_scope(
    server: &ServerRecord,
    server_id: &str,
) -> LakeCatResult<()> {
    server.validate()?;
    if server.server_id != server_id {
        return Err(LakeCatError::Internal(
            "server row scope does not match server identity".to_string(),
        ));
    }
    Ok(())
}

#[cfg(feature = "turso-local")]
pub(crate) fn validate_project_record_scope(
    project: &ProjectRecord,
    project_id: &str,
) -> LakeCatResult<()> {
    project.validate()?;
    if project.project_id != project_id {
        return Err(LakeCatError::Internal(
            "project row scope does not match project identity".to_string(),
        ));
    }
    Ok(())
}

pub(crate) fn validate_project_record_map_scope(
    project: &ProjectRecord,
    project_id: &str,
) -> LakeCatResult<()> {
    project.validate()?;
    if project.project_id != project_id {
        return Err(LakeCatError::Internal(
            "project row scope does not match project identity".to_string(),
        ));
    }
    Ok(())
}

#[cfg(feature = "turso-local")]
pub(crate) fn validate_warehouse_record_scope(
    record: &WarehouseRecord,
    warehouse: &WarehouseName,
    row_project_id: &str,
    row_storage_root: Option<&str>,
) -> LakeCatResult<()> {
    record.validate()?;
    if record.warehouse != *warehouse
        || record.project_id != row_project_id
        || record.storage_root.as_deref() != row_storage_root
    {
        return Err(LakeCatError::Internal(
            "warehouse row scope does not match warehouse identity".to_string(),
        ));
    }
    Ok(())
}

pub(crate) fn validate_warehouse_record_map_scope(
    record: &WarehouseRecord,
    warehouse_key: &str,
) -> LakeCatResult<()> {
    record.validate()?;
    if record.warehouse.as_str() != warehouse_key {
        return Err(LakeCatError::Internal(
            "warehouse row scope does not match warehouse identity".to_string(),
        ));
    }
    Ok(())
}

pub(crate) fn view_key(view: &ViewRecord) -> String {
    view_key_parts(&view.warehouse, &view.namespace, &view.name)
}

pub(crate) fn view_stable_id(view: &ViewRecord) -> String {
    format!(
        "lakecat:view:{}:{}:{}",
        view.warehouse, view.namespace, view.name
    )
}

pub(crate) fn view_key_parts(
    warehouse: &WarehouseName,
    namespace: &Namespace,
    name: &TableName,
) -> String {
    format!("{warehouse}\u{1f}{namespace}\u{1f}{name}")
}

pub(crate) fn audit_event_id(event: &CatalogAuditEvent) -> LakeCatResult<String> {
    content_hash_json(&serde_json::json!({
        "event-type": &event.event_type,
        "table": &event.table,
        "principal": &event.principal,
        "request-hash": &event.request_hash,
        "payload": &event.payload,
        "created-at": event.created_at.to_rfc3339(),
    }))
}

pub(crate) fn audit_outbox_payload(event_id: &str, event: &CatalogAuditEvent) -> Value {
    let mut payload = serde_json::Map::new();
    payload.insert(
        "audit-event-id".to_string(),
        Value::String(event_id.to_string()),
    );
    payload.insert(
        "event-type".to_string(),
        Value::String(event.event_type.clone()),
    );
    if let Some(table) = &event.table {
        payload.insert(
            "table".to_string(),
            serde_json::to_value(table).expect("table serializes"),
        );
    }
    payload.insert("payload".to_string(), event.payload.clone());
    Value::Object(payload)
}

pub(crate) fn validate_audit_payload_authorization_principal(
    payload: &Value,
    principal: &Principal,
) -> LakeCatResult<()> {
    let Some(receipt) = payload.get("authorization-receipt") else {
        return Ok(());
    };
    let action = receipt
        .get("action")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            LakeCatError::InvalidArgument(
                "audit event authorization receipt action is required".to_string(),
            )
        })?;
    if action.trim().is_empty() {
        return Err(LakeCatError::InvalidArgument(
            "audit event authorization receipt action must not be empty".to_string(),
        ));
    }
    let Some(receipt_principal) = receipt.get("principal") else {
        return Ok(());
    };
    let receipt_principal = serde_json::from_value::<Principal>(receipt_principal.clone())
        .map_err(|_| {
            LakeCatError::InvalidArgument(
                "audit event authorization receipt principal is malformed".to_string(),
            )
        })?;
    if &receipt_principal != principal {
        return Err(LakeCatError::InvalidArgument(
            "audit event authorization receipt principal does not match event principal"
                .to_string(),
        ));
    }
    Ok(())
}

pub(crate) fn validate_audit_payload_table_scope(
    payload: &Value,
    table: &TableIdent,
) -> LakeCatResult<()> {
    let Some(payload_table) = payload.get("table") else {
        return Err(LakeCatError::InvalidArgument(
            "audit event payload missing table scope".to_string(),
        ));
    };
    if payload_table.is_object() {
        let payload_table =
            serde_json::from_value::<TableIdent>(payload_table.clone()).map_err(|_| {
                LakeCatError::InvalidArgument(
                    "audit event payload table scope is malformed".to_string(),
                )
            })?;
        if &payload_table != table {
            return Err(LakeCatError::InvalidArgument(
                "audit event payload table scope does not match event table".to_string(),
            ));
        }
        return Ok(());
    }
    let Some(payload_table_name) = payload_table.as_str() else {
        return Err(LakeCatError::InvalidArgument(
            "audit event payload table scope is malformed".to_string(),
        ));
    };
    if payload_table_name != table.name.as_str() {
        return Err(LakeCatError::InvalidArgument(
            "audit event payload table scope does not match event table".to_string(),
        ));
    }
    let payload_warehouse = payload
        .get("warehouse")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            LakeCatError::InvalidArgument(
                "audit event payload missing warehouse scope for table".to_string(),
            )
        })?;
    if payload_warehouse != table.warehouse.as_str() {
        return Err(LakeCatError::InvalidArgument(
            "audit event payload warehouse scope does not match event table".to_string(),
        ));
    }
    let payload_namespace = payload.get("namespace").ok_or_else(|| {
        LakeCatError::InvalidArgument(
            "audit event payload missing namespace scope for table".to_string(),
        )
    })?;
    if !audit_payload_namespace_matches(payload_namespace, &table.namespace) {
        return Err(LakeCatError::InvalidArgument(
            "audit event payload namespace scope does not match event table".to_string(),
        ));
    }
    Ok(())
}

pub(crate) fn audit_payload_namespace_matches(
    payload_namespace: &Value,
    namespace: &Namespace,
) -> bool {
    if let Some(namespace_path) = payload_namespace.as_str() {
        return namespace_path == namespace.path();
    }
    let Some(parts) = payload_namespace.as_array() else {
        return false;
    };
    let payload_parts = parts.iter().filter_map(Value::as_str).collect::<Vec<_>>();
    payload_parts.len() == parts.len()
        && payload_parts
            .iter()
            .copied()
            .eq(namespace.parts().iter().map(String::as_str))
}

pub(crate) fn outbox_event_from_payload(
    payload: &Value,
    created_at: DateTime<Utc>,
) -> LakeCatResult<OutboxEvent> {
    let event_type = payload["event-type"]
        .as_str()
        .ok_or_else(|| LakeCatError::Internal("outbox payload missing event-type".to_string()))?;
    Ok(OutboxEvent {
        event_id: content_hash_json(payload)?,
        sink: "lakecat.lineage-and-graph".to_string(),
        event_type: event_type.to_string(),
        payload: payload.clone(),
        created_at,
        delivered_at: None,
    })
}

pub(crate) fn storage_profile_key(warehouse: &WarehouseName, profile_id: &str) -> String {
    format!("{}:{profile_id}", warehouse.as_str())
}

pub(crate) fn policy_binding_key(warehouse: &WarehouseName, policy_id: &str) -> String {
    format!("{}:{policy_id}", warehouse.as_str())
}

pub(crate) fn namespace_not_found(namespace: &Namespace) -> LakeCatError {
    LakeCatError::NotFound {
        object: "namespace",
        name: namespace.path(),
    }
}

pub(crate) fn namespace_not_empty(namespace: &Namespace, reason: &str) -> LakeCatError {
    LakeCatError::Conflict(format!(
        "cannot drop non-empty namespace {}: contains {reason}",
        namespace.path()
    ))
}

pub(crate) fn storage_profile_match<'a>(
    profiles: impl IntoIterator<Item = &'a StorageProfile>,
    table: &TableRecord,
) -> LakeCatResult<Option<StorageProfile>> {
    let mut best: Option<&StorageProfile> = None;
    for profile in profiles
        .into_iter()
        .filter(|profile| profile.warehouse == table.ident.warehouse)
        .filter(|profile| {
            location_matches_storage_profile_prefix(&table.location, &profile.location_prefix)
        })
    {
        let Some(current) = best else {
            best = Some(profile);
            continue;
        };
        match profile
            .location_prefix
            .len()
            .cmp(&current.location_prefix.len())
        {
            std::cmp::Ordering::Greater => best = Some(profile),
            std::cmp::Ordering::Equal => {
                return Err(LakeCatError::InvalidArgument(format!(
                    "ambiguous storage profile match for {}; location-prefix-hash={}; profile-ids={},{}",
                    table.ident.stable_id(),
                    content_hash_bytes(profile.location_prefix.as_bytes()),
                    current.profile_id,
                    profile.profile_id
                )));
            }
            std::cmp::Ordering::Less => {}
        }
    }
    Ok(best.cloned())
}

pub(crate) fn location_matches_storage_profile_prefix(location: &str, prefix: &str) -> bool {
    if location == prefix {
        return true;
    }
    if prefix.ends_with('/') {
        return location.starts_with(prefix);
    }
    location
        .strip_prefix(prefix)
        .is_some_and(|suffix| suffix.starts_with('/'))
}

pub(crate) fn validate_profile_id(profile_id: &str) -> LakeCatResult<()> {
    if profile_id.is_empty() {
        return Err(LakeCatError::InvalidArgument(
            "storage profile id must not be empty".to_string(),
        ));
    }
    if !profile_id
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.'))
    {
        return Err(LakeCatError::InvalidArgument(format!(
            "storage profile id contains unsupported characters; {}",
            storage_profile_id_hash_context(profile_id)
        )));
    }
    Ok(())
}

pub(crate) fn validate_location_prefix_provider(
    location_prefix: &str,
    provider: StorageProvider,
) -> LakeCatResult<()> {
    let detected = StorageProvider::from_location(location_prefix);
    if detected != StorageProvider::Unknown && detected != provider {
        return Err(LakeCatError::InvalidArgument(format!(
            "storage profile provider '{}' does not match location prefix provider '{}'; {}",
            provider.as_str(),
            detected.as_str(),
            storage_profile_prefix_hash_context(location_prefix)
        )));
    }
    if detected == StorageProvider::Unknown && provider != StorageProvider::Unknown {
        return Err(LakeCatError::InvalidArgument(format!(
            "storage profile location prefix is not supported by provider '{}'; {}",
            provider.as_str(),
            storage_profile_prefix_hash_context(location_prefix)
        )));
    }
    Ok(())
}

pub(crate) fn validate_location_prefix_path(location_prefix: &str) -> LakeCatResult<()> {
    if location_prefix_has_query_fragment_or_userinfo(location_prefix) {
        return Err(LakeCatError::InvalidArgument(format!(
            "storage profile location prefix must not include query strings, fragments, or userinfo; {}",
            storage_profile_prefix_hash_context(location_prefix)
        )));
    }
    if location_prefix_has_dot_path_segment(location_prefix) {
        return Err(LakeCatError::InvalidArgument(format!(
            "storage profile location prefix must not include dot path segments; {}",
            storage_profile_prefix_hash_context(location_prefix)
        )));
    }
    Ok(())
}

pub(crate) fn validate_warehouse_storage_root_path(storage_root: &str) -> LakeCatResult<()> {
    if location_has_query_fragment_or_userinfo(storage_root) {
        return Err(LakeCatError::InvalidArgument(format!(
            "warehouse storage root must not include query strings, fragments, or userinfo; {}",
            warehouse_storage_root_hash_context(storage_root)
        )));
    }
    if location_has_dot_path_segment(storage_root) {
        return Err(LakeCatError::InvalidArgument(format!(
            "warehouse storage root must not include dot path segments; {}",
            warehouse_storage_root_hash_context(storage_root)
        )));
    }
    Ok(())
}

pub(crate) fn validate_server_endpoint_url(endpoint_url: &str) -> LakeCatResult<()> {
    let url = Url::parse(endpoint_url).map_err(|_| {
        LakeCatError::InvalidArgument(format!(
            "server endpoint URL must be an absolute http or https URL; {}",
            server_endpoint_url_hash_context(endpoint_url)
        ))
    })?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(LakeCatError::InvalidArgument(format!(
            "server endpoint URL must use http or https scheme; {}",
            server_endpoint_url_hash_context(endpoint_url)
        )));
    }
    if location_has_query_fragment_or_userinfo(endpoint_url) {
        return Err(LakeCatError::InvalidArgument(format!(
            "server endpoint URL must not include query strings, fragments, or userinfo; {}",
            server_endpoint_url_hash_context(endpoint_url)
        )));
    }
    Ok(())
}

pub(crate) fn location_prefix_has_query_fragment_or_userinfo(location_prefix: &str) -> bool {
    location_has_query_fragment_or_userinfo(location_prefix)
}

pub(crate) fn location_has_query_fragment_or_userinfo(location: &str) -> bool {
    Url::parse(location).is_ok_and(|url| {
        url.query().is_some()
            || url.fragment().is_some()
            || !url.username().is_empty()
            || url.password().is_some()
    }) || location.contains(['?', '#'])
}

pub(crate) fn location_prefix_has_dot_path_segment(location_prefix: &str) -> bool {
    location_has_dot_path_segment(location_prefix)
}

pub(crate) fn location_has_dot_path_segment(location: &str) -> bool {
    let path = location
        .split_once(['?', '#'])
        .map_or(location, |(path, _)| path);
    path.split('/').any(is_dot_path_segment)
}

pub(crate) fn validate_issuance_mode_provider(
    issuance_mode: CredentialIssuanceMode,
    provider: StorageProvider,
    location_prefix: &str,
) -> LakeCatResult<()> {
    match issuance_mode {
        CredentialIssuanceMode::LocalFileNoSecret if provider != StorageProvider::File => {
            Err(LakeCatError::InvalidArgument(format!(
                "local-file-no-secret issuance mode requires file provider, got '{}'; {}",
                provider.as_str(),
                storage_profile_prefix_hash_context(location_prefix)
            )))
        }
        CredentialIssuanceMode::ShortLivedSecretRef
            if !matches!(
                provider,
                StorageProvider::S3 | StorageProvider::Gcs | StorageProvider::Azure
            ) =>
        {
            Err(LakeCatError::InvalidArgument(format!(
                "short-lived-secret-ref issuance mode requires s3, gcs, or azure provider, got '{}'; {}",
                provider.as_str(),
                storage_profile_prefix_hash_context(location_prefix)
            )))
        }
        _ => Ok(()),
    }
}

pub(crate) fn validate_secret_ref_issuance_mode(
    secret_ref: Option<&str>,
    issuance_mode: CredentialIssuanceMode,
) -> LakeCatResult<()> {
    if let Some(secret_ref) = secret_ref
        && !matches!(issuance_mode, CredentialIssuanceMode::ShortLivedSecretRef)
    {
        return Err(LakeCatError::InvalidArgument(format!(
            "storage profile secret reference requires short-lived-secret-ref issuance mode; {}",
            secret_ref_hash_context(secret_ref)
        )));
    }
    Ok(())
}

pub(crate) fn validate_project_id(project_id: &str) -> LakeCatResult<()> {
    if project_id.is_empty() {
        return Err(LakeCatError::InvalidArgument(
            "project id must not be empty".to_string(),
        ));
    }
    if !project_id
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.'))
    {
        return Err(LakeCatError::InvalidArgument(format!(
            "project id contains unsupported characters; {}",
            project_id_hash_context(project_id)
        )));
    }
    Ok(())
}

pub(crate) fn validate_public_config(config: &BTreeMap<String, String>) -> LakeCatResult<()> {
    for (key, value) in config {
        let normalized = key.to_ascii_lowercase();
        if normalized.contains("secret")
            || normalized.contains("token")
            || normalized.contains("password")
            || normalized.contains("credential")
        {
            return Err(LakeCatError::InvalidArgument(format!(
                "storage profile public config key may expose secret material; {}",
                public_config_key_hash_context(key)
            )));
        }
        if embeds_raw_secret_material(value) {
            return Err(LakeCatError::InvalidArgument(format!(
                "storage profile public config value may expose secret material; {}",
                public_config_key_hash_context(key)
            )));
        }
    }
    Ok(())
}

pub(crate) fn validate_storage_profile_public_config(
    config: &BTreeMap<String, String>,
) -> LakeCatResult<()> {
    validate_public_config(config)?;
    for key in config.keys() {
        let normalized = key.to_ascii_lowercase();
        if RESERVED_STORAGE_PROFILE_PUBLIC_CONFIG_KEYS.contains(&normalized.as_str()) {
            return Err(LakeCatError::InvalidArgument(format!(
                "storage profile public config key is reserved for LakeCat credential evidence; {}",
                public_config_key_hash_context(key)
            )));
        }
    }
    Ok(())
}

pub(crate) fn public_config_key_hash_context(key: &str) -> String {
    format!(
        "public-config-key-hash={}",
        content_hash_bytes(key.as_bytes())
    )
}

pub(crate) const RESERVED_STORAGE_PROFILE_PUBLIC_CONFIG_KEYS: &[&str] = &[
    "lakecat.storage-profile-id",
    "lakecat.storage-provider",
    "lakecat.credential-mode",
    "lakecat.governed-read-required",
    "lakecat.authorization-principal",
    "lakecat.max-credential-ttl-seconds",
    "lakecat.credential-kind",
];

pub(crate) fn embeds_raw_secret_material(value: &str) -> bool {
    let normalized = value.to_ascii_lowercase();
    let embedded_secret_patterns = [
        "password=",
        "secret=",
        "token=",
        "credential=",
        "api_key=",
        "apikey=",
        "access_key=",
        "private_key=",
        "pass=",
        "auth=",
        "aws_access_key_id=",
        "aws_secret_access_key=",
        "aws_session_token=",
    ];
    embedded_secret_patterns
        .iter()
        .any(|pattern| normalized.contains(pattern))
}

pub(crate) fn validate_secret_ref(secret_ref: &str) -> LakeCatResult<()> {
    let trimmed = secret_ref.trim();
    if trimmed.is_empty() {
        return Err(LakeCatError::InvalidArgument(
            "storage profile secret reference must not be empty".to_string(),
        ));
    }
    let parsed = Url::parse(trimmed).map_err(|_err| {
        LakeCatError::InvalidArgument(format!(
            "storage profile secret reference must be a valid external secret-store URI; {}",
            secret_ref_hash_context(trimmed)
        ))
    })?;
    if !matches!(
        parsed.scheme(),
        "typesec" | "vault" | "aws-sm" | "gcp-sm" | "azure-kv"
    ) {
        return Err(LakeCatError::InvalidArgument(format!(
            "storage profile secret reference must use an external secret-store URI; {}",
            secret_ref_hash_context(trimmed)
        )));
    }
    if parsed.query().is_some() || parsed.fragment().is_some() || !parsed.username().is_empty() {
        return Err(LakeCatError::InvalidArgument(format!(
            "storage profile secret reference must not include query strings, fragments, or userinfo; {}",
            secret_ref_hash_context(trimmed)
        )));
    }
    if parsed.password().is_some() {
        return Err(LakeCatError::InvalidArgument(format!(
            "storage profile secret reference must not include query strings, fragments, or userinfo; {}",
            secret_ref_hash_context(trimmed)
        )));
    }
    if secret_ref_has_dot_path_segment(trimmed) {
        return Err(LakeCatError::InvalidArgument(format!(
            "storage profile secret reference must not include dot path segments; {}",
            secret_ref_hash_context(trimmed)
        )));
    }
    if embeds_raw_secret_material(trimmed) {
        return Err(LakeCatError::InvalidArgument(format!(
            "storage profile secret reference must not embed raw secret material; {}",
            secret_ref_hash_context(trimmed)
        )));
    }
    Ok(())
}

pub(crate) fn secret_ref_has_dot_path_segment(secret_ref: &str) -> bool {
    let path = secret_ref
        .split_once(['?', '#'])
        .map_or(secret_ref, |(path, _)| path);
    path.split('/').any(is_dot_path_segment)
}

pub(crate) fn is_dot_path_segment(segment: &str) -> bool {
    let Some(decoded) = percent_decode_segment(segment) else {
        return segment == "." || segment == "..";
    };
    decoded.as_slice() == b"." || decoded.as_slice() == b".."
}

pub(crate) fn percent_decode_segment(segment: &str) -> Option<Vec<u8>> {
    if !segment.as_bytes().contains(&b'%') {
        return None;
    }
    let mut decoded = Vec::with_capacity(segment.len());
    let bytes = segment.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            let Some(high) = hex_value(bytes[index + 1]) else {
                decoded.push(bytes[index]);
                index += 1;
                continue;
            };
            let Some(low) = hex_value(bytes[index + 2]) else {
                decoded.push(bytes[index]);
                index += 1;
                continue;
            };
            decoded.push((high << 4) | low);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    Some(decoded)
}

pub(crate) fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

pub(crate) fn secret_ref_hash_context(secret_ref: &str) -> String {
    format!(
        "secret-ref-hash={}",
        content_hash_bytes(secret_ref.as_bytes())
    )
}

pub(crate) fn storage_profile_prefix_hash_context(location_prefix: &str) -> String {
    format!(
        "storage-profile-prefix-hash={}",
        content_hash_bytes(location_prefix.as_bytes())
    )
}

pub(crate) fn warehouse_storage_root_hash_context(storage_root: &str) -> String {
    format!(
        "warehouse-storage-root-hash={}",
        content_hash_bytes(storage_root.as_bytes())
    )
}

pub(crate) fn server_endpoint_url_hash_context(endpoint_url: &str) -> String {
    format!(
        "server-endpoint-url-hash={}",
        content_hash_bytes(endpoint_url.as_bytes())
    )
}

pub(crate) fn project_id_hash_context(project_id: &str) -> String {
    format!(
        "project-id-hash={}",
        content_hash_bytes(project_id.as_bytes())
    )
}

pub(crate) fn storage_profile_id_hash_context(profile_id: &str) -> String {
    format!(
        "storage-profile-id-hash={}",
        content_hash_bytes(profile_id.as_bytes())
    )
}

pub(crate) fn validate_policy_id(policy_id: &str) -> LakeCatResult<()> {
    if policy_id.is_empty() {
        return Err(LakeCatError::InvalidArgument(
            "policy id must not be empty".to_string(),
        ));
    }
    if !policy_id
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.'))
    {
        return Err(LakeCatError::InvalidArgument(format!(
            "policy id contains unsupported characters; {}",
            policy_id_hash_context(policy_id)
        )));
    }
    Ok(())
}

pub(crate) fn policy_id_hash_context(policy_id: &str) -> String {
    format!(
        "policy-id-hash={}",
        content_hash_bytes(policy_id.as_bytes())
    )
}

pub(crate) fn policy_bindings_for_table<'a>(
    bindings: impl IntoIterator<Item = &'a PolicyBinding>,
    table: &TableIdent,
) -> Vec<PolicyBinding> {
    let mut bindings = bindings
        .into_iter()
        .filter(|binding| binding.enforced && binding.applies_to_table(table))
        .cloned()
        .collect::<Vec<_>>();
    bindings.sort_by(|left, right| left.policy_id.cmp(&right.policy_id));
    bindings
}
