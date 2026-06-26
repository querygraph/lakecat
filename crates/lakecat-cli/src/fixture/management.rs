use crate::*;

pub(crate) fn verify_qglake_management_list_replay(
    drain: &LineageDrainResponse,
    expected_policy_binding_count: usize,
) -> lakecat_core::LakeCatResult<()> {
    let Some(policy_list) = drain
        .events
        .iter()
        .find(|event| event.event_type == "policy-binding.listed")
    else {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain did not replay policy list evidence".to_string(),
        ));
    };
    verify_qglake_management_list_receipts(policy_list, "policy list", true)?;
    if policy_list.policy_binding_count != expected_policy_binding_count {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain policy list replay count does not match the accepted QueryGraph bundle"
                .to_string(),
        ));
    }
    verify_qglake_management_ids(
        &policy_list.policy_ids,
        policy_list.policy_binding_count,
        "policy list",
    )?;
    let Some(policy_upsert) = drain
        .events
        .iter()
        .find(|event| event.event_type == "policy-binding.upserted")
    else {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain did not replay policy binding upsert evidence".to_string(),
        ));
    };
    verify_qglake_policy_binding_upsert_replay(policy_upsert, &policy_list.policy_ids)?;
    let Some(storage_profile_list) = drain
        .events
        .iter()
        .find(|event| event.event_type == "storage-profile.listed")
    else {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain did not replay storage profile list evidence".to_string(),
        ));
    };
    verify_qglake_management_list_receipts(storage_profile_list, "storage profile list", true)?;
    if storage_profile_list
        .storage_profile_count
        .unwrap_or_default()
        == 0
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain storage profile list replay did not expose any storage profiles"
                .to_string(),
        ));
    }
    verify_qglake_management_ids(
        &storage_profile_list.storage_profile_ids,
        storage_profile_list
            .storage_profile_count
            .unwrap_or_default(),
        "storage profile list",
    )?;
    let Some(storage_profile_upsert) = drain
        .events
        .iter()
        .find(|event| event.event_type == "storage-profile.upserted")
    else {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain did not replay storage profile upsert evidence".to_string(),
        ));
    };
    verify_qglake_storage_profile_upsert_replay(storage_profile_upsert)?;
    let Some(server_list) = drain
        .events
        .iter()
        .find(|event| event.event_type == "server.listed")
    else {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain did not replay server list evidence".to_string(),
        ));
    };
    verify_qglake_management_list_receipts(server_list, "server list", false)?;
    if server_list.server_count.unwrap_or_default() == 0 {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain server list replay did not expose any servers".to_string(),
        ));
    }
    verify_qglake_management_ids(
        &server_list.server_ids,
        server_list.server_count.unwrap_or_default(),
        "server list",
    )?;
    let Some(project_list) = drain
        .events
        .iter()
        .find(|event| event.event_type == "project.listed")
    else {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain did not replay project list evidence".to_string(),
        ));
    };
    verify_qglake_management_list_receipts(project_list, "project list", false)?;
    if project_list.project_count.unwrap_or_default() == 0 {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain project list replay did not expose any projects".to_string(),
        ));
    }
    verify_qglake_management_ids(
        &project_list.project_ids,
        project_list.project_count.unwrap_or_default(),
        "project list",
    )?;
    let Some(warehouse_list) = drain
        .events
        .iter()
        .find(|event| event.event_type == "warehouse.listed")
    else {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain did not replay warehouse list evidence".to_string(),
        ));
    };
    verify_qglake_management_list_receipts(warehouse_list, "warehouse list", false)?;
    if warehouse_list.warehouse_count.unwrap_or_default() == 0 {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain warehouse list replay did not expose any warehouses".to_string(),
        ));
    }
    verify_qglake_management_ids(
        &warehouse_list.warehouse_names,
        warehouse_list.warehouse_count.unwrap_or_default(),
        "warehouse list",
    )?;
    if let Some(project_id) = warehouse_list.management_scope_project_id.as_deref() {
        if !is_qglake_compact_management_id(project_id) {
            return Err(lakecat_core::LakeCatError::InvalidArgument(
                "qglake lineage drain warehouse list replay contains syntactically invalid project scope evidence"
                    .to_string(),
            ));
        }
        if !project_list
            .project_ids
            .iter()
            .any(|listed_project_id| listed_project_id == project_id)
        {
            return Err(lakecat_core::LakeCatError::InvalidArgument(
                "qglake lineage drain warehouse list project scope does not match project list evidence"
                    .to_string(),
            ));
        }
    }
    Ok(())
}

pub(crate) fn verify_qglake_policy_binding_upsert_replay(
    event: &LineageDrainEventSummary,
    policy_ids: &[String],
) -> lakecat_core::LakeCatResult<()> {
    verify_qglake_management_list_receipts(event, "policy binding upsert", true)?;
    let Some(policy_id) = event
        .policy_id
        .as_deref()
        .filter(|policy_id| !policy_id.trim().is_empty())
    else {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain policy binding upsert replay is missing policy id evidence"
                .to_string(),
        ));
    };
    if !is_qglake_compact_management_id(policy_id) {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain policy binding upsert replay contains syntactically invalid policy id evidence"
                .to_string(),
        ));
    }
    if !policy_ids
        .iter()
        .any(|listed_policy_id| listed_policy_id == policy_id)
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain policy binding upsert policy id does not match policy list evidence"
                .to_string(),
        ));
    }
    if event
        .policy_odrl_hash
        .as_deref()
        .map_or(true, |hash| !is_full_sha256_hash(hash))
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain policy binding upsert replay is missing ODRL hash evidence"
                .to_string(),
        ));
    }
    if event
        .principal_subject
        .as_deref()
        .map_or(true, str::is_empty)
        || event.principal_kind.as_deref().map_or(true, str::is_empty)
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain policy binding upsert replay is missing principal evidence"
                .to_string(),
        ));
    }
    if event
        .authorization_receipt_hash
        .as_deref()
        .map_or(true, |hash| !is_full_sha256_hash(hash))
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain policy binding upsert replay is missing authorization receipt hash evidence"
                .to_string(),
        ));
    }
    if event.authorization_receipt_action.as_deref() != Some("policy-manage") {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain policy binding upsert replay authorization receipt action must be policy-manage"
                .to_string(),
        ));
    }
    if event.graph_events == 0 || event.lineage_events == 0 {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain policy binding upsert replay emitted no graph or lineage projection"
                .to_string(),
        ));
    }
    Ok(())
}

pub(crate) fn verify_qglake_management_ids(
    ids: &[String],
    expected_count: usize,
    label: &str,
) -> lakecat_core::LakeCatResult<()> {
    if ids.len() != expected_count || ids.iter().any(|id| id.trim().is_empty()) {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake lineage drain {label} replay is missing compact management ID evidence"
        )));
    }
    let mut seen = BTreeSet::new();
    if ids.iter().any(|id| !seen.insert(id)) {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake lineage drain {label} replay contains duplicate compact management ID evidence"
        )));
    }
    if ids.iter().any(|id| !is_qglake_compact_management_id(id)) {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake lineage drain {label} replay contains syntactically invalid compact management ID evidence"
        )));
    }
    Ok(())
}

pub(crate) fn is_qglake_compact_management_id(id: &str) -> bool {
    if id.trim() != id || id.is_empty() {
        return false;
    }
    if id == "." || id == ".." {
        return false;
    }
    if id
        .chars()
        .any(|ch| ch.is_control() || ch.is_whitespace() || matches!(ch, '/' | '\\' | '?' | '#'))
    {
        return false;
    }
    true
}

pub(crate) fn verify_qglake_table_commit_history_replay(
    drain: &LineageDrainResponse,
    principal: Option<&str>,
) -> lakecat_core::LakeCatResult<()> {
    let Some(commit_history) = drain
        .events
        .iter()
        .find(|event| event.event_type == "table.commits-listed")
    else {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain did not replay table commit history evidence".to_string(),
        ));
    };
    if commit_history.lineage_events == 0 {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain table commit history replay emitted no lineage projection"
                .to_string(),
        ));
    }
    if commit_history.graph_events == 0 {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain table commit history replay emitted no graph projection"
                .to_string(),
        ));
    }
    let Some(commit_principal_kind) = commit_history.principal_kind.as_deref() else {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain table commit history replay is missing principal kind evidence"
                .to_string(),
        ));
    };
    if commit_principal_kind != "agent" {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake lineage drain table commit history replay principal kind did not match accepted principal kind agent: actual={commit_principal_kind}"
        )));
    }
    if let Some(expected_principal) = principal {
        let Some(commit_principal) = commit_history.principal_subject.as_deref() else {
            return Err(lakecat_core::LakeCatError::InvalidArgument(
                "qglake lineage drain table commit history replay is missing principal subject evidence"
                    .to_string(),
            ));
        };
        if commit_principal != expected_principal {
            return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "qglake lineage drain table commit history replay principal did not match accepted principal {expected_principal}: actual={commit_principal}"
            )));
        }
    }
    if !qglake_has_full_sha256_hashes(&commit_history.replay_event_hashes)
        || !qglake_has_full_sha256_hashes(&commit_history.replay_open_lineage_hashes)
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain table commit history replay is missing full SHA-256 receipt hashes"
                .to_string(),
        ));
    }
    if !commit_history
        .authorization_receipt_hash
        .as_deref()
        .is_some_and(|hash| is_full_sha256_hash(hash))
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain table commit history replay is missing authorization receipt hash evidence"
                .to_string(),
        ));
    }
    if commit_history.authorization_receipt_action.as_deref() != Some("table-load") {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain table commit history replay table.commits-listed authorization receipt action does not match expected table-load"
                .to_string(),
        ));
    }
    let Some(commit_count) = commit_history.table_commit_count else {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain table commit history replay is missing compact commit summary or SHA-256 commit hash evidence"
                .to_string(),
        ));
    };
    if commit_count > 0
        && (commit_history.table_commit_sequence_numbers.is_empty()
            || !qglake_has_full_sha256_hashes(&commit_history.table_commit_hashes))
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain table commit history replay is missing compact commit summary or SHA-256 commit hash evidence"
                .to_string(),
        ));
    }
    if commit_history.table_commit_sequence_numbers.len() != commit_count
        || commit_history.table_commit_hashes.len() != commit_count
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain table commit history replay count does not match sequence-number and commit-hash evidence"
                .to_string(),
        ));
    }
    let mut unique_commit_hashes = BTreeSet::new();
    for commit_hash in &commit_history.table_commit_hashes {
        if !unique_commit_hashes.insert(commit_hash) {
            return Err(lakecat_core::LakeCatError::InvalidArgument(
                "qglake lineage drain table commit history replay commit hashes must not contain duplicates"
                    .to_string(),
            ));
        }
    }
    let mut previous_sequence = 0;
    for sequence_number in &commit_history.table_commit_sequence_numbers {
        if *sequence_number == 0 || *sequence_number <= previous_sequence {
            return Err(lakecat_core::LakeCatError::InvalidArgument(
                "qglake lineage drain table commit history replay sequence numbers must be positive and strictly increasing"
                    .to_string(),
            ));
        }
        previous_sequence = *sequence_number;
    }
    Ok(())
}

pub(crate) fn verify_qglake_storage_profile_upsert_replay(
    event: &LineageDrainEventSummary,
) -> lakecat_core::LakeCatResult<()> {
    verify_qglake_management_list_receipts(event, "storage profile upsert", true)?;
    if event
        .storage_profile_id
        .as_deref()
        .unwrap_or_default()
        .is_empty()
        || event
            .storage_profile_provider
            .as_deref()
            .unwrap_or_default()
            .is_empty()
        || event
            .storage_profile_issuance_mode
            .as_deref()
            .unwrap_or_default()
            .is_empty()
        || event
            .storage_profile_location_prefix_hash
            .as_deref()
            .map_or(true, |hash| !is_full_sha256_hash(hash))
        || event.storage_profile_secret_ref_present.is_none()
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain storage profile upsert replay did not expose redacted credential-root evidence"
                .to_string(),
        ));
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
        "qglake lineage drain storage profile upsert replay",
    )?;
    if event.storage_profile_secret_ref_present == Some(true)
        && event
            .storage_profile_secret_ref_provider
            .as_deref()
            .map_or(true, |provider| provider.trim().is_empty())
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain storage profile upsert replay is missing secret-ref provider evidence"
                .to_string(),
        ));
    }
    if event.storage_profile_secret_ref_present == Some(true)
        && event
            .storage_profile_secret_ref_hash
            .as_deref()
            .map_or(true, |hash| !is_full_sha256_hash(hash))
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain storage profile upsert replay is missing full SHA-256 secret-ref hash evidence"
                .to_string(),
        ));
    }
    if event.storage_profile_secret_ref_present == Some(false)
        && (event.storage_profile_secret_ref_provider.is_some()
            || event.storage_profile_secret_ref_hash.is_some())
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain storage profile upsert replay carried secret-ref evidence without secret-ref presence"
                .to_string(),
        ));
    }
    if event
        .principal_subject
        .as_deref()
        .map_or(true, str::is_empty)
        || event.principal_kind.as_deref().map_or(true, str::is_empty)
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain storage profile upsert replay is missing principal evidence"
                .to_string(),
        ));
    }
    if event
        .authorization_receipt_hash
        .as_deref()
        .map_or(true, |hash| !is_full_sha256_hash(hash))
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain storage profile upsert replay is missing authorization receipt hash evidence"
                .to_string(),
        ));
    }
    if event.authorization_receipt_action.as_deref() != Some("storage-profile-manage") {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain storage profile upsert replay authorization receipt action must be storage-profile-manage"
                .to_string(),
        ));
    }
    Ok(())
}

pub(crate) fn verify_qglake_storage_profile_provider_issuance_mode(
    provider: &str,
    issuance_mode: &str,
    label: &str,
) -> lakecat_core::LakeCatResult<()> {
    match issuance_mode {
        "local-file-no-secret" if provider != "file" => {
            Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "{label} local-file-no-secret issuance mode requires file provider"
            )))
        }
        "short-lived-secret-ref" if !matches!(provider, "s3" | "gcs" | "azure") => {
            Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "{label} short-lived-secret-ref issuance mode requires s3, gcs, or azure provider"
            )))
        }
        "local-file-no-secret" | "short-lived-secret-ref" => Ok(()),
        _ => Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "{label} issuance mode must be local-file-no-secret or short-lived-secret-ref"
        ))),
    }
}

pub(crate) fn verify_qglake_management_list_receipts(
    event: &LineageDrainEventSummary,
    label: &str,
    require_warehouse_scope: bool,
) -> lakecat_core::LakeCatResult<()> {
    if event.lineage_events == 0 {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake lineage drain {label} replay emitted no lineage projection"
        )));
    }
    if event.graph_events == 0 {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake lineage drain {label} replay emitted no catalog graph projection"
        )));
    }
    if require_warehouse_scope
        && event
            .management_scope_warehouse
            .as_deref()
            .map_or(true, str::is_empty)
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake lineage drain {label} replay is missing compact management scope"
        )));
    }
    if event
        .principal_subject
        .as_deref()
        .map_or(true, str::is_empty)
        || event.principal_kind.as_deref().map_or(true, str::is_empty)
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake lineage drain {label} replay is missing principal evidence"
        )));
    }
    if event
        .authorization_receipt_hash
        .as_deref()
        .map_or(true, |hash| !is_full_sha256_hash(hash))
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake lineage drain {label} replay is missing full SHA-256 authorization receipt hash evidence"
        )));
    }
    if !qglake_has_full_sha256_hashes(&event.replay_event_hashes)
        || !qglake_has_full_sha256_hashes(&event.replay_open_lineage_hashes)
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake lineage drain {label} replay is missing full SHA-256 receipt hashes"
        )));
    }
    Ok(())
}

pub(crate) fn qglake_has_full_sha256_hashes(hashes: &[String]) -> bool {
    !hashes.is_empty() && hashes.iter().all(|hash| is_full_sha256_hash(hash))
}

pub(crate) fn verify_qglake_typedid_hash_pair(
    envelope_hash: Option<&str>,
    proof_hash: Option<&str>,
    label: &str,
) -> lakecat_core::LakeCatResult<()> {
    if envelope_hash.is_some_and(|hash| !is_full_sha256_hash(hash)) {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "{label} TypeDID envelope hash must be full SHA-256-shaped"
        )));
    }
    if proof_hash.is_some_and(|hash| !is_full_sha256_hash(hash)) {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "{label} TypeDID proof hash must be full SHA-256-shaped"
        )));
    }
    if proof_hash.is_some() && envelope_hash.is_none() {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "{label} TypeDID proof hash requires an envelope hash"
        )));
    }
    Ok(())
}

pub(crate) fn standards_set(standards: &[String]) -> BTreeSet<&str> {
    standards.iter().map(String::as_str).collect()
}

pub(crate) fn qglake_policy_binding_count(bundle: &QueryGraphBootstrap) -> usize {
    bundle
        .tables
        .iter()
        .map(|table| table.policy_bindings.len())
        .sum()
}
