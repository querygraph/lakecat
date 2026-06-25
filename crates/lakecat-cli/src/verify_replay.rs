use crate::*;

pub(crate) fn verify_lakecat_replay_capture_matches_summary(
    capture: &serde_json::Map<String, Value>,
    lakecat: &serde_json::Map<String, Value>,
    querygraph: &serde_json::Map<String, Value>,
) -> lakecat_core::LakeCatResult<()> {
    require_only_fields(
        capture,
        LAKECAT_REPLAY_CAPTURE_FIELDS,
        "captured LakeCat replay output",
    )?;
    require_string_match(
        capture,
        "schema-version",
        required_str(lakecat, "schemaVersion", "lakecatReplayVerification")?,
        "captured LakeCat replay output",
    )?;
    require_string_match(
        capture,
        "status",
        required_str(lakecat, "status", "lakecatReplayVerification")?,
        "captured LakeCat replay output",
    )?;
    require_handoff_summary_fields_match_capture(
        capture,
        querygraph,
        "captured LakeCat replay output",
    )?;
    verify_lakecat_replay_request_identity_matches_summary(capture, lakecat)?;
    verify_lakecat_replay_querygraph_bootstrap_matches_summary(capture, lakecat)?;
    verify_lakecat_replay_catalog_config_matches_summary(capture, lakecat)?;
    verify_lakecat_replay_scan_matches_summary(capture, lakecat)?;
    verify_lakecat_replay_table_commit_history_matches_summary(capture, lakecat)?;
    verify_lakecat_replay_views_match_summary(capture, lakecat)?;
    verify_lakecat_replay_management_matches_summary(capture, lakecat)?;
    verify_lakecat_replay_storage_profile_matches_summary(capture, lakecat)?;
    verify_lakecat_replay_credentials_match_summary(capture, lakecat)
}

pub(crate) fn verify_lakecat_replay_request_identity_matches_summary(
    capture: &serde_json::Map<String, Value>,
    lakecat: &serde_json::Map<String, Value>,
) -> lakecat_core::LakeCatResult<()> {
    let captured_request_identity = lakecat_replay_request_identity(capture)?;
    require_only_fields(
        captured_request_identity,
        REQUEST_IDENTITY_PROOF_FIELDS,
        "captured LakeCat replay output.replay-evidence.requestIdentity",
    )?;
    let summary_request_identity =
        required_object(lakecat, "requestIdentityProof", "lakecatReplayVerification")?;

    for field in [
        "principalSubject",
        "principalKind",
        "requestIdentitySource",
        "requestIdentityState",
        "authorizationReceiptHash",
        "authorizationReceiptAction",
    ] {
        require_string_match(
            captured_request_identity,
            field,
            required_str(summary_request_identity, field, "requestIdentityProof")?,
            "captured LakeCat replay output.replay-evidence.requestIdentity",
        )?;
    }

    for field in ["typedidEnvelopeHash", "typedidProofHash"] {
        require_value_match(
            captured_request_identity,
            field,
            required_value(summary_request_identity, field, "requestIdentityProof")?,
            "captured LakeCat replay output.replay-evidence.requestIdentity",
        )?;
    }

    Ok(())
}

pub(crate) fn lakecat_replay_request_identity(
    capture: &serde_json::Map<String, Value>,
) -> lakecat_core::LakeCatResult<&serde_json::Map<String, Value>> {
    required_object(
        lakecat_replay_evidence(capture)?,
        "requestIdentity",
        "captured LakeCat replay output.replay-evidence",
    )
}

pub(crate) fn verify_lakecat_replay_querygraph_bootstrap_matches_summary(
    capture: &serde_json::Map<String, Value>,
    lakecat: &serde_json::Map<String, Value>,
) -> lakecat_core::LakeCatResult<()> {
    let captured_bootstrap = lakecat_replay_querygraph_bootstrap(capture)?;
    require_only_fields(
        captured_bootstrap,
        QUERYGRAPH_BOOTSTRAP_PROOF_FIELDS,
        "captured LakeCat replay output.replay-evidence.queryGraphBootstrap",
    )?;
    let summary_bootstrap = required_object(
        lakecat,
        "queryGraphBootstrapProof",
        "lakecatReplayVerification",
    )?;

    for field in [
        "bundleHash",
        "graphHash",
        "openLineageHash",
        "queryGraphImportHash",
        "principalSubject",
        "principalKind",
        "requestIdentitySource",
        "requestIdentityState",
        "authorizationReceiptHash",
        "authorizationReceiptAction",
        "agentDelegationHash",
        "agentSummarySignatureHash",
    ] {
        require_string_match(
            captured_bootstrap,
            field,
            required_str(summary_bootstrap, field, "queryGraphBootstrapProof")?,
            "captured LakeCat replay output.replay-evidence.queryGraphBootstrap",
        )?;
    }

    for field in [
        "tableArtifactCount",
        "viewArtifactCount",
        "policyBindingCount",
        "standards",
        "typedidEnvelopeHash",
        "typedidProofHash",
        "viewVersionReceiptHashes",
        "replayEventHashes",
        "openLineageHashes",
    ] {
        require_value_match(
            captured_bootstrap,
            field,
            required_value(summary_bootstrap, field, "queryGraphBootstrapProof")?,
            "captured LakeCat replay output.replay-evidence.queryGraphBootstrap",
        )?;
    }

    Ok(())
}

pub(crate) fn lakecat_replay_querygraph_bootstrap(
    capture: &serde_json::Map<String, Value>,
) -> lakecat_core::LakeCatResult<&serde_json::Map<String, Value>> {
    required_object(
        lakecat_replay_evidence(capture)?,
        "queryGraphBootstrap",
        "captured LakeCat replay output.replay-evidence",
    )
}

pub(crate) fn verify_lakecat_replay_catalog_config_matches_summary(
    capture: &serde_json::Map<String, Value>,
    lakecat: &serde_json::Map<String, Value>,
) -> lakecat_core::LakeCatResult<()> {
    let captured_config = lakecat_replay_catalog_config(capture)?;
    require_only_fields(
        captured_config,
        CATALOG_CONFIG_PROOF_FIELDS,
        "captured LakeCat replay output.replay-evidence.catalogConfig",
    )?;
    let summary_config =
        required_object(lakecat, "catalogConfigProof", "lakecatReplayVerification")?;

    for field in [
        "defaults",
        "overrides",
        "endpoints",
        "principalSubject",
        "principalKind",
        "authorizationReceiptHash",
        "authorizationReceiptAction",
        "graphEvents",
        "replayEventHashes",
        "openLineageHashes",
    ] {
        require_value_match(
            captured_config,
            field,
            required_value(summary_config, field, "catalogConfigProof")?,
            "captured LakeCat replay output.replay-evidence.catalogConfig",
        )?;
    }

    Ok(())
}

pub(crate) fn lakecat_replay_catalog_config(
    capture: &serde_json::Map<String, Value>,
) -> lakecat_core::LakeCatResult<&serde_json::Map<String, Value>> {
    required_object(
        lakecat_replay_evidence(capture)?,
        "catalogConfig",
        "captured LakeCat replay output.replay-evidence",
    )
}

pub(crate) fn verify_lakecat_replay_scan_matches_summary(
    capture: &serde_json::Map<String, Value>,
    lakecat: &serde_json::Map<String, Value>,
) -> lakecat_core::LakeCatResult<()> {
    let captured_scan = lakecat_replay_scan(capture)?;
    require_only_fields(
        captured_scan,
        GOVERNED_SCAN_PROOF_FIELDS,
        "captured LakeCat replay output.replay-evidence.scan",
    )?;
    require_read_restriction_evidence(
        required_object(
            captured_scan,
            "plannedReadRestriction",
            "captured LakeCat replay output.replay-evidence.scan",
        )?,
        "captured LakeCat replay output.replay-evidence.scan.plannedReadRestriction",
    )?;
    require_read_restriction_evidence(
        required_object(
            captured_scan,
            "fetchedReadRestriction",
            "captured LakeCat replay output.replay-evidence.scan",
        )?,
        "captured LakeCat replay output.replay-evidence.scan.fetchedReadRestriction",
    )?;
    let summary_scan = required_object(lakecat, "governedScanProof", "lakecatReplayVerification")?;

    for field in GOVERNED_SCAN_PROOF_FIELDS {
        require_value_match(
            captured_scan,
            field,
            required_value(summary_scan, field, "governedScanProof")?,
            "captured LakeCat replay output.replay-evidence.scan",
        )?;
    }
    let expected_scan_replay = expected_scan_replay_line_from_summary(summary_scan)?;
    require_string_match(
        capture,
        "scan-replay",
        expected_scan_replay.as_str(),
        "captured LakeCat replay output",
    )?;

    Ok(())
}

pub(crate) fn expected_scan_replay_line_from_summary(
    scan: &serde_json::Map<String, Value>,
) -> lakecat_core::LakeCatResult<String> {
    let planned = required_object(scan, "plannedReadRestriction", "governedScanProof")?;
    let fetched = required_object(scan, "fetchedReadRestriction", "governedScanProof")?;
    require_read_restriction_evidence(planned, "governedScanProof.plannedReadRestriction")?;
    require_read_restriction_evidence(fetched, "governedScanProof.fetchedReadRestriction")?;
    Ok(format!(
        "scan replay plan_tasks={} plan_graph_events={} planned_ttl={} planned_purpose={} file_tasks={} delete_files={} child_plan_tasks={} fetched_ttl={} fetched_purpose={}",
        require_positive_u64(scan, "planTaskCount", "governedScanProof")?,
        require_positive_u64(scan, "planGraphEvents", "governedScanProof")?,
        require_positive_u64(
            planned,
            "max-credential-ttl-seconds",
            "governedScanProof.plannedReadRestriction",
        )?,
        require_non_empty_str(
            planned,
            "purpose",
            "governedScanProof.plannedReadRestriction"
        )?,
        require_positive_u64(scan, "fileTaskCount", "governedScanProof")?,
        require_positive_u64(scan, "deleteFileCount", "governedScanProof")?,
        require_positive_u64(scan, "childPlanTaskCount", "governedScanProof")?,
        require_positive_u64(
            fetched,
            "max-credential-ttl-seconds",
            "governedScanProof.fetchedReadRestriction",
        )?,
        require_non_empty_str(
            fetched,
            "purpose",
            "governedScanProof.fetchedReadRestriction"
        )?,
    ))
}

pub(crate) fn lakecat_replay_scan(
    capture: &serde_json::Map<String, Value>,
) -> lakecat_core::LakeCatResult<&serde_json::Map<String, Value>> {
    required_object(
        lakecat_replay_evidence(capture)?,
        "scan",
        "captured LakeCat replay output.replay-evidence",
    )
}

pub(crate) fn verify_lakecat_replay_table_commit_history_matches_summary(
    capture: &serde_json::Map<String, Value>,
    lakecat: &serde_json::Map<String, Value>,
) -> lakecat_core::LakeCatResult<()> {
    let captured_commit_history = lakecat_replay_table_commit_history(capture)?;
    require_only_fields(
        captured_commit_history,
        TABLE_COMMIT_HISTORY_PROOF_FIELDS,
        "captured LakeCat replay output.replay-evidence.tableCommitHistory",
    )?;
    let summary_commit_history = required_object(
        lakecat,
        "tableCommitHistoryProof",
        "lakecatReplayVerification",
    )?;

    for field in [
        "commitCount",
        "sequenceNumbers",
        "commitHashes",
        "principalSubject",
        "principalKind",
        "authorizationReceiptHash",
        "authorizationReceiptAction",
        "graphEvents",
        "replayEventHashes",
        "openLineageHashes",
    ] {
        require_value_match(
            captured_commit_history,
            field,
            required_value(summary_commit_history, field, "tableCommitHistoryProof")?,
            "captured LakeCat replay output.replay-evidence.tableCommitHistory",
        )?;
    }
    let expected_commit_history_replay =
        expected_table_commit_history_replay_line_from_summary(summary_commit_history)?;
    require_string_match(
        capture,
        "table-commit-history-replay",
        expected_commit_history_replay.as_str(),
        "captured LakeCat replay output",
    )?;

    Ok(())
}

pub(crate) fn expected_table_commit_history_replay_line_from_summary(
    commit_history: &serde_json::Map<String, Value>,
) -> lakecat_core::LakeCatResult<String> {
    let mut sequence_numbers = Vec::new();
    let sequence_values =
        required_array(commit_history, "sequenceNumbers", "tableCommitHistoryProof")?;
    let mut previous = 0;
    for (index, value) in sequence_values.iter().enumerate() {
        let sequence_number = value.as_u64().ok_or_else(|| {
            lakecat_core::LakeCatError::InvalidArgument(format!(
                "tableCommitHistoryProof.sequenceNumbers[{index}] must be a positive integer"
            ))
        })?;
        if sequence_number == 0 {
            return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "tableCommitHistoryProof.sequenceNumbers[{index}] must be positive"
            )));
        }
        if sequence_number <= previous {
            return Err(lakecat_core::LakeCatError::InvalidArgument(
                "tableCommitHistoryProof.sequenceNumbers must be strictly increasing".to_string(),
            ));
        }
        previous = sequence_number;
        sequence_numbers.push(sequence_number);
    }
    Ok(format!(
        "table commit history commits={} sequences={} hashes={} graph_events={}",
        required_u64(commit_history, "commitCount", "tableCommitHistoryProof")?,
        join_u64s(&sequence_numbers),
        required_string_array(commit_history, "commitHashes", "tableCommitHistoryProof")?.join(","),
        required_u64(commit_history, "graphEvents", "tableCommitHistoryProof")?,
    ))
}

pub(crate) fn lakecat_replay_table_commit_history(
    capture: &serde_json::Map<String, Value>,
) -> lakecat_core::LakeCatResult<&serde_json::Map<String, Value>> {
    required_object(
        lakecat_replay_evidence(capture)?,
        "tableCommitHistory",
        "captured LakeCat replay output.replay-evidence",
    )
}

pub(crate) fn verify_lakecat_replay_views_match_summary(
    capture: &serde_json::Map<String, Value>,
    lakecat: &serde_json::Map<String, Value>,
) -> lakecat_core::LakeCatResult<()> {
    let captured_views = lakecat_replay_views(capture)?;
    require_view_receipt_chain_schema(
        captured_views,
        "captured LakeCat replay output.replay-evidence.views",
    )?;
    let summary_views = required_object(
        lakecat,
        "viewReceiptChainProof",
        "lakecatReplayVerification",
    )?;

    for field in ["viewCount", "views", "tombstoneReceipts", "receiptChains"] {
        require_value_match(
            captured_views,
            field,
            required_value(summary_views, field, "viewReceiptChainProof")?,
            "captured LakeCat replay output.replay-evidence.views",
        )?;
    }

    Ok(())
}

pub(crate) fn lakecat_replay_views(
    capture: &serde_json::Map<String, Value>,
) -> lakecat_core::LakeCatResult<&serde_json::Map<String, Value>> {
    required_object(
        lakecat_replay_evidence(capture)?,
        "views",
        "captured LakeCat replay output.replay-evidence",
    )
}

pub(crate) fn verify_lakecat_replay_storage_profile_matches_summary(
    capture: &serde_json::Map<String, Value>,
    lakecat: &serde_json::Map<String, Value>,
) -> lakecat_core::LakeCatResult<()> {
    let captured_storage_profile = lakecat_replay_storage_profile_upsert(capture)?;
    require_only_fields(
        captured_storage_profile,
        STORAGE_PROFILE_UPSERT_PROOF_FIELDS,
        "captured LakeCat replay output.replay-evidence.management.storageProfileUpsert",
    )?;
    let summary_storage_profile = required_object(
        lakecat,
        "storageProfileUpsertProof",
        "lakecatReplayVerification",
    )?;

    for field in [
        "profileId",
        "provider",
        "issuanceMode",
        "locationPrefixHash",
        "principalSubject",
        "principalKind",
        "authorizationReceiptHash",
        "authorizationReceiptAction",
    ] {
        require_string_match(
            captured_storage_profile,
            field,
            required_str(summary_storage_profile, field, "storageProfileUpsertProof")?,
            "captured LakeCat replay output.replay-evidence.management.storageProfileUpsert",
        )?;
    }

    for field in [
        "secretRefPresent",
        "graphEvents",
        "replayEventHashes",
        "openLineageHashes",
    ] {
        require_value_match(
            captured_storage_profile,
            field,
            required_value(summary_storage_profile, field, "storageProfileUpsertProof")?,
            "captured LakeCat replay output.replay-evidence.management.storageProfileUpsert",
        )?;
    }
    for field in ["secretRefProvider", "secretRefHash"] {
        require_optional_null_value_match(
            captured_storage_profile,
            field,
            summary_storage_profile.get(field),
            "captured LakeCat replay output.replay-evidence.management.storageProfileUpsert",
        )?;
    }

    Ok(())
}

pub(crate) fn verify_lakecat_replay_management_matches_summary(
    capture: &serde_json::Map<String, Value>,
    lakecat: &serde_json::Map<String, Value>,
) -> lakecat_core::LakeCatResult<()> {
    let captured_management = lakecat_replay_management(capture)?;
    require_only_fields(
        captured_management,
        CAPTURED_MANAGEMENT_PROOF_FIELDS,
        "captured LakeCat replay output.replay-evidence.management",
    )?;
    require_only_fields(
        required_object(
            captured_management,
            "policyUpsertProof",
            "captured LakeCat replay output.replay-evidence.management",
        )?,
        MANAGEMENT_POLICY_UPSERT_PROOF_FIELDS,
        "captured LakeCat replay output.replay-evidence.management.policyUpsertProof",
    )?;
    let summary_management =
        required_object(lakecat, "managementProof", "lakecatReplayVerification")?;

    for field in MANAGEMENT_REQUIRED_PROOF_FIELDS {
        require_value_match(
            captured_management,
            field,
            required_value(summary_management, field, "managementProof")?,
            "captured LakeCat replay output.replay-evidence.management",
        )?;
    }
    require_optional_null_value_match(
        captured_management,
        "warehouseProjectId",
        summary_management.get("warehouseProjectId"),
        "captured LakeCat replay output.replay-evidence.management",
    )?;
    let expected_management_replay =
        expected_management_replay_line_from_summary(summary_management, lakecat)?;
    require_string_match(
        capture,
        "management-replay",
        expected_management_replay.as_str(),
        "captured LakeCat replay output",
    )?;

    Ok(())
}

pub(crate) fn expected_management_replay_line_from_summary(
    management: &serde_json::Map<String, Value>,
    lakecat: &serde_json::Map<String, Value>,
) -> lakecat_core::LakeCatResult<String> {
    let storage_profile = required_object(
        lakecat,
        "storageProfileUpsertProof",
        "lakecatReplayVerification",
    )?;
    let policy_upsert = required_object(management, "policyUpsertProof", "managementProof")?;
    Ok(format!(
        "management replay servers={} projects={} warehouses={} policies={} policy_upserts={} policy={} storage_profiles={} storage_profile_upserts={} credential_root={}",
        required_u64(management, "serverCount", "managementProof")?,
        required_u64(management, "projectCount", "managementProof")?,
        required_u64(management, "warehouseCount", "managementProof")?,
        required_u64(management, "policyBindingCount", "managementProof")?,
        1,
        expected_management_policy_upsert_line_from_summary(policy_upsert)?,
        required_u64(management, "storageProfileCount", "managementProof")?,
        1,
        expected_management_storage_profile_upsert_line_from_summary(storage_profile)?,
    ))
}

pub(crate) fn expected_management_policy_upsert_line_from_summary(
    policy_upsert: &serde_json::Map<String, Value>,
) -> lakecat_core::LakeCatResult<String> {
    Ok(format!(
        "{}:odrl_hash={}",
        require_non_blank_str(
            policy_upsert,
            "policyId",
            "managementProof.policyUpsertProof"
        )?,
        require_full_hash_str(
            policy_upsert,
            "odrlHash",
            "managementProof.policyUpsertProof"
        )?,
    ))
}

pub(crate) fn expected_management_storage_profile_upsert_line_from_summary(
    storage_profile: &serde_json::Map<String, Value>,
) -> lakecat_core::LakeCatResult<String> {
    let label = "storageProfileUpsertProof";
    let secret_ref = if required_bool(storage_profile, "secretRefPresent", label)? {
        format!(
            "{}:secret_ref_hash={}",
            require_non_blank_str(storage_profile, "secretRefProvider", label)?,
            require_full_hash_str(storage_profile, "secretRefHash", label)?,
        )
    } else {
        require_absent_or_null_field(storage_profile, "secretRefProvider", label)?;
        require_absent_or_null_field(storage_profile, "secretRefHash", label)?;
        "none".to_string()
    };
    Ok(format!(
        "{}:{}:{}:location_prefix_hash={}:secret_ref={}",
        required_str(storage_profile, "profileId", label)?,
        required_str(storage_profile, "provider", label)?,
        required_str(storage_profile, "issuanceMode", label)?,
        required_str(storage_profile, "locationPrefixHash", label)?,
        secret_ref,
    ))
}

pub(crate) fn lakecat_replay_management_proof_value(
    capture: &serde_json::Map<String, Value>,
) -> lakecat_core::LakeCatResult<Value> {
    Ok(Value::Object(lakecat_replay_management(capture)?.clone()))
}

pub(crate) fn lakecat_replay_management(
    capture: &serde_json::Map<String, Value>,
) -> lakecat_core::LakeCatResult<&serde_json::Map<String, Value>> {
    required_object(
        lakecat_replay_evidence(capture)?,
        "management",
        "captured LakeCat replay output.replay-evidence",
    )
}

pub(crate) fn lakecat_replay_storage_profile_upsert(
    capture: &serde_json::Map<String, Value>,
) -> lakecat_core::LakeCatResult<&serde_json::Map<String, Value>> {
    let management = lakecat_replay_management(capture)?;
    required_object(
        management,
        "storageProfileUpsert",
        "captured LakeCat replay output.replay-evidence.management",
    )
}

pub(crate) fn verify_lakecat_replay_credentials_match_summary(
    capture: &serde_json::Map<String, Value>,
    lakecat: &serde_json::Map<String, Value>,
) -> lakecat_core::LakeCatResult<()> {
    let captured_credentials = lakecat_replay_credentials(capture)?;
    require_only_fields(
        captured_credentials,
        CREDENTIAL_VENDING_PROOF_FIELDS,
        "captured LakeCat replay output.replay-evidence.credentials",
    )?;
    let summary_credentials = required_object(
        lakecat,
        "credentialVendingProof",
        "lakecatReplayVerification",
    )?;

    for branch in ["restricted", "trustedHuman"] {
        let captured = required_object(
            captured_credentials,
            branch,
            "captured LakeCat replay output.replay-evidence.credentials",
        )?;
        let captured_label =
            format!("captured LakeCat replay output.replay-evidence.credentials.{branch}");
        require_only_fields(
            captured,
            CREDENTIAL_VENDING_BRANCH_FIELDS,
            captured_label.as_str(),
        )?;
        require_credential_storage_profile_schema(
            required_object(captured, "storageProfile", captured_label.as_str())?,
            &format!("{captured_label}.storageProfile"),
        )?;
        let summary = required_object(summary_credentials, branch, "credentialVendingProof")?;
        for field in ["principalSubject", "principalKind"] {
            require_string_match(
                captured,
                field,
                required_str(summary, field, "credentialVendingProof")?,
                "captured LakeCat replay output.replay-evidence.credentials",
            )?;
        }
        for field in [
            "credentialCount",
            "credentialPrefixHashes",
            "maxCredentialTtlSeconds",
            "authorizationReceiptHash",
            "authorizationReceiptAction",
            "replayEventHashes",
            "openLineageHashes",
        ] {
            require_value_match(
                captured,
                field,
                required_value(summary, field, "credentialVendingProof")?,
                "captured LakeCat replay output.replay-evidence.credentials",
            )?;
        }
        require_value_match(
            captured,
            "storageProfile",
            required_value(summary, "storageProfile", "credentialVendingProof")?,
            "captured LakeCat replay output.replay-evidence.credentials",
        )?;
    }

    let captured_restricted = required_object(
        captured_credentials,
        "restricted",
        "captured LakeCat replay output.replay-evidence.credentials",
    )?;
    let summary_restricted =
        required_object(summary_credentials, "restricted", "credentialVendingProof")?;
    require_string_match(
        captured_restricted,
        "blockReason",
        required_str(summary_restricted, "blockReason", "credentialVendingProof")?,
        "captured LakeCat replay output.replay-evidence.credentials.restricted",
    )?;
    require_value_match(
        captured_restricted,
        "rawCredentialExceptionAllowed",
        required_value(
            summary_restricted,
            "rawCredentialExceptionAllowed",
            "credentialVendingProof",
        )?,
        "captured LakeCat replay output.replay-evidence.credentials.restricted",
    )?;

    let captured_trusted = required_object(
        captured_credentials,
        "trustedHuman",
        "captured LakeCat replay output.replay-evidence.credentials",
    )?;
    let summary_trusted = required_object(
        summary_credentials,
        "trustedHuman",
        "credentialVendingProof",
    )?;
    for field in [
        "blockReason",
        "rawCredentialExceptionAllowed",
        "rawCredentialExceptionReason",
    ] {
        require_value_match(
            captured_trusted,
            field,
            required_value(summary_trusted, field, "credentialVendingProof")?,
            "captured LakeCat replay output.replay-evidence.credentials.trustedHuman",
        )?;
    }
    let expected_credential_replay =
        expected_credential_replay_line_from_summary(summary_credentials)?;
    require_string_match(
        capture,
        "credential-replay",
        expected_credential_replay.as_str(),
        "captured LakeCat replay output",
    )?;

    Ok(())
}

pub(crate) fn expected_credential_replay_line_from_summary(
    credentials: &serde_json::Map<String, Value>,
) -> lakecat_core::LakeCatResult<String> {
    let restricted = required_object(credentials, "restricted", "credentialVendingProof")?;
    let human = required_object(credentials, "trustedHuman", "credentialVendingProof")?;
    Ok(format!(
        "credential replay restricted=blocked:sail-planned-read-required restricted_count={} restricted_ttl={} restricted_profile={} human=allowed:trusted-human-audited-raw human_count={} human_ttl={} human_profile={}",
        required_u64(
            restricted,
            "credentialCount",
            "credentialVendingProof.restricted"
        )?,
        require_positive_u64(
            restricted,
            "maxCredentialTtlSeconds",
            "credentialVendingProof.restricted",
        )?,
        expected_credential_profile_line_from_summary(
            required_object(
                restricted,
                "storageProfile",
                "credentialVendingProof.restricted"
            )?,
            "credentialVendingProof.restricted.storageProfile",
        )?,
        required_u64(
            human,
            "credentialCount",
            "credentialVendingProof.trustedHuman"
        )?,
        require_positive_u64(
            human,
            "maxCredentialTtlSeconds",
            "credentialVendingProof.trustedHuman",
        )?,
        expected_credential_profile_line_from_summary(
            required_object(
                human,
                "storageProfile",
                "credentialVendingProof.trustedHuman"
            )?,
            "credentialVendingProof.trustedHuman.storageProfile",
        )?,
    ))
}

pub(crate) fn expected_credential_profile_line_from_summary(
    profile: &serde_json::Map<String, Value>,
    label: &str,
) -> lakecat_core::LakeCatResult<String> {
    let secret_ref = if required_bool(profile, "secretRefPresent", label)? {
        let provider = require_non_blank_str(profile, "secretRefProvider", label)?;
        let hash = require_full_hash_str(profile, "secretRefHash", label)?;
        format!("{provider}:secret_ref_hash={hash}")
    } else {
        require_absent_or_null_field(profile, "secretRefProvider", label)?;
        require_absent_or_null_field(profile, "secretRefHash", label)?;
        "none".to_string()
    };
    Ok(format!(
        "{}:{}:{}:location_prefix_hash={}:secret_ref={}:graph_events={}",
        require_non_empty_str(profile, "profileId", label)?,
        require_non_empty_str(profile, "provider", label)?,
        require_non_empty_str(profile, "issuanceMode", label)?,
        require_full_hash_str(profile, "locationPrefixHash", label)?,
        secret_ref,
        require_positive_u64(profile, "graphEvents", label)?,
    ))
}

pub(crate) fn lakecat_replay_credentials(
    capture: &serde_json::Map<String, Value>,
) -> lakecat_core::LakeCatResult<&serde_json::Map<String, Value>> {
    required_object(
        lakecat_replay_evidence(capture)?,
        "credentials",
        "captured LakeCat replay output.replay-evidence",
    )
}

pub(crate) fn lakecat_replay_evidence(
    capture: &serde_json::Map<String, Value>,
) -> lakecat_core::LakeCatResult<&serde_json::Map<String, Value>> {
    required_object(capture, "replay-evidence", "captured LakeCat replay output")
}
