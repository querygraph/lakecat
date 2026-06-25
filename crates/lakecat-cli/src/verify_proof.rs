use crate::*;

pub(crate) fn verify_querygraph_capture_matches_summary(
    capture: &serde_json::Map<String, Value>,
    querygraph: &serde_json::Map<String, Value>,
    table_scope: &HandoffTableScope,
    view_scope: &HandoffViewScope,
    label: &str,
) -> lakecat_core::LakeCatResult<()> {
    require_only_fields(capture, QUERYGRAPH_CAPTURE_FIELDS, label)?;
    require_string_match(capture, "warehouse", table_scope.warehouse.as_str(), label)?;
    require_verified_table_scope(capture, table_scope, label)?;
    require_verified_view_scope(capture, view_scope, label)?;
    require_handoff_summary_fields_match_capture(capture, querygraph, label)?;
    require_querygraph_verified_ids_match_capture(capture, querygraph, label)
}

pub(crate) fn require_handoff_summary_fields_match_capture(
    capture: &serde_json::Map<String, Value>,
    querygraph: &serde_json::Map<String, Value>,
    label: &str,
) -> lakecat_core::LakeCatResult<()> {
    require_u64_match(
        capture,
        "table-count",
        required_u64(querygraph, "tableCount", "querygraphVerification")?,
        label,
    )?;
    require_u64_match(
        capture,
        "view-count",
        required_u64(querygraph, "viewCount", "querygraphVerification")?,
        label,
    )?;
    require_string_match(
        capture,
        "bundle-hash",
        required_str(querygraph, "bundleHash", "querygraphVerification")?,
        label,
    )?;
    require_string_match(
        capture,
        "graph-hash",
        required_str(querygraph, "graphHash", "querygraphVerification")?,
        label,
    )?;
    require_string_match(
        capture,
        "open-lineage-hash",
        required_str(querygraph, "openLineageHash", "querygraphVerification")?,
        label,
    )?;
    require_string_match(
        capture,
        "querygraph-import-hash",
        required_str(querygraph, "querygraphImportHash", "querygraphVerification")?,
        label,
    )?;
    require_value_match(
        capture,
        "standards",
        required_value(querygraph, "standards", "querygraphVerification")?,
        label,
    )
}

pub(crate) fn require_querygraph_verified_ids_match_capture(
    capture: &serde_json::Map<String, Value>,
    querygraph: &serde_json::Map<String, Value>,
    label: &str,
) -> lakecat_core::LakeCatResult<()> {
    require_value_match(
        capture,
        "verified-tables",
        required_value(querygraph, "verifiedTables", "querygraphVerification")?,
        label,
    )?;
    require_value_match(
        capture,
        "verified-views",
        required_value(querygraph, "verifiedViews", "querygraphVerification")?,
        label,
    )
}

pub(crate) fn querygraph_capture_semantics_json(
    capture: &serde_json::Map<String, Value>,
    label: &str,
) -> lakecat_core::LakeCatResult<Value> {
    Ok(json!({
        "warehouse": required_str(capture, "warehouse", label)?,
        "verifiedTables": required_value(capture, "verified-tables", label)?,
        "verifiedViews": required_value(capture, "verified-views", label)?,
        "tableCount": required_u64(capture, "table-count", label)?,
        "viewCount": required_u64(capture, "view-count", label)?,
        "bundleHash": required_str(capture, "bundle-hash", label)?,
        "graphHash": required_str(capture, "graph-hash", label)?,
        "openLineageHash": required_str(capture, "open-lineage-hash", label)?,
        "queryGraphImportHash": required_str(capture, "querygraph-import-hash", label)?,
        "standards": required_value(capture, "standards", label)?,
    }))
}

pub(crate) const LAKECAT_REPLAY_CAPTURE_FIELDS: &[&str] = &[
    "schema-version",
    "status",
    "bundle-hash",
    "graph-hash",
    "open-lineage-hash",
    "querygraph-import-hash",
    "table-count",
    "view-count",
    "verified-tables",
    "verified-views",
    "standards",
    "scan-replay",
    "management-replay",
    "credential-replay",
    "table-commit-history-replay",
    "replay-evidence",
];

pub(crate) const QUERYGRAPH_CAPTURE_FIELDS: &[&str] = &[
    "warehouse",
    "verified-tables",
    "verified-views",
    "table-count",
    "view-count",
    "bundle-hash",
    "graph-hash",
    "open-lineage-hash",
    "querygraph-import-hash",
    "standards",
];

pub(crate) struct HandoffTableScope {
    pub(crate) warehouse: String,
    pub(crate) namespace: String,
    pub(crate) table: String,
}

impl HandoffTableScope {
    pub(crate) fn from_summary(
        summary: &serde_json::Map<String, Value>,
        warehouse: &str,
    ) -> lakecat_core::LakeCatResult<Self> {
        Ok(Self {
            warehouse: require_non_blank_input(warehouse, "handoff summary.warehouse")?.to_string(),
            namespace: require_non_blank_str(summary, "namespace", "handoff summary")?.to_string(),
            table: require_non_blank_str(summary, "table", "handoff summary")?.to_string(),
        })
    }

    fn stable_table_id(&self) -> String {
        format!(
            "lakecat:table:{}:{}:{}",
            self.warehouse, self.namespace, self.table
        )
    }
}

pub(crate) struct HandoffViewScope {
    stable_view_ids: Vec<String>,
}

impl HandoffViewScope {
    pub(crate) fn from_lakecat(
        lakecat: &serde_json::Map<String, Value>,
    ) -> lakecat_core::LakeCatResult<Self> {
        let views = required_object(
            lakecat,
            "viewReceiptChainProof",
            "lakecatReplayVerification",
        )?;
        let mut stable_view_ids = Vec::new();
        for (index, view) in required_array(views, "views", "viewReceiptChainProof")?
            .iter()
            .enumerate()
        {
            let view = view.as_object().ok_or_else(|| {
                lakecat_core::LakeCatError::InvalidArgument(format!(
                    "viewReceiptChainProof.views[{index}] must be an object"
                ))
            })?;
            stable_view_ids
                .push(required_str(view, "stableId", "viewReceiptChainProof.views[]")?.to_string());
        }
        Ok(Self { stable_view_ids })
    }
}

pub(crate) fn require_verified_table_scope(
    capture: &serde_json::Map<String, Value>,
    scope: &HandoffTableScope,
    label: &str,
) -> lakecat_core::LakeCatResult<()> {
    let expected_table = scope.stable_table_id();
    let tables = required_array(capture, "verified-tables", label)?;
    if !tables
        .iter()
        .any(|table| table.as_str() == Some(expected_table.as_str()))
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "{label}.verified-tables must include {expected_table}"
        )));
    }
    Ok(())
}

pub(crate) fn require_verified_view_scope(
    capture: &serde_json::Map<String, Value>,
    scope: &HandoffViewScope,
    label: &str,
) -> lakecat_core::LakeCatResult<()> {
    let views = required_array(capture, "verified-views", label)?;
    for expected_view in &scope.stable_view_ids {
        if !views
            .iter()
            .any(|view| view.as_str() == Some(expected_view.as_str()))
        {
            return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "{label}.verified-views must include {expected_view}"
            )));
        }
    }
    Ok(())
}

pub(crate) fn verify_qglake_handoff_summary_value(
    summary: &Value,
) -> lakecat_core::LakeCatResult<Value> {
    let summary = summary.as_object().ok_or_else(|| {
        lakecat_core::LakeCatError::InvalidArgument(
            "handoff summary root must be an object".to_string(),
        )
    })?;
    require_only_fields(summary, QGLAKE_HANDOFF_SUMMARY_FIELDS, "handoff summary")?;
    require_string_eq(
        summary,
        "schemaVersion",
        "lakecat.qglake.handoff-summary.v1",
        "handoff summary",
    )?;
    require_string_eq(summary, "status", "verified", "handoff summary")?;
    let principal = require_non_blank_str(summary, "principal", "handoff summary")?;
    let scope = require_handoff_scope(summary)?;
    let graph_projection = require_graph_projection_proof(summary)?;
    let querygraph = required_object(summary, "querygraphVerification", "handoff summary")?;
    require_only_fields(
        querygraph,
        QUERYGRAPH_VERIFICATION_FIELDS,
        "querygraphVerification",
    )?;
    let import = required_object(summary, "querygraphImportVerification", "handoff summary")?;
    require_only_fields(
        import,
        QUERYGRAPH_IMPORT_VERIFICATION_FIELDS,
        "querygraphImportVerification",
    )?;
    if required_bool(import, "matchesVerify", "querygraphImportVerification")? != true {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "handoff summary querygraphImportVerification.matchesVerify must be true".to_string(),
        ));
    }
    require_querygraph_import_matches_verify(import, querygraph)?;
    require_core_querygraph_hash_evidence(
        querygraph,
        "querygraphImportHash",
        "querygraphVerification",
    )?;
    require_core_querygraph_hash_evidence(
        import,
        "querygraphImportHash",
        "querygraphImportVerification",
    )?;
    require_qglake_standards_value(
        required_value(querygraph, "standards", "querygraphVerification")?,
        "querygraphVerification.standards",
    )?;
    let lakecat = required_object(summary, "lakecatReplayVerification", "handoff summary")?;
    require_only_fields(
        lakecat,
        LAKECAT_REPLAY_VERIFICATION_FIELDS,
        "lakecatReplayVerification",
    )?;
    require_string_eq(
        lakecat,
        "schemaVersion",
        "lakecat.qglake.replay-verification.v1",
        "lakecatReplayVerification",
    )?;
    require_string_eq(lakecat, "status", "verified", "lakecatReplayVerification")?;
    if required_bool(lakecat, "matchesQueryGraph", "lakecatReplayVerification")? != true {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "handoff summary lakecatReplayVerification.matchesQueryGraph must be true".to_string(),
        ));
    }
    let request_identity = verify_handoff_request_identity_proof(lakecat, principal)?;

    let bootstrap = required_object(
        lakecat,
        "queryGraphBootstrapProof",
        "lakecatReplayVerification",
    )?;
    verify_handoff_bootstrap_proof(bootstrap, querygraph, request_identity, principal)?;

    let catalog_config =
        required_object(lakecat, "catalogConfigProof", "lakecatReplayVerification")?;
    require_catalog_config_evidence(catalog_config, principal)?;

    verify_handoff_governed_scan_proof(lakecat, principal)?;
    verify_handoff_replay_tail(lakecat, querygraph, bootstrap, &scope, principal)?;

    Ok(json!({
        "schemaVersion": "lakecat.qglake.handoff-verification.v1",
        "status": "verified",
        "principal": principal,
        "catalogUrl": scope.catalog_url,
        "warehouse": scope.warehouse,
        "namespace": scope.namespace,
        "table": scope.table,
        "tableCount": required_u64(querygraph, "tableCount", "querygraphVerification")?,
        "viewCount": required_u64(querygraph, "viewCount", "querygraphVerification")?,
        "verifiedTables": required_value(querygraph, "verifiedTables", "querygraphVerification")?,
        "verifiedViews": required_value(querygraph, "verifiedViews", "querygraphVerification")?,
        "standards": required_value(querygraph, "standards", "querygraphVerification")?,
        "graphProjectionProof": graph_projection,
        "queryGraphBootstrapProof": bootstrap,
        "requestIdentityProof": request_identity,
    }))
}

/// Verify the `requestIdentityProof` section of a handoff summary's
/// `lakecatReplayVerification`, returning the borrowed proof object for reuse.
fn verify_handoff_request_identity_proof<'a>(
    lakecat: &'a serde_json::Map<String, Value>,
    principal: &str,
) -> lakecat_core::LakeCatResult<&'a serde_json::Map<String, Value>> {
    let request_identity =
        required_object(lakecat, "requestIdentityProof", "lakecatReplayVerification")?;
    require_only_fields(
        request_identity,
        REQUEST_IDENTITY_PROOF_FIELDS,
        "requestIdentityProof",
    )?;
    require_string_match(
        request_identity,
        "principalSubject",
        principal,
        "requestIdentityProof",
    )?;
    require_string_eq(
        request_identity,
        "principalKind",
        "agent",
        "requestIdentityProof",
    )?;
    require_non_blank_str(
        request_identity,
        "requestIdentitySource",
        "requestIdentityProof",
    )?;
    require_non_blank_str(
        request_identity,
        "requestIdentityState",
        "requestIdentityProof",
    )?;
    require_full_hash_str(
        request_identity,
        "authorizationReceiptHash",
        "requestIdentityProof",
    )?;
    require_string_eq(
        request_identity,
        "authorizationReceiptAction",
        "lineage-read",
        "requestIdentityProof",
    )?;
    require_typedid_hash_pair(request_identity, "requestIdentityProof")?;
    Ok(request_identity)
}

/// Verify the `queryGraphBootstrapProof` section against the querygraph and
/// request-identity evidence already validated for the same summary.
fn verify_handoff_bootstrap_proof(
    bootstrap: &serde_json::Map<String, Value>,
    querygraph: &serde_json::Map<String, Value>,
    request_identity: &serde_json::Map<String, Value>,
    principal: &str,
) -> lakecat_core::LakeCatResult<()> {
    require_only_fields(
        bootstrap,
        QUERYGRAPH_BOOTSTRAP_PROOF_FIELDS,
        "queryGraphBootstrapProof",
    )?;
    require_core_querygraph_hash_evidence(
        bootstrap,
        "queryGraphImportHash",
        "queryGraphBootstrapProof",
    )?;
    require_string_match(
        bootstrap,
        "bundleHash",
        required_str(querygraph, "bundleHash", "querygraphVerification")?,
        "queryGraphBootstrapProof",
    )?;
    require_string_match(
        bootstrap,
        "graphHash",
        required_str(querygraph, "graphHash", "querygraphVerification")?,
        "queryGraphBootstrapProof",
    )?;
    require_string_match(
        bootstrap,
        "openLineageHash",
        required_str(querygraph, "openLineageHash", "querygraphVerification")?,
        "queryGraphBootstrapProof",
    )?;
    require_string_match(
        bootstrap,
        "queryGraphImportHash",
        required_str(querygraph, "querygraphImportHash", "querygraphVerification")?,
        "queryGraphBootstrapProof",
    )?;
    require_u64_match(
        bootstrap,
        "tableArtifactCount",
        required_u64(querygraph, "tableCount", "querygraphVerification")?,
        "queryGraphBootstrapProof",
    )?;
    require_u64_match(
        bootstrap,
        "viewArtifactCount",
        required_u64(querygraph, "viewCount", "querygraphVerification")?,
        "queryGraphBootstrapProof",
    )?;
    require_positive_u64(bootstrap, "policyBindingCount", "queryGraphBootstrapProof")?;
    require_value_match(
        bootstrap,
        "standards",
        required_value(querygraph, "standards", "querygraphVerification")?,
        "queryGraphBootstrapProof",
    )?;
    require_string_match(
        bootstrap,
        "principalSubject",
        principal,
        "queryGraphBootstrapProof",
    )?;
    require_string_eq(
        bootstrap,
        "principalKind",
        "agent",
        "queryGraphBootstrapProof",
    )?;
    for field in ["requestIdentitySource", "requestIdentityState"] {
        require_string_match(
            bootstrap,
            field,
            required_str(request_identity, field, "requestIdentityProof")?,
            "queryGraphBootstrapProof",
        )?;
    }
    require_full_hash_str(
        bootstrap,
        "authorizationReceiptHash",
        "queryGraphBootstrapProof",
    )?;
    require_string_eq(
        bootstrap,
        "authorizationReceiptAction",
        "graph-read",
        "queryGraphBootstrapProof",
    )?;
    require_full_hash_str(bootstrap, "agentDelegationHash", "queryGraphBootstrapProof")?;
    require_full_hash_str(
        bootstrap,
        "agentSummarySignatureHash",
        "queryGraphBootstrapProof",
    )?;
    require_typedid_hash_pair(bootstrap, "queryGraphBootstrapProof")?;
    if required_u64(querygraph, "viewCount", "querygraphVerification")? > 0 {
        require_full_hash_array(
            bootstrap,
            "viewVersionReceiptHashes",
            "queryGraphBootstrapProof",
        )?;
    } else {
        required_array(
            bootstrap,
            "viewVersionReceiptHashes",
            "queryGraphBootstrapProof",
        )?;
    }
    require_full_hash_array(bootstrap, "replayEventHashes", "queryGraphBootstrapProof")?;
    require_full_hash_array(bootstrap, "openLineageHashes", "queryGraphBootstrapProof")?;
    Ok(())
}

/// Verify the `governedScanProof` section, including the planned/fetched read
/// restriction parity and projection/stats evidence.
fn verify_handoff_governed_scan_proof(
    lakecat: &serde_json::Map<String, Value>,
    principal: &str,
) -> lakecat_core::LakeCatResult<()> {
    let governed_scan = required_object(lakecat, "governedScanProof", "lakecatReplayVerification")?;
    require_only_fields(
        governed_scan,
        GOVERNED_SCAN_PROOF_FIELDS,
        "governedScanProof",
    )?;
    require_positive_u64(governed_scan, "planTaskCount", "governedScanProof")?;
    require_positive_u64(governed_scan, "planGraphEvents", "governedScanProof")?;
    require_positive_u64(governed_scan, "fileTaskCount", "governedScanProof")?;
    require_positive_u64(governed_scan, "deleteFileCount", "governedScanProof")?;
    require_positive_u64(governed_scan, "childPlanTaskCount", "governedScanProof")?;
    require_string_match(
        governed_scan,
        "plannedPrincipalSubject",
        principal,
        "governedScanProof",
    )?;
    require_string_match(
        governed_scan,
        "fetchedPrincipalSubject",
        principal,
        "governedScanProof",
    )?;
    require_string_eq(
        governed_scan,
        "plannedPrincipalKind",
        "agent",
        "governedScanProof",
    )?;
    require_string_eq(
        governed_scan,
        "fetchedPrincipalKind",
        "agent",
        "governedScanProof",
    )?;
    require_full_hash_str(
        governed_scan,
        "plannedAuthorizationReceiptHash",
        "governedScanProof",
    )?;
    require_full_hash_str(
        governed_scan,
        "fetchedAuthorizationReceiptHash",
        "governedScanProof",
    )?;
    require_string_eq(
        governed_scan,
        "plannedAuthorizationReceiptAction",
        "table-plan-scan",
        "governedScanProof",
    )?;
    require_string_eq(
        governed_scan,
        "fetchedAuthorizationReceiptAction",
        "table-plan-scan",
        "governedScanProof",
    )?;
    let planned_restriction =
        required_object(governed_scan, "plannedReadRestriction", "governedScanProof")?;
    let fetched_restriction =
        required_object(governed_scan, "fetchedReadRestriction", "governedScanProof")?;
    require_read_restriction_evidence(
        planned_restriction,
        "governedScanProof.plannedReadRestriction",
    )?;
    require_read_restriction_evidence(
        fetched_restriction,
        "governedScanProof.fetchedReadRestriction",
    )?;
    require_value_match(
        planned_restriction,
        "policy-hashes",
        required_value(
            fetched_restriction,
            "policy-hashes",
            "governedScanProof.fetchedReadRestriction",
        )?,
        "governedScanProof.plannedReadRestriction",
    )?;
    require_value_match(
        planned_restriction,
        "allowed-columns",
        required_value(
            fetched_restriction,
            "allowed-columns",
            "governedScanProof.fetchedReadRestriction",
        )?,
        "governedScanProof.plannedReadRestriction",
    )?;
    require_value_match(
        planned_restriction,
        "row-predicate",
        required_value(
            fetched_restriction,
            "row-predicate",
            "governedScanProof.fetchedReadRestriction",
        )?,
        "governedScanProof.plannedReadRestriction",
    )?;
    require_value_match(
        planned_restriction,
        "purpose",
        required_value(
            fetched_restriction,
            "purpose",
            "governedScanProof.fetchedReadRestriction",
        )?,
        "governedScanProof.plannedReadRestriction",
    )?;
    require_value_match(
        planned_restriction,
        "max-credential-ttl-seconds",
        required_value(
            fetched_restriction,
            "max-credential-ttl-seconds",
            "governedScanProof.fetchedReadRestriction",
        )?,
        "governedScanProof.plannedReadRestriction",
    )?;
    require_value_match(
        fetched_restriction,
        "allowed-columns",
        required_value(
            governed_scan,
            "fetchedRequiredProjection",
            "governedScanProof",
        )?,
        "governedScanProof.fetchedReadRestriction",
    )?;
    require_value_match(
        fetched_restriction,
        "allowed-columns",
        required_value(
            governed_scan,
            "fetchedEffectiveProjection",
            "governedScanProof",
        )?,
        "governedScanProof.fetchedReadRestriction",
    )?;
    require_governed_scan_projection_evidence(governed_scan, planned_restriction)?;
    require_governed_scan_stats_field_evidence(
        governed_scan,
        planned_restriction,
        fetched_restriction,
    )?;
    let fetched_required_filters =
        required_array(governed_scan, "fetchedRequiredFilters", "governedScanProof")?;
    let expected_fetched_filters = vec![
        required_value(
            fetched_restriction,
            "row-predicate",
            "governedScanProof.fetchedReadRestriction",
        )?
        .clone(),
    ];
    if fetched_required_filters.as_slice() != expected_fetched_filters.as_slice() {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "governedScanProof fetchedRequiredFilters did not exactly preserve fetched row predicate: {}",
            Value::Array(fetched_required_filters.clone())
        )));
    }
    require_full_hash_array(
        governed_scan,
        "plannedReplayEventHashes",
        "governedScanProof",
    )?;
    require_full_hash_array(
        governed_scan,
        "fetchedReplayEventHashes",
        "governedScanProof",
    )?;
    require_full_hash_array(
        governed_scan,
        "plannedOpenLineageHashes",
        "governedScanProof",
    )?;
    require_full_hash_array(
        governed_scan,
        "fetchedOpenLineageHashes",
        "governedScanProof",
    )?;
    Ok(())
}

/// Verify the remaining replay-proof sections (commit history, management,
/// storage-profile upsert, credential vending, and view receipt chains).
fn verify_handoff_replay_tail(
    lakecat: &serde_json::Map<String, Value>,
    querygraph: &serde_json::Map<String, Value>,
    bootstrap: &serde_json::Map<String, Value>,
    scope: &HandoffScope,
    principal: &str,
) -> lakecat_core::LakeCatResult<()> {
    let commit_history = required_object(
        lakecat,
        "tableCommitHistoryProof",
        "lakecatReplayVerification",
    )?;
    require_table_commit_history_evidence(commit_history, principal, "agent")?;

    let management = required_object(lakecat, "managementProof", "lakecatReplayVerification")?;
    require_management_evidence(
        management,
        required_u64(bootstrap, "policyBindingCount", "queryGraphBootstrapProof")?,
    )?;

    let storage_profile = required_object(
        lakecat,
        "storageProfileUpsertProof",
        "lakecatReplayVerification",
    )?;
    require_storage_profile_upsert_evidence(storage_profile)?;

    let credentials = required_object(
        lakecat,
        "credentialVendingProof",
        "lakecatReplayVerification",
    )?;
    require_credential_vending_evidence(credentials, principal, storage_profile)?;

    let views = required_object(
        lakecat,
        "viewReceiptChainProof",
        "lakecatReplayVerification",
    )?;
    require_querygraph_verified_scope(querygraph, scope, views)?;
    require_u64_match(
        views,
        "viewCount",
        required_u64(querygraph, "viewCount", "querygraphVerification")?,
        "viewReceiptChainProof",
    )?;
    required_array(views, "views", "viewReceiptChainProof")?;
    required_array(views, "tombstoneReceipts", "viewReceiptChainProof")?;
    required_array(views, "receiptChains", "viewReceiptChainProof")?;
    require_bootstrap_view_receipt_hashes_match_views(bootstrap, views)?;
    require_view_tombstone_expected_versions(views)?;
    require_view_receipt_chain_evidence(views)?;
    Ok(())
}

pub(crate) const QGLAKE_HANDOFF_SUMMARY_FIELDS: &[&str] = &[
    "schemaVersion",
    "status",
    "catalogUrl",
    "principal",
    "warehouse",
    "namespace",
    "table",
    "querygraphVerification",
    "querygraphImportVerification",
    "lakecatReplayVerification",
    "graphProjectionProof",
    "artifacts",
];

pub(crate) const GRAPH_PROJECTION_PROOF_FIELDS: &[&str] = &[
    "backend",
    "feature",
    "pathHash",
    "tablePrefix",
    "catalogGraphSink",
];
pub(crate) const QGLAKE_GRUST_TURSO_TABLE_PREFIX: &str = "lakecat_graph";

pub(crate) const QUERYGRAPH_VERIFICATION_FIELDS: &[&str] = &[
    "tableCount",
    "viewCount",
    "verifiedTables",
    "verifiedViews",
    "bundleHash",
    "graphHash",
    "openLineageHash",
    "querygraphImportHash",
    "standards",
];

pub(crate) const QUERYGRAPH_IMPORT_VERIFICATION_FIELDS: &[&str] = &[
    "matchesVerify",
    "tableCount",
    "viewCount",
    "verifiedTables",
    "verifiedViews",
    "bundleHash",
    "graphHash",
    "openLineageHash",
    "querygraphImportHash",
    "standards",
];

pub(crate) const LAKECAT_REPLAY_VERIFICATION_FIELDS: &[&str] = &[
    "schemaVersion",
    "status",
    "matchesQueryGraph",
    "requestIdentityProof",
    "queryGraphBootstrapProof",
    "catalogConfigProof",
    "governedScanProof",
    "tableCommitHistoryProof",
    "managementProof",
    "viewReceiptChainProof",
    "storageProfileUpsertProof",
    "credentialVendingProof",
    "replayEvidence",
];

pub(crate) fn require_graph_projection_proof(
    summary: &serde_json::Map<String, Value>,
) -> lakecat_core::LakeCatResult<Value> {
    let proof = required_object(summary, "graphProjectionProof", "handoff summary")?;
    require_only_fields(proof, GRAPH_PROJECTION_PROOF_FIELDS, "graphProjectionProof")?;
    require_string_eq(proof, "backend", "grust-turso", "graphProjectionProof")?;
    require_string_eq(
        proof,
        "feature",
        "grust-turso-local",
        "graphProjectionProof",
    )?;
    require_full_hash_str(proof, "pathHash", "graphProjectionProof")?;
    require_string_eq(
        proof,
        "tablePrefix",
        QGLAKE_GRUST_TURSO_TABLE_PREFIX,
        "graphProjectionProof",
    )?;
    require_string_eq(
        proof,
        "catalogGraphSink",
        "GrustCatalogGraphSink<TursoGraphStore>",
        "graphProjectionProof",
    )?;
    Ok(Value::Object(proof.clone()))
}

pub(crate) const REQUEST_IDENTITY_PROOF_FIELDS: &[&str] = &[
    "principalSubject",
    "principalKind",
    "requestIdentitySource",
    "requestIdentityState",
    "authorizationReceiptHash",
    "authorizationReceiptAction",
    "typedidEnvelopeHash",
    "typedidProofHash",
];

pub(crate) const QUERYGRAPH_BOOTSTRAP_PROOF_FIELDS: &[&str] = &[
    "bundleHash",
    "graphHash",
    "openLineageHash",
    "queryGraphImportHash",
    "tableArtifactCount",
    "viewArtifactCount",
    "policyBindingCount",
    "standards",
    "principalSubject",
    "principalKind",
    "requestIdentitySource",
    "requestIdentityState",
    "authorizationReceiptHash",
    "authorizationReceiptAction",
    "agentDelegationHash",
    "agentSummarySignatureHash",
    "typedidEnvelopeHash",
    "typedidProofHash",
    "viewVersionReceiptHashes",
    "replayEventHashes",
    "openLineageHashes",
];

pub(crate) fn require_querygraph_verified_scope(
    querygraph: &serde_json::Map<String, Value>,
    scope: &HandoffScope<'_>,
    views: &serde_json::Map<String, Value>,
) -> lakecat_core::LakeCatResult<()> {
    let table_count = required_u64(querygraph, "tableCount", "querygraphVerification")?;
    let verified_tables = required_array(querygraph, "verifiedTables", "querygraphVerification")?;
    let verified_tables = require_unique_stable_id_array(
        verified_tables,
        table_count,
        "querygraphVerification.verifiedTables",
    )?;
    let expected_table = format!(
        "lakecat:table:{}:{}:{}",
        scope.warehouse, scope.namespace, scope.table
    );
    if !verified_tables
        .iter()
        .any(|table| *table == expected_table.as_str())
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "querygraphVerification.verifiedTables must include {expected_table}"
        )));
    }

    let view_count = required_u64(querygraph, "viewCount", "querygraphVerification")?;
    let verified_views = required_array(querygraph, "verifiedViews", "querygraphVerification")?;
    let verified_views = require_unique_stable_id_array(
        verified_views,
        view_count,
        "querygraphVerification.verifiedViews",
    )?;
    for (index, view) in required_array(views, "views", "viewReceiptChainProof")?
        .iter()
        .enumerate()
    {
        let view = view.as_object().ok_or_else(|| {
            lakecat_core::LakeCatError::InvalidArgument(format!(
                "viewReceiptChainProof.views[{index}] must be an object"
            ))
        })?;
        let expected_view = required_str(view, "stableId", "viewReceiptChainProof.views[]")?;
        if !verified_views.iter().any(|view| *view == expected_view) {
            return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "querygraphVerification.verifiedViews must include {expected_view}"
            )));
        }
    }
    Ok(())
}

pub(crate) fn require_qglake_standards_value(
    value: &Value,
    label: &str,
) -> lakecat_core::LakeCatResult<()> {
    let standards = value.as_array().ok_or_else(|| {
        lakecat_core::LakeCatError::InvalidArgument(format!("{label} must be an array"))
    })?;
    let expected = QGLAKE_BOOTSTRAP_STANDARDS
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    let mut seen = BTreeSet::new();
    for (index, standard) in standards.iter().enumerate() {
        let standard = standard.as_str().ok_or_else(|| {
            lakecat_core::LakeCatError::InvalidArgument(format!(
                "{label}[{index}] must be a string"
            ))
        })?;
        if standard.trim().is_empty() {
            return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "{label} must contain non-empty strings"
            )));
        }
        if !expected.contains(standard) {
            return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "{label} contains unsupported QGLake standard {standard}"
            )));
        }
        if !seen.insert(standard) {
            return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "{label} must be duplicate-free"
            )));
        }
    }
    for expected in QGLAKE_BOOTSTRAP_STANDARDS {
        if !seen.contains(expected) {
            return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "{label} did not include required QGLake standard {expected}"
            )));
        }
    }
    Ok(())
}

pub(crate) fn require_unique_stable_id_array<'a>(
    values: &'a [Value],
    expected_count: u64,
    label: &str,
) -> lakecat_core::LakeCatResult<Vec<&'a str>> {
    if values.len() as u64 != expected_count {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "{label} length mismatch: expected={expected_count} actual={}",
            values.len()
        )));
    }
    let mut stable_ids = Vec::with_capacity(values.len());
    let mut seen = BTreeSet::new();
    for (index, value) in values.iter().enumerate() {
        let stable_id = value.as_str().ok_or_else(|| {
            lakecat_core::LakeCatError::InvalidArgument(format!(
                "{label}[{index}] must be a string"
            ))
        })?;
        if stable_id.trim().is_empty() {
            return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "{label}[{index}] must not be empty"
            )));
        }
        if !seen.insert(stable_id) {
            return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "{label} must be duplicate-free"
            )));
        }
        stable_ids.push(stable_id);
    }
    Ok(stable_ids)
}

pub(crate) fn require_querygraph_import_matches_verify(
    import: &serde_json::Map<String, Value>,
    querygraph: &serde_json::Map<String, Value>,
) -> lakecat_core::LakeCatResult<()> {
    for field in [
        "tableCount",
        "viewCount",
        "verifiedTables",
        "verifiedViews",
        "bundleHash",
        "graphHash",
        "openLineageHash",
        "querygraphImportHash",
        "standards",
    ] {
        require_value_match(
            import,
            field,
            required_value(querygraph, field, "querygraphVerification")?,
            "querygraphImportVerification",
        )?;
    }
    Ok(())
}

pub(crate) fn require_core_querygraph_hash_evidence(
    value: &serde_json::Map<String, Value>,
    import_hash_field: &str,
    label: &str,
) -> lakecat_core::LakeCatResult<()> {
    for field in ["bundleHash", "graphHash", "openLineageHash"] {
        require_full_hash_str(value, field, label)?;
    }
    require_full_hash_str(value, import_hash_field, label)?;
    Ok(())
}

pub(crate) struct HandoffScope<'a> {
    catalog_url: &'a str,
    warehouse: &'a str,
    namespace: &'a str,
    table: &'a str,
}

pub(crate) fn require_handoff_scope<'a>(
    summary: &'a serde_json::Map<String, Value>,
) -> lakecat_core::LakeCatResult<HandoffScope<'a>> {
    let catalog_url = require_handoff_catalog_url(summary)?;
    let warehouse = require_non_blank_str(summary, "warehouse", "handoff summary")?;
    let namespace = require_non_blank_str(summary, "namespace", "handoff summary")?;
    let table = require_non_blank_str(summary, "table", "handoff summary")?;
    Ok(HandoffScope {
        catalog_url,
        warehouse,
        namespace,
        table,
    })
}

pub(crate) fn require_handoff_catalog_url<'a>(
    summary: &'a serde_json::Map<String, Value>,
) -> lakecat_core::LakeCatResult<&'a str> {
    let catalog_url = require_non_empty_str(summary, "catalogUrl", "handoff summary")?;
    let parsed = Url::parse(catalog_url).map_err(|err| {
        lakecat_core::LakeCatError::InvalidArgument(format!(
            "handoff summary catalogUrl must be an absolute HTTP(S) URL: {err}"
        ))
    })?;
    if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "handoff summary catalogUrl must be an absolute HTTP(S) URL with a host: {catalog_url}"
        )));
    }
    Ok(catalog_url)
}

pub(crate) fn require_storage_profile_upsert_evidence(
    storage_profile: &serde_json::Map<String, Value>,
) -> lakecat_core::LakeCatResult<()> {
    require_only_fields(
        storage_profile,
        STORAGE_PROFILE_UPSERT_PROOF_FIELDS,
        "storageProfileUpsertProof",
    )?;
    require_non_empty_str(storage_profile, "profileId", "storageProfileUpsertProof")?;
    let provider = require_non_empty_str(storage_profile, "provider", "storageProfileUpsertProof")?;
    let issuance_mode =
        require_non_empty_str(storage_profile, "issuanceMode", "storageProfileUpsertProof")?;
    require_storage_profile_provider_issuance_compatibility(
        provider,
        issuance_mode,
        "storageProfileUpsertProof",
    )?;
    require_full_hash_str(
        storage_profile,
        "locationPrefixHash",
        "storageProfileUpsertProof",
    )?;
    if required_bool(
        storage_profile,
        "secretRefPresent",
        "storageProfileUpsertProof",
    )? {
        require_non_blank_str(
            storage_profile,
            "secretRefProvider",
            "storageProfileUpsertProof",
        )?;
        require_full_hash_str(
            storage_profile,
            "secretRefHash",
            "storageProfileUpsertProof",
        )?;
    } else {
        require_absent_or_null_field(
            storage_profile,
            "secretRefProvider",
            "storageProfileUpsertProof",
        )?;
        require_absent_or_null_field(
            storage_profile,
            "secretRefHash",
            "storageProfileUpsertProof",
        )?;
    }
    require_full_hash_array(
        storage_profile,
        "replayEventHashes",
        "storageProfileUpsertProof",
    )?;
    require_full_hash_array(
        storage_profile,
        "openLineageHashes",
        "storageProfileUpsertProof",
    )?;
    require_non_blank_str(
        storage_profile,
        "principalSubject",
        "storageProfileUpsertProof",
    )?;
    require_non_blank_str(
        storage_profile,
        "principalKind",
        "storageProfileUpsertProof",
    )?;
    require_full_hash_str(
        storage_profile,
        "authorizationReceiptHash",
        "storageProfileUpsertProof",
    )?;
    require_string_eq(
        storage_profile,
        "authorizationReceiptAction",
        "storage-profile-manage",
        "storageProfileUpsertProof",
    )?;
    require_positive_u64(storage_profile, "graphEvents", "storageProfileUpsertProof")?;
    Ok(())
}

pub(crate) fn require_typedid_hash_pair(
    value: &serde_json::Map<String, Value>,
    label: &str,
) -> lakecat_core::LakeCatResult<()> {
    let envelope_present = require_optional_hash_value(value, "typedidEnvelopeHash", label)?;
    let proof_present = require_optional_hash_value(value, "typedidProofHash", label)?;
    if proof_present && !envelope_present {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "{label}.typedidProofHash requires typedidEnvelopeHash"
        )));
    }
    Ok(())
}

pub(crate) fn require_management_evidence(
    management: &serde_json::Map<String, Value>,
    expected_policy_binding_count: u64,
) -> lakecat_core::LakeCatResult<()> {
    require_only_fields(management, MANAGEMENT_PROOF_FIELDS, "managementProof")?;
    let server_count = require_positive_u64(management, "serverCount", "managementProof")?;
    require_positive_u64(management, "serverGraphEvents", "managementProof")?;
    require_unique_string_array_count(management, "serverIds", server_count, "managementProof")?;
    let project_count = require_positive_u64(management, "projectCount", "managementProof")?;
    require_positive_u64(management, "projectGraphEvents", "managementProof")?;
    require_unique_string_array_count(management, "projectIds", project_count, "managementProof")?;
    let project_ids = required_string_array(management, "projectIds", "managementProof")?;
    let warehouse_count = require_positive_u64(management, "warehouseCount", "managementProof")?;
    require_positive_u64(management, "warehouseGraphEvents", "managementProof")?;
    require_unique_string_array_count(
        management,
        "warehouseNames",
        warehouse_count,
        "managementProof",
    )?;
    if let Some(warehouse_project_id) =
        optional_compact_management_id(management, "warehouseProjectId", "managementProof")?
    {
        if !project_ids
            .iter()
            .any(|project_id| project_id == warehouse_project_id)
        {
            return Err(lakecat_core::LakeCatError::InvalidArgument(
                "managementProof.warehouseProjectId must match projectIds".to_string(),
            ));
        }
    }
    require_positive_u64(management, "policyGraphEvents", "managementProof")?;
    require_unique_string_array_count(
        management,
        "policyIds",
        expected_policy_binding_count,
        "managementProof",
    )?;
    let policy_ids = required_string_array(management, "policyIds", "managementProof")?;
    let policy_upsert = required_object(management, "policyUpsertProof", "managementProof")?;
    require_only_fields(
        policy_upsert,
        MANAGEMENT_POLICY_UPSERT_PROOF_FIELDS,
        "managementProof.policyUpsertProof",
    )?;
    let policy_upsert_id = require_non_blank_str(
        policy_upsert,
        "policyId",
        "managementProof.policyUpsertProof",
    )?;
    if !policy_ids
        .iter()
        .any(|policy_id| policy_id == policy_upsert_id)
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "managementProof.policyUpsertProof.policyId must match policyIds".to_string(),
        ));
    }
    require_full_hash_str(
        policy_upsert,
        "odrlHash",
        "managementProof.policyUpsertProof",
    )?;
    require_non_blank_str(
        policy_upsert,
        "principalSubject",
        "managementProof.policyUpsertProof",
    )?;
    require_non_blank_str(
        policy_upsert,
        "principalKind",
        "managementProof.policyUpsertProof",
    )?;
    require_full_hash_str(
        policy_upsert,
        "authorizationReceiptHash",
        "managementProof.policyUpsertProof",
    )?;
    let policy_upsert_action = require_non_empty_str(
        policy_upsert,
        "authorizationReceiptAction",
        "managementProof.policyUpsertProof",
    )?;
    if policy_upsert_action != "policy-manage" {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "managementProof.policyUpsertProof.authorizationReceiptAction must be policy-manage"
                .to_string(),
        ));
    }
    require_positive_u64(
        policy_upsert,
        "graphEvents",
        "managementProof.policyUpsertProof",
    )?;
    require_full_hash_array(
        policy_upsert,
        "replayEventHashes",
        "managementProof.policyUpsertProof",
    )?;
    require_full_hash_array(
        policy_upsert,
        "openLineageHashes",
        "managementProof.policyUpsertProof",
    )?;
    let storage_profile_count =
        require_positive_u64(management, "storageProfileCount", "managementProof")?;
    require_positive_u64(management, "storageProfileGraphEvents", "managementProof")?;
    require_unique_string_array_count(
        management,
        "storageProfileIds",
        storage_profile_count,
        "managementProof",
    )?;
    require_u64_match(
        management,
        "policyBindingCount",
        expected_policy_binding_count,
        "managementProof",
    )?;
    for field in [
        "serverReplayEventHashes",
        "serverOpenLineageHashes",
        "projectReplayEventHashes",
        "projectOpenLineageHashes",
        "warehouseReplayEventHashes",
        "warehouseOpenLineageHashes",
        "policyReplayEventHashes",
        "policyOpenLineageHashes",
        "storageProfileReplayEventHashes",
        "storageProfileOpenLineageHashes",
    ] {
        require_full_hash_array(management, field, "managementProof")?;
    }
    Ok(())
}

pub(crate) fn optional_compact_management_id<'a>(
    value: &'a serde_json::Map<String, Value>,
    field: &str,
    label: &str,
) -> lakecat_core::LakeCatResult<Option<&'a str>> {
    let Some(value) = value.get(field) else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(None);
    }
    let Some(id) = value.as_str() else {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "{label}.{field} must be a string when present"
        )));
    };
    if !is_qglake_compact_management_id(id) {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "{label}.{field} contains syntactically invalid compact management ID evidence"
        )));
    }
    Ok(Some(id))
}

pub(crate) fn require_table_commit_history_evidence(
    commit_history: &serde_json::Map<String, Value>,
    principal: &str,
    principal_kind: &str,
) -> lakecat_core::LakeCatResult<()> {
    require_only_fields(
        commit_history,
        TABLE_COMMIT_HISTORY_PROOF_FIELDS,
        "tableCommitHistoryProof",
    )?;
    require_string_match(
        commit_history,
        "principalSubject",
        principal,
        "tableCommitHistoryProof",
    )?;
    require_string_match(
        commit_history,
        "principalKind",
        principal_kind,
        "tableCommitHistoryProof",
    )?;
    require_full_hash_str(
        commit_history,
        "authorizationReceiptHash",
        "tableCommitHistoryProof",
    )?;
    require_string_eq(
        commit_history,
        "authorizationReceiptAction",
        "table-load",
        "tableCommitHistoryProof",
    )?;
    let commit_count = required_u64(commit_history, "commitCount", "tableCommitHistoryProof")?;
    let sequence_numbers =
        required_array(commit_history, "sequenceNumbers", "tableCommitHistoryProof")?;
    if sequence_numbers.len() as u64 != commit_count {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "tableCommitHistoryProof.sequenceNumbers length mismatch: expected={commit_count} actual={}",
            sequence_numbers.len()
        )));
    }
    let mut previous = 0;
    for (index, sequence_number) in sequence_numbers.iter().enumerate() {
        let Some(sequence_number) = sequence_number.as_u64() else {
            return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "tableCommitHistoryProof.sequenceNumbers[{index}] must be a positive integer"
            )));
        };
        if sequence_number == 0 {
            return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "tableCommitHistoryProof.sequenceNumbers[{index}] must be positive"
            )));
        }
        if sequence_number <= previous {
            return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "tableCommitHistoryProof.sequenceNumbers must be strictly increasing"
            )));
        }
        previous = sequence_number;
    }

    let commit_hashes = required_array(commit_history, "commitHashes", "tableCommitHistoryProof")?;
    if commit_hashes.len() as u64 != commit_count {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "tableCommitHistoryProof.commitHashes length mismatch: expected={commit_count} actual={}",
            commit_hashes.len()
        )));
    }
    if commit_count > 0 {
        require_full_hash_array(commit_history, "commitHashes", "tableCommitHistoryProof")?;
    }
    let mut unique_commit_hashes = BTreeSet::new();
    for commit_hash in commit_hashes.iter().filter_map(Value::as_str) {
        if !unique_commit_hashes.insert(commit_hash) {
            return Err(lakecat_core::LakeCatError::InvalidArgument(
                "tableCommitHistoryProof.commitHashes must not contain duplicate hashes"
                    .to_string(),
            ));
        }
    }
    require_positive_u64(commit_history, "graphEvents", "tableCommitHistoryProof")?;
    require_full_hash_array(
        commit_history,
        "replayEventHashes",
        "tableCommitHistoryProof",
    )?;
    require_full_hash_array(
        commit_history,
        "openLineageHashes",
        "tableCommitHistoryProof",
    )?;
    Ok(())
}

pub(crate) const TABLE_COMMIT_HISTORY_PROOF_FIELDS: &[&str] = &[
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
];

pub(crate) const GOVERNED_SCAN_PROOF_FIELDS: &[&str] = &[
    "planTaskCount",
    "fileTaskCount",
    "deleteFileCount",
    "childPlanTaskCount",
    "planGraphEvents",
    "plannedPrincipalSubject",
    "plannedPrincipalKind",
    "plannedAuthorizationReceiptHash",
    "plannedAuthorizationReceiptAction",
    "fetchedPrincipalSubject",
    "fetchedPrincipalKind",
    "fetchedAuthorizationReceiptHash",
    "fetchedAuthorizationReceiptAction",
    "plannedReadRestriction",
    "fetchedReadRestriction",
    "plannedRequestedProjection",
    "plannedEffectiveProjection",
    "plannedRequestedStatsFields",
    "plannedEffectiveStatsFields",
    "fetchedRequestedStatsFields",
    "fetchedEffectiveStatsFields",
    "fetchedRequiredProjection",
    "fetchedEffectiveProjection",
    "fetchedRequiredFilters",
    "plannedReplayEventHashes",
    "fetchedReplayEventHashes",
    "plannedOpenLineageHashes",
    "fetchedOpenLineageHashes",
];

pub(crate) const STORAGE_PROFILE_UPSERT_PROOF_FIELDS: &[&str] = &[
    "profileId",
    "provider",
    "issuanceMode",
    "locationPrefixHash",
    "secretRefPresent",
    "secretRefProvider",
    "secretRefHash",
    "principalSubject",
    "principalKind",
    "authorizationReceiptHash",
    "authorizationReceiptAction",
    "graphEvents",
    "replayEventHashes",
    "openLineageHashes",
];

pub(crate) const CREDENTIAL_VENDING_PROOF_FIELDS: &[&str] = &["restricted", "trustedHuman"];

pub(crate) const CREDENTIAL_VENDING_BRANCH_FIELDS: &[&str] = &[
    "principalSubject",
    "principalKind",
    "credentialCount",
    "credentialPrefixHashes",
    "rawCredentialExceptionAllowed",
    "rawCredentialExceptionReason",
    "blockReason",
    "maxCredentialTtlSeconds",
    "storageProfile",
    "authorizationReceiptHash",
    "authorizationReceiptAction",
    "replayEventHashes",
    "openLineageHashes",
];

pub(crate) const CREDENTIAL_STORAGE_PROFILE_FIELDS: &[&str] = &[
    "profileId",
    "provider",
    "issuanceMode",
    "locationPrefixHash",
    "secretRefPresent",
    "secretRefProvider",
    "secretRefHash",
    "graphEvents",
];

pub(crate) const MANAGEMENT_REQUIRED_PROOF_FIELDS: &[&str] = &[
    "serverCount",
    "serverIds",
    "serverGraphEvents",
    "projectCount",
    "projectIds",
    "projectGraphEvents",
    "warehouseCount",
    "warehouseNames",
    "warehouseGraphEvents",
    "policyBindingCount",
    "policyIds",
    "policyGraphEvents",
    "storageProfileCount",
    "storageProfileIds",
    "storageProfileGraphEvents",
    "serverReplayEventHashes",
    "serverOpenLineageHashes",
    "projectReplayEventHashes",
    "projectOpenLineageHashes",
    "warehouseReplayEventHashes",
    "warehouseOpenLineageHashes",
    "policyReplayEventHashes",
    "policyOpenLineageHashes",
    "policyUpsertProof",
    "storageProfileReplayEventHashes",
    "storageProfileOpenLineageHashes",
];

pub(crate) const MANAGEMENT_PROOF_FIELDS: &[&str] = &[
    "serverCount",
    "serverIds",
    "serverGraphEvents",
    "projectCount",
    "projectIds",
    "projectGraphEvents",
    "warehouseCount",
    "warehouseNames",
    "warehouseProjectId",
    "warehouseGraphEvents",
    "policyBindingCount",
    "policyIds",
    "policyGraphEvents",
    "storageProfileCount",
    "storageProfileIds",
    "storageProfileGraphEvents",
    "serverReplayEventHashes",
    "serverOpenLineageHashes",
    "projectReplayEventHashes",
    "projectOpenLineageHashes",
    "warehouseReplayEventHashes",
    "warehouseOpenLineageHashes",
    "policyReplayEventHashes",
    "policyOpenLineageHashes",
    "policyUpsertProof",
    "storageProfileReplayEventHashes",
    "storageProfileOpenLineageHashes",
];

pub(crate) const CAPTURED_MANAGEMENT_PROOF_FIELDS: &[&str] = &[
    "serverCount",
    "serverIds",
    "serverGraphEvents",
    "projectCount",
    "projectIds",
    "projectGraphEvents",
    "warehouseCount",
    "warehouseNames",
    "warehouseProjectId",
    "warehouseGraphEvents",
    "policyBindingCount",
    "policyIds",
    "policyGraphEvents",
    "storageProfileCount",
    "storageProfileIds",
    "storageProfileGraphEvents",
    "serverReplayEventHashes",
    "serverOpenLineageHashes",
    "projectReplayEventHashes",
    "projectOpenLineageHashes",
    "warehouseReplayEventHashes",
    "warehouseOpenLineageHashes",
    "policyReplayEventHashes",
    "policyOpenLineageHashes",
    "policyUpsertProof",
    "storageProfileUpsert",
    "storageProfileReplayEventHashes",
    "storageProfileOpenLineageHashes",
];

pub(crate) const MANAGEMENT_POLICY_UPSERT_PROOF_FIELDS: &[&str] = &[
    "policyId",
    "odrlHash",
    "principalSubject",
    "principalKind",
    "authorizationReceiptHash",
    "authorizationReceiptAction",
    "graphEvents",
    "replayEventHashes",
    "openLineageHashes",
];

pub(crate) const VIEW_RECEIPT_CHAIN_PROOF_FIELDS: &[&str] =
    &["viewCount", "views", "tombstoneReceipts", "receiptChains"];

pub(crate) const VIEW_RECEIPT_CHAIN_VIEW_FIELDS: &[&str] = &[
    "stableId",
    "warehouse",
    "namespace",
    "name",
    "viewVersion",
    "acceptedViewVersion",
    "acceptedReceiptHash",
    "acceptedReceiptChainHash",
    "eventType",
    "expectedViewVersion",
    "graphEvents",
    "replayEventHashes",
    "openLineageHashes",
];

pub(crate) const VIEW_RECEIPT_CHAIN_TOMBSTONE_FIELDS: &[&str] = &[
    "stableId",
    "warehouse",
    "namespace",
    "name",
    "expectedViewVersion",
    "receiptHashes",
    "replayEventHashes",
    "openLineageHashes",
];

pub(crate) const VIEW_RECEIPT_CHAIN_GROUP_FIELDS: &[&str] = &[
    "warehouse",
    "namespace",
    "verifiedChainCount",
    "receiptHashes",
    "chainHashes",
    "chains",
    "replayEventHashes",
    "openLineageHashes",
];

pub(crate) const VIEW_RECEIPT_CHAIN_CHAIN_FIELDS: &[&str] = &[
    "stableId",
    "warehouse",
    "namespace",
    "name",
    "chainHash",
    "chainVerified",
    "latestViewVersion",
    "latestOperation",
    "tombstoned",
    "receiptCount",
    "receipts",
];

pub(crate) const VIEW_RECEIPT_CHAIN_RECEIPT_FIELDS: &[&str] = &[
    "stableId",
    "warehouse",
    "namespace",
    "name",
    "viewVersion",
    "previousViewVersion",
    "previousReceiptHash",
    "operation",
    "viewHash",
    "receiptHash",
    "principalSubject",
    "principalKind",
    "recordedAt",
];

pub(crate) fn require_catalog_config_evidence(
    config: &serde_json::Map<String, Value>,
    principal: &str,
) -> lakecat_core::LakeCatResult<()> {
    require_only_fields(config, CATALOG_CONFIG_PROOF_FIELDS, "catalogConfigProof")?;
    require_config_defaults(config)?;
    require_config_overrides(config)?;
    require_config_endpoints(config)?;
    require_string_match(config, "principalSubject", principal, "catalogConfigProof")?;
    require_string_eq(config, "principalKind", "agent", "catalogConfigProof")?;
    require_full_hash_str(config, "authorizationReceiptHash", "catalogConfigProof")?;
    require_string_eq(
        config,
        "authorizationReceiptAction",
        "catalog-config",
        "catalogConfigProof",
    )?;
    require_positive_u64(config, "graphEvents", "catalogConfigProof")?;
    require_full_hash_array(config, "replayEventHashes", "catalogConfigProof")?;
    require_full_hash_array(config, "openLineageHashes", "catalogConfigProof")?;
    Ok(())
}

pub(crate) const CATALOG_CONFIG_PROOF_FIELDS: &[&str] = &[
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
];

pub(crate) const CONFIG_ENTRY_FIELDS: &[&str] = &["key", "value"];

pub(crate) fn require_config_defaults(
    config: &serde_json::Map<String, Value>,
) -> lakecat_core::LakeCatResult<()> {
    let defaults = required_config_entries(config, "defaults", "catalogConfigProof")?;
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
    for entry in &defaults {
        if entry.key.starts_with("lakecat.format.v4")
            && !allowed_v4_keys.contains(entry.key.as_str())
        {
            return Err(lakecat_core::LakeCatError::InvalidArgument(
                "catalogConfigProof.defaults contain unsupported v4 bridge keys".to_string(),
            ));
        }
    }
    for (required_key, required_value) in required {
        if !defaults
            .iter()
            .any(|entry| entry.key == required_key && entry.value == required_value)
        {
            return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "catalogConfigProof.defaults must include {required_key}={required_value}"
            )));
        }
    }
    Ok(())
}

pub(crate) fn require_config_overrides(
    config: &serde_json::Map<String, Value>,
) -> lakecat_core::LakeCatResult<()> {
    for entry in required_config_entries(config, "overrides", "catalogConfigProof")? {
        if entry.key.starts_with("lakecat.format.v4") {
            return Err(lakecat_core::LakeCatError::InvalidArgument(
                "catalogConfigProof.overrides must not contain v4 bridge keys".to_string(),
            ));
        }
    }
    Ok(())
}

pub(crate) fn required_config_entries(
    config: &serde_json::Map<String, Value>,
    field: &str,
    label: &str,
) -> lakecat_core::LakeCatResult<Vec<ConfigEntry>> {
    let entries = required_array(config, field, label)?;
    let mut seen = BTreeSet::new();
    entries
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            let entry = entry.as_object().ok_or_else(|| {
                lakecat_core::LakeCatError::InvalidArgument(format!(
                    "{label}.{field}[{index}] must be an object"
                ))
            })?;
            require_only_fields(entry, CONFIG_ENTRY_FIELDS, &format!("{label}.{field}[]"))?;
            let key = require_non_blank_str(entry, "key", &format!("{label}.{field}[]"))?;
            let value = require_non_blank_str(entry, "value", &format!("{label}.{field}[]"))?;
            if !seen.insert(key.to_string()) {
                return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                    "{label}.{field} must not contain duplicate keys"
                )));
            }
            Ok(ConfigEntry::new(key, value))
        })
        .collect()
}

pub(crate) fn require_config_endpoints(
    config: &serde_json::Map<String, Value>,
) -> lakecat_core::LakeCatResult<()> {
    let endpoints = required_string_array(config, "endpoints", "catalogConfigProof")?;
    require_non_empty_unique_strings(&endpoints, "catalogConfigProof.endpoints")?;
    let required = [
        "GET /catalog/v1/config",
        "GET /catalog/v1/{warehouse}/config",
        "GET /catalog/v1/namespaces",
        "GET /catalog/v1/{warehouse}/namespaces",
        "POST /catalog/v1/namespaces",
        "POST /catalog/v1/{warehouse}/namespaces",
        "POST /catalog/v1/namespaces/{namespace}/tables",
        "POST /catalog/v1/{warehouse}/namespaces/{namespace}/tables",
        "GET /catalog/v1/namespaces/{namespace}/tables/{table}",
        "GET /catalog/v1/{warehouse}/namespaces/{namespace}/tables/{table}",
        "POST /catalog/v1/namespaces/{namespace}/tables/{table}/commit",
        "POST /catalog/v1/{warehouse}/namespaces/{namespace}/tables/{table}/commit",
        "POST /catalog/v1/namespaces/{namespace}/tables/{table}/plan",
        "POST /catalog/v1/{warehouse}/namespaces/{namespace}/tables/{table}/plan",
        "POST /catalog/v1/namespaces/{namespace}/tables/{table}/fetch-scan-tasks",
        "POST /catalog/v1/{warehouse}/namespaces/{namespace}/tables/{table}/fetch-scan-tasks",
        "GET /catalog/v1/namespaces/{namespace}/tables/{table}/credentials",
        "GET /catalog/v1/{warehouse}/namespaces/{namespace}/tables/{table}/credentials",
        "POST /management/v1/lineage/drain",
        "GET /querygraph/v1/bootstrap",
    ];
    for endpoint in required {
        if !endpoints.iter().any(|candidate| candidate == endpoint) {
            return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "catalogConfigProof.endpoints must include {endpoint}"
            )));
        }
    }
    Ok(())
}

pub(crate) fn require_credential_vending_evidence(
    credentials: &serde_json::Map<String, Value>,
    principal: &str,
    storage_profile_upsert: &serde_json::Map<String, Value>,
) -> lakecat_core::LakeCatResult<()> {
    require_only_fields(
        credentials,
        CREDENTIAL_VENDING_PROOF_FIELDS,
        "credentialVendingProof",
    )?;
    let restricted = required_object(credentials, "restricted", "credentialVendingProof")?;
    require_only_fields(
        restricted,
        CREDENTIAL_VENDING_BRANCH_FIELDS,
        "credentialVendingProof.restricted",
    )?;
    require_string_eq(
        restricted,
        "principalSubject",
        principal,
        "credentialVendingProof.restricted",
    )?;
    require_string_eq(
        restricted,
        "principalKind",
        "agent",
        "credentialVendingProof.restricted",
    )?;
    require_u64_match(
        restricted,
        "credentialCount",
        0,
        "credentialVendingProof.restricted",
    )?;
    require_credential_prefix_hash_evidence(restricted, "credentialVendingProof.restricted")?;
    require_full_hash_str(
        restricted,
        "authorizationReceiptHash",
        "credentialVendingProof.restricted",
    )?;
    require_string_eq(
        restricted,
        "authorizationReceiptAction",
        "credentials-vend",
        "credentialVendingProof.restricted",
    )?;
    if required_bool(
        restricted,
        "rawCredentialExceptionAllowed",
        "credentialVendingProof.restricted",
    )? != false
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "credentialVendingProof.restricted.rawCredentialExceptionAllowed must not allow a raw credential exception"
                .to_string(),
        ));
    }
    require_string_eq(
        restricted,
        "blockReason",
        QGLAKE_RESTRICTED_CREDENTIAL_BLOCK_REASON,
        "credentialVendingProof.restricted",
    )?;
    if let Some(reason) = restricted.get("rawCredentialExceptionReason") {
        if !reason.is_null() {
            return Err(lakecat_core::LakeCatError::InvalidArgument(
                "credentialVendingProof.restricted.rawCredentialExceptionReason must be null when raw credentials are blocked"
                    .to_string(),
            ));
        }
    }
    require_full_hash_array(
        restricted,
        "replayEventHashes",
        "credentialVendingProof.restricted",
    )?;
    require_full_hash_array(
        restricted,
        "openLineageHashes",
        "credentialVendingProof.restricted",
    )?;
    let restricted_ttl = require_positive_u64(
        restricted,
        "maxCredentialTtlSeconds",
        "credentialVendingProof.restricted",
    )?;
    require_credential_storage_profile_evidence(restricted, "credentialVendingProof.restricted")?;
    require_credential_storage_profile_matches_upsert(
        restricted,
        storage_profile_upsert,
        "credentialVendingProof.restricted",
    )?;

    let trusted = required_object(credentials, "trustedHuman", "credentialVendingProof")?;
    require_only_fields(
        trusted,
        CREDENTIAL_VENDING_BRANCH_FIELDS,
        "credentialVendingProof.trustedHuman",
    )?;
    require_non_empty_str(
        trusted,
        "principalSubject",
        "credentialVendingProof.trustedHuman",
    )?;
    require_string_eq(
        trusted,
        "principalKind",
        "human",
        "credentialVendingProof.trustedHuman",
    )?;
    require_positive_u64(
        trusted,
        "credentialCount",
        "credentialVendingProof.trustedHuman",
    )?;
    require_credential_prefix_hash_evidence(trusted, "credentialVendingProof.trustedHuman")?;
    require_full_hash_str(
        trusted,
        "authorizationReceiptHash",
        "credentialVendingProof.trustedHuman",
    )?;
    require_string_eq(
        trusted,
        "authorizationReceiptAction",
        "credentials-vend",
        "credentialVendingProof.trustedHuman",
    )?;
    if required_bool(
        trusted,
        "rawCredentialExceptionAllowed",
        "credentialVendingProof.trustedHuman",
    )? != true
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "handoff summary trusted-human proof must allow the audited raw credential exception"
                .to_string(),
        ));
    }
    require_string_eq(
        trusted,
        "rawCredentialExceptionReason",
        QGLAKE_HUMAN_RAW_CREDENTIAL_EXCEPTION_REASON,
        "credentialVendingProof.trustedHuman",
    )?;
    if !required_value(
        trusted,
        "blockReason",
        "credentialVendingProof.trustedHuman",
    )?
    .is_null()
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "credentialVendingProof.trustedHuman.blockReason must be null for the audited raw credential exception"
                .to_string(),
        ));
    }
    require_full_hash_array(
        trusted,
        "replayEventHashes",
        "credentialVendingProof.trustedHuman",
    )?;
    require_full_hash_array(
        trusted,
        "openLineageHashes",
        "credentialVendingProof.trustedHuman",
    )?;
    let trusted_ttl = require_positive_u64(
        trusted,
        "maxCredentialTtlSeconds",
        "credentialVendingProof.trustedHuman",
    )?;
    if trusted_ttl != restricted_ttl {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "credentialVendingProof.trustedHuman.maxCredentialTtlSeconds mismatch: expected={restricted_ttl} actual={trusted_ttl}"
        )));
    }
    require_credential_storage_profile_evidence(trusted, "credentialVendingProof.trustedHuman")?;
    require_credential_storage_profile_matches_upsert(
        trusted,
        storage_profile_upsert,
        "credentialVendingProof.trustedHuman",
    )?;

    Ok(())
}

pub(crate) fn require_credential_prefix_hash_evidence(
    credential: &serde_json::Map<String, Value>,
    label: &str,
) -> lakecat_core::LakeCatResult<()> {
    let credential_count = required_u64(credential, "credentialCount", label)?;
    let hashes = required_array(credential, "credentialPrefixHashes", label)?;
    if hashes.len() as u64 != credential_count {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "{label}.credentialPrefixHashes count mismatch: expected={credential_count} actual={}",
            hashes.len()
        )));
    }
    if credential_count == 0 {
        return Ok(());
    }
    require_full_hash_array(credential, "credentialPrefixHashes", label)
}

pub(crate) fn require_credential_storage_profile_matches_upsert(
    credential: &serde_json::Map<String, Value>,
    storage_profile_upsert: &serde_json::Map<String, Value>,
    label: &str,
) -> lakecat_core::LakeCatResult<()> {
    let storage_profile = required_object(credential, "storageProfile", label)?;
    let storage_label = format!("{label}.storageProfile");
    for field in [
        "profileId",
        "provider",
        "issuanceMode",
        "locationPrefixHash",
        "secretRefPresent",
    ] {
        require_value_match(
            storage_profile,
            field,
            required_value(storage_profile_upsert, field, "storageProfileUpsertProof")?,
            storage_label.as_str(),
        )?;
    }
    for field in ["secretRefProvider", "secretRefHash"] {
        require_optional_null_value_match(
            storage_profile,
            field,
            storage_profile_upsert.get(field),
            storage_label.as_str(),
        )?;
    }
    Ok(())
}

pub(crate) fn require_credential_storage_profile_evidence(
    credential: &serde_json::Map<String, Value>,
    label: &str,
) -> lakecat_core::LakeCatResult<()> {
    let storage_profile = required_object(credential, "storageProfile", label)?;
    let storage_label = format!("{label}.storageProfile");
    require_credential_storage_profile_schema(storage_profile, storage_label.as_str())?;
    require_non_empty_str(storage_profile, "profileId", storage_label.as_str())?;
    let provider = require_non_empty_str(storage_profile, "provider", storage_label.as_str())?;
    let issuance_mode =
        require_non_empty_str(storage_profile, "issuanceMode", storage_label.as_str())?;
    require_storage_profile_provider_issuance_compatibility(
        provider,
        issuance_mode,
        storage_label.as_str(),
    )?;
    require_full_hash_str(
        storage_profile,
        "locationPrefixHash",
        storage_label.as_str(),
    )?;
    if required_bool(storage_profile, "secretRefPresent", storage_label.as_str())? {
        require_non_blank_str(storage_profile, "secretRefProvider", storage_label.as_str())?;
        require_full_hash_str(storage_profile, "secretRefHash", storage_label.as_str())?;
    } else {
        require_absent_or_null_field(storage_profile, "secretRefProvider", storage_label.as_str())?;
        require_absent_or_null_field(storage_profile, "secretRefHash", storage_label.as_str())?;
    }
    require_positive_u64(storage_profile, "graphEvents", storage_label.as_str())?;
    Ok(())
}

pub(crate) fn require_credential_storage_profile_schema(
    storage_profile: &serde_json::Map<String, Value>,
    label: &str,
) -> lakecat_core::LakeCatResult<()> {
    require_only_fields(storage_profile, CREDENTIAL_STORAGE_PROFILE_FIELDS, label)
}

pub(crate) fn require_storage_profile_provider_issuance_compatibility(
    provider: &str,
    issuance_mode: &str,
    label: &str,
) -> lakecat_core::LakeCatResult<()> {
    match issuance_mode {
        "local-file-no-secret" if provider != "file" => {
            Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "{label} local-file-no-secret issuanceMode requires file provider"
            )))
        }
        "short-lived-secret-ref" if !matches!(provider, "s3" | "gcs" | "azure") => {
            Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "{label} short-lived-secret-ref issuanceMode requires s3, gcs, or azure provider"
            )))
        }
        "local-file-no-secret" | "short-lived-secret-ref" => Ok(()),
        _ => Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "{label}.issuanceMode must be local-file-no-secret or short-lived-secret-ref"
        ))),
    }
}

pub(crate) fn require_read_restriction_evidence(
    restriction: &serde_json::Map<String, Value>,
    label: &str,
) -> lakecat_core::LakeCatResult<()> {
    require_only_fields(restriction, READ_RESTRICTION_FIELDS, label)?;
    let allowed_columns = required_array(restriction, "allowed-columns", label)?;
    if allowed_columns.is_empty()
        || allowed_columns.iter().any(|column| {
            !column
                .as_str()
                .is_some_and(|column| !column.trim().is_empty())
        })
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "{label}.allowed-columns must contain column names"
        )));
    }
    let mut seen_columns = BTreeSet::new();
    if allowed_columns
        .iter()
        .filter_map(Value::as_str)
        .any(|column| !seen_columns.insert(column))
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "{label}.allowed-columns must be duplicate-free"
        )));
    }
    let row_predicate = required_object(restriction, "row-predicate", label)?;
    require_row_predicate_evidence(row_predicate, &format!("{label}.row-predicate"))?;
    require_non_empty_str(restriction, "purpose", label)?;
    require_full_hash_array(restriction, "policy-hashes", label)?;
    require_positive_u64(restriction, "max-credential-ttl-seconds", label)?;
    Ok(())
}

pub(crate) fn require_row_predicate_evidence(
    predicate: &serde_json::Map<String, Value>,
    label: &str,
) -> lakecat_core::LakeCatResult<()> {
    require_only_fields(predicate, ROW_PREDICATE_FIELDS, label)?;
    if predicate.is_empty() {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "{label} must contain predicate evidence"
        )));
    }
    let predicate_type = require_non_empty_str(predicate, "type", label)?;
    if predicate_type.trim().is_empty() {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "{label}.type must not be blank"
        )));
    }
    if predicate_type == "always-true" {
        return Ok(());
    }

    let term = require_non_empty_str(predicate, "term", label)?;
    if term.trim().is_empty() {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "{label}.term must not be blank"
        )));
    }
    if matches!(predicate_type, "eq" | "not-eq") && !predicate.contains_key("value") {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "{label}.value is required for {predicate_type} predicate evidence"
        )));
    }
    Ok(())
}

pub(crate) const READ_RESTRICTION_FIELDS: &[&str] = &[
    "allowed-columns",
    "row-predicate",
    "purpose",
    "policy-hashes",
    "max-credential-ttl-seconds",
];

pub(crate) const ROW_PREDICATE_FIELDS: &[&str] = &["type", "term", "value"];
