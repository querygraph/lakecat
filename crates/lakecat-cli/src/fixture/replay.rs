use crate::*;

pub(crate) fn verify_qglake_scan_replay(
    drain: &LineageDrainResponse,
) -> lakecat_core::LakeCatResult<()> {
    let planned = qglake_drain_event(drain, "table.scan-planned").ok_or_else(|| {
        lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain did not replay scan planning evidence".to_string(),
        )
    })?;
    if planned.lineage_events == 0
        || planned.graph_events == 0
        || planned
            .authorization_receipt_hash
            .as_deref()
            .map_or(true, |hash| !is_full_sha256_hash(hash))
        || planned
            .request_identity_state
            .as_deref()
            .map_or(true, str::is_empty)
        || !qglake_has_full_sha256_hashes(&planned.replay_event_hashes)
        || !qglake_has_full_sha256_hashes(&planned.replay_open_lineage_hashes)
        || planned.scan_task_count.unwrap_or_default() == 0
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain scan planning replay is missing compact task, graph, or SHA-256 receipt evidence"
                .to_string(),
        ));
    }

    let fetched = qglake_drain_event(drain, "table.scan-tasks-fetched").ok_or_else(|| {
        lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain did not replay scan task fetch evidence".to_string(),
        )
    })?;
    if fetched.lineage_events == 0
        || fetched
            .authorization_receipt_hash
            .as_deref()
            .map_or(true, |hash| !is_full_sha256_hash(hash))
        || fetched
            .request_identity_state
            .as_deref()
            .map_or(true, str::is_empty)
        || !qglake_has_full_sha256_hashes(&fetched.replay_event_hashes)
        || !qglake_has_full_sha256_hashes(&fetched.replay_open_lineage_hashes)
        || fetched.file_scan_task_count.unwrap_or_default() == 0
        || fetched.delete_file_count.unwrap_or_default() == 0
        || fetched.child_plan_task_count.unwrap_or_default() == 0
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain scan task fetch replay is missing compact file/delete task or SHA-256 receipt evidence"
                .to_string(),
        ));
    }
    verify_qglake_scan_restriction_replay(planned, fetched)?;
    Ok(())
}

pub(crate) fn verify_qglake_scan_restriction_replay(
    planned: &LineageDrainEventSummary,
    fetched: &LineageDrainEventSummary,
) -> lakecat_core::LakeCatResult<()> {
    let planned_restriction = qglake_lineage_drain_read_restriction(planned, "scan planning")?;
    let fetched_restriction = qglake_lineage_drain_read_restriction(fetched, "scan task fetch")?;
    require_read_restriction_evidence(
        planned_restriction,
        "qglake lineage drain scan planning read restriction",
    )?;
    require_read_restriction_evidence(
        fetched_restriction,
        "qglake lineage drain scan task fetch read restriction",
    )?;
    for field in [
        "policy-hashes",
        "allowed-columns",
        "row-predicate",
        "purpose",
        "max-credential-ttl-seconds",
    ] {
        require_value_match(
            planned_restriction,
            field,
            required_value(
                fetched_restriction,
                field,
                "qglake lineage drain scan task fetch read restriction",
            )?,
            "qglake lineage drain scan planning read restriction",
        )?;
    }

    let fetched_allowed_columns = required_value(
        fetched_restriction,
        "allowed-columns",
        "qglake lineage drain scan task fetch read restriction",
    )?;
    let fetched_projection = Value::Array(
        fetched
            .required_projection
            .iter()
            .cloned()
            .map(Value::String)
            .collect(),
    );
    if &fetched_projection != fetched_allowed_columns {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain scan task fetch replay required projection does not match fetched read restriction"
                .to_string(),
        ));
    }
    let fetched_effective_projection = Value::Array(
        fetched
            .effective_projection
            .iter()
            .cloned()
            .map(Value::String)
            .collect(),
    );
    if &fetched_effective_projection != fetched_allowed_columns {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain scan task fetch replay effective projection does not match fetched read restriction"
                .to_string(),
        ));
    }

    let fetched_row_predicate = required_value(
        fetched_restriction,
        "row-predicate",
        "qglake lineage drain scan task fetch read restriction",
    )?;
    let expected_filters = vec![fetched_row_predicate.clone()];
    if fetched.required_filters.as_slice() != expected_filters.as_slice() {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain scan task fetch replay required filters do not exactly preserve fetched row predicate"
                .to_string(),
        ));
    }
    if planned.requested_projection.is_empty() || planned.effective_projection.is_empty() {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain scan planning replay is missing requested/effective projection evidence"
                .to_string(),
        ));
    }
    require_non_empty_unique_strings(
        &planned.requested_projection,
        "qglake lineage drain scan planning requested projection",
    )?;
    require_non_empty_unique_strings(
        &planned.effective_projection,
        "qglake lineage drain scan planning effective projection",
    )?;
    if planned.requested_projection.len() <= planned.effective_projection.len() {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain scan planning replay does not prove projection narrowing"
                .to_string(),
        ));
    }
    let requested_projection = planned
        .requested_projection
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    for field in &planned.effective_projection {
        if !requested_projection.contains(field.as_str()) {
            return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "qglake lineage drain scan planning replay effective projection field {field} was not requested"
            )));
        }
    }
    let effective_projection = Value::Array(
        planned
            .effective_projection
            .iter()
            .cloned()
            .map(Value::String)
            .collect(),
    );
    let planned_allowed_columns = required_value(
        planned_restriction,
        "allowed-columns",
        "qglake lineage drain scan planning read restriction",
    )?;
    if &effective_projection != planned_allowed_columns {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain scan planning replay effective projection does not match planned read restriction"
                .to_string(),
        ));
    }
    if planned.requested_stats_fields.is_empty() || planned.effective_stats_fields.is_empty() {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain scan planning replay is missing requested/effective stats-field evidence"
                .to_string(),
        ));
    }
    require_non_empty_unique_strings(
        &planned.requested_stats_fields,
        "qglake lineage drain scan planning requested stats fields",
    )?;
    require_non_empty_unique_strings(
        &planned.effective_stats_fields,
        "qglake lineage drain scan planning effective stats fields",
    )?;
    if planned.requested_stats_fields.len() <= planned.effective_stats_fields.len() {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain scan planning replay does not prove stats-field narrowing"
                .to_string(),
        ));
    }
    let requested_stats = planned
        .requested_stats_fields
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    for field in &planned.effective_stats_fields {
        if !requested_stats.contains(field.as_str()) {
            return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "qglake lineage drain scan planning replay effective stats field {field} was not requested"
            )));
        }
    }
    let effective_stats = Value::Array(
        planned
            .effective_stats_fields
            .iter()
            .cloned()
            .map(Value::String)
            .collect(),
    );
    let planned_allowed_columns = required_value(
        planned_restriction,
        "allowed-columns",
        "qglake lineage drain scan planning read restriction",
    )?;
    if &effective_stats != planned_allowed_columns {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain scan planning replay effective stats fields do not match planned read restriction"
                .to_string(),
        ));
    }
    Ok(())
}

pub(crate) fn qglake_lineage_drain_read_restriction<'a>(
    event: &'a LineageDrainEventSummary,
    label: &str,
) -> lakecat_core::LakeCatResult<&'a serde_json::Map<String, Value>> {
    event
        .read_restriction
        .as_ref()
        .and_then(Value::as_object)
        .ok_or_else(|| {
            lakecat_core::LakeCatError::InvalidArgument(format!(
                "qglake lineage drain {label} replay is missing governed read restriction evidence"
            ))
        })
}

pub(crate) fn verify_qglake_replay_artifacts(
    bundle: &QueryGraphBootstrap,
    drain: &LineageDrainResponse,
    principal: Option<&str>,
) -> lakecat_core::LakeCatResult<QueryGraphBootstrapVerification> {
    let verification = bundle.verify_manifest()?;
    verify_qglake_querygraph_import_contract(bundle)?;
    verify_qglake_lineage_drain(
        drain,
        &verification,
        principal,
        qglake_policy_binding_count(bundle),
    )?;
    Ok(verification)
}

pub(crate) fn verify_qglake_view_replay(
    drain: &LineageDrainResponse,
    verification: &QueryGraphBootstrapVerification,
) -> lakecat_core::LakeCatResult<()> {
    for view_stable_id in &verification.verified_views {
        let Some(view_replay) = drain.events.iter().find(|event| {
            matches!(
                event.event_type.as_str(),
                "view.upserted" | "view.loaded" | "view.dropped"
            ) && event.view_stable_id.as_deref() == Some(view_stable_id.as_str())
        }) else {
            return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "qglake lineage drain did not replay view evidence for {view_stable_id}"
            )));
        };
        if view_replay.graph_events == 0 || view_replay.lineage_events == 0 {
            return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "qglake lineage drain view replay for {view_stable_id} did not emit graph and lineage projections"
            )));
        }
        if let Some(expected_version) = verification.verified_view_versions.get(view_stable_id) {
            if view_replay.view_version != Some(*expected_version) {
                return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                    "qglake lineage drain view replay for {view_stable_id} did not preserve accepted view version {expected_version}"
                )));
            }
        }
        if view_replay
            .view_warehouse
            .as_deref()
            .map_or(true, str::is_empty)
            || view_replay.view_namespace.is_empty()
            || view_replay.view_name.as_deref().map_or(true, str::is_empty)
            || !qglake_has_full_sha256_hashes(&view_replay.replay_event_hashes)
            || !qglake_has_full_sha256_hashes(&view_replay.replay_open_lineage_hashes)
        {
            return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "qglake lineage drain view replay for {view_stable_id} is missing compact identity or full SHA-256 receipt hashes"
            )));
        }
        if drain.events.iter().any(|event| {
            event.event_type == "view.dropped"
                && event.view_stable_id.as_deref() == Some(view_stable_id.as_str())
        }) {
            let drop_replay = drain.events.iter().find(|event| {
                event.event_type == "view.dropped"
                    && event.view_stable_id.as_deref() == Some(view_stable_id.as_str())
            });
            if let (Some(drop_replay), Some(expected_version)) = (
                drop_replay,
                verification.verified_view_versions.get(view_stable_id),
            ) {
                if drop_replay.expected_view_version != Some(*expected_version) {
                    return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                        "qglake lineage drain view drop replay for {view_stable_id} did not preserve expected view version {expected_version}"
                    )));
                }
            }
            let Some(tombstone_receipts) = drain.events.iter().find(|event| {
                event.event_type == "view.version-receipts-listed"
                    && event.view_stable_id.as_deref() == Some(view_stable_id.as_str())
                    && qglake_has_full_sha256_hashes(&event.view_version_receipt_hashes)
            }) else {
                return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                    "qglake lineage drain view drop replay for {view_stable_id} is missing full SHA-256 tombstone receipt evidence"
                )));
            };
            if tombstone_receipts.lineage_events == 0 {
                return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                    "qglake lineage drain tombstone receipt replay for {view_stable_id} emitted no lineage projection"
                )));
            }
            let Some(receipt_chain_read) = drain.events.iter().find(|event| {
                event.event_type == "view.version-receipt-chains-listed"
                    && event.view_warehouse == view_replay.view_warehouse
                    && event.view_namespace == view_replay.view_namespace
                    && qglake_has_full_sha256_hashes(&event.view_version_receipt_chain_hashes)
                    && event.view_version_receipt_chain_verified_count > 0
                    && qglake_has_full_sha256_hashes(&event.view_version_receipt_hashes)
            }) else {
                return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                    "qglake lineage drain view drop replay for {view_stable_id} is missing full SHA-256 namespace receipt-chain evidence for the accepted view namespace"
                )));
            };
            if receipt_chain_read.view_version_receipt_chain_hashes.len()
                != receipt_chain_read.view_version_receipt_chain_verified_count
            {
                return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                    "qglake lineage drain namespace receipt-chain replay for {view_stable_id} verified-chain count does not match chain hash evidence"
                )));
            }
            if receipt_chain_read.view_version_receipt_hashes.len()
                < receipt_chain_read.view_version_receipt_chain_hashes.len()
            {
                return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                    "qglake lineage drain namespace receipt-chain replay for {view_stable_id} receipt hashes do not cover verified chain hashes"
                )));
            }
            let tombstone_hashes = tombstone_receipts
                .view_version_receipt_hashes
                .iter()
                .collect::<BTreeSet<_>>();
            let receipt_chain_hashes = receipt_chain_read
                .view_version_receipt_hashes
                .iter()
                .collect::<BTreeSet<_>>();
            if !tombstone_hashes.is_subset(&receipt_chain_hashes) {
                return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                    "qglake lineage drain view drop replay for {view_stable_id} tombstone receipt hashes are not covered by namespace receipt-chain evidence"
                )));
            }
            if receipt_chain_read.lineage_events == 0
                || !qglake_has_full_sha256_hashes(&receipt_chain_read.replay_event_hashes)
                || !qglake_has_full_sha256_hashes(&receipt_chain_read.replay_open_lineage_hashes)
            {
                return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                    "qglake lineage drain namespace receipt-chain replay for {view_stable_id} is missing chain, lineage, or full SHA-256 sink receipt hashes"
                )));
            }
        }
    }
    Ok(())
}

pub(crate) fn verify_qglake_credential_replay(
    drain: &LineageDrainResponse,
    principal: Option<&str>,
) -> lakecat_core::LakeCatResult<()> {
    let expected_restricted_subject = principal.unwrap_or("anonymous");
    let expected_restricted_kind = if principal.is_some() {
        "agent"
    } else {
        "anonymous"
    };
    let Some(restricted_probe) =
        qglake_credential_event(drain, expected_restricted_subject, expected_restricted_kind)
    else {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain did not replay the restricted credential probe".to_string(),
        ));
    };
    if restricted_probe.credential_count != Some(0)
        || restricted_probe.raw_credential_exception_allowed != Some(false)
        || restricted_probe.credential_block_reason.as_deref()
            != Some(QGLAKE_RESTRICTED_CREDENTIAL_BLOCK_REASON)
        || restricted_probe.raw_credential_exception_reason.is_some()
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain restricted credential replay did not prove raw credentials were blocked"
                .to_string(),
        ));
    }
    verify_qglake_credential_lineage_projection(restricted_probe, "restricted")?;

    let Some(human_probe) = qglake_credential_event(drain, "human:qglake-operator", "human") else {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain did not replay the trusted human credential probe".to_string(),
        ));
    };
    if human_probe.credential_count.unwrap_or_default() == 0
        || human_probe.raw_credential_exception_allowed != Some(true)
        || human_probe.credential_block_reason.is_some()
        || human_probe.raw_credential_exception_reason.as_deref()
            != Some(QGLAKE_HUMAN_RAW_CREDENTIAL_EXCEPTION_REASON)
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain trusted human credential replay did not prove audited standard credential vending"
                .to_string(),
        ));
    }
    verify_qglake_credential_lineage_projection(human_probe, "trusted human")?;
    let restricted_ttl = qglake_event_max_credential_ttl_seconds(restricted_probe).ok_or_else(|| {
        lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain restricted credential replay is missing max credential TTL evidence"
                .to_string(),
        )
    })?;
    let human_ttl = qglake_event_max_credential_ttl_seconds(human_probe).ok_or_else(|| {
        lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain trusted human credential replay is missing max credential TTL evidence"
                .to_string(),
        )
    })?;
    if human_ttl != restricted_ttl {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake lineage drain trusted human credential replay TTL cap mismatch: expected={restricted_ttl} actual={human_ttl}"
        )));
    }
    verify_qglake_credential_restriction_match(restricted_probe, human_probe)?;
    Ok(())
}

pub(crate) fn verify_qglake_credential_lineage_projection(
    event: &LineageDrainEventSummary,
    label: &str,
) -> lakecat_core::LakeCatResult<()> {
    verify_qglake_credential_prefix_hashes(event, label)?;
    if event.lineage_events == 0 {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake lineage drain {label} credential replay emitted no lineage projection"
        )));
    }
    if event.graph_events == 0 {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake lineage drain {label} credential replay emitted no credential-root graph projection"
        )));
    }
    if qglake_event_max_credential_ttl_seconds(event).is_none() {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake lineage drain {label} credential replay is missing max credential TTL evidence"
        )));
    }
    let Some(authorization_receipt_hash) = event
        .authorization_receipt_hash
        .as_deref()
        .filter(|hash| is_full_sha256_hash(hash))
    else {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake lineage drain {label} credential replay is missing full SHA-256 authorization receipt hash evidence"
        )));
    };
    if authorization_receipt_hash.trim().is_empty() {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake lineage drain {label} credential replay authorization receipt hash must be non-empty"
        )));
    }
    if event.authorization_receipt_action.as_deref() != Some("credentials-vend") {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake lineage drain {label} credential replay authorization receipt action must be credentials-vend"
        )));
    }
    let restriction = qglake_lineage_drain_read_restriction(event, &format!("{label} credential"))?;
    require_read_restriction_evidence(
        restriction,
        &format!("qglake lineage drain {label} credential read restriction"),
    )?;
    verify_qglake_credential_storage_profile_projection(event, label)?;
    if !qglake_has_full_sha256_hashes(&event.replay_event_hashes)
        || !qglake_has_full_sha256_hashes(&event.replay_open_lineage_hashes)
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake lineage drain {label} credential replay is missing full SHA-256 sink receipt hashes"
        )));
    }
    Ok(())
}

pub(crate) fn verify_qglake_credential_prefix_hashes(
    event: &LineageDrainEventSummary,
    label: &str,
) -> lakecat_core::LakeCatResult<()> {
    let credential_count = event.credential_count.ok_or_else(|| {
        lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake lineage drain {label} credential replay is missing credential count evidence"
        ))
    })?;
    if event.credential_prefix_hashes.len() != credential_count {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake lineage drain {label} credential replay credentialPrefixHashes count mismatch: expected={credential_count} actual={}",
            event.credential_prefix_hashes.len()
        )));
    }
    if credential_count == 0 {
        return Ok(());
    }
    if !qglake_has_full_sha256_hashes(&event.credential_prefix_hashes) {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake lineage drain {label} credential replay is missing full SHA-256 credential prefix hash evidence"
        )));
    }
    require_qglake_duplicate_free_strings(
        &event.credential_prefix_hashes,
        &format!("qglake lineage drain {label} credential credentialPrefixHashes"),
    )
}

pub(crate) fn verify_qglake_credential_restriction_match(
    restricted: &LineageDrainEventSummary,
    human: &LineageDrainEventSummary,
) -> lakecat_core::LakeCatResult<()> {
    let restricted_restriction =
        qglake_lineage_drain_read_restriction(restricted, "restricted credential")?;
    let human_restriction =
        qglake_lineage_drain_read_restriction(human, "trusted human credential")?;
    for field in [
        "policy-hashes",
        "allowed-columns",
        "row-predicate",
        "purpose",
        "max-credential-ttl-seconds",
    ] {
        require_value_match(
            restricted_restriction,
            field,
            required_value(
                human_restriction,
                field,
                "qglake lineage drain trusted human credential read restriction",
            )?,
            "qglake lineage drain restricted credential read restriction",
        )?;
    }
    Ok(())
}

pub(crate) fn verify_qglake_credential_storage_profile_projection(
    event: &LineageDrainEventSummary,
    label: &str,
) -> lakecat_core::LakeCatResult<()> {
    if event
        .storage_profile_id
        .as_deref()
        .map_or(true, str::is_empty)
        || event
            .storage_profile_provider
            .as_deref()
            .map_or(true, str::is_empty)
        || event
            .storage_profile_issuance_mode
            .as_deref()
            .map_or(true, str::is_empty)
        || event
            .storage_profile_location_prefix_hash
            .as_deref()
            .map_or(true, |hash| !is_full_sha256_hash(hash))
        || event.storage_profile_secret_ref_present.is_none()
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake lineage drain {label} credential replay is missing redacted storage-profile graph evidence"
        )));
    }
    verify_qglake_storage_profile_provider_issuance_mode(
        event
            .storage_profile_provider
            .as_deref()
            .unwrap_or_default(),
        event
            .storage_profile_issuance_mode
            .as_deref()
            .unwrap_or_default(),
        &format!("qglake lineage drain {label} credential replay storage-profile"),
    )?;
    if event.storage_profile_secret_ref_present == Some(false)
        && event.storage_profile_secret_ref_provider.is_some()
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake lineage drain {label} credential replay carried a secret-ref provider without secret-ref presence"
        )));
    }
    if event.storage_profile_secret_ref_present == Some(false)
        && event.storage_profile_secret_ref_hash.is_some()
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake lineage drain {label} credential replay carried a secret-ref hash without secret-ref presence"
        )));
    }
    if event.storage_profile_secret_ref_present == Some(true)
        && event
            .storage_profile_secret_ref_provider
            .as_deref()
            .map_or(true, |provider| provider.trim().is_empty())
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake lineage drain {label} credential replay is missing secret-ref provider evidence"
        )));
    }
    if event.storage_profile_secret_ref_present == Some(true)
        && event
            .storage_profile_secret_ref_hash
            .as_deref()
            .map_or(true, |hash| !is_full_sha256_hash(hash))
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake lineage drain {label} credential replay is missing full SHA-256 secret-ref hash evidence"
        )));
    }
    Ok(())
}

pub(crate) fn verify_qglake_credential_replay_matches_storage_profile_upsert(
    drain: &LineageDrainResponse,
    principal: Option<&str>,
) -> lakecat_core::LakeCatResult<()> {
    let Some(storage_profile_upsert) = drain
        .events
        .iter()
        .find(|event| event.event_type == "storage-profile.upserted")
    else {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain did not replay storage profile upsert evidence".to_string(),
        ));
    };
    let expected_restricted_subject = principal.unwrap_or("anonymous");
    let expected_restricted_kind = if principal.is_some() {
        "agent"
    } else {
        "anonymous"
    };
    let Some(restricted_probe) =
        qglake_credential_event(drain, expected_restricted_subject, expected_restricted_kind)
    else {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain did not replay the restricted credential probe".to_string(),
        ));
    };
    verify_qglake_credential_storage_profile_matches_upsert(
        restricted_probe,
        storage_profile_upsert,
        "restricted",
    )?;

    let Some(human_probe) = qglake_credential_event(drain, "human:qglake-operator", "human") else {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain did not replay the trusted human credential probe".to_string(),
        ));
    };
    verify_qglake_credential_storage_profile_matches_upsert(
        human_probe,
        storage_profile_upsert,
        "trusted human",
    )
}

pub(crate) fn verify_qglake_credential_storage_profile_matches_upsert(
    credential: &LineageDrainEventSummary,
    storage_profile_upsert: &LineageDrainEventSummary,
    label: &str,
) -> lakecat_core::LakeCatResult<()> {
    if credential.storage_profile_id != storage_profile_upsert.storage_profile_id
        || credential.storage_profile_provider != storage_profile_upsert.storage_profile_provider
        || credential.storage_profile_issuance_mode
            != storage_profile_upsert.storage_profile_issuance_mode
        || credential.storage_profile_location_prefix_hash
            != storage_profile_upsert.storage_profile_location_prefix_hash
        || credential.storage_profile_secret_ref_present
            != storage_profile_upsert.storage_profile_secret_ref_present
        || credential.storage_profile_secret_ref_provider
            != storage_profile_upsert.storage_profile_secret_ref_provider
        || credential.storage_profile_secret_ref_hash
            != storage_profile_upsert.storage_profile_secret_ref_hash
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake lineage drain {label} credential replay storage-profile evidence does not match storage profile upsert replay"
        )));
    }
    Ok(())
}
