use lakecat_api::{
    ConfigEntry, LineageDrainEventSummary, LineageDrainResponse, ViewVersionReceiptChainResponse,
};
use lakecat_core::{LakeCatError, content_hash_json};
#[cfg(feature = "sail-local")]
use lakecat_sail::catalog_provider::{
    LakeCatCatalogProvider, ProviderFetchScanTasksRequest, ProviderScanPlanningRequest,
};
use lakecat_security::AuthorizationReceipt;
use lakecat_store::OutboxEvent;
use serde_json::Value;

use crate::*;

pub(crate) fn attach_lineage_drain_authorization(
    response: &mut LineageDrainResponse,
    receipt: &AuthorizationReceipt,
) -> Result<(), LakeCatError> {
    let receipt_value = serde_json::to_value(receipt).map_err(|err| {
        LakeCatError::InvalidArgument(format!(
            "lineage read receipt was not JSON encodable: {err}"
        ))
    })?;
    response.principal_subject = Some(receipt.principal.subject.clone());
    response.principal_kind = receipt_value
        .pointer("/principal/kind")
        .and_then(Value::as_str)
        .map(str::to_string);
    response.authorization_receipt_hash = Some(content_hash_json(&receipt_value)?);
    response.authorization_receipt_action = receipt_value
        .get("action")
        .and_then(Value::as_str)
        .map(str::to_string);
    response.request_identity_state = receipt
        .context
        .get("request-identity")
        .and_then(|identity| identity.get("attestation-state"))
        .and_then(Value::as_str)
        .map(str::to_string);
    response.request_identity_source = receipt
        .context
        .get("request-identity")
        .and_then(|identity| identity.get("source"))
        .and_then(Value::as_str)
        .map(str::to_string);
    response.typedid_envelope_hash = receipt
        .context
        .get("request-identity")
        .and_then(|identity| identity.get("typedid-envelope-sha256"))
        .and_then(Value::as_str)
        .map(str::to_string);
    response.typedid_proof_hash = receipt
        .context
        .get("request-identity")
        .and_then(|identity| identity.get("typedid-proof-sha256"))
        .and_then(Value::as_str)
        .map(str::to_string);
    Ok(())
}

pub(crate) fn lineage_drain_event_summary(
    event: &OutboxEvent,
    receipt: &OutboxProjectionReceipt,
) -> Result<LineageDrainEventSummary, LakeCatError> {
    if event.event_type == "table.commit" {
        validate_table_commit_hash_evidence(event)?;
    }
    if event.event_type == "credentials.vend-attempted" {
        let payload = event.payload.get("payload").unwrap_or(&event.payload);
        validate_credential_vend_event_evidence(event, payload)?;
    }
    if event.event_type == "catalog.config-read" {
        let payload = event.payload.get("payload").unwrap_or(&event.payload);
        validate_catalog_config_read_event_evidence(event, payload)?;
    }
    if event.event_type == "storage-profile.upserted" {
        let payload = event.payload.get("payload").unwrap_or(&event.payload);
        validate_storage_profile_upsert_event_evidence(event, payload)?;
    }
    if event.event_type == "policy-binding.upserted" {
        let payload = event.payload.get("payload").unwrap_or(&event.payload);
        validate_policy_binding_upsert_event_evidence(event, payload)?;
    }
    if event.event_type == "project.upserted" {
        let payload = event.payload.get("payload").unwrap_or(&event.payload);
        validate_project_upsert_event_evidence(event, payload)?;
    }
    if event.event_type == "server.upserted" {
        let payload = event.payload.get("payload").unwrap_or(&event.payload);
        validate_server_upsert_event_evidence(event, payload)?;
    }
    if event.event_type == "warehouse.upserted" {
        let payload = event.payload.get("payload").unwrap_or(&event.payload);
        validate_warehouse_upsert_event_evidence(event, payload)?;
    }
    if matches!(
        event.event_type.as_str(),
        "policy-binding.listed"
            | "project.listed"
            | "server.listed"
            | "storage-profile.listed"
            | "warehouse.listed"
    ) {
        let payload = event.payload.get("payload").unwrap_or(&event.payload);
        validate_management_list_event_evidence(event, payload)?;
    }
    if event.event_type == "namespace.listed" {
        let payload = event.payload.get("payload").unwrap_or(&event.payload);
        validate_namespace_list_event_evidence(event, payload)?;
    }
    if matches!(
        event.event_type.as_str(),
        "namespace.created" | "namespace.dropped" | "namespace.loaded"
    ) {
        let payload = event.payload.get("payload").unwrap_or(&event.payload);
        validate_namespace_lifecycle_event_evidence(event, payload)?;
    }
    if matches!(
        event.event_type.as_str(),
        "table.created" | "table.loaded" | "table.deleted" | "table.restored"
    ) {
        let payload = event.payload.get("payload").unwrap_or(&event.payload);
        validate_table_lifecycle_event_evidence(event, payload)?;
    }
    if event.event_type == "view.listed" {
        let payload = event.payload.get("payload").unwrap_or(&event.payload);
        validate_view_list_event_evidence(event, payload)?;
    }
    if matches!(
        event.event_type.as_str(),
        "view.upserted" | "view.loaded" | "view.dropped"
    ) {
        let payload = event.payload.get("payload").unwrap_or(&event.payload);
        validate_view_lifecycle_event_evidence(event, payload)?;
    }
    if event.event_type == "querygraph.bootstrap" {
        let payload = event.payload.get("payload").unwrap_or(&event.payload);
        validate_querygraph_bootstrap_event_evidence(event, payload)?;
    }
    let payload = event.payload.get("payload").unwrap_or(&event.payload);
    let view = payload.get("view");
    let view_warehouse = lineage_summary_optional_nonblank_string_value(
        event,
        view.and_then(|view| view.get("warehouse"))
            .or_else(|| payload.get("warehouse")),
        "view warehouse",
    )?;
    let view_namespace = lineage_summary_namespace_parts(
        event,
        view.and_then(|view| view.get("namespace"))
            .or_else(|| payload.get("namespace")),
        "view namespace",
    )?;
    let view_name = lineage_summary_optional_nonblank_string_value(
        event,
        view.and_then(|view| view.get("name"))
            .or_else(|| payload.get("view")),
        "view name",
    )?;
    let view_name_ref = view_name.as_deref();
    let view_version = if let Some(view) = view.filter(|view| view.get("view-version").is_some()) {
        Some(lineage_summary_positive_u64_field(
            event,
            view,
            "view-version",
        )?)
    } else {
        None
    };
    let expected_view_version = payload
        .get("expected-view-version")
        .filter(|value| !value.is_null())
        .map(|_| lineage_summary_positive_u64_field(event, payload, "expected-view-version"))
        .transpose()?;
    let view_stable_id = match (
        view_warehouse.as_deref(),
        view_namespace.as_slice(),
        view_name_ref,
    ) {
        (Some(warehouse), namespace, Some(name)) if !namespace.is_empty() => Some(format!(
            "lakecat:view:{warehouse}:{}:{name}",
            namespace.join(".")
        )),
        _ => None,
    };
    let view_version_receipt_hashes = lineage_summary_view_receipt_hashes(event, payload)?;
    let view_version_receipt_chain_hashes =
        lineage_summary_view_receipt_chain_hashes(event, payload)?;
    let view_version_receipt_chains = lineage_summary_view_receipt_chains(event, payload)?;
    let view_version_receipt_chain_verified_count =
        lineage_summary_view_receipt_chain_verified_count(
            event,
            payload,
            &view_version_receipt_chains,
        )?;
    let storage_profile = lineage_summary_storage_profile(event, payload.get("storage-profile"))?;
    validate_lineage_summary_credential_secret_ref_presence(event, payload, &storage_profile)?;
    let authorization_receipt = payload
        .get("authorization-receipt")
        .or_else(|| payload.pointer("/payload/authorization-receipt"));
    validate_lineage_summary_commit_history_receipt_shape(event, payload, authorization_receipt)?;
    let request_identity = authorization_receipt
        .and_then(|receipt| receipt.get("request-identity"))
        .or_else(|| {
            authorization_receipt
                .and_then(|receipt| receipt.get("context"))
                .and_then(|context| context.get("request-identity"))
        });
    let credential_prefix_hashes = payload
        .get("credential-response-evidence")
        .map(|_| lineage_summary_credential_prefix_hashes(event, payload))
        .transpose()?
        .unwrap_or_default();
    let credential_count =
        lineage_summary_optional_count_alias_field(event, payload, &["credential-count"])?;
    validate_lineage_summary_credential_counts(event, credential_count, &credential_prefix_hashes)?;
    let (
        raw_credential_exception_allowed,
        raw_credential_exception_reason,
        credential_block_reason,
    ) = lineage_summary_credential_exception_fields(event, payload, credential_count)?;
    let (principal_subject, principal_kind) =
        lineage_summary_authorization_principal_fields(event, authorization_receipt)?;
    let policy = payload.get("policy");
    if payload.get("plan-task").is_some() {
        validate_plan_task_evidence(event, payload.get("plan-task"), "lineage drain summary")?;
    }
    let table_commit_count =
        lineage_summary_optional_count_alias_field(event, payload, &["commit-count"])?;
    let table_commit_sequence_numbers = payload
        .get("sequence-numbers")
        .map(|_| lineage_summary_u64_array_field(event, payload, "sequence-numbers"))
        .transpose()?
        .unwrap_or_default();
    let table_commit_hashes = payload
        .get("commit-hashes")
        .map(|_| lineage_summary_full_hash_array_field(event, payload, "commit-hashes"))
        .transpose()?
        .unwrap_or_default();
    validate_lineage_summary_commit_history_counts(
        event,
        table_commit_count,
        &table_commit_sequence_numbers,
        &table_commit_hashes,
    )?;
    let requested_stats_fields = payload
        .get("requested-stats-fields")
        .map(|_| lineage_summary_string_array_field(event, payload, "requested-stats-fields"))
        .transpose()?
        .unwrap_or_default();
    let effective_stats_fields = payload
        .get("effective-stats-fields")
        .map(|_| lineage_summary_string_array_field(event, payload, "effective-stats-fields"))
        .transpose()?
        .unwrap_or_default();
    validate_lineage_summary_stats_fields(event, payload, &effective_stats_fields)?;
    let summary = LineageDrainEventSummary {
        event_id: event.event_id.clone(),
        event_type: event.event_type.clone(),
        catalog_config_defaults: lineage_summary_config_entries(
            event,
            payload,
            "defaults",
            "lineage drain catalog config defaults",
        )?,
        catalog_config_overrides: lineage_summary_config_entries(
            event,
            payload,
            "overrides",
            "lineage drain catalog config overrides",
        )?,
        catalog_config_endpoints: payload
            .get("endpoints")
            .map(|_| lineage_summary_string_array_field(event, payload, "endpoints"))
            .transpose()?
            .unwrap_or_default(),
        principal_subject,
        principal_kind,
        authorization_receipt_hash: payload
            .get("authorization-receipt")
            .and_then(|receipt| content_hash_json(receipt).ok()),
        authorization_receipt_action: authorization_receipt
            .map(|receipt| lineage_summary_optional_nonblank_string_field(event, receipt, "action"))
            .transpose()?
            .flatten(),
        request_identity_state: request_identity
            .map(|identity| {
                lineage_summary_optional_nonblank_string_field(event, identity, "attestation-state")
            })
            .transpose()?
            .flatten(),
        request_identity_source: request_identity
            .map(|identity| {
                lineage_summary_optional_nonblank_string_field(event, identity, "source")
            })
            .transpose()?
            .flatten(),
        typedid_envelope_hash: request_identity
            .map(|identity| {
                lineage_summary_optional_full_hash_field(event, identity, "typedid-envelope-sha256")
            })
            .transpose()?
            .flatten(),
        typedid_proof_hash: request_identity
            .map(|identity| {
                lineage_summary_optional_full_hash_field(event, identity, "typedid-proof-sha256")
            })
            .transpose()?
            .flatten(),
        agent_delegation_hash: request_identity
            .map(|identity| {
                lineage_summary_optional_full_hash_field(event, identity, "agent-delegation-sha256")
            })
            .transpose()?
            .flatten(),
        agent_summary_signature_hash: request_identity
            .map(|identity| {
                lineage_summary_optional_full_hash_field(
                    event,
                    identity,
                    "agent-summary-signature-sha256",
                )
            })
            .transpose()?
            .flatten(),
        graph_events: receipt.graph_events,
        lineage_events: receipt.lineage_events,
        bundle_hash: lineage_summary_optional_full_hash_field(event, payload, "bundle-hash")?,
        graph_hash: lineage_summary_optional_full_hash_field(event, payload, "graph-hash")?,
        open_lineage_hash: lineage_summary_optional_full_hash_field(
            event,
            payload,
            "open-lineage-hash",
        )?,
        querygraph_import_hash: lineage_summary_optional_full_hash_field(
            event,
            payload,
            "querygraph-import-hash",
        )?,
        table_artifact_count: lineage_summary_array_len_field(event, payload, "table-artifacts")?,
        view_artifact_count: lineage_summary_array_len_field(event, payload, "view-artifacts")?,
        view_version_receipt_hashes,
        view_version_receipt_chain_hashes,
        view_version_receipt_chain_verified_count,
        view_version_receipt_chains,
        view_warehouse,
        view_namespace,
        view_name,
        view_stable_id,
        view_version,
        expected_view_version,
        policy_binding_count: lineage_summary_optional_count_alias_field(
            event,
            payload,
            &["policy-binding-count", "policy-count"],
        )?
        .unwrap_or_default(),
        policy_ids: payload
            .get("policy-ids")
            .map(|_| {
                lineage_summary_counted_string_array_field(
                    event,
                    payload,
                    "policy-ids",
                    &["policy-binding-count", "policy-count"],
                )
            })
            .transpose()?
            .unwrap_or_default(),
        policy_id: policy
            .map(|policy| {
                lineage_summary_optional_nonblank_string_field(event, policy, "policy-id")
            })
            .transpose()?
            .flatten(),
        policy_odrl_hash: policy
            .map(|policy| lineage_summary_optional_full_hash_field(event, policy, "odrl-hash"))
            .transpose()?
            .flatten(),
        project_count: lineage_summary_optional_count_alias_field(
            event,
            payload,
            &["project-count"],
        )?,
        project_ids: payload
            .get("project-ids")
            .map(|_| {
                lineage_summary_counted_string_array_field(
                    event,
                    payload,
                    "project-ids",
                    &["project-count"],
                )
            })
            .transpose()?
            .unwrap_or_default(),
        server_count: lineage_summary_optional_count_alias_field(
            event,
            payload,
            &["server-count"],
        )?,
        server_ids: payload
            .get("server-ids")
            .map(|_| {
                lineage_summary_counted_string_array_field(
                    event,
                    payload,
                    "server-ids",
                    &["server-count"],
                )
            })
            .transpose()?
            .unwrap_or_default(),
        storage_profile_count: lineage_summary_optional_count_alias_field(
            event,
            payload,
            &["storage-profile-count"],
        )?,
        storage_profile_ids: payload
            .get("storage-profile-ids")
            .map(|_| {
                lineage_summary_counted_string_array_field(
                    event,
                    payload,
                    "storage-profile-ids",
                    &["storage-profile-count"],
                )
            })
            .transpose()?
            .unwrap_or_default(),
        storage_profile_id: storage_profile
            .as_ref()
            .map(|profile| profile.profile_id.clone()),
        storage_profile_provider: storage_profile
            .as_ref()
            .map(|profile| profile.provider.clone()),
        storage_profile_issuance_mode: storage_profile
            .as_ref()
            .map(|profile| profile.issuance_mode.clone()),
        storage_profile_location_prefix_hash: storage_profile
            .as_ref()
            .map(|profile| profile.location_prefix_hash.clone()),
        storage_profile_secret_ref_present: storage_profile
            .as_ref()
            .map(|profile| profile.secret_ref_present),
        storage_profile_secret_ref_provider: storage_profile
            .as_ref()
            .and_then(|profile| profile.secret_ref_provider.clone()),
        storage_profile_secret_ref_hash: storage_profile
            .as_ref()
            .and_then(|profile| profile.secret_ref_hash.clone()),
        warehouse_count: lineage_summary_optional_count_alias_field(
            event,
            payload,
            &["warehouse-count"],
        )?,
        warehouse_names: payload
            .get("warehouse-names")
            .map(|_| {
                lineage_summary_counted_string_array_field(
                    event,
                    payload,
                    "warehouse-names",
                    &["warehouse-count"],
                )
            })
            .transpose()?
            .unwrap_or_default(),
        table_commit_count,
        table_commit_sequence_numbers,
        table_commit_hashes,
        scan_task_count: lineage_summary_optional_count_alias_field(
            event,
            payload,
            &["scan-task-count"],
        )?,
        file_scan_task_count: lineage_summary_optional_count_alias_field(
            event,
            payload,
            &["file-scan-task-count"],
        )?,
        delete_file_count: lineage_summary_optional_count_alias_field(
            event,
            payload,
            &["delete-file-count"],
        )?,
        child_plan_task_count: lineage_summary_optional_count_alias_field(
            event,
            payload,
            &["child-plan-task-count"],
        )?,
        read_restriction: payload.get("read-restriction").cloned(),
        required_projection: payload
            .get("required-projection")
            .map(|_| lineage_summary_string_array_field(event, payload, "required-projection"))
            .transpose()?
            .unwrap_or_default(),
        requested_projection: payload
            .get("requested-projection")
            .map(|_| lineage_summary_string_array_field(event, payload, "requested-projection"))
            .transpose()?
            .unwrap_or_default(),
        effective_projection: payload
            .get("effective-projection")
            .map(|_| lineage_summary_string_array_field(event, payload, "effective-projection"))
            .transpose()?
            .unwrap_or_default(),
        required_filters: lineage_summary_required_filters(event, payload)?,
        requested_stats_fields,
        effective_stats_fields,
        management_scope_project_id: lineage_summary_optional_nonblank_string_field(
            event,
            payload,
            "project-id",
        )?,
        management_scope_warehouse: lineage_summary_optional_nonblank_string_field(
            event,
            payload,
            "warehouse",
        )?,
        standards: payload
            .get("standards")
            .map(|_| lineage_summary_string_array_field(event, payload, "standards"))
            .transpose()?
            .unwrap_or_default(),
        credential_count,
        credential_prefix_hashes,
        credential_block_reason,
        raw_credential_exception_allowed,
        raw_credential_exception_reason,
        replay_event_hashes: receipt.lineage_event_hashes.clone(),
        replay_open_lineage_hashes: receipt.open_lineage_hashes.clone(),
    };
    validate_lineage_summary_table_operation_event_evidence(event, payload)?;
    validate_lineage_summary_view_receipt_event_evidence(event, payload)?;
    Ok(summary)
}

pub(crate) fn validate_lineage_summary_table_operation_event_evidence(
    event: &OutboxEvent,
    payload: &Value,
) -> Result<(), LakeCatError> {
    match event.event_type.as_str() {
        "table.commits-listed" => validate_table_commit_history_event_evidence(event, payload),
        "table.scan-planned" => validate_scan_planned_event_evidence(event, payload),
        "table.scan-tasks-fetched" => validate_scan_tasks_fetched_event_evidence(event, payload),
        _ => Ok(()),
    }
}

pub(crate) fn validate_lineage_summary_view_receipt_event_evidence(
    event: &OutboxEvent,
    payload: &Value,
) -> Result<(), LakeCatError> {
    match event.event_type.as_str() {
        "view.version-receipts-listed" => validate_view_receipt_list_event_evidence(event, payload),
        "view.version-receipt-chains-listed" => {
            validate_view_receipt_chain_event_evidence(event, payload)
        }
        _ => Ok(()),
    }
}

pub(crate) fn lineage_summary_view_receipt_hashes(
    event: &OutboxEvent,
    payload: &Value,
) -> Result<Vec<String>, LakeCatError> {
    if let Some(receipts) = optional_array_field(event, payload, "view-version-receipts")? {
        return lineage_summary_required_hashes_from_objects(
            event,
            receipts,
            "receipt-hash",
            "lineage drain view-version-receipts",
        );
    }
    if let Some(chains) = optional_array_field(event, payload, "view-version-receipt-chains")? {
        let mut hashes = Vec::new();
        for chain in chains {
            let Some(receipts) = optional_array_field(event, chain, "receipts")? else {
                continue;
            };
            hashes.extend(lineage_summary_required_hashes_from_objects(
                event,
                receipts,
                "receipt-hash",
                "lineage drain view-version-receipt-chain receipts",
            )?);
        }
        return Ok(hashes);
    }
    if payload.get("receipt-hashes").is_some() {
        return Ok(
            validate_required_full_hash_array_field(event, payload, "receipt-hashes")?
                .into_iter()
                .map(str::to_string)
                .collect(),
        );
    }
    if payload.get("drop-receipt-hashes").is_some() {
        return Ok(
            validate_required_full_hash_array_field(event, payload, "drop-receipt-hashes")?
                .into_iter()
                .map(str::to_string)
                .collect(),
        );
    }
    Ok(Vec::new())
}

pub(crate) fn lineage_summary_view_receipt_chain_hashes(
    event: &OutboxEvent,
    payload: &Value,
) -> Result<Vec<String>, LakeCatError> {
    if let Some(chains) = optional_array_field(event, payload, "view-version-receipt-chains")? {
        return lineage_summary_required_hashes_from_objects(
            event,
            chains,
            "chain-hash",
            "lineage drain view-version-receipt-chains",
        );
    }
    if payload.get("chain-hashes").is_some() {
        return Ok(
            validate_required_full_hash_array_field(event, payload, "chain-hashes")?
                .into_iter()
                .map(str::to_string)
                .collect(),
        );
    }
    Ok(Vec::new())
}

pub(crate) fn lineage_summary_view_receipt_chains(
    event: &OutboxEvent,
    payload: &Value,
) -> Result<Vec<ViewVersionReceiptChainResponse>, LakeCatError> {
    let Some(chains) = optional_array_field(event, payload, "view-version-receipt-chains")? else {
        return Ok(Vec::new());
    };
    let mut decoded = Vec::with_capacity(chains.len());
    for (index, chain) in chains.iter().enumerate() {
        let chain = serde_json::from_value::<ViewVersionReceiptChainResponse>(chain.clone())
            .map_err(|err| {
                outbox_evidence_error(
                    event,
                    &format!(
                        "lineage drain view-version-receipt-chains entry {index} must match ViewVersionReceiptChainResponse JSON shape: {err}"
                    ),
                )
            })?;
        if !chain.chain_verified {
            return Err(outbox_evidence_error(
                event,
                &format!(
                    "lineage drain view-version-receipt-chains entry {index} must be structurally verified"
                ),
            ));
        }
        decoded.push(chain);
    }
    Ok(decoded)
}

pub(crate) fn lineage_summary_view_receipt_chain_verified_count(
    event: &OutboxEvent,
    payload: &Value,
    chains: &[ViewVersionReceiptChainResponse],
) -> Result<usize, LakeCatError> {
    let verified_count = chains.iter().filter(|chain| chain.chain_verified).count();
    if let Some(count) = payload.get("chain-verified-count") {
        let Some(count) = count.as_u64() else {
            return Err(outbox_evidence_error(
                event,
                "chain-verified-count must be an unsigned integer when present",
            ));
        };
        let count = usize::try_from(count).map_err(|_| {
            outbox_evidence_error(
                event,
                "chain-verified-count is too large to summarize on this platform",
            )
        })?;
        if count != verified_count {
            return Err(outbox_evidence_error(
                event,
                "chain-verified-count must match verified view receipt chains",
            ));
        }
        return Ok(count);
    }
    Ok(verified_count)
}

pub(crate) fn validate_lineage_summary_commit_history_counts(
    event: &OutboxEvent,
    commit_count: Option<usize>,
    sequence_numbers: &[u64],
    commit_hashes: &[String],
) -> Result<(), LakeCatError> {
    let mut previous_sequence = None;
    for sequence_number in sequence_numbers {
        if *sequence_number == 0 {
            return Err(outbox_evidence_error(
                event,
                "sequence-numbers must be positive in lineage drain summary",
            ));
        }
        if let Some(previous_sequence) = previous_sequence {
            if *sequence_number <= previous_sequence {
                return Err(outbox_evidence_error(
                    event,
                    "sequence-numbers must be strictly increasing in lineage drain summary",
                ));
            }
        }
        previous_sequence = Some(*sequence_number);
    }
    if let Some(commit_count) = commit_count {
        if commit_count != sequence_numbers.len() {
            return Err(outbox_evidence_error(
                event,
                "commit-count must match sequence-numbers in lineage drain summary",
            ));
        }
        if commit_count != commit_hashes.len() {
            return Err(outbox_evidence_error(
                event,
                "commit-count must match commit-hashes in lineage drain summary",
            ));
        }
    }
    if !sequence_numbers.is_empty() && sequence_numbers.len() != commit_hashes.len() {
        return Err(outbox_evidence_error(
            event,
            "sequence-numbers must match commit-hashes in lineage drain summary",
        ));
    }
    Ok(())
}

pub(crate) fn validate_lineage_summary_stats_fields(
    event: &OutboxEvent,
    payload: &Value,
    effective_stats_fields: &[String],
) -> Result<(), LakeCatError> {
    if payload.get("stats-fields").is_none() {
        return Ok(());
    }
    let stats_fields = lineage_summary_string_array_field(event, payload, "stats-fields")?;
    if stats_fields.is_empty() {
        return Err(outbox_evidence_error(
            event,
            "stats-fields must not be empty when present",
        ));
    }
    validate_effective_evidence_subset(
        event,
        "lineage drain summary stats-fields",
        &stats_fields,
        "effective-stats-fields",
        effective_stats_fields,
        true,
    )
}

pub(crate) fn optional_array_field<'a>(
    event: &OutboxEvent,
    object: &'a Value,
    field: &str,
) -> Result<Option<&'a Vec<Value>>, LakeCatError> {
    let Some(value) = object.get(field) else {
        return Ok(None);
    };
    value.as_array().map(Some).ok_or_else(|| {
        outbox_evidence_error(event, &format!("{field} must be an array when present"))
    })
}

pub(crate) fn lineage_summary_array_len_field(
    event: &OutboxEvent,
    object: &Value,
    field: &str,
) -> Result<usize, LakeCatError> {
    Ok(optional_array_field(event, object, field)?.map_or(0, Vec::len))
}

pub(crate) fn lineage_summary_required_hashes_from_objects(
    event: &OutboxEvent,
    objects: &[Value],
    field: &str,
    label: &str,
) -> Result<Vec<String>, LakeCatError> {
    let mut hashes = Vec::with_capacity(objects.len());
    for object in objects {
        validate_required_full_hash_field(event, object, field)?;
        hashes.push(
            object
                .get(field)
                .and_then(Value::as_str)
                .expect("hash field was validated")
                .to_string(),
        );
    }
    let hash_refs = hashes.iter().map(String::as_str).collect::<Vec<_>>();
    validate_unique_hash_array(event, &hash_refs, label)?;
    Ok(hashes)
}

pub(crate) fn lineage_summary_u64_array_field(
    event: &OutboxEvent,
    object: &Value,
    field: &str,
) -> Result<Vec<u64>, LakeCatError> {
    let values =
        optional_array_field(event, object, field)?.expect("field presence was checked by caller");
    let mut numbers = Vec::with_capacity(values.len());
    for value in values {
        let Some(number) = value.as_u64() else {
            return Err(outbox_evidence_error(
                event,
                &format!("{field} must contain unsigned integers"),
            ));
        };
        numbers.push(number);
    }
    Ok(numbers)
}

pub(crate) fn lineage_summary_full_hash_array_field(
    event: &OutboxEvent,
    object: &Value,
    field: &str,
) -> Result<Vec<String>, LakeCatError> {
    let hashes = validate_required_full_hash_array_field(event, object, field)?;
    let hash_refs = hashes.to_vec();
    validate_unique_hash_array(event, &hash_refs, field)?;
    Ok(hashes.into_iter().map(str::to_string).collect())
}

pub(crate) fn lineage_summary_optional_full_hash_field(
    event: &OutboxEvent,
    object: &Value,
    field: &str,
) -> Result<Option<String>, LakeCatError> {
    validate_optional_full_hash_field(event, object, field)?;
    Ok(object
        .get(field)
        .and_then(Value::as_str)
        .map(str::to_string))
}

pub(crate) fn lineage_summary_credential_prefix_hashes(
    event: &OutboxEvent,
    payload: &Value,
) -> Result<Vec<String>, LakeCatError> {
    let entries = optional_array_field(event, payload, "credential-response-evidence")?
        .expect("field presence was checked by caller");
    let storage_profile = payload.get("storage-profile").ok_or_else(|| {
        outbox_evidence_error(
            event,
            "lineage drain credential-response evidence must contain storage-profile",
        )
    })?;
    let mut hashes = Vec::with_capacity(entries.len());
    for entry in entries {
        hashes.push(validate_credential_response_entry_evidence(
            event,
            payload,
            storage_profile,
            entry,
        )?);
    }
    let hash_refs = hashes.iter().map(String::as_str).collect::<Vec<_>>();
    validate_unique_hash_array(
        event,
        &hash_refs,
        "lineage drain credential-response prefix-hashes",
    )?;
    Ok(hashes)
}

pub(crate) fn validate_lineage_summary_credential_counts(
    event: &OutboxEvent,
    credential_count: Option<usize>,
    credential_prefix_hashes: &[String],
) -> Result<(), LakeCatError> {
    if let Some(credential_count) = credential_count {
        if credential_count != credential_prefix_hashes.len() {
            return Err(outbox_evidence_error(
                event,
                "credential-count must match credential-response prefix-hashes in lineage drain summary",
            ));
        }
    }
    Ok(())
}

pub(crate) fn lineage_summary_required_filters(
    event: &OutboxEvent,
    payload: &Value,
) -> Result<Vec<Value>, LakeCatError> {
    let Some(filters) = optional_array_field(event, payload, "required-filters")? else {
        if matches!(
            event.event_type.as_str(),
            "table.scan-planned" | "table.scan-tasks-fetched"
        ) && payload.get("read-restriction").is_some()
        {
            return Err(outbox_evidence_error(
                event,
                &format!(
                    "lineage drain {} required-filters must be an array",
                    event.event_type
                ),
            ));
        }
        return Ok(Vec::new());
    };
    if matches!(
        event.event_type.as_str(),
        "table.scan-planned" | "table.scan-tasks-fetched"
    ) {
        validate_scan_required_filters_match_row_predicate(
            event,
            payload,
            &format!("lineage drain {}", event.event_type),
        )?;
    }
    Ok(filters.clone())
}

pub(crate) fn lineage_summary_config_entries(
    event: &OutboxEvent,
    payload: &Value,
    field: &str,
    label: &str,
) -> Result<Vec<ConfigEntry>, LakeCatError> {
    let Some(entries) = payload.get(field) else {
        return Ok(Vec::new());
    };
    if entries.is_null() {
        return Ok(Vec::new());
    }
    validate_catalog_config_entries(event, entries, label)?;
    serde_json::from_value(entries.clone()).map_err(|err| {
        outbox_evidence_error(
            event,
            &format!("{label} must match ConfigEntry JSON shape: {err}"),
        )
    })
}

#[derive(Debug, Clone)]
pub(crate) struct LineageSummaryStorageProfile {
    profile_id: String,
    provider: String,
    issuance_mode: String,
    location_prefix_hash: String,
    secret_ref_present: bool,
    secret_ref_provider: Option<String>,
    secret_ref_hash: Option<String>,
}

pub(crate) fn lineage_summary_storage_profile(
    event: &OutboxEvent,
    storage_profile: Option<&Value>,
) -> Result<Option<LineageSummaryStorageProfile>, LakeCatError> {
    let Some(storage_profile) = storage_profile else {
        return Ok(None);
    };
    validate_storage_profile_evidence_schema(
        event,
        storage_profile,
        "lineage drain storage-profile summary",
    )?;
    validate_storage_profile_public_config_evidence(
        event,
        storage_profile,
        "lineage drain storage-profile summary",
    )?;
    validate_required_full_hash_field(event, storage_profile, "location-prefix-hash")?;
    validate_secret_ref_evidence(
        event,
        storage_profile,
        "lineage drain storage-profile summary",
    )?;
    validate_storage_profile_provider_mode_evidence(
        event,
        storage_profile,
        "lineage drain storage-profile summary",
    )?;
    let profile_id = required_string_field(
        event,
        storage_profile,
        "profile-id",
        "lineage drain storage-profile summary",
    )?
    .to_string();
    let provider = required_string_field(
        event,
        storage_profile,
        "provider",
        "lineage drain storage-profile summary",
    )?
    .to_string();
    let issuance_mode = required_string_field(
        event,
        storage_profile,
        "issuance-mode",
        "lineage drain storage-profile summary",
    )?
    .to_string();
    let location_prefix_hash = required_string_field(
        event,
        storage_profile,
        "location-prefix-hash",
        "lineage drain storage-profile summary",
    )?
    .to_string();
    let secret_ref_present = storage_profile
        .get("secret-ref-present")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let secret_ref_provider = storage_profile
        .get("secret-ref-provider")
        .and_then(Value::as_str)
        .map(str::to_string);
    let secret_ref_hash = storage_profile
        .get("secret-ref-hash")
        .and_then(Value::as_str)
        .map(str::to_string);

    Ok(Some(LineageSummaryStorageProfile {
        profile_id,
        provider,
        issuance_mode,
        location_prefix_hash,
        secret_ref_present,
        secret_ref_provider,
        secret_ref_hash,
    }))
}

pub(crate) fn validate_lineage_summary_credential_secret_ref_presence(
    event: &OutboxEvent,
    payload: &Value,
    storage_profile: &Option<LineageSummaryStorageProfile>,
) -> Result<(), LakeCatError> {
    if !matches!(
        event.event_type.as_str(),
        "credentials.vend-attempted" | "credentials.summary-only"
    ) {
        return Ok(());
    }
    let Some(storage_profile) = storage_profile else {
        return Ok(());
    };
    validate_required_bool_field_equals(
        event,
        payload,
        "secret-ref-present",
        storage_profile.secret_ref_present,
        "lineage drain credential summary",
    )
}

pub(crate) fn lineage_summary_string_array_field(
    event: &OutboxEvent,
    object: &Value,
    field: &str,
) -> Result<Vec<String>, LakeCatError> {
    let values =
        optional_array_field(event, object, field)?.expect("field presence was checked by caller");
    let mut strings = Vec::with_capacity(values.len());
    for value in values {
        let Some(string) = value.as_str() else {
            return Err(outbox_evidence_error(
                event,
                &format!("{field} must contain strings"),
            ));
        };
        if string.trim().is_empty() {
            return Err(outbox_evidence_error(
                event,
                &format!("{field} must not contain blank strings"),
            ));
        }
        strings.push(string.to_string());
    }
    validate_unique_string_array(event, &strings, field)?;
    Ok(strings)
}

pub(crate) fn lineage_summary_optional_count_alias_field(
    event: &OutboxEvent,
    object: &Value,
    fields: &[&str],
) -> Result<Option<usize>, LakeCatError> {
    for field in fields {
        if object.get(*field).is_some() {
            return lineage_summary_optional_count_field(event, object, field);
        }
    }
    Ok(None)
}

pub(crate) fn lineage_summary_optional_count_field(
    event: &OutboxEvent,
    object: &Value,
    field: &str,
) -> Result<Option<usize>, LakeCatError> {
    let Some(value) = object.get(field) else {
        return Ok(None);
    };
    let Some(count) = value.as_u64() else {
        return Err(outbox_evidence_error(
            event,
            &format!("{field} must be an unsigned integer when present"),
        ));
    };
    usize::try_from(count).map(Some).map_err(|_| {
        outbox_evidence_error(
            event,
            &format!("{field} is too large to summarize on this platform"),
        )
    })
}

pub(crate) fn lineage_summary_positive_u64_field(
    event: &OutboxEvent,
    object: &Value,
    field: &str,
) -> Result<u64, LakeCatError> {
    let Some(value) = object.get(field) else {
        return Err(outbox_evidence_error(
            event,
            &format!("{field} must be present"),
        ));
    };
    let Some(value) = value.as_u64() else {
        return Err(outbox_evidence_error(
            event,
            &format!("{field} must be an unsigned integer when present"),
        ));
    };
    if value == 0 {
        return Err(outbox_evidence_error(
            event,
            &format!("{field} must be positive when present"),
        ));
    }
    Ok(value)
}

pub(crate) type CredentialExceptionSummary = (Option<bool>, Option<String>, Option<String>);

pub(crate) fn lineage_summary_credential_exception_fields(
    event: &OutboxEvent,
    payload: &Value,
    credential_count: Option<usize>,
) -> Result<CredentialExceptionSummary, LakeCatError> {
    let block_reason = lineage_summary_optional_nonblank_string_field(
        event,
        payload,
        "lakecat:credential-block-reason",
    )?;
    validate_raw_credential_exception_receipt_match(event, payload)?;
    let Some(raw_exception) = payload.get("lakecat:raw-credential-exception") else {
        return Ok((None, None, block_reason));
    };
    let Some(raw_exception) = raw_exception.as_object() else {
        return Err(outbox_evidence_error(
            event,
            "lakecat:raw-credential-exception must be an object when present",
        ));
    };
    for field in raw_exception.keys() {
        if !RAW_CREDENTIAL_EXCEPTION_EVIDENCE_FIELDS.contains(&field.as_str()) {
            return Err(outbox_evidence_error(
                event,
                &format!("lakecat:raw-credential-exception contains unexpected field {field}"),
            ));
        }
    }
    let Some(allowed) = raw_exception.get("allowed") else {
        return Err(outbox_evidence_error(
            event,
            "lakecat:raw-credential-exception.allowed must be present",
        ));
    };
    let Some(allowed) = allowed.as_bool() else {
        return Err(outbox_evidence_error(
            event,
            "lakecat:raw-credential-exception.allowed must be boolean when present",
        ));
    };
    let reason = raw_exception
        .get("reason")
        .map(|value| {
            let Some(reason) = value.as_str() else {
                return Err(outbox_evidence_error(
                    event,
                    "lakecat:raw-credential-exception.reason must be a string when present",
                ));
            };
            if reason.trim().is_empty() {
                return Err(outbox_evidence_error(
                    event,
                    "lakecat:raw-credential-exception.reason must not be blank",
                ));
            }
            Ok(reason.to_string())
        })
        .transpose()?;

    if allowed {
        if block_reason.is_some() {
            return Err(outbox_evidence_error(
                event,
                "lakecat:credential-block-reason must be absent when raw credentials are allowed",
            ));
        }
        return Ok((Some(true), reason, None));
    }

    if credential_count.is_some_and(|count| count != 0) {
        return Err(outbox_evidence_error(
            event,
            "blocked raw-credential exception must not carry credentials",
        ));
    }
    let Some(reason) = reason else {
        return Err(outbox_evidence_error(
            event,
            "blocked raw-credential exception must carry non-empty reason",
        ));
    };
    let Some(block_reason) = block_reason else {
        return Err(outbox_evidence_error(
            event,
            "blocked credential evidence must contain block reason",
        ));
    };
    if block_reason != reason {
        return Err(outbox_evidence_error(
            event,
            "credential block reason must match raw-credential exception reason",
        ));
    }
    Ok((Some(false), None, Some(block_reason)))
}

pub(crate) fn lineage_summary_authorization_principal_fields(
    event: &OutboxEvent,
    authorization_receipt: Option<&Value>,
) -> Result<(Option<String>, Option<String>), LakeCatError> {
    let Some(authorization_receipt) = authorization_receipt else {
        return Ok((None, None));
    };
    let Some(principal) = authorization_receipt.get("principal") else {
        return Ok((None, None));
    };
    let subject = lineage_summary_optional_nonblank_string_field(event, principal, "subject")?;
    let kind = lineage_summary_optional_nonblank_string_field(event, principal, "kind")?;
    Ok((subject, kind))
}

pub(crate) fn validate_lineage_summary_commit_history_receipt_shape(
    event: &OutboxEvent,
    payload: &Value,
    authorization_receipt: Option<&Value>,
) -> Result<(), LakeCatError> {
    if event.event_type != "table.commits-listed" || authorization_receipt.is_none() {
        return Ok(());
    }
    validate_authorization_receipt_principal(event, payload, "table commit-history")?;
    Ok(())
}

pub(crate) fn lineage_summary_optional_nonblank_string_field(
    event: &OutboxEvent,
    object: &Value,
    field: &str,
) -> Result<Option<String>, LakeCatError> {
    let Some(value) = object.get(field) else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(None);
    }
    let Some(value) = value.as_str() else {
        return Err(outbox_evidence_error(
            event,
            &format!("{field} must be a string when present"),
        ));
    };
    if value.trim().is_empty() {
        return Err(outbox_evidence_error(
            event,
            &format!("{field} must not be blank"),
        ));
    }
    Ok(Some(value.to_string()))
}

pub(crate) fn lineage_summary_optional_nonblank_string_value(
    event: &OutboxEvent,
    value: Option<&Value>,
    label: &str,
) -> Result<Option<String>, LakeCatError> {
    let Some(value) = value else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(None);
    }
    let Some(value) = value.as_str() else {
        return Err(outbox_evidence_error(
            event,
            &format!("{label} must be a string when present"),
        ));
    };
    if value.trim().is_empty() {
        return Err(outbox_evidence_error(
            event,
            &format!("{label} must not be blank"),
        ));
    }
    Ok(Some(value.to_string()))
}

pub(crate) fn lineage_summary_counted_string_array_field(
    event: &OutboxEvent,
    object: &Value,
    field: &str,
    count_fields: &[&str],
) -> Result<Vec<String>, LakeCatError> {
    let strings = lineage_summary_string_array_field(event, object, field)?;
    let Some((count_field, count_value)) = count_fields
        .iter()
        .find_map(|count_field| object.get(*count_field).map(|value| (*count_field, value)))
    else {
        return Err(outbox_evidence_error(
            event,
            &format!("{field} must carry a matching count field"),
        ));
    };
    let Some(count) = count_value.as_u64() else {
        return Err(outbox_evidence_error(
            event,
            &format!("{count_field} must be an unsigned integer when {field} is present"),
        ));
    };
    let count = usize::try_from(count).map_err(|_| {
        outbox_evidence_error(
            event,
            &format!("{count_field} is too large to summarize on this platform"),
        )
    })?;
    if count != strings.len() {
        return Err(outbox_evidence_error(
            event,
            &format!("{field} count must match {count_field}"),
        ));
    }
    Ok(strings)
}

pub(crate) fn lineage_summary_namespace_parts(
    event: &OutboxEvent,
    value: Option<&Value>,
    label: &str,
) -> Result<Vec<String>, LakeCatError> {
    match value {
        None | Some(Value::Null) => Ok(Vec::new()),
        Some(Value::Array(parts)) => {
            let mut namespace = Vec::with_capacity(parts.len());
            for part in parts {
                let Some(part) = part.as_str() else {
                    return Err(outbox_evidence_error(
                        event,
                        &format!("{label} must contain strings"),
                    ));
                };
                if part.trim().is_empty() {
                    return Err(outbox_evidence_error(
                        event,
                        &format!("{label} must not contain blank strings"),
                    ));
                }
                namespace.push(part.to_string());
            }
            Ok(namespace)
        }
        Some(Value::String(path)) => {
            let mut namespace = Vec::new();
            for part in path.split('.') {
                if part.trim().is_empty() {
                    return Err(outbox_evidence_error(
                        event,
                        &format!("{label} must not contain blank strings"),
                    ));
                }
                namespace.push(part.to_string());
            }
            Ok(namespace)
        }
        Some(_) => Err(outbox_evidence_error(
            event,
            &format!("{label} must be a string or string array when present"),
        )),
    }
}
