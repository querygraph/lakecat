use crate::*;

pub(crate) fn qglake_management_replay_line(drain: &LineageDrainResponse) -> Option<String> {
    let storage_profile_upsert = qglake_drain_event(drain, "storage-profile.upserted")?;
    let credential_root = qglake_storage_profile_upsert_line(storage_profile_upsert)?;
    let policy_upsert = qglake_drain_event(drain, "policy-binding.upserted")?;
    let policy = qglake_policy_upsert_line(policy_upsert)?;
    Some(format!(
        "management replay servers={} projects={} warehouses={} policies={} policy_upserts={} policy={} storage_profiles={} storage_profile_upserts={} credential_root={}",
        qglake_drain_event(drain, "server.listed")?
            .server_count
            .unwrap_or_default(),
        qglake_drain_event(drain, "project.listed")?
            .project_count
            .unwrap_or_default(),
        qglake_drain_event(drain, "warehouse.listed")?
            .warehouse_count
            .unwrap_or_default(),
        qglake_drain_event(drain, "policy-binding.listed")?.policy_binding_count,
        usize::from(policy_upsert.policy_id.is_some()),
        policy,
        qglake_drain_event(drain, "storage-profile.listed")?
            .storage_profile_count
            .unwrap_or_default(),
        usize::from(storage_profile_upsert.storage_profile_id.is_some()),
        credential_root
    ))
}

pub(crate) fn qglake_policy_upsert_line(event: &LineageDrainEventSummary) -> Option<String> {
    let policy_id = event.policy_id.as_deref()?.trim();
    let odrl_hash = event.policy_odrl_hash.as_deref()?.trim();
    if policy_id.is_empty() || !is_full_sha256_hash(odrl_hash) {
        return None;
    }
    Some(format!("{policy_id}:odrl_hash={odrl_hash}"))
}

pub(crate) fn qglake_storage_profile_upsert_line(
    event: &LineageDrainEventSummary,
) -> Option<String> {
    let profile_id = event.storage_profile_id.as_deref()?.trim();
    let provider = event.storage_profile_provider.as_deref()?.trim();
    let issuance_mode = event.storage_profile_issuance_mode.as_deref()?.trim();
    let location_prefix_hash = event
        .storage_profile_location_prefix_hash
        .as_deref()?
        .trim();
    if profile_id.is_empty()
        || provider.is_empty()
        || issuance_mode.is_empty()
        || !is_full_sha256_hash(location_prefix_hash)
    {
        return None;
    }
    let secret_ref = if event.storage_profile_secret_ref_present? {
        let provider = event
            .storage_profile_secret_ref_provider
            .as_deref()
            .filter(|provider| !provider.trim().is_empty())?;
        let hash = event
            .storage_profile_secret_ref_hash
            .as_deref()
            .filter(|hash| is_full_sha256_hash(hash))?;
        format!("{provider}:secret_ref_hash={hash}")
    } else {
        if event.storage_profile_secret_ref_provider.is_some()
            || event.storage_profile_secret_ref_hash.is_some()
        {
            return None;
        }
        "none".to_string()
    };
    Some(format!(
        "{profile_id}:{provider}:{issuance_mode}:location_prefix_hash={location_prefix_hash}:secret_ref={secret_ref}"
    ))
}

pub(crate) fn qglake_replay_verification_json(
    verification: &QueryGraphBootstrapVerification,
    scan_replay: Option<String>,
    management_replay: Option<String>,
    credential_replay: Option<String>,
    table_commit_history_replay: Option<String>,
    replay_evidence: Value,
) -> Value {
    json!({
        "schema-version": "lakecat.qglake.replay-verification.v1",
        "status": "verified",
        "bundle-hash": verification.bundle_hash,
        "graph-hash": verification.graph_hash,
        "open-lineage-hash": verification.open_lineage_hash,
        "querygraph-import-hash": verification.querygraph_import_hash,
        "table-count": verification.table_count,
        "view-count": verification.view_count,
        "verified-tables": verification.verified_tables,
        "verified-views": verification.verified_views,
        "standards": verification.standards,
        "scan-replay": scan_replay,
        "management-replay": management_replay,
        "credential-replay": credential_replay,
        "table-commit-history-replay": table_commit_history_replay,
        "replay-evidence": replay_evidence,
    })
}

pub(crate) fn qglake_replay_evidence_json(
    drain: &LineageDrainResponse,
    principal: Option<&str>,
    verification: &QueryGraphBootstrapVerification,
) -> Value {
    json!({
        "requestIdentity": qglake_request_identity_replay_evidence_json(drain),
        "queryGraphBootstrap": qglake_querygraph_bootstrap_replay_evidence_json(drain),
        "catalogConfig": qglake_catalog_config_replay_evidence_json(drain),
        "scan": qglake_scan_replay_evidence_json(drain),
        "management": qglake_management_replay_evidence_json(drain),
        "credentials": qglake_credential_replay_evidence_json(drain, principal),
        "tableCommitHistory": qglake_table_commit_history_replay_evidence_json(drain),
        "views": qglake_view_replay_evidence_json(drain, verification),
    })
}

pub(crate) fn qglake_request_identity_replay_evidence_json(
    drain: &LineageDrainResponse,
) -> Option<Value> {
    Some(json!({
        "principalSubject": drain.principal_subject.as_deref()?,
        "principalKind": drain.principal_kind.as_deref()?,
        "requestIdentitySource": drain.request_identity_source.as_deref()?,
        "authorizationReceiptHash": drain.authorization_receipt_hash.as_deref()?,
        "authorizationReceiptAction": drain.authorization_receipt_action.as_deref()?,
        "requestIdentityState": drain.request_identity_state.as_deref()?,
        "typedidEnvelopeHash": drain.typedid_envelope_hash.as_deref(),
        "typedidProofHash": drain.typedid_proof_hash.as_deref(),
    }))
}

pub(crate) fn qglake_querygraph_bootstrap_replay_evidence_json(
    drain: &LineageDrainResponse,
) -> Option<Value> {
    let bootstrap = qglake_drain_event(drain, "querygraph.bootstrap")?;
    Some(json!({
        "bundleHash": bootstrap.bundle_hash.as_deref(),
        "graphHash": bootstrap.graph_hash.as_deref(),
        "openLineageHash": bootstrap.open_lineage_hash.as_deref(),
        "queryGraphImportHash": bootstrap.querygraph_import_hash.as_deref(),
        "tableArtifactCount": bootstrap.table_artifact_count,
        "viewArtifactCount": bootstrap.view_artifact_count,
        "policyBindingCount": bootstrap.policy_binding_count,
        "standards": &bootstrap.standards,
        "principalSubject": bootstrap.principal_subject.as_deref(),
        "principalKind": bootstrap.principal_kind.as_deref(),
        "requestIdentitySource": bootstrap.request_identity_source.as_deref(),
        "requestIdentityState": bootstrap.request_identity_state.as_deref(),
        "authorizationReceiptHash": bootstrap.authorization_receipt_hash.as_deref(),
        "authorizationReceiptAction": bootstrap.authorization_receipt_action.as_deref(),
        "agentDelegationHash": bootstrap.agent_delegation_hash.as_deref(),
        "agentSummarySignatureHash": bootstrap.agent_summary_signature_hash.as_deref(),
        "typedidEnvelopeHash": bootstrap.typedid_envelope_hash.as_deref(),
        "typedidProofHash": bootstrap.typedid_proof_hash.as_deref(),
        "viewVersionReceiptHashes": &bootstrap.view_version_receipt_hashes,
        "replayEventHashes": &bootstrap.replay_event_hashes,
        "openLineageHashes": &bootstrap.replay_open_lineage_hashes,
    }))
}

pub(crate) fn qglake_catalog_config_replay_evidence_json(
    drain: &LineageDrainResponse,
) -> Option<Value> {
    let config = qglake_drain_event(drain, "catalog.config-read")?;
    Some(json!({
        "defaults": &config.catalog_config_defaults,
        "overrides": &config.catalog_config_overrides,
        "endpoints": &config.catalog_config_endpoints,
        "principalSubject": config.principal_subject.as_deref(),
        "principalKind": config.principal_kind.as_deref(),
        "authorizationReceiptHash": config.authorization_receipt_hash.as_deref(),
        "authorizationReceiptAction": config.authorization_receipt_action.as_deref(),
        "graphEvents": config.graph_events,
        "replayEventHashes": &config.replay_event_hashes,
        "openLineageHashes": &config.replay_open_lineage_hashes,
    }))
}

pub(crate) fn qglake_scan_replay_evidence_json(drain: &LineageDrainResponse) -> Option<Value> {
    let planned = qglake_drain_event(drain, "table.scan-planned")?;
    let fetched = qglake_drain_event(drain, "table.scan-tasks-fetched")?;
    Some(json!({
        "planTaskCount": planned.scan_task_count.unwrap_or_default(),
        "planGraphEvents": planned.graph_events,
        "fileTaskCount": fetched.file_scan_task_count.unwrap_or_default(),
        "deleteFileCount": fetched.delete_file_count.unwrap_or_default(),
        "childPlanTaskCount": fetched.child_plan_task_count.unwrap_or_default(),
        "plannedPrincipalSubject": planned.principal_subject.as_deref(),
        "plannedPrincipalKind": planned.principal_kind.as_deref(),
        "plannedAuthorizationReceiptHash": planned.authorization_receipt_hash.as_deref(),
        "plannedAuthorizationReceiptAction": planned.authorization_receipt_action.as_deref(),
        "fetchedPrincipalSubject": fetched.principal_subject.as_deref(),
        "fetchedPrincipalKind": fetched.principal_kind.as_deref(),
        "fetchedAuthorizationReceiptHash": fetched.authorization_receipt_hash.as_deref(),
        "fetchedAuthorizationReceiptAction": fetched.authorization_receipt_action.as_deref(),
        "plannedReadRestriction": planned.read_restriction.as_ref(),
        "fetchedReadRestriction": fetched.read_restriction.as_ref(),
        "plannedRequestedProjection": &planned.requested_projection,
        "plannedEffectiveProjection": &planned.effective_projection,
        "plannedRequestedStatsFields": &planned.requested_stats_fields,
        "plannedEffectiveStatsFields": &planned.effective_stats_fields,
        "fetchedRequestedStatsFields": &fetched.requested_stats_fields,
        "fetchedEffectiveStatsFields": &fetched.effective_stats_fields,
        "fetchedRequiredProjection": &fetched.required_projection,
        "fetchedEffectiveProjection": &fetched.effective_projection,
        "fetchedRequiredFilters": &fetched.required_filters,
        "plannedReplayEventHashes": &planned.replay_event_hashes,
        "fetchedReplayEventHashes": &fetched.replay_event_hashes,
        "plannedOpenLineageHashes": &planned.replay_open_lineage_hashes,
        "fetchedOpenLineageHashes": &fetched.replay_open_lineage_hashes,
    }))
}

pub(crate) fn qglake_management_replay_evidence_json(
    drain: &LineageDrainResponse,
) -> Option<Value> {
    let server = qglake_drain_event(drain, "server.listed")?;
    let project = qglake_drain_event(drain, "project.listed")?;
    let warehouse = qglake_drain_event(drain, "warehouse.listed")?;
    let policy = qglake_drain_event(drain, "policy-binding.listed")?;
    let storage_profile = qglake_drain_event(drain, "storage-profile.listed")?;
    let storage_profile_upsert = qglake_drain_event(drain, "storage-profile.upserted")?;
    let policy_upsert = qglake_drain_event(drain, "policy-binding.upserted")?;
    let storage_profile_upsert_proof = json!({
        "profileId": storage_profile_upsert.storage_profile_id.as_deref(),
        "provider": storage_profile_upsert.storage_profile_provider.as_deref(),
        "issuanceMode": storage_profile_upsert.storage_profile_issuance_mode.as_deref(),
        "locationPrefixHash": storage_profile_upsert.storage_profile_location_prefix_hash.as_deref(),
        "principalSubject": storage_profile_upsert.principal_subject.as_deref(),
        "principalKind": storage_profile_upsert.principal_kind.as_deref(),
        "authorizationReceiptHash": storage_profile_upsert.authorization_receipt_hash.as_deref(),
        "authorizationReceiptAction": storage_profile_upsert.authorization_receipt_action.as_deref(),
        "secretRefPresent": storage_profile_upsert.storage_profile_secret_ref_present.unwrap_or_default(),
        "secretRefProvider": storage_profile_upsert.storage_profile_secret_ref_provider.as_deref(),
        "secretRefHash": storage_profile_upsert.storage_profile_secret_ref_hash.as_deref(),
        "graphEvents": storage_profile_upsert.graph_events,
        "replayEventHashes": &storage_profile_upsert.replay_event_hashes,
        "openLineageHashes": &storage_profile_upsert.replay_open_lineage_hashes,
    });
    Some(json!({
        "serverCount": server.server_count.unwrap_or_default(),
        "serverIds": &server.server_ids,
        "serverGraphEvents": server.graph_events,
        "projectCount": project.project_count.unwrap_or_default(),
        "projectIds": &project.project_ids,
        "projectGraphEvents": project.graph_events,
        "warehouseCount": warehouse.warehouse_count.unwrap_or_default(),
        "warehouseNames": &warehouse.warehouse_names,
        "warehouseProjectId": warehouse.management_scope_project_id.as_deref(),
        "warehouseGraphEvents": warehouse.graph_events,
        "policyBindingCount": policy.policy_binding_count,
        "policyIds": &policy.policy_ids,
        "policyGraphEvents": policy.graph_events,
        "policyUpsertProof": {
            "policyId": policy_upsert.policy_id.as_deref(),
            "odrlHash": policy_upsert.policy_odrl_hash.as_deref(),
            "principalSubject": policy_upsert.principal_subject.as_deref(),
            "principalKind": policy_upsert.principal_kind.as_deref(),
            "authorizationReceiptHash": policy_upsert.authorization_receipt_hash.as_deref(),
            "authorizationReceiptAction": policy_upsert.authorization_receipt_action.as_deref(),
            "graphEvents": policy_upsert.graph_events,
            "replayEventHashes": &policy_upsert.replay_event_hashes,
            "openLineageHashes": &policy_upsert.replay_open_lineage_hashes,
        },
        "storageProfileCount": storage_profile.storage_profile_count.unwrap_or_default(),
        "storageProfileIds": &storage_profile.storage_profile_ids,
        "storageProfileGraphEvents": storage_profile.graph_events,
        "serverReplayEventHashes": &server.replay_event_hashes,
        "serverOpenLineageHashes": &server.replay_open_lineage_hashes,
        "projectReplayEventHashes": &project.replay_event_hashes,
        "projectOpenLineageHashes": &project.replay_open_lineage_hashes,
        "warehouseReplayEventHashes": &warehouse.replay_event_hashes,
        "warehouseOpenLineageHashes": &warehouse.replay_open_lineage_hashes,
        "policyReplayEventHashes": &policy.replay_event_hashes,
        "policyOpenLineageHashes": &policy.replay_open_lineage_hashes,
        "storageProfileReplayEventHashes": &storage_profile.replay_event_hashes,
        "storageProfileOpenLineageHashes": &storage_profile.replay_open_lineage_hashes,
        "storageProfileUpsert": storage_profile_upsert_proof,
    }))
}

pub(crate) fn qglake_credential_replay_evidence_json(
    drain: &LineageDrainResponse,
    principal: Option<&str>,
) -> Option<Value> {
    let restricted_subject = principal.unwrap_or("anonymous");
    let restricted_kind = if principal.is_some() {
        "agent"
    } else {
        "anonymous"
    };
    let restricted = qglake_credential_event(drain, restricted_subject, restricted_kind)?;
    let human = qglake_credential_event(drain, "human:qglake-operator", "human")?;
    Some(json!({
        "restricted": {
            "principalSubject": restricted.principal_subject.as_deref(),
            "principalKind": restricted.principal_kind.as_deref(),
            "credentialCount": restricted.credential_count.unwrap_or_default(),
            "credentialPrefixHashes": &restricted.credential_prefix_hashes,
            "rawCredentialExceptionAllowed": restricted.raw_credential_exception_allowed.unwrap_or_default(),
            "blockReason": restricted.credential_block_reason.as_deref(),
            "maxCredentialTtlSeconds": qglake_event_max_credential_ttl_seconds(restricted),
            "authorizationReceiptHash": restricted.authorization_receipt_hash.as_deref(),
            "authorizationReceiptAction": restricted.authorization_receipt_action.as_deref(),
            "storageProfile": qglake_credential_storage_profile_evidence_json(restricted),
            "replayEventHashes": &restricted.replay_event_hashes,
            "openLineageHashes": &restricted.replay_open_lineage_hashes,
        },
        "trustedHuman": {
            "principalSubject": human.principal_subject.as_deref(),
            "principalKind": human.principal_kind.as_deref(),
            "credentialCount": human.credential_count.unwrap_or_default(),
            "credentialPrefixHashes": &human.credential_prefix_hashes,
            "rawCredentialExceptionAllowed": human.raw_credential_exception_allowed.unwrap_or_default(),
            "rawCredentialExceptionReason": human.raw_credential_exception_reason.as_deref(),
            "blockReason": human.credential_block_reason.as_deref(),
            "maxCredentialTtlSeconds": qglake_event_max_credential_ttl_seconds(human),
            "authorizationReceiptHash": human.authorization_receipt_hash.as_deref(),
            "authorizationReceiptAction": human.authorization_receipt_action.as_deref(),
            "storageProfile": qglake_credential_storage_profile_evidence_json(human),
            "replayEventHashes": &human.replay_event_hashes,
            "openLineageHashes": &human.replay_open_lineage_hashes,
        }
    }))
}

pub(crate) fn qglake_event_max_credential_ttl_seconds(
    event: &LineageDrainEventSummary,
) -> Option<u64> {
    event
        .read_restriction
        .as_ref()
        .and_then(|restriction| restriction.get("max-credential-ttl-seconds"))
        .and_then(Value::as_u64)
}

pub(crate) fn qglake_event_read_restriction_purpose(
    event: &LineageDrainEventSummary,
) -> Option<&str> {
    event
        .read_restriction
        .as_ref()
        .and_then(|restriction| restriction.get("purpose"))
        .and_then(Value::as_str)
        .filter(|purpose| !purpose.trim().is_empty())
}

pub(crate) fn qglake_credential_storage_profile_evidence_json(
    event: &LineageDrainEventSummary,
) -> Value {
    json!({
        "profileId": event.storage_profile_id.as_deref(),
        "provider": event.storage_profile_provider.as_deref(),
        "issuanceMode": event.storage_profile_issuance_mode.as_deref(),
        "locationPrefixHash": event.storage_profile_location_prefix_hash.as_deref(),
        "secretRefPresent": event.storage_profile_secret_ref_present.unwrap_or_default(),
        "secretRefProvider": event.storage_profile_secret_ref_provider.as_deref(),
        "secretRefHash": event.storage_profile_secret_ref_hash.as_deref(),
        "graphEvents": event.graph_events,
    })
}

pub(crate) fn qglake_table_commit_history_replay_evidence_json(
    drain: &LineageDrainResponse,
) -> Option<Value> {
    let commit_history = qglake_drain_event(drain, "table.commits-listed")?;
    Some(json!({
        "commitCount": commit_history.table_commit_count.unwrap_or_default(),
        "sequenceNumbers": &commit_history.table_commit_sequence_numbers,
        "commitHashes": &commit_history.table_commit_hashes,
        "principalSubject": commit_history.principal_subject.as_deref(),
        "principalKind": commit_history.principal_kind.as_deref(),
        "authorizationReceiptHash": commit_history.authorization_receipt_hash.as_deref(),
        "authorizationReceiptAction": commit_history.authorization_receipt_action.as_deref(),
        "graphEvents": commit_history.graph_events,
        "replayEventHashes": &commit_history.replay_event_hashes,
        "openLineageHashes": &commit_history.replay_open_lineage_hashes,
    }))
}

pub(crate) fn qglake_view_replay_evidence_json(
    drain: &LineageDrainResponse,
    verification: &QueryGraphBootstrapVerification,
) -> Option<Value> {
    if verification.verified_views.is_empty() {
        return Some(json!({
            "viewCount": 0,
            "views": [],
            "tombstoneReceipts": [],
            "receiptChains": []
        }));
    }

    let views = verification
        .verified_views
        .iter()
        .map(|view_stable_id| {
            let view_replay = drain.events.iter().find(|event| {
                matches!(
                    event.event_type.as_str(),
                    "view.upserted" | "view.loaded" | "view.dropped"
                ) && event.view_stable_id.as_deref() == Some(view_stable_id.as_str())
            })?;
            Some(json!({
                "stableId": view_stable_id,
                "warehouse": view_replay.view_warehouse.as_deref(),
                "namespace": &view_replay.view_namespace,
                "name": view_replay.view_name.as_deref(),
                "viewVersion": view_replay.view_version,
                "acceptedViewVersion": verification.verified_view_versions.get(view_stable_id),
                "acceptedReceiptHash": verification.verified_view_receipt_hashes.get(view_stable_id),
                "acceptedReceiptChainHash": verification
                    .verified_view_receipt_chain_hashes
                    .get(view_stable_id),
                "eventType": view_replay.event_type,
                "expectedViewVersion": view_replay.expected_view_version,
                "graphEvents": view_replay.graph_events,
                "replayEventHashes": &view_replay.replay_event_hashes,
                "openLineageHashes": &view_replay.replay_open_lineage_hashes,
            }))
        })
        .collect::<Option<Vec<_>>>()?;

    let tombstone_receipts = drain
        .events
        .iter()
        .filter(|event| event.event_type == "view.version-receipts-listed")
        .map(|event| {
            let expected_view_version = event.view_stable_id.as_deref().and_then(|stable_id| {
                drain
                    .events
                    .iter()
                    .find(|candidate| {
                        candidate.event_type == "view.dropped"
                            && candidate.view_stable_id.as_deref() == Some(stable_id)
                    })
                    .and_then(|candidate| candidate.expected_view_version)
            });
            json!({
                "stableId": event.view_stable_id.as_deref(),
                "warehouse": event.view_warehouse.as_deref(),
                "namespace": &event.view_namespace,
                "name": event.view_name.as_deref(),
                "expectedViewVersion": expected_view_version,
                "receiptHashes": &event.view_version_receipt_hashes,
                "replayEventHashes": &event.replay_event_hashes,
                "openLineageHashes": &event.replay_open_lineage_hashes,
            })
        })
        .collect::<Vec<_>>();

    let receipt_chains = drain
        .events
        .iter()
        .filter(|event| event.event_type == "view.version-receipt-chains-listed")
        .map(|event| {
            let mut chain_hashes = event
                .view_version_receipt_chain_hashes
                .iter()
                .cloned()
                .collect::<BTreeSet<_>>();
            for view_stable_id in &verification.verified_views {
                let Some(accepted_chain_hash) = verification
                    .verified_view_receipt_chain_hashes
                    .get(view_stable_id)
                else {
                    continue;
                };
                let Some(view_replay) = drain.events.iter().find(|candidate| {
                    matches!(
                        candidate.event_type.as_str(),
                        "view.upserted" | "view.loaded" | "view.dropped"
                    ) && candidate.view_stable_id.as_deref() == Some(view_stable_id.as_str())
                }) else {
                    continue;
                };
                if view_replay.view_warehouse == event.view_warehouse
                    && view_replay.view_namespace == event.view_namespace
                {
                    chain_hashes.insert(accepted_chain_hash.clone());
                }
            }
            let chains = qglake_compact_view_receipt_chains(event, verification);
            let mut chain_hashes = chain_hashes;
            for chain in &chains {
                if let Some(chain_hash) = chain.get("chainHash").and_then(Value::as_str) {
                    chain_hashes.insert(chain_hash.to_string());
                }
            }
            let chain_hashes = chain_hashes.into_iter().collect::<Vec<_>>();
            let verified_chain_count = chains.len();
            json!({
                "warehouse": event.view_warehouse.as_deref(),
                "namespace": &event.view_namespace,
                "receiptHashes": &event.view_version_receipt_hashes,
                "chainHashes": chain_hashes,
                "verifiedChainCount": verified_chain_count,
                "chains": chains,
                "replayEventHashes": &event.replay_event_hashes,
                "openLineageHashes": &event.replay_open_lineage_hashes,
            })
        })
        .collect::<Vec<_>>();

    Some(json!({
        "viewCount": verification.view_count,
        "views": views,
        "tombstoneReceipts": tombstone_receipts,
        "receiptChains": receipt_chains,
    }))
}

pub(crate) fn qglake_compact_view_receipt_chains(
    event: &LineageDrainEventSummary,
    verification: &QueryGraphBootstrapVerification,
) -> Vec<Value> {
    let mut chains = event
        .view_version_receipt_chains
        .iter()
        .filter(|chain| chain.chain_verified)
        .map(qglake_compact_view_receipt_chain)
        .collect::<Vec<_>>();
    let mut chain_hashes = chains
        .iter()
        .filter_map(|chain| chain.get("chainHash").and_then(Value::as_str))
        .map(str::to_string)
        .collect::<BTreeSet<_>>();
    for view_stable_id in &verification.verified_views {
        let Some(accepted_chain_hash) = verification
            .verified_view_receipt_chain_hashes
            .get(view_stable_id)
            .map(String::as_str)
        else {
            continue;
        };
        if chain_hashes.contains(accepted_chain_hash) {
            continue;
        }
        let Some(accepted_receipt_hash) = verification
            .verified_view_receipt_hashes
            .get(view_stable_id)
            .map(String::as_str)
        else {
            continue;
        };
        let Some(accepted_view_version) = verification
            .verified_view_versions
            .get(view_stable_id)
            .copied()
        else {
            continue;
        };
        let Some(chain) = event
            .view_version_receipt_chains
            .iter()
            .find(|chain| chain.stable_id == *view_stable_id)
        else {
            continue;
        };
        let Some(receipt_index) = chain
            .receipts
            .iter()
            .position(|receipt| receipt.receipt_hash == accepted_receipt_hash)
        else {
            continue;
        };
        let receipts = chain.receipts[..=receipt_index]
            .iter()
            .map(qglake_compact_view_receipt)
            .collect::<Vec<_>>();
        chains.push(json!({
            "stableId": chain.stable_id,
            "warehouse": chain.warehouse,
            "namespace": chain.namespace,
            "name": chain.name,
            "chainHash": accepted_chain_hash,
            "chainVerified": true,
            "latestViewVersion": accepted_view_version,
            "latestOperation": chain.receipts[receipt_index].operation,
            "tombstoned": false,
            "receiptCount": receipts.len(),
            "receipts": receipts,
        }));
        chain_hashes.insert(accepted_chain_hash.to_string());
    }
    chains
}

pub(crate) fn qglake_compact_view_receipt_chain(chain: &ViewVersionReceiptChainResponse) -> Value {
    json!({
        "stableId": chain.stable_id,
        "warehouse": chain.warehouse,
        "namespace": chain.namespace,
        "name": chain.name,
        "chainHash": chain.chain_hash,
        "chainVerified": chain.chain_verified,
        "latestViewVersion": chain.latest_view_version,
        "latestOperation": chain.latest_operation,
        "tombstoned": chain.tombstoned,
        "receiptCount": chain.receipt_count,
        "receipts": chain
            .receipts
            .iter()
            .map(qglake_compact_view_receipt)
            .collect::<Vec<_>>(),
    })
}

pub(crate) fn qglake_compact_view_receipt(receipt: &ViewVersionReceiptResponse) -> Value {
    json!({
        "stableId": receipt.stable_id,
        "warehouse": receipt.warehouse,
        "namespace": receipt.namespace,
        "name": receipt.name,
        "viewVersion": receipt.view_version,
        "previousViewVersion": receipt.previous_view_version,
        "previousReceiptHash": receipt.previous_receipt_hash,
        "operation": receipt.operation,
        "viewHash": receipt.view_hash,
        "receiptHash": receipt.receipt_hash,
        "principalSubject": receipt.principal_subject,
        "principalKind": receipt.principal_kind,
        "recordedAt": receipt.recorded_at,
    })
}

pub(crate) fn qglake_credential_replay_line(
    drain: &LineageDrainResponse,
    principal: Option<&str>,
) -> Option<String> {
    let restricted_subject = principal.unwrap_or("anonymous");
    let restricted_kind = if principal.is_some() {
        "agent"
    } else {
        "anonymous"
    };
    let restricted = qglake_credential_event(drain, restricted_subject, restricted_kind)?;
    let human = qglake_credential_event(drain, "human:qglake-operator", "human")?;
    if restricted.credential_block_reason.as_deref()
        != Some(QGLAKE_RESTRICTED_CREDENTIAL_BLOCK_REASON)
        || human.raw_credential_exception_reason.as_deref()
            != Some(QGLAKE_HUMAN_RAW_CREDENTIAL_EXCEPTION_REASON)
    {
        return None;
    }
    let restricted_profile = qglake_credential_storage_profile_line(restricted)?;
    let human_profile = qglake_credential_storage_profile_line(human)?;
    let restricted_ttl = qglake_event_max_credential_ttl_seconds(restricted)?;
    let human_ttl = qglake_event_max_credential_ttl_seconds(human)?;
    Some(format!(
        "credential replay restricted=blocked:sail-planned-read-required restricted_count={} restricted_ttl={} restricted_profile={} human=allowed:trusted-human-audited-raw human_count={} human_ttl={} human_profile={}",
        restricted.credential_count.unwrap_or_default(),
        restricted_ttl,
        restricted_profile,
        human.credential_count.unwrap_or_default(),
        human_ttl,
        human_profile
    ))
}

pub(crate) fn qglake_credential_storage_profile_line(
    event: &LineageDrainEventSummary,
) -> Option<String> {
    let profile_id = event.storage_profile_id.as_deref()?.trim();
    let provider = event.storage_profile_provider.as_deref()?.trim();
    let issuance_mode = event.storage_profile_issuance_mode.as_deref()?.trim();
    let location_prefix_hash = event
        .storage_profile_location_prefix_hash
        .as_deref()?
        .trim();
    let graph_events = event.graph_events;
    if profile_id.is_empty()
        || provider.is_empty()
        || issuance_mode.is_empty()
        || !is_full_sha256_hash(location_prefix_hash)
        || graph_events == 0
    {
        return None;
    }
    let secret_ref = if event.storage_profile_secret_ref_present? {
        let provider = event
            .storage_profile_secret_ref_provider
            .as_deref()
            .filter(|provider| !provider.trim().is_empty())?;
        let hash = event
            .storage_profile_secret_ref_hash
            .as_deref()
            .filter(|hash| is_full_sha256_hash(hash))?;
        format!("{provider}:secret_ref_hash={hash}")
    } else {
        if event.storage_profile_secret_ref_provider.is_some()
            || event.storage_profile_secret_ref_hash.is_some()
        {
            return None;
        }
        "none".to_string()
    };
    Some(format!(
        "{}:{}:{}:location_prefix_hash={}:secret_ref={}:graph_events={}",
        profile_id, provider, issuance_mode, location_prefix_hash, secret_ref, graph_events
    ))
}

pub(crate) fn qglake_table_commit_history_replay_line(
    drain: &LineageDrainResponse,
) -> Option<String> {
    let commit_history = qglake_drain_event(drain, "table.commits-listed")?;
    Some(format!(
        "table commit history commits={} sequences={} hashes={} graph_events={}",
        commit_history.table_commit_count.unwrap_or_default(),
        join_u64s(&commit_history.table_commit_sequence_numbers),
        commit_history.table_commit_hashes.join(","),
        commit_history.graph_events
    ))
}

pub(crate) fn qglake_scan_replay_line(drain: &LineageDrainResponse) -> Option<String> {
    let planned = qglake_drain_event(drain, "table.scan-planned")?;
    let fetched = qglake_drain_event(drain, "table.scan-tasks-fetched")?;
    let planned_ttl = qglake_event_max_credential_ttl_seconds(planned)?;
    let fetched_ttl = qglake_event_max_credential_ttl_seconds(fetched)?;
    let planned_purpose = qglake_event_read_restriction_purpose(planned)?;
    let fetched_purpose = qglake_event_read_restriction_purpose(fetched)?;
    Some(format!(
        "scan replay plan_tasks={} plan_graph_events={} planned_ttl={} planned_purpose={} file_tasks={} delete_files={} child_plan_tasks={} fetched_ttl={} fetched_purpose={}",
        planned.scan_task_count.unwrap_or_default(),
        planned.graph_events,
        planned_ttl,
        planned_purpose,
        fetched.file_scan_task_count.unwrap_or_default(),
        fetched.delete_file_count.unwrap_or_default(),
        fetched.child_plan_task_count.unwrap_or_default(),
        fetched_ttl,
        fetched_purpose
    ))
}

pub(crate) fn qglake_drain_event<'a>(
    drain: &'a LineageDrainResponse,
    event_type: &str,
) -> Option<&'a LineageDrainEventSummary> {
    drain
        .events
        .iter()
        .find(|event| event.event_type == event_type)
}

pub(crate) fn qglake_credential_event<'a>(
    drain: &'a LineageDrainResponse,
    principal_subject: &str,
    principal_kind: &str,
) -> Option<&'a LineageDrainEventSummary> {
    drain.events.iter().find(|event| {
        event.event_type == "credentials.vend-attempted"
            && event.principal_subject.as_deref() == Some(principal_subject)
            && event.principal_kind.as_deref() == Some(principal_kind)
    })
}
