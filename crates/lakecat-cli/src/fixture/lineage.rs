use crate::*;

pub(crate) fn verify_qglake_lineage_drain(
    drain: &LineageDrainResponse,
    verification: &QueryGraphBootstrapVerification,
    principal: Option<&str>,
    expected_policy_binding_count: usize,
) -> lakecat_core::LakeCatResult<()> {
    if drain.delivered == 0 {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain delivered no outbox events".to_string(),
        ));
    }
    if drain.lineage_events == 0 {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain emitted no lineage events".to_string(),
        ));
    }
    if drain.graph_events == 0 {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain emitted no graph events".to_string(),
        ));
    }
    if !drain
        .event_types
        .iter()
        .any(|event_type| event_type == "querygraph.bootstrap")
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain did not replay querygraph.bootstrap".to_string(),
        ));
    }
    let expected_principal = principal.unwrap_or("anonymous");
    if drain.principal_subject.as_deref() != Some(expected_principal) {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake lineage drain read principal did not match accepted principal {expected_principal}"
        )));
    }
    let expected_principal_kind = if principal.is_some() {
        "agent"
    } else {
        "anonymous"
    };
    if drain.principal_kind.as_deref() != Some(expected_principal_kind) {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake lineage drain read principal kind did not match accepted principal kind {expected_principal_kind}"
        )));
    }
    if drain
        .authorization_receipt_hash
        .as_deref()
        .map_or(true, |hash| !is_full_sha256_hash(hash))
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain read is missing full SHA-256 authorization receipt hash"
                .to_string(),
        ));
    }
    if drain.authorization_receipt_action.as_deref() != Some("lineage-read") {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain read authorization receipt action must be lineage-read"
                .to_string(),
        ));
    }
    if drain
        .request_identity_source
        .as_deref()
        .map_or(true, str::is_empty)
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain read is missing request identity source".to_string(),
        ));
    }
    if drain
        .request_identity_state
        .as_deref()
        .map_or(true, str::is_empty)
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain read is missing request identity attestation state".to_string(),
        ));
    }
    verify_qglake_typedid_hash_pair(
        drain.typedid_envelope_hash.as_deref(),
        drain.typedid_proof_hash.as_deref(),
        "qglake lineage drain read",
    )?;
    let Some(bootstrap) = drain
        .events
        .iter()
        .find(|event| event.event_type == "querygraph.bootstrap")
    else {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain did not expose querygraph.bootstrap replay evidence".to_string(),
        ));
    };
    if bootstrap
        .bundle_hash
        .as_deref()
        .map_or(true, |hash| !is_full_sha256_hash(hash))
        || bootstrap
            .graph_hash
            .as_deref()
            .map_or(true, |hash| !is_full_sha256_hash(hash))
        || bootstrap
            .open_lineage_hash
            .as_deref()
            .map_or(true, |hash| !is_full_sha256_hash(hash))
        || bootstrap
            .querygraph_import_hash
            .as_deref()
            .map_or(true, |hash| !is_full_sha256_hash(hash))
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain replay evidence is missing full SHA-256 QueryGraph hashes"
                .to_string(),
        ));
    }
    if bootstrap.principal_subject.as_deref() != Some(expected_principal) {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake lineage drain replay principal did not match accepted principal {expected_principal}"
        )));
    }
    if bootstrap.principal_kind.as_deref() != Some(expected_principal_kind) {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake lineage drain replay principal kind did not match accepted principal kind {expected_principal_kind}"
        )));
    }
    if bootstrap
        .authorization_receipt_hash
        .as_deref()
        .map_or(true, |hash| !is_full_sha256_hash(hash))
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain replay evidence is missing full SHA-256 authorization receipt hash".to_string(),
        ));
    }
    if bootstrap
        .request_identity_source
        .as_deref()
        .map_or(true, str::is_empty)
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain replay evidence is missing request identity source".to_string(),
        ));
    }
    if bootstrap
        .request_identity_state
        .as_deref()
        .map_or(true, str::is_empty)
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain replay evidence is missing request identity attestation state"
                .to_string(),
        ));
    }
    if principal.is_some() {
        if bootstrap
            .agent_delegation_hash
            .as_deref()
            .map_or(true, |hash| !is_full_sha256_hash(hash))
        {
            return Err(lakecat_core::LakeCatError::InvalidArgument(
                "qglake lineage drain replay evidence is missing full SHA-256 agent delegation hash".to_string(),
            ));
        }
        if bootstrap
            .agent_summary_signature_hash
            .as_deref()
            .map_or(true, |hash| !is_full_sha256_hash(hash))
        {
            return Err(lakecat_core::LakeCatError::InvalidArgument(
                "qglake lineage drain replay evidence is missing full SHA-256 agent summary signature hash".to_string(),
            ));
        }
    }
    verify_qglake_typedid_hash_pair(
        bootstrap.typedid_envelope_hash.as_deref(),
        bootstrap.typedid_proof_hash.as_deref(),
        "qglake lineage drain bootstrap replay",
    )?;
    if bootstrap.bundle_hash.as_deref() != Some(verification.bundle_hash.as_str())
        || bootstrap.graph_hash.as_deref() != Some(verification.graph_hash.as_str())
        || bootstrap.open_lineage_hash.as_deref() != Some(verification.open_lineage_hash.as_str())
        || bootstrap.querygraph_import_hash.as_deref()
            != Some(verification.querygraph_import_hash.as_str())
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain replay evidence does not match the accepted QueryGraph bundle"
                .to_string(),
        ));
    }
    if bootstrap.table_artifact_count == 0 {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain replay evidence has no QueryGraph table artifacts".to_string(),
        ));
    }
    if bootstrap.table_artifact_count != verification.table_count
        || bootstrap.view_artifact_count != verification.view_count
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain replay artifact counts do not match the accepted QueryGraph bundle"
                .to_string(),
        ));
    }
    if !verification.verified_views.is_empty()
        && (bootstrap.view_version_receipt_hashes.len() != verification.verified_views.len()
            || !qglake_has_full_sha256_hashes(&bootstrap.view_version_receipt_hashes))
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain replay evidence is missing full SHA-256 view version receipt hashes"
                .to_string(),
        ));
    }
    if !verification.verified_views.is_empty() {
        let accepted_view_receipt_hashes = verification
            .verified_view_receipt_hashes
            .values()
            .map(String::as_str)
            .collect::<BTreeSet<_>>();
        let replayed_view_receipt_hashes = bootstrap
            .view_version_receipt_hashes
            .iter()
            .map(String::as_str)
            .collect::<BTreeSet<_>>();
        if accepted_view_receipt_hashes != replayed_view_receipt_hashes {
            return Err(lakecat_core::LakeCatError::InvalidArgument(
                "qglake lineage drain replay evidence view version receipt hashes do not match the accepted QueryGraph bundle".to_string(),
            ));
        }
    }
    if standards_set(&bootstrap.standards) != standards_set(&verification.standards) {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain replay standards do not match the accepted QueryGraph bundle"
                .to_string(),
        ));
    }
    if bootstrap.policy_binding_count != expected_policy_binding_count {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain replay policy binding count does not match the accepted QueryGraph bundle"
                .to_string(),
        ));
    }
    if bootstrap.lineage_events == 0 {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain bootstrap replay emitted no lineage projection".to_string(),
        ));
    }
    if !qglake_has_full_sha256_hashes(&bootstrap.replay_event_hashes)
        || !qglake_has_full_sha256_hashes(&bootstrap.replay_open_lineage_hashes)
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain querygraph.bootstrap replayEventHashes and replayOpenLineageHashes must contain full SHA-256 sink receipt hashes"
                .to_string(),
        ));
    }
    verify_qglake_view_replay(drain, verification)?;
    verify_qglake_credential_replay(drain, principal)?;
    verify_qglake_management_list_replay(drain, expected_policy_binding_count)?;
    verify_qglake_catalog_config_replay(drain)?;
    verify_qglake_credential_replay_matches_storage_profile_upsert(drain, principal)?;
    verify_qglake_table_commit_history_replay(drain, principal)?;
    verify_qglake_scan_replay(drain)?;
    require_qglake_lineage_event_types_cover_summaries(drain)?;
    require_qglake_lineage_authorization_actions_match_events(drain)?;
    require_qglake_lineage_drain_counts_match_summaries(drain)?;
    require_qglake_lineage_drain_sink_hashes_duplicate_free(drain)?;
    Ok(())
}

pub(crate) fn verify_qglake_catalog_config_replay(
    drain: &LineageDrainResponse,
) -> lakecat_core::LakeCatResult<()> {
    for event in drain
        .events
        .iter()
        .filter(|event| event.event_type == "catalog.config-read")
    {
        verify_qglake_catalog_config_defaults(event)?;
        verify_qglake_catalog_config_overrides(event)?;
        verify_qglake_catalog_config_endpoints(event)?;
    }
    Ok(())
}

pub(crate) fn verify_qglake_catalog_config_defaults(
    event: &LineageDrainEventSummary,
) -> lakecat_core::LakeCatResult<()> {
    if event.catalog_config_defaults.is_empty() {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain catalog config replay is missing config defaults".to_string(),
        ));
    }
    let mut keys = BTreeSet::new();
    for entry in &event.catalog_config_defaults {
        if entry.key.trim().is_empty() || entry.value.trim().is_empty() {
            return Err(lakecat_core::LakeCatError::InvalidArgument(
                "qglake lineage drain catalog config replay defaults must contain non-empty string key/value entries".to_string(),
            ));
        }
        if !keys.insert(entry.key.as_str()) {
            return Err(lakecat_core::LakeCatError::InvalidArgument(
                "qglake lineage drain catalog config replay defaults must not contain duplicate keys"
                    .to_string(),
            ));
        }
    }
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
        .collect::<BTreeSet<_>>();
    for key in &keys {
        if key.starts_with("lakecat.format.v4") && !allowed_v4_keys.contains(key) {
            return Err(lakecat_core::LakeCatError::InvalidArgument(
                "qglake lineage drain catalog config replay defaults contain unsupported v4 bridge keys"
                    .to_string(),
            ));
        }
    }
    for (required_key, required_value) in required {
        if !event
            .catalog_config_defaults
            .iter()
            .any(|entry| entry.key == required_key && entry.value == required_value)
        {
            return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "qglake lineage drain catalog config replay defaults must include {required_key}={required_value}"
            )));
        }
    }
    Ok(())
}

pub(crate) fn verify_qglake_catalog_config_overrides(
    event: &LineageDrainEventSummary,
) -> lakecat_core::LakeCatResult<()> {
    let mut keys = BTreeSet::new();
    for entry in &event.catalog_config_overrides {
        if entry.key.trim().is_empty() || entry.value.trim().is_empty() {
            return Err(lakecat_core::LakeCatError::InvalidArgument(
                "qglake lineage drain catalog config replay overrides must contain non-empty string key/value entries".to_string(),
            ));
        }
        if !keys.insert(entry.key.as_str()) {
            return Err(lakecat_core::LakeCatError::InvalidArgument(
                "qglake lineage drain catalog config replay overrides must not contain duplicate keys"
                    .to_string(),
            ));
        }
        if entry.key.starts_with("lakecat.format.v4") {
            return Err(lakecat_core::LakeCatError::InvalidArgument(
                "qglake lineage drain catalog config replay overrides must not contain v4 bridge keys"
                    .to_string(),
            ));
        }
    }
    Ok(())
}

pub(crate) fn verify_qglake_catalog_config_endpoints(
    event: &LineageDrainEventSummary,
) -> lakecat_core::LakeCatResult<()> {
    if event.catalog_config_endpoints.is_empty() {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain catalog config replay is missing advertised endpoints"
                .to_string(),
        ));
    }
    let mut endpoints = BTreeSet::new();
    for endpoint in &event.catalog_config_endpoints {
        if endpoint.trim().is_empty() {
            return Err(lakecat_core::LakeCatError::InvalidArgument(
                "qglake lineage drain catalog config replay endpoints must contain non-empty strings"
                    .to_string(),
            ));
        }
        if !endpoints.insert(endpoint.as_str()) {
            return Err(lakecat_core::LakeCatError::InvalidArgument(
                "qglake lineage drain catalog config replay endpoints must not contain duplicates"
                    .to_string(),
            ));
        }
    }
    for required in required_qglake_catalog_config_endpoints() {
        if !endpoints.contains(required) {
            return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "qglake lineage drain catalog config replay endpoints must include {required}"
            )));
        }
    }
    Ok(())
}

pub(crate) fn required_qglake_catalog_config_endpoints() -> [&'static str; 20] {
    [
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
    ]
}

pub(crate) fn require_qglake_lineage_authorization_actions_match_events(
    drain: &LineageDrainResponse,
) -> lakecat_core::LakeCatResult<()> {
    for event in &drain.events {
        let Some(expected_action) =
            qglake_expected_authorization_receipt_action(event.event_type.as_str())
        else {
            return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "qglake lineage drain replay summary {} has no authorization action contract",
                event.event_type
            )));
        };
        let Some(action) = event
            .authorization_receipt_action
            .as_deref()
            .filter(|action| !action.trim().is_empty())
        else {
            return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "qglake lineage drain replay summary {} is missing authorization receipt action",
                event.event_type
            )));
        };
        if action != expected_action {
            return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "qglake lineage drain replay summary {} authorization receipt action {action} does not match expected {expected_action}",
                event.event_type
            )));
        }
    }
    Ok(())
}

pub(crate) fn qglake_expected_authorization_receipt_action(
    event_type: &str,
) -> Option<&'static str> {
    match event_type {
        "catalog.config-read" => Some("catalog-config"),
        "credentials.vend-attempted" => Some("credentials-vend"),
        "namespace.created" => Some("namespace-create"),
        "namespace.dropped" => Some("namespace-drop"),
        "namespace.listed" => Some("namespace-list"),
        "namespace.loaded" => Some("namespace-load"),
        "policy-binding.listed" | "policy-binding.upserted" => Some("policy-manage"),
        "project.listed" | "project.upserted" => Some("project-manage"),
        "querygraph.bootstrap" => Some("graph-read"),
        "server.listed" | "server.upserted" => Some("server-manage"),
        "storage-profile.listed" | "storage-profile.upserted" => Some("storage-profile-manage"),
        "table.commit" => Some("table-commit"),
        "table.commits-listed" | "table.loaded" => Some("table-load"),
        "table.created" => Some("table-create"),
        "table.deleted" => Some("table-drop"),
        "table.restored" => Some("table-restore"),
        "table.scan-planned" | "table.scan-tasks-fetched" => Some("table-plan-scan"),
        "view.dropped" => Some("view-drop"),
        "view.listed"
        | "view.loaded"
        | "view.version-receipts-listed"
        | "view.version-receipt-chains-listed" => Some("view-load"),
        "view.upserted" => Some("view-manage"),
        "warehouse.listed" | "warehouse.upserted" => Some("warehouse-manage"),
        _ => None,
    }
}

pub(crate) fn require_qglake_lineage_drain_sink_hashes_duplicate_free(
    drain: &LineageDrainResponse,
) -> lakecat_core::LakeCatResult<()> {
    for event in &drain.events {
        require_qglake_duplicate_free_strings(
            &event.replay_event_hashes,
            &format!(
                "qglake lineage drain {} replayEventHashes",
                event.event_type
            ),
        )?;
        require_qglake_duplicate_free_strings(
            &event.replay_open_lineage_hashes,
            &format!(
                "qglake lineage drain {} openLineageHashes",
                event.event_type
            ),
        )?;
        require_qglake_duplicate_free_strings(
            &event.view_version_receipt_hashes,
            &format!(
                "qglake lineage drain {} viewVersionReceiptHashes",
                event.event_type
            ),
        )?;
        require_qglake_duplicate_free_strings(
            &event.view_version_receipt_chain_hashes,
            &format!(
                "qglake lineage drain {} viewVersionReceiptChainHashes",
                event.event_type
            ),
        )?;
    }
    Ok(())
}

pub(crate) fn require_qglake_duplicate_free_strings(
    values: &[String],
    label: &str,
) -> lakecat_core::LakeCatResult<()> {
    let mut seen = BTreeSet::new();
    for value in values {
        if !seen.insert(value.as_str()) {
            return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "{label} must be duplicate-free"
            )));
        }
    }
    Ok(())
}

pub(crate) fn require_qglake_lineage_drain_counts_match_summaries(
    drain: &LineageDrainResponse,
) -> lakecat_core::LakeCatResult<()> {
    if drain.delivered != drain.event_types.len() {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake lineage drain delivered count {} does not match eventTypes count {}",
            drain.delivered,
            drain.event_types.len()
        )));
    }
    if drain.delivered != drain.events.len() {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake lineage drain delivered count {} does not match replay summary count {}",
            drain.delivered,
            drain.events.len()
        )));
    }
    let mut event_ids = BTreeSet::new();
    for (index, event) in drain.events.iter().enumerate() {
        if event.event_id.trim().is_empty() {
            return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "qglake lineage drain replay summary event id at index {index} must be non-empty"
            )));
        }
        if !event_ids.insert(event.event_id.as_str()) {
            return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "qglake lineage drain replay summary event id at index {index} must be duplicate-free"
            )));
        }
    }
    let summed_graph_events = drain
        .events
        .iter()
        .map(|event| event.graph_events)
        .sum::<usize>();
    if drain.graph_events != summed_graph_events {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake lineage drain graphEvents count {} does not match replay summary graph event count {}",
            drain.graph_events, summed_graph_events
        )));
    }
    let summed_lineage_events = drain
        .events
        .iter()
        .map(|event| event.lineage_events)
        .sum::<usize>();
    if drain.lineage_events != summed_lineage_events {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake lineage drain lineageEvents count {} does not match replay summary lineage event count {}",
            drain.lineage_events, summed_lineage_events
        )));
    }
    Ok(())
}

pub(crate) fn require_qglake_lineage_event_types_cover_summaries(
    drain: &LineageDrainResponse,
) -> lakecat_core::LakeCatResult<()> {
    let declared_event_types = drain.event_types.iter().collect::<BTreeSet<_>>();
    for summary in &drain.events {
        if !declared_event_types.contains(&summary.event_type) {
            return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "qglake lineage drain replay summary {} was not declared in event types",
                summary.event_type
            )));
        }
    }
    let declared_counts = qglake_event_type_counts(drain.event_types.iter().map(String::as_str));
    let summary_counts =
        qglake_event_type_counts(drain.events.iter().map(|event| event.event_type.as_str()));
    if declared_counts != summary_counts {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain eventTypes multiset does not match replay summary event types"
                .to_string(),
        ));
    }
    for (index, (declared, summary)) in drain.event_types.iter().zip(&drain.events).enumerate() {
        if declared != &summary.event_type {
            return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "qglake lineage drain eventTypes order drift at index {index}: declared {declared} but replay summary is {}",
                summary.event_type
            )));
        }
    }
    Ok(())
}

pub(crate) fn qglake_event_type_counts<'a>(
    event_types: impl IntoIterator<Item = &'a str>,
) -> BTreeMap<&'a str, usize> {
    let mut counts = BTreeMap::new();
    for event_type in event_types {
        *counts.entry(event_type).or_default() += 1;
    }
    counts
}
