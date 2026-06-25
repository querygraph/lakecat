use std::collections::{BTreeMap, BTreeSet};

use lakecat_api::{
    LAKECAT_COMPATIBILITY_KEY, LAKECAT_COMPATIBILITY_VALUE, LAKECAT_FORMAT_BASELINE_KEY,
    LAKECAT_FORMAT_BASELINE_VALUE, LAKECAT_FORMAT_V4_BRIDGE_KEY, LAKECAT_FORMAT_V4_BRIDGE_VALUE,
    LAKECAT_FORMAT_V4_KEY, LAKECAT_FORMAT_V4_TYPED_SAIL_KEY, LAKECAT_FORMAT_V4_TYPED_SAIL_VALUE,
    LAKECAT_FORMAT_V4_VALUE,
};
use lakecat_core::{
    LakeCatError, Namespace, Principal, TableName, WarehouseName, content_hash_bytes,
    content_hash_json,
};
#[cfg(feature = "sail-local")]
use lakecat_sail::catalog_provider::{
    LakeCatCatalogProvider, ProviderFetchScanTasksRequest, ProviderScanPlanningRequest,
};
use lakecat_store::{
    CredentialIssuanceMode, OutboxEvent, PolicyBinding, ProjectRecord, ServerRecord,
    StorageProfile, StorageProvider, WarehouseRecord,
};
use serde_json::{Value, json};

use crate::*;

pub(crate) fn validate_storage_profile_upsert_event_evidence(
    event: &OutboxEvent,
    payload: &Value,
) -> Result<(), LakeCatError> {
    if event.payload.get("payload").is_some() {
        validate_object_evidence_schema(
            event,
            &event.payload,
            "storage-profile upsert outbox payload",
            STORAGE_PROFILE_UPSERT_OUTBOX_PAYLOAD_FIELDS,
        )?;
    }
    validate_object_evidence_schema(
        event,
        payload,
        "storage-profile upsert",
        STORAGE_PROFILE_UPSERT_EVIDENCE_FIELDS,
    )?;
    let Some(storage_profile) = payload.get("storage-profile") else {
        return Err(outbox_evidence_error(
            event,
            "storage-profile upsert evidence must contain storage-profile",
        ));
    };
    required_string_field(
        event,
        storage_profile,
        "profile-id",
        "storage-profile upsert",
    )
    .and_then(|profile_id| {
        validate_storage_profile_id_evidence(event, profile_id, "storage-profile upsert")?;
        Ok(profile_id)
    })?;
    let warehouse_name = required_string_field(
        event,
        storage_profile,
        "warehouse",
        "storage-profile upsert",
    )?;
    WarehouseName::new(warehouse_name).map_err(|_| {
        outbox_evidence_error(
            event,
            "storage-profile upsert evidence has invalid warehouse",
        )
    })?;
    if let Some(payload_warehouse) = payload
        .get("warehouse")
        .and_then(Value::as_str)
        .filter(|warehouse| !warehouse.is_empty())
    {
        if payload_warehouse != warehouse_name {
            return Err(outbox_evidence_error(
                event,
                "storage-profile upsert warehouse must match storage-profile",
            ));
        }
    }
    validate_storage_profile_provider_mode_evidence(
        event,
        storage_profile,
        "storage-profile upsert",
    )?;
    if storage_profile.get("secret-ref").is_some() {
        return Err(outbox_evidence_error(
            event,
            "storage-profile upsert evidence must not contain raw secret-ref",
        ));
    }
    if storage_profile.get("location-prefix").is_some() {
        return Err(outbox_evidence_error(
            event,
            "storage-profile upsert evidence must not contain raw location-prefix",
        ));
    }
    validate_storage_profile_evidence_schema(
        event,
        storage_profile,
        "storage-profile upsert storage-profile",
    )?;
    validate_storage_profile_public_config_evidence(
        event,
        storage_profile,
        "storage-profile upsert storage-profile",
    )?;
    validate_required_full_hash_field(event, storage_profile, "location-prefix-hash")?;
    validate_secret_ref_evidence(event, storage_profile, "storage-profile upsert")?;
    validate_authorization_receipt_principal(event, payload, "storage-profile upsert")?;
    Ok(())
}

pub(crate) fn validate_policy_binding_upsert_event_evidence(
    event: &OutboxEvent,
    payload: &Value,
) -> Result<(), LakeCatError> {
    if event.payload.get("payload").is_some() {
        validate_object_evidence_schema(
            event,
            &event.payload,
            "policy-binding upsert outbox payload",
            MANAGEMENT_UPSERT_OUTBOX_PAYLOAD_FIELDS,
        )?;
    }
    validate_object_evidence_schema(
        event,
        payload,
        "policy-binding upsert",
        POLICY_BINDING_UPSERT_EVIDENCE_FIELDS,
    )?;
    let Some(policy) = payload.get("policy") else {
        return Err(outbox_evidence_error(
            event,
            "policy-binding upsert evidence must contain policy",
        ));
    };
    validate_policy_binding_evidence_schema(event, policy)?;
    let Some(policy_id) = policy
        .get("policy-id")
        .and_then(Value::as_str)
        .filter(|policy_id| !policy_id.is_empty())
    else {
        return Err(outbox_evidence_error(
            event,
            "policy-binding upsert evidence must contain policy-id",
        ));
    };
    validate_management_id_evidence(event, policy_id, "policy-binding upsert", "policy-id")?;
    let Some(warehouse_name) = policy
        .get("warehouse")
        .or_else(|| payload.get("warehouse"))
        .and_then(Value::as_str)
        .filter(|warehouse| !warehouse.is_empty())
    else {
        return Err(outbox_evidence_error(
            event,
            "policy-binding upsert evidence must contain warehouse",
        ));
    };
    if let Some(payload_warehouse) = payload.get("warehouse").and_then(Value::as_str) {
        if policy.get("warehouse").and_then(Value::as_str) != Some(payload_warehouse) {
            return Err(outbox_evidence_error(
                event,
                "policy-binding upsert policy warehouse must match payload warehouse",
            ));
        }
    }
    let warehouse = WarehouseName::new(warehouse_name).map_err(|_| {
        outbox_evidence_error(
            event,
            "policy-binding upsert evidence has invalid warehouse",
        )
    })?;
    let namespace = match policy.get("namespace") {
        Some(Value::Array(parts)) => {
            let parts = parts
                .iter()
                .map(|part| {
                    part.as_str()
                        .filter(|part| !part.is_empty())
                        .map(ToString::to_string)
                        .ok_or_else(|| {
                            outbox_evidence_error(
                                event,
                                "policy-binding upsert namespace components must be non-empty strings",
                            )
                        })
                })
                .collect::<Result<Vec<_>, _>>()?;
            Some(Namespace::new(parts).map_err(|_| {
                outbox_evidence_error(
                    event,
                    "policy-binding upsert evidence has invalid namespace",
                )
            })?)
        }
        Some(Value::Null) | None => None,
        _ => {
            return Err(outbox_evidence_error(
                event,
                "policy-binding upsert namespace must be an array when present",
            ));
        }
    };
    let table = match policy.get("table") {
        Some(Value::String(table)) if !table.is_empty() => {
            Some(TableName::new(table).map_err(|_| {
                outbox_evidence_error(event, "policy-binding upsert evidence has invalid table")
            })?)
        }
        Some(Value::Null) | None => None,
        _ => {
            return Err(outbox_evidence_error(
                event,
                "policy-binding upsert table must be a non-empty string when present",
            ));
        }
    };
    let Some(enforced) = policy.get("enforced").and_then(Value::as_bool) else {
        return Err(outbox_evidence_error(
            event,
            "policy-binding upsert evidence must contain enforced",
        ));
    };
    let Some(odrl) = policy.get("odrl") else {
        return Err(outbox_evidence_error(
            event,
            "policy-binding upsert evidence must contain odrl",
        ));
    };
    let odrl_hash = content_hash_json(odrl)
        .map_err(|_| outbox_evidence_error(event, "policy-binding upsert odrl must be hashable"))?;
    let Some(recorded_odrl_hash) = policy
        .get("odrl-hash")
        .and_then(Value::as_str)
        .filter(|hash| !hash.is_empty())
    else {
        return Err(outbox_evidence_error(
            event,
            "policy-binding upsert evidence must contain odrl-hash",
        ));
    };
    validate_required_full_hash_field(event, policy, "odrl-hash")?;
    if recorded_odrl_hash != odrl_hash {
        return Err(outbox_evidence_error(
            event,
            "policy-binding upsert odrl-hash must match odrl",
        ));
    }
    PolicyBinding::new(
        policy_id,
        warehouse,
        namespace,
        table,
        enforced,
        odrl.clone(),
    )
    .map_err(|_| {
        outbox_evidence_error(
            event,
            "policy-binding upsert evidence has invalid scope or identifier",
        )
    })?;
    validate_authorization_receipt_principal(event, payload, "policy-binding upsert")?;
    Ok(())
}

pub(crate) fn validate_project_upsert_event_evidence(
    event: &OutboxEvent,
    payload: &Value,
) -> Result<(), LakeCatError> {
    if event.payload.get("payload").is_some() {
        validate_object_evidence_schema(
            event,
            &event.payload,
            "project upsert outbox payload",
            MANAGEMENT_UPSERT_OUTBOX_PAYLOAD_FIELDS,
        )?;
    }
    validate_object_evidence_schema(
        event,
        payload,
        "project upsert",
        PROJECT_UPSERT_EVIDENCE_FIELDS,
    )?;
    let Some(project_record) = payload.get("project-record") else {
        return Err(outbox_evidence_error(
            event,
            "project upsert evidence must contain project-record",
        ));
    };
    validate_object_evidence_schema(
        event,
        project_record,
        "project upsert project-record",
        PROJECT_RECORD_EVIDENCE_FIELDS,
    )?;
    let Some(project_id) = project_record
        .get("project-id")
        .and_then(Value::as_str)
        .filter(|project_id| !project_id.is_empty())
    else {
        return Err(outbox_evidence_error(
            event,
            "project upsert evidence must contain project-id",
        ));
    };
    validate_management_id_evidence(event, project_id, "project upsert", "project-id")?;
    if let Some(payload_project_id) = payload.get("project-id").and_then(Value::as_str) {
        if payload_project_id != project_id {
            return Err(outbox_evidence_error(
                event,
                "project upsert project-id must match project-record",
            ));
        }
    }
    let server_id = optional_string_field(event, project_record, "server-id", "project upsert")?;
    if let Some(server_id) = server_id.as_deref() {
        validate_management_id_evidence(event, server_id, "project upsert", "server-id")?;
    }
    let display_name =
        optional_string_field(event, project_record, "display-name", "project upsert")?;
    let properties =
        optional_string_map_field(event, project_record, "properties", "project upsert")?;
    ProjectRecord::new(
        project_id,
        server_id,
        display_name,
        properties,
        Principal::anonymous(),
    )
    .map_err(|_| {
        outbox_evidence_error(
            event,
            "project upsert evidence has invalid identifier, server scope, or properties",
        )
    })?;
    validate_authorization_receipt_principal(event, payload, "project upsert")?;
    Ok(())
}

pub(crate) fn validate_server_upsert_event_evidence(
    event: &OutboxEvent,
    payload: &Value,
) -> Result<(), LakeCatError> {
    if event.payload.get("payload").is_some() {
        validate_object_evidence_schema(
            event,
            &event.payload,
            "server upsert outbox payload",
            MANAGEMENT_UPSERT_OUTBOX_PAYLOAD_FIELDS,
        )?;
    }
    validate_object_evidence_schema(
        event,
        payload,
        "server upsert",
        SERVER_UPSERT_EVIDENCE_FIELDS,
    )?;
    let Some(server_record) = payload.get("server-record") else {
        return Err(outbox_evidence_error(
            event,
            "server upsert evidence must contain server-record",
        ));
    };
    validate_object_evidence_schema(
        event,
        server_record,
        "server upsert server-record",
        SERVER_RECORD_EVIDENCE_FIELDS,
    )?;
    let Some(server_id) = server_record
        .get("server-id")
        .and_then(Value::as_str)
        .filter(|server_id| !server_id.is_empty())
    else {
        return Err(outbox_evidence_error(
            event,
            "server upsert evidence must contain server-id",
        ));
    };
    validate_management_id_evidence(event, server_id, "server upsert", "server-id")?;
    if let Some(payload_server_id) = payload.get("server-id").and_then(Value::as_str) {
        if payload_server_id != server_id {
            return Err(outbox_evidence_error(
                event,
                "server upsert server-id must match server-record",
            ));
        }
    }
    let endpoint_url =
        optional_string_field(event, server_record, "endpoint-url", "server upsert")?;
    if endpoint_url.is_some() {
        validate_required_full_hash_field(event, server_record, "endpoint-url-hash")?;
        validate_string_content_hash_matches_field(
            event,
            server_record,
            "endpoint-url",
            "endpoint-url-hash",
            "server upsert",
        )?;
    } else {
        validate_optional_full_hash_field(event, server_record, "endpoint-url-hash")?;
    }
    let display_name =
        optional_string_field(event, server_record, "display-name", "server upsert")?;
    let properties =
        optional_string_map_field(event, server_record, "properties", "server upsert")?;
    ServerRecord::new(
        server_id,
        display_name,
        endpoint_url,
        properties,
        Principal::anonymous(),
    )
    .map_err(|_| {
        outbox_evidence_error(
            event,
            "server upsert evidence has invalid endpoint, properties, or identifier",
        )
    })?;
    validate_authorization_receipt_principal(event, payload, "server upsert")?;
    Ok(())
}

pub(crate) fn validate_warehouse_upsert_event_evidence(
    event: &OutboxEvent,
    payload: &Value,
) -> Result<(), LakeCatError> {
    if event.payload.get("payload").is_some() {
        validate_object_evidence_schema(
            event,
            &event.payload,
            "warehouse upsert outbox payload",
            MANAGEMENT_UPSERT_OUTBOX_PAYLOAD_FIELDS,
        )?;
    }
    validate_object_evidence_schema(
        event,
        payload,
        "warehouse upsert",
        WAREHOUSE_UPSERT_EVIDENCE_FIELDS,
    )?;
    let Some(warehouse_record) = payload.get("warehouse-record") else {
        return Err(outbox_evidence_error(
            event,
            "warehouse upsert evidence must contain warehouse-record",
        ));
    };
    validate_object_evidence_schema(
        event,
        warehouse_record,
        "warehouse upsert warehouse-record",
        WAREHOUSE_RECORD_EVIDENCE_FIELDS,
    )?;
    let Some(warehouse_name) = warehouse_record
        .get("warehouse")
        .or_else(|| payload.get("warehouse"))
        .and_then(Value::as_str)
        .filter(|warehouse| !warehouse.is_empty())
    else {
        return Err(outbox_evidence_error(
            event,
            "warehouse upsert evidence must contain warehouse",
        ));
    };
    if let Some(payload_warehouse) = payload.get("warehouse").and_then(Value::as_str) {
        if warehouse_record.get("warehouse").and_then(Value::as_str) != Some(payload_warehouse) {
            return Err(outbox_evidence_error(
                event,
                "warehouse upsert warehouse must match warehouse-record",
            ));
        }
    }
    let warehouse = WarehouseName::new(warehouse_name).map_err(|_| {
        outbox_evidence_error(event, "warehouse upsert evidence has invalid warehouse")
    })?;
    let Some(project_id) = warehouse_record
        .get("project-id")
        .and_then(Value::as_str)
        .filter(|project_id| !project_id.is_empty())
    else {
        return Err(outbox_evidence_error(
            event,
            "warehouse upsert evidence must contain project-id",
        ));
    };
    validate_management_id_evidence(event, project_id, "warehouse upsert", "project-id")?;
    let storage_root =
        optional_string_field(event, warehouse_record, "storage-root", "warehouse upsert")?;
    if storage_root.is_some() {
        validate_required_full_hash_field(event, warehouse_record, "storage-root-hash")?;
        validate_string_content_hash_matches_field(
            event,
            warehouse_record,
            "storage-root",
            "storage-root-hash",
            "warehouse upsert",
        )?;
    } else {
        validate_optional_full_hash_field(event, warehouse_record, "storage-root-hash")?;
    }
    let properties =
        optional_string_map_field(event, warehouse_record, "properties", "warehouse upsert")?;
    WarehouseRecord::new(
        warehouse,
        project_id,
        storage_root,
        properties,
        Principal::anonymous(),
    )
    .map_err(|_| {
        outbox_evidence_error(
            event,
            "warehouse upsert evidence has invalid storage root, properties, or scope",
        )
    })?;
    validate_authorization_receipt_principal(event, payload, "warehouse upsert")?;
    Ok(())
}

pub(crate) fn validate_namespace_lifecycle_event_evidence(
    event: &OutboxEvent,
    payload: &Value,
) -> Result<(), LakeCatError> {
    if event.payload.get("payload").is_some() {
        validate_object_evidence_schema(
            event,
            &event.payload,
            "namespace lifecycle outbox payload",
            NAMESPACE_LIFECYCLE_OUTBOX_PAYLOAD_FIELDS,
        )?;
    }
    validate_object_evidence_schema(
        event,
        payload,
        "namespace lifecycle",
        NAMESPACE_LIFECYCLE_EVIDENCE_FIELDS,
    )?;
    validate_required_warehouse_field(event, payload, "namespace lifecycle")?;
    let Some(namespace) = payload.get("namespace") else {
        return Err(outbox_evidence_error(
            event,
            "namespace lifecycle evidence must contain namespace",
        ));
    };
    validate_namespace_lifecycle_value(event, namespace)?;
    validate_authorization_receipt_principal(event, payload, "namespace lifecycle")?;
    Ok(())
}

pub(crate) fn validate_catalog_config_read_event_evidence(
    event: &OutboxEvent,
    payload: &Value,
) -> Result<(), LakeCatError> {
    if event.payload.get("payload").is_some() {
        validate_object_evidence_schema(
            event,
            &event.payload,
            "catalog config-read outbox payload",
            LIST_OUTBOX_PAYLOAD_FIELDS,
        )?;
    }
    validate_object_evidence_schema(
        event,
        payload,
        "catalog config-read",
        CATALOG_CONFIG_READ_EVIDENCE_FIELDS,
    )?;
    validate_required_warehouse_field(event, payload, "catalog config-read")?;
    validate_catalog_config_defaults(event, payload)?;
    validate_catalog_config_overrides(event, payload)?;
    validate_authorization_receipt_principal(event, payload, "catalog config-read")?;
    validate_catalog_config_endpoints(event, payload)?;
    validate_catalog_config_tenant_records(event, payload)?;
    Ok(())
}

pub(crate) fn validate_catalog_config_tenant_records(
    event: &OutboxEvent,
    payload: &Value,
) -> Result<(), LakeCatError> {
    if let Some(warehouse_record) = payload.get("warehouse-record") {
        validate_object_evidence_schema(
            event,
            warehouse_record,
            "catalog config-read warehouse-record",
            WAREHOUSE_RECORD_EVIDENCE_FIELDS,
        )?;
        validate_sensitive_record_hash_binding(
            event,
            warehouse_record,
            "storage-root",
            "storage-root-hash",
            "catalog config-read warehouse-record",
        )?;
    }
    if let Some(project_record) = payload.get("project-record") {
        validate_object_evidence_schema(
            event,
            project_record,
            "catalog config-read project-record",
            PROJECT_RECORD_EVIDENCE_FIELDS,
        )?;
    }
    if let Some(server_record) = payload.get("server-record") {
        validate_object_evidence_schema(
            event,
            server_record,
            "catalog config-read server-record",
            SERVER_RECORD_EVIDENCE_FIELDS,
        )?;
        validate_sensitive_record_hash_binding(
            event,
            server_record,
            "endpoint-url",
            "endpoint-url-hash",
            "catalog config-read server-record",
        )?;
    }
    Ok(())
}

pub(crate) fn validate_sensitive_record_hash_binding(
    event: &OutboxEvent,
    record: &Value,
    value_field: &str,
    hash_field: &str,
    label: &str,
) -> Result<(), LakeCatError> {
    if record.get(value_field).is_some() {
        validate_required_full_hash_field(event, record, hash_field)?;
        validate_string_content_hash_matches_field(event, record, value_field, hash_field, label)?;
    } else {
        validate_optional_full_hash_field(event, record, hash_field)?;
    }
    Ok(())
}

pub(crate) fn validate_catalog_config_entries(
    event: &OutboxEvent,
    entries: &Value,
    label: &str,
) -> Result<BTreeSet<String>, LakeCatError> {
    let Some(entries) = entries.as_array() else {
        return Err(outbox_evidence_error(
            event,
            &format!("{label} must be an array"),
        ));
    };
    let mut keys = BTreeSet::new();
    for entry in entries {
        validate_object_evidence_schema(event, entry, label, CATALOG_CONFIG_ENTRY_EVIDENCE_FIELDS)?;
        let Some(key) = entry
            .get("key")
            .and_then(Value::as_str)
            .filter(|key| !key.is_empty())
        else {
            return Err(outbox_evidence_error(
                event,
                &format!("{label} must contain non-empty string keys"),
            ));
        };
        if entry.get("value").and_then(Value::as_str).is_none() {
            return Err(outbox_evidence_error(
                event,
                &format!("{label} must contain string values"),
            ));
        }
        if !keys.insert(key.to_string()) {
            return Err(outbox_evidence_error(
                event,
                &format!("{label} must not contain duplicate keys"),
            ));
        }
    }
    Ok(keys)
}

pub(crate) fn validate_catalog_config_defaults(
    event: &OutboxEvent,
    payload: &Value,
) -> Result<(), LakeCatError> {
    let Some(defaults) = payload.get("defaults") else {
        return Err(outbox_evidence_error(
            event,
            "catalog config-read evidence must contain defaults",
        ));
    };
    let default_keys =
        validate_catalog_config_entries(event, defaults, "catalog config-read defaults")?;
    let default_entries = defaults
        .as_array()
        .expect("validated config defaults array");
    let required = [
        (LAKECAT_COMPATIBILITY_KEY, LAKECAT_COMPATIBILITY_VALUE),
        (LAKECAT_FORMAT_BASELINE_KEY, LAKECAT_FORMAT_BASELINE_VALUE),
        (LAKECAT_FORMAT_V4_KEY, LAKECAT_FORMAT_V4_VALUE),
        (LAKECAT_FORMAT_V4_BRIDGE_KEY, LAKECAT_FORMAT_V4_BRIDGE_VALUE),
        (
            LAKECAT_FORMAT_V4_TYPED_SAIL_KEY,
            LAKECAT_FORMAT_V4_TYPED_SAIL_VALUE,
        ),
    ];
    let allowed_v4_keys = required
        .iter()
        .map(|(key, _)| *key)
        .filter(|key| key.starts_with("lakecat.format.v4"))
        .map(str::to_string)
        .collect::<BTreeSet<_>>();
    for key in &default_keys {
        if key.starts_with("lakecat.format.v4") && !allowed_v4_keys.contains(key) {
            return Err(outbox_evidence_error(
                event,
                "catalog config-read defaults must not contain unsupported v4 bridge keys",
            ));
        }
    }
    for (required_key, required_value) in required {
        let found = default_entries.iter().any(|entry| {
            entry.get("key").and_then(Value::as_str) == Some(required_key)
                && entry.get("value").and_then(Value::as_str) == Some(required_value)
        });
        if !found {
            return Err(outbox_evidence_error(
                event,
                &format!(
                    "catalog config-read defaults must include {required_key}={required_value}"
                ),
            ));
        }
    }
    Ok(())
}

pub(crate) fn validate_catalog_config_overrides(
    event: &OutboxEvent,
    payload: &Value,
) -> Result<(), LakeCatError> {
    let Some(overrides) = payload.get("overrides") else {
        return Ok(());
    };
    if overrides.is_null() {
        return Ok(());
    }
    let override_keys =
        validate_catalog_config_entries(event, overrides, "catalog config-read overrides")?;
    for key in override_keys {
        if key.starts_with("lakecat.format.v4") {
            return Err(outbox_evidence_error(
                event,
                "catalog config-read overrides must not contain v4 bridge keys",
            ));
        }
    }
    Ok(())
}

pub(crate) fn validate_catalog_config_endpoints(
    event: &OutboxEvent,
    payload: &Value,
) -> Result<(), LakeCatError> {
    let Some(endpoints) = payload.get("endpoints") else {
        return Err(outbox_evidence_error(
            event,
            "catalog config-read evidence must contain endpoints",
        ));
    };
    let Some(endpoints) = endpoints.as_array() else {
        return Err(outbox_evidence_error(
            event,
            "catalog config-read endpoints must be an array",
        ));
    };
    let mut endpoint_set = BTreeSet::new();
    for endpoint in endpoints {
        let Some(endpoint) = endpoint
            .as_str()
            .filter(|endpoint| !endpoint.trim().is_empty())
        else {
            return Err(outbox_evidence_error(
                event,
                "catalog config-read endpoints must contain non-empty strings",
            ));
        };
        if !endpoint_set.insert(endpoint.to_string()) {
            return Err(outbox_evidence_error(
                event,
                "catalog config-read endpoints must not contain duplicate entries",
            ));
        }
    }
    let required = [
        "GET /catalog/v1/config",
        "GET /catalog/v1/namespaces",
        "POST /catalog/v1/namespaces",
        "POST /catalog/v1/namespaces/{namespace}/tables",
        "GET /catalog/v1/namespaces/{namespace}/tables/{table}",
        "POST /catalog/v1/namespaces/{namespace}/tables/{table}/commit",
        "POST /catalog/v1/namespaces/{namespace}/tables/{table}/plan",
        "POST /catalog/v1/namespaces/{namespace}/tables/{table}/fetch-scan-tasks",
        "GET /catalog/v1/namespaces/{namespace}/tables/{table}/credentials",
        "GET /catalog/v1/{warehouse}/config",
        "GET /catalog/v1/{warehouse}/namespaces",
        "POST /catalog/v1/{warehouse}/namespaces",
        "POST /catalog/v1/{warehouse}/namespaces/{namespace}/tables",
        "GET /catalog/v1/{warehouse}/namespaces/{namespace}/tables/{table}",
        "POST /catalog/v1/{warehouse}/namespaces/{namespace}/tables/{table}/commit",
        "POST /catalog/v1/{warehouse}/namespaces/{namespace}/tables/{table}/plan",
        "POST /catalog/v1/{warehouse}/namespaces/{namespace}/tables/{table}/fetch-scan-tasks",
        "GET /catalog/v1/{warehouse}/namespaces/{namespace}/tables/{table}/credentials",
        "POST /management/v1/lineage/drain",
        "GET /querygraph/v1/bootstrap",
    ];
    for required_endpoint in required {
        if !endpoint_set.contains(required_endpoint) {
            return Err(outbox_evidence_error(
                event,
                &format!("catalog config-read endpoints must include {required_endpoint}"),
            ));
        }
    }
    Ok(())
}

pub(crate) fn validate_namespace_list_event_evidence(
    event: &OutboxEvent,
    payload: &Value,
) -> Result<(), LakeCatError> {
    if event.payload.get("payload").is_some() {
        validate_object_evidence_schema(
            event,
            &event.payload,
            "namespace list outbox payload",
            LIST_OUTBOX_PAYLOAD_FIELDS,
        )?;
    }
    validate_object_evidence_schema(
        event,
        payload,
        "namespace list",
        NAMESPACE_LIST_EVIDENCE_FIELDS,
    )?;
    validate_required_warehouse_field(event, payload, "namespace list")?;
    let count = validate_required_unsigned_count_field(
        event,
        payload,
        "namespace-count",
        "namespace list",
    )?;
    validate_required_namespace_path_array(event, payload, count)?;
    validate_authorization_receipt_principal(event, payload, "namespace list")?;
    Ok(())
}

pub(crate) fn validate_required_namespace_path_array(
    event: &OutboxEvent,
    payload: &Value,
    expected_count: u64,
) -> Result<(), LakeCatError> {
    let Some(paths) = payload.get("namespace-paths") else {
        return Err(outbox_evidence_error(
            event,
            "namespace list evidence must contain namespace-paths",
        ));
    };
    let Some(paths) = paths.as_array() else {
        return Err(outbox_evidence_error(
            event,
            "namespace list namespace-paths must be an array",
        ));
    };
    if paths.len() as u64 != expected_count {
        return Err(outbox_evidence_error(
            event,
            "namespace list namespace-paths count must match namespace list count",
        ));
    }
    let mut unique_paths = BTreeSet::new();
    for path in paths {
        let Some(path) = path.as_str().filter(|path| !path.trim().is_empty()) else {
            return Err(outbox_evidence_error(
                event,
                "namespace list namespace-paths must contain non-empty strings",
            ));
        };
        path.parse::<Namespace>().map_err(|_| {
            outbox_evidence_error(
                event,
                "namespace list namespace-paths contains an invalid namespace",
            )
        })?;
        if !unique_paths.insert(path) {
            return Err(outbox_evidence_error(
                event,
                "namespace list namespace-paths must not contain duplicate namespaces",
            ));
        }
    }
    Ok(())
}

pub(crate) fn validate_view_list_event_evidence(
    event: &OutboxEvent,
    payload: &Value,
) -> Result<(), LakeCatError> {
    if event.payload.get("payload").is_some() {
        validate_object_evidence_schema(
            event,
            &event.payload,
            "view list outbox payload",
            LIST_OUTBOX_PAYLOAD_FIELDS,
        )?;
    }
    validate_object_evidence_schema(event, payload, "view list", VIEW_LIST_EVIDENCE_FIELDS)?;
    validate_required_warehouse_field(event, payload, "view list")?;
    let Some(namespace) = payload.get("namespace") else {
        return Err(outbox_evidence_error(
            event,
            "view list evidence must contain namespace",
        ));
    };
    validate_namespace_value(event, namespace, "view list")?;
    let count = validate_required_unsigned_count_field(event, payload, "view-count", "view list")?;
    validate_required_view_name_array(event, payload, count)?;
    validate_authorization_receipt_principal(event, payload, "view list")?;
    Ok(())
}

pub(crate) fn validate_required_view_name_array(
    event: &OutboxEvent,
    payload: &Value,
    expected_count: u64,
) -> Result<(), LakeCatError> {
    let Some(names) = payload.get("view-names") else {
        return Err(outbox_evidence_error(
            event,
            "view list evidence must contain view-names",
        ));
    };
    let Some(names) = names.as_array() else {
        return Err(outbox_evidence_error(
            event,
            "view list view-names must be an array",
        ));
    };
    if names.len() as u64 != expected_count {
        return Err(outbox_evidence_error(
            event,
            "view list view-names count must match view list count",
        ));
    }
    let mut unique_names = BTreeSet::new();
    for name in names {
        let Some(name) = name.as_str().filter(|name| !name.trim().is_empty()) else {
            return Err(outbox_evidence_error(
                event,
                "view list view-names must contain non-empty strings",
            ));
        };
        TableName::new(name).map_err(|_| {
            outbox_evidence_error(event, "view list view-names contains an invalid view name")
        })?;
        if !unique_names.insert(name) {
            return Err(outbox_evidence_error(
                event,
                "view list view-names must not contain duplicate view names",
            ));
        }
    }
    Ok(())
}

pub(crate) fn validate_view_lifecycle_event_evidence(
    event: &OutboxEvent,
    payload: &Value,
) -> Result<(), LakeCatError> {
    if event.payload.get("payload").is_some() {
        validate_object_evidence_schema(
            event,
            &event.payload,
            "view lifecycle outbox payload",
            VIEW_LIFECYCLE_OUTBOX_PAYLOAD_FIELDS,
        )?;
    }
    validate_object_evidence_schema(
        event,
        payload,
        "view lifecycle",
        VIEW_LIFECYCLE_EVIDENCE_FIELDS,
    )?;
    let Some(view) = payload.get("view") else {
        return Err(outbox_evidence_error(
            event,
            "view lifecycle evidence must contain view",
        ));
    };
    validate_object_evidence_schema(
        event,
        view,
        "view lifecycle view",
        VIEW_RECORD_EVIDENCE_FIELDS,
    )?;
    let view_warehouse = view.get("warehouse").and_then(Value::as_str);
    let payload_warehouse = payload.get("warehouse").and_then(Value::as_str);
    let Some(warehouse_name) = view_warehouse
        .or(payload_warehouse)
        .filter(|warehouse| !warehouse.is_empty())
    else {
        return Err(outbox_evidence_error(
            event,
            "view lifecycle evidence must contain warehouse",
        ));
    };
    WarehouseName::new(warehouse_name).map_err(|_| {
        outbox_evidence_error(event, "view lifecycle evidence has invalid warehouse")
    })?;
    if let Some(payload_warehouse) = payload_warehouse
        && view_warehouse.is_some()
        && view_warehouse != Some(payload_warehouse)
    {
        return Err(outbox_evidence_error(
            event,
            "view lifecycle view warehouse must match payload warehouse",
        ));
    }
    let view_namespace = view.get("namespace");
    let payload_namespace = payload.get("namespace");
    let Some(namespace) = view_namespace.or(payload_namespace) else {
        return Err(outbox_evidence_error(
            event,
            "view lifecycle evidence must contain namespace",
        ));
    };
    let namespace = decode_namespace_value(event, namespace, "view lifecycle")?;
    if let (Some(_), Some(payload_namespace)) = (view_namespace, payload_namespace) {
        let payload_namespace =
            decode_namespace_value(event, payload_namespace, "view lifecycle payload")?;
        if namespace != payload_namespace {
            return Err(outbox_evidence_error(
                event,
                "view lifecycle view namespace must match payload namespace",
            ));
        }
    }
    let Some(view_name) = view
        .get("name")
        .and_then(Value::as_str)
        .filter(|name| !name.is_empty())
    else {
        return Err(outbox_evidence_error(
            event,
            "view lifecycle evidence must contain view name",
        ));
    };
    TableName::new(view_name).map_err(|_| {
        outbox_evidence_error(event, "view lifecycle evidence has invalid view name")
    })?;
    let Some(view_version) = view.get("view-version").and_then(Value::as_u64) else {
        return Err(outbox_evidence_error(
            event,
            "view lifecycle evidence must contain positive view-version",
        ));
    };
    if view_version == 0 {
        return Err(outbox_evidence_error(
            event,
            "view lifecycle evidence must contain positive view-version",
        ));
    }
    if let Some(expected) = payload.get("expected-view-version")
        && !expected.is_null()
        && expected.as_u64().filter(|version| *version > 0).is_none()
    {
        return Err(outbox_evidence_error(
            event,
            "view lifecycle expected-view-version must be positive when present",
        ));
    }
    validate_authorization_receipt_principal(event, payload, "view lifecycle")?;
    Ok(())
}

pub(crate) fn validate_management_list_event_evidence(
    event: &OutboxEvent,
    payload: &Value,
) -> Result<(), LakeCatError> {
    if event.payload.get("payload").is_some() {
        validate_object_evidence_schema(
            event,
            &event.payload,
            "management list outbox payload",
            LIST_OUTBOX_PAYLOAD_FIELDS,
        )?;
    }
    match event.event_type.as_str() {
        "policy-binding.listed" => {
            validate_object_evidence_schema(
                event,
                payload,
                "policy-binding list",
                POLICY_BINDING_LIST_EVIDENCE_FIELDS,
            )?;
            let warehouse =
                validate_required_warehouse_field(event, payload, "policy-binding list")?;
            let count = validate_required_unsigned_count_field(
                event,
                payload,
                "policy-count",
                "policy-binding list",
            )?;
            validate_required_management_id_array(
                event,
                payload,
                "policy-ids",
                count,
                "policy-binding list",
                "policy-id",
                |id| PolicyBinding::new(id, warehouse.clone(), None, None, true, json!({})).is_ok(),
            )?;
        }
        "project.listed" => {
            validate_object_evidence_schema(
                event,
                payload,
                "project list",
                PROJECT_LIST_EVIDENCE_FIELDS,
            )?;
            let count = validate_required_unsigned_count_field(
                event,
                payload,
                "project-count",
                "project list",
            )?;
            validate_required_management_id_array(
                event,
                payload,
                "project-ids",
                count,
                "project list",
                "project-id",
                |id| {
                    ProjectRecord::new(id, None, None, BTreeMap::new(), Principal::anonymous())
                        .is_ok()
                },
            )?;
        }
        "server.listed" => {
            validate_object_evidence_schema(
                event,
                payload,
                "server list",
                SERVER_LIST_EVIDENCE_FIELDS,
            )?;
            let count = validate_required_unsigned_count_field(
                event,
                payload,
                "server-count",
                "server list",
            )?;
            validate_required_management_id_array(
                event,
                payload,
                "server-ids",
                count,
                "server list",
                "server-id",
                |id| {
                    ServerRecord::new(id, None, None, BTreeMap::new(), Principal::anonymous())
                        .is_ok()
                },
            )?;
        }
        "storage-profile.listed" => {
            validate_object_evidence_schema(
                event,
                payload,
                "storage-profile list",
                STORAGE_PROFILE_LIST_EVIDENCE_FIELDS,
            )?;
            let warehouse =
                validate_required_warehouse_field(event, payload, "storage-profile list")?;
            let count = validate_required_unsigned_count_field(
                event,
                payload,
                "storage-profile-count",
                "storage-profile list",
            )?;
            validate_required_management_id_array(
                event,
                payload,
                "storage-profile-ids",
                count,
                "storage-profile list",
                "storage-profile-id",
                |id| {
                    StorageProfile::new(
                        id,
                        warehouse.clone(),
                        "file:///tmp/lakecat-evidence",
                        StorageProvider::File,
                        CredentialIssuanceMode::LocalFileNoSecret,
                        None,
                        BTreeMap::new(),
                    )
                    .is_ok()
                },
            )?;
        }
        "warehouse.listed" => {
            validate_object_evidence_schema(
                event,
                payload,
                "warehouse list",
                WAREHOUSE_LIST_EVIDENCE_FIELDS,
            )?;
            let count = validate_required_unsigned_count_field(
                event,
                payload,
                "warehouse-count",
                "warehouse list",
            )?;
            if let Some(project_id) =
                optional_non_empty_string_field(event, payload, "project-id", "warehouse list")?
            {
                if ProjectRecord::new(
                    &project_id,
                    None,
                    None,
                    BTreeMap::new(),
                    Principal::anonymous(),
                )
                .is_err()
                {
                    return Err(outbox_evidence_error(
                        event,
                        &format!(
                            "warehouse list project-id contains an invalid identifier; project-id-hash={}",
                            content_hash_bytes(project_id.as_bytes())
                        ),
                    ));
                }
            }
            validate_required_management_id_array(
                event,
                payload,
                "warehouse-names",
                count,
                "warehouse list",
                "warehouse-name",
                |id| WarehouseName::new(id).is_ok(),
            )?;
        }
        _ => {}
    }
    validate_authorization_receipt_principal(event, payload, "management-list")?;
    Ok(())
}

pub(crate) fn validate_required_management_id_array(
    event: &OutboxEvent,
    payload: &Value,
    field: &str,
    expected_count: u64,
    label: &str,
    hash_field: &str,
    is_valid_id: impl Fn(&str) -> bool,
) -> Result<(), LakeCatError> {
    let Some(ids) = payload.get(field) else {
        return Err(outbox_evidence_error(
            event,
            &format!("{label} evidence must contain {field}"),
        ));
    };
    let Some(ids) = ids.as_array() else {
        return Err(outbox_evidence_error(
            event,
            &format!("{label} {field} must be an array when present"),
        ));
    };
    if ids.len() as u64 != expected_count {
        return Err(outbox_evidence_error(
            event,
            &format!("{label} {field} count must match {label} count"),
        ));
    }
    let mut unique_ids = BTreeSet::new();
    for id in ids {
        let Some(id) = id.as_str().filter(|id| !id.trim().is_empty()) else {
            return Err(outbox_evidence_error(
                event,
                &format!("{label} {field} must contain non-empty strings"),
            ));
        };
        if !unique_ids.insert(id) {
            return Err(outbox_evidence_error(
                event,
                &format!("{label} {field} must not contain duplicate identifiers"),
            ));
        }
        if !is_valid_id(id) {
            return Err(outbox_evidence_error(
                event,
                &format!(
                    "{label} {field} contains an invalid identifier; {hash_field}-hash={}",
                    content_hash_bytes(id.as_bytes())
                ),
            ));
        }
    }
    Ok(())
}

pub(crate) fn validate_required_warehouse_field(
    event: &OutboxEvent,
    payload: &Value,
    label: &str,
) -> Result<WarehouseName, LakeCatError> {
    let Some(warehouse_name) = payload
        .get("warehouse")
        .and_then(Value::as_str)
        .filter(|warehouse| !warehouse.is_empty())
    else {
        return Err(outbox_evidence_error(
            event,
            &format!("{label} evidence must contain warehouse"),
        ));
    };
    WarehouseName::new(warehouse_name).map_err(|_| {
        outbox_evidence_error(event, &format!("{label} evidence has invalid warehouse"))
    })
}

pub(crate) fn validate_required_unsigned_count_field(
    event: &OutboxEvent,
    payload: &Value,
    field: &str,
    label: &str,
) -> Result<u64, LakeCatError> {
    payload.get(field).and_then(Value::as_u64).ok_or_else(|| {
        outbox_evidence_error(
            event,
            &format!("{label} evidence must contain unsigned {field}"),
        )
    })
}

pub(crate) fn validate_namespace_lifecycle_value(
    event: &OutboxEvent,
    namespace: &Value,
) -> Result<(), LakeCatError> {
    validate_namespace_value(event, namespace, "namespace lifecycle")
}

pub(crate) fn validate_namespace_value(
    event: &OutboxEvent,
    namespace: &Value,
    label: &str,
) -> Result<(), LakeCatError> {
    decode_namespace_value(event, namespace, label)?;
    Ok(())
}

pub(crate) fn decode_namespace_value(
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
