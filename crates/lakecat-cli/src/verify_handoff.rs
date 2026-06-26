use crate::*;

pub(crate) fn qglake_verify_replay(
    bundle_path: PathBuf,
    drain_path: PathBuf,
    principal: Option<String>,
    json_output: bool,
) -> lakecat_core::LakeCatResult<()> {
    let bundle =
        read_typed_json_file::<QueryGraphBootstrap>(&bundle_path, "QueryGraph bootstrap bundle")?;
    let drain =
        read_typed_json_file::<LineageDrainResponse>(&drain_path, "lineage drain response")?;
    let verification = verify_qglake_replay_artifacts(&bundle, &drain, principal.as_deref())?;
    let scan_replay = qglake_scan_replay_line(&drain);
    let management_replay = qglake_management_replay_line(&drain);
    let credential_replay = qglake_credential_replay_line(&drain, principal.as_deref());
    let table_commit_history_replay = qglake_table_commit_history_replay_line(&drain);
    let replay_evidence = qglake_replay_evidence_json(&drain, principal.as_deref(), &verification);
    if json_output {
        print_json(&qglake_replay_verification_json(
            &verification,
            scan_replay,
            management_replay,
            credential_replay,
            table_commit_history_replay,
            replay_evidence,
        ))?;
        return Ok(());
    }
    println!("verified qglake replay evidence");
    println!("bundle {}", verification.bundle_hash);
    println!("querygraph import {}", verification.querygraph_import_hash);
    println!("tables {}", verification.table_count);
    println!("views {}", verification.view_count);
    if let Some(line) = scan_replay {
        println!("{line}");
    }
    if let Some(line) = management_replay {
        println!("{line}");
    }
    if let Some(line) = credential_replay {
        println!("{line}");
    }
    if let Some(line) = table_commit_history_replay {
        println!("{line}");
    }
    Ok(())
}

pub(crate) fn qglake_verify_handoff(
    summary_path: PathBuf,
    json_output: bool,
) -> lakecat_core::LakeCatResult<()> {
    let summary = read_json_file(&summary_path)?;
    let mut verification = verify_qglake_handoff_summary_value(&summary)?;
    let artifact_files = verify_qglake_handoff_artifact_files(&summary_path, &summary)?;
    let captured_output_semantics =
        verify_qglake_handoff_captured_output_semantics(&summary_path, &summary)?;
    let bundle_artifact_semantics =
        verify_qglake_handoff_bundle_artifact_semantics(&summary_path, &summary)?;
    let querygraph_import_plan_semantics =
        verify_qglake_handoff_querygraph_import_plan_semantics(&summary_path, &summary)?;
    require_qglake_import_plan_graph_counts_match_bundle(
        &bundle_artifact_semantics,
        &querygraph_import_plan_semantics,
    )?;
    let lineage_drain_artifact_semantics =
        verify_qglake_handoff_lineage_drain_artifact_semantics(&summary_path, &summary)?;
    verification
        .as_object_mut()
        .ok_or_else(|| {
            lakecat_core::LakeCatError::Internal(
                "handoff verification must be an object".to_string(),
            )
        })?
        .insert("artifactFiles".to_string(), artifact_files);
    verification
        .as_object_mut()
        .ok_or_else(|| {
            lakecat_core::LakeCatError::Internal(
                "handoff verification must be an object".to_string(),
            )
        })?
        .insert(
            "capturedOutputSemantics".to_string(),
            captured_output_semantics,
        );
    verification
        .as_object_mut()
        .ok_or_else(|| {
            lakecat_core::LakeCatError::Internal(
                "handoff verification must be an object".to_string(),
            )
        })?
        .insert(
            "bundleArtifactSemantics".to_string(),
            bundle_artifact_semantics,
        );
    verification
        .as_object_mut()
        .ok_or_else(|| {
            lakecat_core::LakeCatError::Internal(
                "handoff verification must be an object".to_string(),
            )
        })?
        .insert(
            "querygraphImportPlanSemantics".to_string(),
            querygraph_import_plan_semantics,
        );
    verification
        .as_object_mut()
        .ok_or_else(|| {
            lakecat_core::LakeCatError::Internal(
                "handoff verification must be an object".to_string(),
            )
        })?
        .insert(
            "lineageDrainArtifactSemantics".to_string(),
            lineage_drain_artifact_semantics,
        );
    if json_output {
        print_json(&verification)?;
        return Ok(());
    }
    let verification = verification.as_object().ok_or_else(|| {
        lakecat_core::LakeCatError::Internal("handoff verification must be an object".to_string())
    })?;
    println!("verified qglake handoff summary");
    println!(
        "bundle {}",
        required_str(
            required_object(
                verification,
                "queryGraphBootstrapProof",
                "handoff verification"
            )?,
            "bundleHash",
            "handoff verification queryGraphBootstrapProof"
        )?
    );
    println!(
        "querygraph import {}",
        required_str(
            required_object(
                verification,
                "queryGraphBootstrapProof",
                "handoff verification"
            )?,
            "queryGraphImportHash",
            "handoff verification queryGraphBootstrapProof"
        )?
    );
    println!(
        "tables {}",
        required_u64(verification, "tableCount", "handoff verification")?
    );
    println!(
        "views {}",
        required_u64(verification, "viewCount", "handoff verification")?
    );
    Ok(())
}

pub(crate) fn verify_qglake_handoff_artifact_files(
    summary_path: &Path,
    summary: &Value,
) -> lakecat_core::LakeCatResult<Value> {
    let summary = summary.as_object().ok_or_else(|| {
        lakecat_core::LakeCatError::InvalidArgument(
            "handoff summary root must be an object".to_string(),
        )
    })?;
    let artifacts = required_object(summary, "artifacts", "handoff summary")?;
    require_only_fields(
        artifacts,
        &[
            "bundle",
            "lineageDrain",
            "querygraphImportPlan",
            "lakecatReplayOutput",
            "lakecatHandoffVerifyOutput",
            "lakecatHandoffVerifyOutputHash",
            "querygraphVerifyOutput",
            "querygraphImportOutput",
            "capturedOutputs",
            "serviceLog",
            "serviceLogHash",
        ],
        "handoff summary artifacts",
    )?;
    let base_dir = summary_path.parent().unwrap_or_else(|| Path::new(""));
    let bundle = verify_qglake_handoff_artifact_file(artifacts, "bundle", base_dir)?;
    let lineage_drain = verify_qglake_handoff_artifact_file(artifacts, "lineageDrain", base_dir)?;
    let querygraph_import_plan =
        verify_qglake_handoff_artifact_file(artifacts, "querygraphImportPlan", base_dir)?;
    let captured_outputs =
        verify_qglake_handoff_captured_outputs(artifacts, "capturedOutputs", base_dir)?;
    let lineage_drain_path = required_resolved_artifact_path(
        required_object(artifacts, "lineageDrain", "handoff summary artifacts")?,
        "path",
        base_dir,
    )?;
    let lineage_drain_semantics = qglake_handoff_lineage_drain_summary_fields(&lineage_drain_path)?;
    let lineage_drain_semantics = lineage_drain_semantics.as_object().ok_or_else(|| {
        lakecat_core::LakeCatError::Internal(
            "lineage drain artifact semantics must be an object".to_string(),
        )
    })?;
    let path_aliases = verify_qglake_handoff_artifact_path_aliases(
        artifacts,
        summary,
        base_dir,
        lineage_drain_semantics,
    )?;
    Ok(json!({
        "bundle": bundle,
        "lineageDrain": lineage_drain,
        "querygraphImportPlan": querygraph_import_plan,
        "capturedOutputs": captured_outputs,
        "pathAliases": path_aliases,
        "serviceLogHash": required_value(artifacts, "serviceLogHash", "handoff summary artifacts")?,
    }))
}

pub(crate) fn qglake_handoff_lineage_drain_summary_fields(
    path: &Path,
) -> lakecat_core::LakeCatResult<Value> {
    let bytes = fs::read(path).map_err(|err| {
        lakecat_core::LakeCatError::InvalidArgument(format!(
            "failed to read handoff lineage drain artifact at {}: {err}",
            path.display()
        ))
    })?;
    let drain: LineageDrainResponse = serde_json::from_slice(&bytes).map_err(|err| {
        lakecat_core::LakeCatError::InvalidArgument(format!(
            "handoff lineage drain artifact at {} is not JSON: {err}",
            path.display()
        ))
    })?;
    let catalog_config = qglake_catalog_config_replay_evidence_json(&drain).ok_or_else(|| {
        lakecat_core::LakeCatError::InvalidArgument(
            "handoff lineage drain artifact is missing catalog.config-read proof".to_string(),
        )
    })?;
    Ok(json!({
        "delivered": drain.delivered,
        "eventTypes": drain.event_types,
        "graphEvents": drain.graph_events,
        "lineageEvents": drain.lineage_events,
        "catalogConfigProof": catalog_config,
    }))
}

pub(crate) fn verify_qglake_handoff_captured_outputs(
    artifacts: &serde_json::Map<String, Value>,
    field: &str,
    base_dir: &Path,
) -> lakecat_core::LakeCatResult<Value> {
    let outputs = required_object(artifacts, field, "handoff summary artifacts")?;
    require_only_fields(
        outputs,
        &["lakecatReplay", "querygraphVerify", "querygraphImport"],
        "handoff summary artifacts.capturedOutputs",
    )?;
    Ok(json!({
        "lakecatReplay": verify_qglake_handoff_artifact_file(outputs, "lakecatReplay", base_dir)?,
        "querygraphVerify": verify_qglake_handoff_artifact_file(outputs, "querygraphVerify", base_dir)?,
        "querygraphImport": verify_qglake_handoff_artifact_file(outputs, "querygraphImport", base_dir)?,
    }))
}

pub(crate) fn verify_qglake_handoff_artifact_path_aliases(
    artifacts: &serde_json::Map<String, Value>,
    summary: &serde_json::Map<String, Value>,
    base_dir: &Path,
    lineage_drain_semantics: &serde_json::Map<String, Value>,
) -> lakecat_core::LakeCatResult<Value> {
    let outputs = required_object(artifacts, "capturedOutputs", "handoff summary artifacts")?;
    let lakecat_replay = verify_qglake_handoff_path_alias(
        artifacts,
        outputs,
        "lakecatReplayOutput",
        "lakecatReplay",
        base_dir,
    )?;
    let querygraph_verify = verify_qglake_handoff_path_alias(
        artifacts,
        outputs,
        "querygraphVerifyOutput",
        "querygraphVerify",
        base_dir,
    )?;
    let querygraph_import = verify_qglake_handoff_path_alias(
        artifacts,
        outputs,
        "querygraphImportOutput",
        "querygraphImport",
        base_dir,
    )?;
    let handoff_verify_output =
        required_resolved_artifact_path(artifacts, "lakecatHandoffVerifyOutput", base_dir)?;
    let handoff_verify_output_hash = verify_qglake_handoff_verify_output_artifact(
        artifacts,
        summary,
        &handoff_verify_output,
        lineage_drain_semantics,
    )?;
    let service_log = verify_qglake_handoff_service_log(artifacts, base_dir)?;
    Ok(json!({
        "lakecatReplayOutput": lakecat_replay.display().to_string(),
        "querygraphVerifyOutput": querygraph_verify.display().to_string(),
        "querygraphImportOutput": querygraph_import.display().to_string(),
        "lakecatHandoffVerifyOutput": handoff_verify_output.display().to_string(),
        "lakecatHandoffVerifyOutputHash": handoff_verify_output_hash,
        "serviceLog": service_log.display().to_string(),
    }))
}

pub(crate) fn verify_qglake_handoff_verify_output_artifact(
    artifacts: &serde_json::Map<String, Value>,
    summary: &serde_json::Map<String, Value>,
    output_path: &Path,
    lineage_drain_semantics: &serde_json::Map<String, Value>,
) -> lakecat_core::LakeCatResult<Value> {
    let Some(expected_sha256) = artifacts.get("lakecatHandoffVerifyOutputHash") else {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "handoff summary artifacts.lakecatHandoffVerifyOutputHash is required".to_string(),
        ));
    };
    let Some(expected_sha256) = expected_sha256
        .as_str()
        .filter(|value| is_full_sha256_hash(value))
    else {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "handoff summary artifacts.lakecatHandoffVerifyOutputHash must be a full SHA-256 hash"
                .to_string(),
        ));
    };
    let bytes = fs::read(output_path).map_err(|err| {
        lakecat_core::LakeCatError::InvalidArgument(format!(
            "failed to read handoff artifact lakecatHandoffVerifyOutput at {}: {err}",
            output_path.display()
        ))
    })?;
    let actual_sha256 = content_hash_bytes(&bytes);
    if actual_sha256 != expected_sha256 {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "handoff artifact lakecatHandoffVerifyOutput hash mismatch: expected={expected_sha256} actual={actual_sha256}"
        )));
    }
    let output: Value = serde_json::from_slice(&bytes).map_err(|err| {
        lakecat_core::LakeCatError::InvalidArgument(format!(
            "handoff artifact lakecatHandoffVerifyOutput is not JSON: {err}"
        ))
    })?;
    let output = output.as_object().ok_or_else(|| {
        lakecat_core::LakeCatError::InvalidArgument(
            "handoff artifact lakecatHandoffVerifyOutput must be a JSON object".to_string(),
        )
    })?;
    require_only_fields(
        output,
        &[
            "schemaVersion",
            "status",
            "principal",
            "catalogUrl",
            "warehouse",
            "namespace",
            "table",
            "tableCount",
            "viewCount",
            "verifiedTables",
            "verifiedViews",
            "standards",
            "graphProjectionProof",
            "requestIdentityProof",
            "queryGraphBootstrapProof",
            "artifactFiles",
            "capturedOutputSemantics",
            "bundleArtifactSemantics",
            "querygraphImportPlanSemantics",
            "lineageDrainArtifactSemantics",
        ],
        "lakecatHandoffVerifyOutput",
    )?;
    require_string_eq(
        output,
        "schemaVersion",
        "lakecat.qglake.handoff-verification.v1",
        "lakecatHandoffVerifyOutput",
    )?;
    require_string_eq(output, "status", "verified", "lakecatHandoffVerifyOutput")?;
    for field in ["principal", "catalogUrl", "warehouse", "namespace", "table"] {
        require_value_match(
            output,
            field,
            required_value(summary, field, "handoff summary")?,
            "lakecatHandoffVerifyOutput",
        )?;
    }
    require_qglake_handoff_verify_output_matches_summary(output, summary, lineage_drain_semantics)?;
    Ok(Value::String(actual_sha256))
}

pub(crate) fn require_qglake_handoff_verify_output_matches_summary(
    output: &serde_json::Map<String, Value>,
    summary: &serde_json::Map<String, Value>,
    lineage_drain_semantics: &serde_json::Map<String, Value>,
) -> lakecat_core::LakeCatResult<()> {
    let querygraph = required_object(summary, "querygraphVerification", "handoff summary")?;
    for field in [
        "tableCount",
        "viewCount",
        "verifiedTables",
        "verifiedViews",
        "standards",
    ] {
        require_value_match(
            output,
            field,
            required_value(querygraph, field, "querygraphVerification")?,
            "lakecatHandoffVerifyOutput",
        )?;
    }
    require_value_match(
        output,
        "graphProjectionProof",
        required_value(summary, "graphProjectionProof", "handoff summary")?,
        "lakecatHandoffVerifyOutput",
    )?;
    let lakecat = required_object(summary, "lakecatReplayVerification", "handoff summary")?;
    for field in ["requestIdentityProof", "queryGraphBootstrapProof"] {
        require_value_match(
            output,
            field,
            required_value(lakecat, field, "lakecatReplayVerification")?,
            "lakecatHandoffVerifyOutput",
        )?;
    }
    require_qglake_handoff_verify_output_artifact_hashes_match_summary(output, summary)?;
    require_qglake_handoff_verify_output_semantic_sections_match_summary(
        output,
        summary,
        lineage_drain_semantics,
    )?;
    Ok(())
}

pub(crate) fn require_qglake_handoff_verify_output_semantic_sections_match_summary(
    output: &serde_json::Map<String, Value>,
    summary: &serde_json::Map<String, Value>,
    expected_lineage_drain: &serde_json::Map<String, Value>,
) -> lakecat_core::LakeCatResult<()> {
    let querygraph = required_object(summary, "querygraphVerification", "handoff summary")?;
    let import = required_object(summary, "querygraphImportVerification", "handoff summary")?;
    let lakecat = required_object(summary, "lakecatReplayVerification", "handoff summary")?;

    let captured = required_object(
        output,
        "capturedOutputSemantics",
        "lakecatHandoffVerifyOutput",
    )?;
    require_only_fields(
        captured,
        &["lakecatReplay", "querygraphVerify", "querygraphImport"],
        "lakecatHandoffVerifyOutput.capturedOutputSemantics",
    )?;
    let captured_lakecat = required_object(
        captured,
        "lakecatReplay",
        "lakecatHandoffVerifyOutput.capturedOutputSemantics",
    )?;
    require_only_fields(
        captured_lakecat,
        &[
            "requestIdentityProof",
            "queryGraphBootstrapProof",
            "governedScanProof",
            "catalogConfigProof",
            "tableCommitHistoryProof",
            "viewReceiptChainProof",
            "managementProof",
            "storageProfileUpsertProof",
            "credentialVendingProof",
        ],
        "lakecatHandoffVerifyOutput.capturedOutputSemantics.lakecatReplay",
    )?;
    for field in [
        "requestIdentityProof",
        "queryGraphBootstrapProof",
        "governedScanProof",
        "catalogConfigProof",
        "tableCommitHistoryProof",
        "viewReceiptChainProof",
        "managementProof",
        "storageProfileUpsertProof",
        "credentialVendingProof",
    ] {
        require_value_match(
            captured_lakecat,
            field,
            required_value(lakecat, field, "lakecatReplayVerification")?,
            "lakecatHandoffVerifyOutput.capturedOutputSemantics.lakecatReplay",
        )?;
    }
    let captured_querygraph_verify = required_object(
        captured,
        "querygraphVerify",
        "lakecatHandoffVerifyOutput.capturedOutputSemantics",
    )?;
    require_qglake_handoff_verify_output_querygraph_semantics_fields(
        captured_querygraph_verify,
        "lakecatHandoffVerifyOutput.capturedOutputSemantics.querygraphVerify",
        &[],
    )?;
    require_qglake_handoff_verify_output_querygraph_semantics_match_summary(
        captured_querygraph_verify,
        querygraph,
        "lakecatHandoffVerifyOutput.capturedOutputSemantics.querygraphVerify",
    )?;
    let captured_querygraph_import = required_object(
        captured,
        "querygraphImport",
        "lakecatHandoffVerifyOutput.capturedOutputSemantics",
    )?;
    require_qglake_handoff_verify_output_querygraph_semantics_fields(
        captured_querygraph_import,
        "lakecatHandoffVerifyOutput.capturedOutputSemantics.querygraphImport",
        &[],
    )?;
    require_qglake_handoff_verify_output_querygraph_semantics_match_summary(
        captured_querygraph_import,
        import,
        "lakecatHandoffVerifyOutput.capturedOutputSemantics.querygraphImport",
    )?;

    let bundle = required_object(
        output,
        "bundleArtifactSemantics",
        "lakecatHandoffVerifyOutput",
    )?;
    require_qglake_handoff_verify_output_querygraph_semantics_fields(
        bundle,
        "lakecatHandoffVerifyOutput.bundleArtifactSemantics",
        &["graphNodes", "graphEdges"],
    )?;
    require_qglake_handoff_verify_output_querygraph_semantics_match_summary(
        bundle,
        querygraph,
        "lakecatHandoffVerifyOutput.bundleArtifactSemantics",
    )?;
    let import_plan = required_object(
        output,
        "querygraphImportPlanSemantics",
        "lakecatHandoffVerifyOutput",
    )?;
    require_qglake_handoff_verify_output_querygraph_semantics_fields(
        import_plan,
        "lakecatHandoffVerifyOutput.querygraphImportPlanSemantics",
        &["graphNodes", "graphEdges"],
    )?;
    require_qglake_handoff_verify_output_querygraph_semantics_match_summary(
        import_plan,
        import,
        "lakecatHandoffVerifyOutput.querygraphImportPlanSemantics",
    )?;
    for field in ["graphNodes", "graphEdges"] {
        require_value_match(
            import_plan,
            field,
            required_value(
                bundle,
                field,
                "lakecatHandoffVerifyOutput.bundleArtifactSemantics",
            )?,
            "lakecatHandoffVerifyOutput.querygraphImportPlanSemantics",
        )?;
    }

    let lineage_drain = required_object(
        output,
        "lineageDrainArtifactSemantics",
        "lakecatHandoffVerifyOutput",
    )?;
    require_qglake_handoff_verify_output_lineage_drain_semantics_fields(lineage_drain)?;
    require_qglake_handoff_verify_output_querygraph_semantics_match_summary(
        lineage_drain,
        querygraph,
        "lakecatHandoffVerifyOutput.lineageDrainArtifactSemantics",
    )?;
    require_qglake_handoff_verify_output_lineage_drain_identity_match_summary(
        lineage_drain,
        required_object(lakecat, "requestIdentityProof", "lakecatReplayVerification")?,
    )?;
    require_qglake_handoff_verify_output_lineage_drain_semantics_match_artifact(
        lineage_drain,
        expected_lineage_drain,
    )?;
    Ok(())
}

pub(crate) fn require_qglake_handoff_verify_output_lineage_drain_semantics_match_artifact(
    semantics: &serde_json::Map<String, Value>,
    expected: &serde_json::Map<String, Value>,
) -> lakecat_core::LakeCatResult<()> {
    for field in [
        "delivered",
        "eventTypes",
        "graphEvents",
        "lineageEvents",
        "catalogConfigProof",
    ] {
        require_value_match(
            semantics,
            field,
            required_value(expected, field, "lineageDrainArtifactSemantics")?,
            "lakecatHandoffVerifyOutput.lineageDrainArtifactSemantics",
        )?;
    }
    Ok(())
}

pub(crate) fn require_qglake_handoff_verify_output_lineage_drain_semantics_fields(
    semantics: &serde_json::Map<String, Value>,
) -> lakecat_core::LakeCatResult<()> {
    require_only_fields(
        semantics,
        &[
            "tableCount",
            "viewCount",
            "verifiedTables",
            "verifiedViews",
            "bundleHash",
            "graphHash",
            "openLineageHash",
            "queryGraphImportHash",
            "standards",
            "delivered",
            "eventTypes",
            "graphEvents",
            "lineageEvents",
            "catalogConfigProof",
            "principalSubject",
            "principalKind",
            "authorizationReceiptHash",
            "authorizationReceiptAction",
            "requestIdentitySource",
            "requestIdentityState",
            "typedidEnvelopeHash",
            "typedidProofHash",
        ],
        "lakecatHandoffVerifyOutput.lineageDrainArtifactSemantics",
    )
}

pub(crate) fn require_qglake_handoff_verify_output_lineage_drain_identity_match_summary(
    semantics: &serde_json::Map<String, Value>,
    request_identity: &serde_json::Map<String, Value>,
) -> lakecat_core::LakeCatResult<()> {
    for field in [
        "principalSubject",
        "principalKind",
        "authorizationReceiptHash",
        "authorizationReceiptAction",
        "requestIdentitySource",
        "requestIdentityState",
        "typedidEnvelopeHash",
        "typedidProofHash",
    ] {
        require_value_match(
            semantics,
            field,
            required_value(
                request_identity,
                field,
                "lakecatReplayVerification.requestIdentityProof",
            )?,
            "lakecatHandoffVerifyOutput.lineageDrainArtifactSemantics",
        )?;
    }
    Ok(())
}

pub(crate) fn require_qglake_handoff_verify_output_querygraph_semantics_fields(
    semantics: &serde_json::Map<String, Value>,
    label: &str,
    extra_fields: &[&str],
) -> lakecat_core::LakeCatResult<()> {
    let mut allowed = vec![
        "tableCount",
        "viewCount",
        "verifiedTables",
        "verifiedViews",
        "bundleHash",
        "graphHash",
        "openLineageHash",
        "queryGraphImportHash",
        "standards",
    ];
    allowed.extend_from_slice(extra_fields);
    require_only_fields(semantics, &allowed, label)
}

pub(crate) fn require_qglake_handoff_verify_output_querygraph_semantics_match_summary(
    semantics: &serde_json::Map<String, Value>,
    expected: &serde_json::Map<String, Value>,
    label: &str,
) -> lakecat_core::LakeCatResult<()> {
    for field in [
        "tableCount",
        "viewCount",
        "verifiedTables",
        "verifiedViews",
        "bundleHash",
        "graphHash",
        "openLineageHash",
        "standards",
    ] {
        require_value_match(
            semantics,
            field,
            required_value(expected, field, label)?,
            label,
        )?;
    }
    require_value_match(
        semantics,
        "queryGraphImportHash",
        required_value(expected, "querygraphImportHash", label)?,
        label,
    )?;
    Ok(())
}

pub(crate) fn require_qglake_handoff_verify_output_artifact_hashes_match_summary(
    output: &serde_json::Map<String, Value>,
    summary: &serde_json::Map<String, Value>,
) -> lakecat_core::LakeCatResult<()> {
    let output_artifacts = required_object(output, "artifactFiles", "lakecatHandoffVerifyOutput")?;
    require_only_fields(
        output_artifacts,
        &[
            "bundle",
            "lineageDrain",
            "querygraphImportPlan",
            "capturedOutputs",
            "serviceLogHash",
        ],
        "lakecatHandoffVerifyOutput.artifactFiles",
    )?;
    let summary_artifacts = required_object(summary, "artifacts", "handoff summary")?;
    for field in ["bundle", "lineageDrain", "querygraphImportPlan"] {
        let output_artifact_label = format!("lakecatHandoffVerifyOutput.artifactFiles.{field}");
        let summary_artifact_label = format!("handoff summary artifacts.{field}");
        let output_artifact = required_object(
            output_artifacts,
            field,
            "lakecatHandoffVerifyOutput.artifactFiles",
        )?;
        require_only_fields(output_artifact, &["sha256"], &output_artifact_label)?;
        let summary_artifact =
            required_object(summary_artifacts, field, "handoff summary artifacts")?;
        require_full_hash_str(output_artifact, "sha256", &output_artifact_label)?;
        require_value_match(
            output_artifact,
            "sha256",
            required_value(summary_artifact, "sha256", &summary_artifact_label)?,
            &output_artifact_label,
        )?;
    }
    let output_captures = required_object(
        output_artifacts,
        "capturedOutputs",
        "lakecatHandoffVerifyOutput.artifactFiles",
    )?;
    require_only_fields(
        output_captures,
        &["lakecatReplay", "querygraphVerify", "querygraphImport"],
        "lakecatHandoffVerifyOutput.artifactFiles.capturedOutputs",
    )?;
    let summary_captures = required_object(
        summary_artifacts,
        "capturedOutputs",
        "handoff summary artifacts",
    )?;
    for field in ["lakecatReplay", "querygraphVerify", "querygraphImport"] {
        let output_capture_label =
            format!("lakecatHandoffVerifyOutput.artifactFiles.capturedOutputs.{field}");
        let summary_capture_label = format!("handoff summary artifacts.capturedOutputs.{field}");
        let output_capture = required_object(
            output_captures,
            field,
            "lakecatHandoffVerifyOutput.artifactFiles.capturedOutputs",
        )?;
        require_only_fields(output_capture, &["sha256"], &output_capture_label)?;
        let summary_capture = required_object(
            summary_captures,
            field,
            "handoff summary artifacts.capturedOutputs",
        )?;
        require_full_hash_str(output_capture, "sha256", &output_capture_label)?;
        require_value_match(
            output_capture,
            "sha256",
            required_value(summary_capture, "sha256", &summary_capture_label)?,
            &output_capture_label,
        )?;
    }
    require_full_hash_str(
        output_artifacts,
        "serviceLogHash",
        "lakecatHandoffVerifyOutput.artifactFiles",
    )?;
    require_value_match(
        output_artifacts,
        "serviceLogHash",
        required_value(
            summary_artifacts,
            "serviceLogHash",
            "handoff summary artifacts",
        )?,
        "lakecatHandoffVerifyOutput.artifactFiles",
    )?;
    Ok(())
}

pub(crate) fn verify_qglake_handoff_service_log(
    artifacts: &serde_json::Map<String, Value>,
    base_dir: &Path,
) -> lakecat_core::LakeCatResult<PathBuf> {
    let service_log = required_resolved_artifact_path(artifacts, "serviceLog", base_dir)?;
    let expected_sha256 =
        require_full_hash_str(artifacts, "serviceLogHash", "handoff summary artifacts")?;
    let bytes = fs::read(&service_log).map_err(|err| {
        lakecat_core::LakeCatError::InvalidArgument(format!(
            "failed to read handoff artifact serviceLog at {}: {err}",
            service_log.display()
        ))
    })?;
    let actual_sha256 = content_hash_bytes(&bytes);
    if actual_sha256 != expected_sha256 {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "handoff artifact serviceLog hash mismatch: expected={expected_sha256} actual={actual_sha256}"
        )));
    }
    Ok(service_log)
}

pub(crate) fn verify_qglake_handoff_path_alias(
    artifacts: &serde_json::Map<String, Value>,
    outputs: &serde_json::Map<String, Value>,
    alias_field: &str,
    captured_field: &str,
    base_dir: &Path,
) -> lakecat_core::LakeCatResult<PathBuf> {
    let alias_path = required_resolved_artifact_path(artifacts, alias_field, base_dir)?;
    let captured = required_object(outputs, captured_field, "handoff captured outputs")?;
    let captured_path = required_resolved_artifact_path(captured, "path", base_dir)?;
    if alias_path != captured_path {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "handoff artifact path alias {alias_field} does not match capturedOutputs.{captured_field}.path: alias={} captured={}",
            alias_path.display(),
            captured_path.display()
        )));
    }
    Ok(alias_path)
}

pub(crate) fn required_resolved_artifact_path(
    object: &serde_json::Map<String, Value>,
    field: &str,
    base_dir: &Path,
) -> lakecat_core::LakeCatResult<PathBuf> {
    let path = required_str(object, field, "handoff summary artifacts")?;
    if path.trim().is_empty() {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "handoff artifact path {field} must be non-empty"
        )));
    }
    let path = PathBuf::from(path);
    let resolved = if path.is_absolute() {
        path
    } else {
        base_dir.join(path)
    };
    let canonical_base = fs::canonicalize(base_dir).map_err(|err| {
        lakecat_core::LakeCatError::InvalidArgument(format!(
            "handoff summary artifact base directory {} is not readable: {err}",
            base_dir.display()
        ))
    })?;
    let canonical_path = fs::canonicalize(&resolved).map_err(|err| {
        lakecat_core::LakeCatError::InvalidArgument(format!(
            "handoff artifact path {field} at {} is not readable: {err}",
            resolved.display()
        ))
    })?;
    if !canonical_path.starts_with(&canonical_base) {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "handoff artifact path {field} must stay under handoff summary directory {}: {}",
            canonical_base.display(),
            canonical_path.display()
        )));
    }
    Ok(canonical_path)
}

pub(crate) fn verify_qglake_handoff_artifact_file(
    artifacts: &serde_json::Map<String, Value>,
    field: &str,
    base_dir: &Path,
) -> lakecat_core::LakeCatResult<Value> {
    let artifact = required_object(artifacts, field, "handoff summary artifacts")?;
    let artifact_label = format!("handoff summary artifacts.{field}");
    require_only_fields(artifact, &["path", "sha256"], &artifact_label)?;
    let expected_sha256 = require_full_hash_str(artifact, "sha256", &artifact_label)?;
    let resolved_path = required_resolved_artifact_path(artifact, "path", base_dir)?;
    let bytes = fs::read(&resolved_path).map_err(|err| {
        lakecat_core::LakeCatError::InvalidArgument(format!(
            "failed to read handoff artifact {} at {}: {err}",
            field,
            resolved_path.display()
        ))
    })?;
    let actual_sha256 = content_hash_bytes(&bytes);
    if actual_sha256 != expected_sha256 {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "handoff artifact {field} hash mismatch: expected={expected_sha256} actual={actual_sha256}"
        )));
    }
    Ok(json!({
        "path": resolved_path.display().to_string(),
        "sha256": actual_sha256,
    }))
}

pub(crate) fn verify_qglake_handoff_captured_output_semantics(
    summary_path: &Path,
    summary: &Value,
) -> lakecat_core::LakeCatResult<Value> {
    let summary = summary.as_object().ok_or_else(|| {
        lakecat_core::LakeCatError::InvalidArgument(
            "handoff summary root must be an object".to_string(),
        )
    })?;
    let warehouse = required_str(summary, "warehouse", "handoff summary")?;
    let querygraph = required_object(summary, "querygraphVerification", "handoff summary")?;
    let import = required_object(summary, "querygraphImportVerification", "handoff summary")?;
    let lakecat = required_object(summary, "lakecatReplayVerification", "handoff summary")?;
    let artifacts = required_object(summary, "artifacts", "handoff summary")?;
    let outputs = required_object(artifacts, "capturedOutputs", "handoff summary artifacts")?;
    let base_dir = summary_path.parent().unwrap_or_else(|| Path::new(""));

    let lakecat_replay = read_qglake_handoff_artifact_json(outputs, "lakecatReplay", base_dir)?;
    let querygraph_verify =
        read_qglake_handoff_artifact_json(outputs, "querygraphVerify", base_dir)?;
    let querygraph_import =
        read_qglake_handoff_artifact_json(outputs, "querygraphImport", base_dir)?;

    let lakecat_replay = lakecat_replay.as_object().ok_or_else(|| {
        lakecat_core::LakeCatError::InvalidArgument(
            "captured LakeCat replay output must be a JSON object".to_string(),
        )
    })?;
    let querygraph_verify = querygraph_verify.as_object().ok_or_else(|| {
        lakecat_core::LakeCatError::InvalidArgument(
            "captured QueryGraph verify output must be a JSON object".to_string(),
        )
    })?;
    let querygraph_import = querygraph_import.as_object().ok_or_else(|| {
        lakecat_core::LakeCatError::InvalidArgument(
            "captured QueryGraph import output must be a JSON object".to_string(),
        )
    })?;

    verify_lakecat_replay_capture_matches_summary(lakecat_replay, lakecat, querygraph)?;
    let table_scope = HandoffTableScope::from_summary(summary, warehouse)?;
    let view_scope = HandoffViewScope::from_lakecat(lakecat)?;
    verify_querygraph_capture_matches_summary(
        querygraph_verify,
        querygraph,
        &table_scope,
        &view_scope,
        "captured QueryGraph verify output",
    )?;
    require_querygraph_import_matches_verify(import, querygraph)?;
    verify_querygraph_capture_matches_summary(
        querygraph_import,
        import,
        &table_scope,
        &view_scope,
        "captured QueryGraph import output",
    )?;
    let request_identity = Value::Object(lakecat_replay_request_identity(lakecat_replay)?.clone());
    let querygraph_bootstrap =
        Value::Object(lakecat_replay_querygraph_bootstrap(lakecat_replay)?.clone());
    let governed_scan = Value::Object(lakecat_replay_scan(lakecat_replay)?.clone());
    let catalog_config = Value::Object(lakecat_replay_catalog_config(lakecat_replay)?.clone());
    let table_commit_history =
        Value::Object(lakecat_replay_table_commit_history(lakecat_replay)?.clone());
    let view_receipt_chain = Value::Object(lakecat_replay_views(lakecat_replay)?.clone());
    let management = lakecat_replay_management_proof_value(lakecat_replay)?;
    let storage_profile_upsert =
        Value::Object(lakecat_replay_storage_profile_upsert(lakecat_replay)?.clone());
    let credential_vending = Value::Object(lakecat_replay_credentials(lakecat_replay)?.clone());

    Ok(json!({
        "lakecatReplay": {
            "schemaVersion": required_str(lakecat_replay, "schema-version", "captured LakeCat replay output")?,
            "status": required_str(lakecat_replay, "status", "captured LakeCat replay output")?,
            "tableCount": required_u64(lakecat_replay, "table-count", "captured LakeCat replay output")?,
            "viewCount": required_u64(lakecat_replay, "view-count", "captured LakeCat replay output")?,
            "bundleHash": required_str(lakecat_replay, "bundle-hash", "captured LakeCat replay output")?,
            "graphHash": required_str(lakecat_replay, "graph-hash", "captured LakeCat replay output")?,
            "openLineageHash": required_str(lakecat_replay, "open-lineage-hash", "captured LakeCat replay output")?,
            "queryGraphImportHash": required_str(lakecat_replay, "querygraph-import-hash", "captured LakeCat replay output")?,
            "standards": required_value(lakecat_replay, "standards", "captured LakeCat replay output")?,
            "requestIdentityProof": request_identity,
            "queryGraphBootstrapProof": querygraph_bootstrap,
            "governedScanProof": governed_scan,
            "catalogConfigProof": catalog_config,
            "tableCommitHistoryProof": table_commit_history,
            "viewReceiptChainProof": view_receipt_chain,
            "managementProof": management,
            "storageProfileUpsertProof": storage_profile_upsert,
            "credentialVendingProof": credential_vending,
        },
        "querygraphVerify": querygraph_capture_semantics_json(querygraph_verify, "captured QueryGraph verify output")?,
        "querygraphImport": querygraph_capture_semantics_json(querygraph_import, "captured QueryGraph import output")?,
    }))
}

pub(crate) fn verify_qglake_handoff_bundle_artifact_semantics(
    summary_path: &Path,
    summary: &Value,
) -> lakecat_core::LakeCatResult<Value> {
    let summary = summary.as_object().ok_or_else(|| {
        lakecat_core::LakeCatError::InvalidArgument(
            "handoff summary root must be an object".to_string(),
        )
    })?;
    let warehouse = required_str(summary, "warehouse", "handoff summary")?;
    let querygraph = required_object(summary, "querygraphVerification", "handoff summary")?;
    let lakecat = required_object(summary, "lakecatReplayVerification", "handoff summary")?;
    let bootstrap = required_object(
        lakecat,
        "queryGraphBootstrapProof",
        "lakecatReplayVerification",
    )?;
    let artifacts = required_object(summary, "artifacts", "handoff summary")?;
    let base_dir = summary_path.parent().unwrap_or_else(|| Path::new(""));
    let bundle_value = read_qglake_handoff_artifact_json(artifacts, "bundle", base_dir)?;
    let bundle: QueryGraphBootstrap = serde_json::from_value(bundle_value).map_err(|err| {
        lakecat_core::LakeCatError::InvalidArgument(format!(
            "handoff bundle artifact is not a QueryGraph bootstrap bundle: {err}"
        ))
    })?;
    let table_scope = HandoffTableScope::from_summary(summary, warehouse)?;
    let namespace = table_scope
        .namespace
        .split('.')
        .map(str::to_string)
        .collect::<Vec<_>>();
    verify_qglake_bootstrap_bundle(&bundle, &namespace, &table_scope.table)?;
    let verification = bundle.verify_manifest()?;
    require_string_match(
        querygraph,
        "bundleHash",
        verification.bundle_hash.as_str(),
        "querygraphVerification",
    )?;
    require_string_match(
        querygraph,
        "graphHash",
        verification.graph_hash.as_str(),
        "querygraphVerification",
    )?;
    require_string_match(
        querygraph,
        "openLineageHash",
        verification.open_lineage_hash.as_str(),
        "querygraphVerification",
    )?;
    require_string_match(
        querygraph,
        "querygraphImportHash",
        verification.querygraph_import_hash.as_str(),
        "querygraphVerification",
    )?;
    require_u64_match(
        querygraph,
        "tableCount",
        verification.table_count as u64,
        "querygraphVerification",
    )?;
    require_u64_match(
        querygraph,
        "viewCount",
        verification.view_count as u64,
        "querygraphVerification",
    )?;
    require_value_match(
        querygraph,
        "verifiedTables",
        &json!(verification.verified_tables),
        "querygraphVerification",
    )?;
    require_value_match(
        querygraph,
        "verifiedViews",
        &json!(verification.verified_views),
        "querygraphVerification",
    )?;
    require_value_match(
        querygraph,
        "standards",
        &json!(verification.standards),
        "querygraphVerification",
    )?;

    require_string_match(
        bootstrap,
        "bundleHash",
        verification.bundle_hash.as_str(),
        "queryGraphBootstrapProof",
    )?;
    require_string_match(
        bootstrap,
        "graphHash",
        verification.graph_hash.as_str(),
        "queryGraphBootstrapProof",
    )?;
    require_string_match(
        bootstrap,
        "openLineageHash",
        verification.open_lineage_hash.as_str(),
        "queryGraphBootstrapProof",
    )?;
    require_string_match(
        bootstrap,
        "queryGraphImportHash",
        verification.querygraph_import_hash.as_str(),
        "queryGraphBootstrapProof",
    )?;
    Ok(json!({
        "warehouse": verification.warehouse,
        "tableCount": verification.table_count,
        "viewCount": verification.view_count,
        "verifiedTables": verification.verified_tables,
        "verifiedViews": verification.verified_views,
        "bundleHash": verification.bundle_hash,
        "graphHash": verification.graph_hash,
        "openLineageHash": verification.open_lineage_hash,
        "queryGraphImportHash": verification.querygraph_import_hash,
        "standards": verification.standards,
        "graphNodes": bundle.graph.nodes.len(),
        "graphEdges": bundle.graph.edges.len(),
    }))
}

pub(crate) fn require_qglake_import_plan_graph_counts_match_bundle(
    bundle_semantics: &Value,
    import_plan_semantics: &Value,
) -> lakecat_core::LakeCatResult<()> {
    let bundle = bundle_semantics.as_object().ok_or_else(|| {
        lakecat_core::LakeCatError::Internal(
            "handoff bundle artifact semantics must be an object".to_string(),
        )
    })?;
    let import_plan = import_plan_semantics.as_object().ok_or_else(|| {
        lakecat_core::LakeCatError::Internal(
            "handoff QueryGraph import plan semantics must be an object".to_string(),
        )
    })?;
    for field in ["graphNodes", "graphEdges"] {
        require_value_match(
            import_plan,
            field,
            required_value(bundle, field, "bundleArtifactSemantics")?,
            "querygraphImportPlanSemantics",
        )?;
    }
    Ok(())
}

pub(crate) const QUERYGRAPH_IMPORT_PLAN_ARTIFACT_FIELDS: &[&str] = &[
    "verification",
    "graph-nodes",
    "graph-edges",
    "tables",
    "views",
    // Informational, derived by the QueryGraph importer from the (graph-hash-
    // covered) catalog-graph projection: the distinct node labels and the
    // `MATCH (t:Table)` node count. Allowed but not separately verified — their
    // integrity rides on the verified graph hash.
    "catalog-labels",
    "table-count",
];
pub(crate) const QUERYGRAPH_IMPORT_PLAN_VERIFICATION_FIELDS: &[&str] = &[
    "warehouse",
    "table-count",
    "view-count",
    "verified-tables",
    "verified-views",
    "bundle-hash",
    "graph-hash",
    "open-lineage-hash",
    "querygraph-import-hash",
    "standards",
];
pub(crate) const QUERYGRAPH_IMPORT_PLAN_TABLE_FIELDS: &[&str] = &[
    "stable-id",
    "croissant-name",
    "cdif-title",
    "osi-model",
    "odrl-policy",
];
pub(crate) const QUERYGRAPH_IMPORT_PLAN_VIEW_FIELDS: &[&str] =
    &["stable-id", "name", "view-version", "dialect", "osi-model"];

pub(crate) fn verify_qglake_handoff_querygraph_import_plan_semantics(
    summary_path: &Path,
    summary: &Value,
) -> lakecat_core::LakeCatResult<Value> {
    let summary = summary.as_object().ok_or_else(|| {
        lakecat_core::LakeCatError::InvalidArgument(
            "handoff summary root must be an object".to_string(),
        )
    })?;
    let warehouse = required_str(summary, "warehouse", "handoff summary")?;
    let import = required_object(summary, "querygraphImportVerification", "handoff summary")?;
    let lakecat = required_object(summary, "lakecatReplayVerification", "handoff summary")?;
    let artifacts = required_object(summary, "artifacts", "handoff summary")?;
    let base_dir = summary_path.parent().unwrap_or_else(|| Path::new(""));
    let plan_value =
        read_qglake_handoff_artifact_json(artifacts, "querygraphImportPlan", base_dir)?;
    let plan = plan_value.as_object().ok_or_else(|| {
        lakecat_core::LakeCatError::InvalidArgument(
            "handoff QueryGraph import plan artifact must be a JSON object".to_string(),
        )
    })?;
    require_only_fields(
        plan,
        QUERYGRAPH_IMPORT_PLAN_ARTIFACT_FIELDS,
        "handoff QueryGraph import plan artifact",
    )?;
    let verification = required_object(
        plan,
        "verification",
        "handoff QueryGraph import plan artifact",
    )?;
    require_only_fields(
        verification,
        QUERYGRAPH_IMPORT_PLAN_VERIFICATION_FIELDS,
        "handoff QueryGraph import plan artifact.verification",
    )?;
    let table_scope = HandoffTableScope::from_summary(summary, warehouse)?;
    let view_scope = HandoffViewScope::from_lakecat(lakecat)?;

    verify_querygraph_import_plan_verification_matches_summary(
        verification,
        import,
        &table_scope,
        &view_scope,
    )?;
    verify_querygraph_import_plan_artifact_lists(plan, verification)?;

    Ok(json!({
        "warehouse": required_str(verification, "warehouse", "handoff QueryGraph import plan artifact.verification")?,
        "tableCount": required_u64(verification, "table-count", "handoff QueryGraph import plan artifact.verification")?,
        "viewCount": required_u64(verification, "view-count", "handoff QueryGraph import plan artifact.verification")?,
        "verifiedTables": required_value(verification, "verified-tables", "handoff QueryGraph import plan artifact.verification")?,
        "verifiedViews": required_value(verification, "verified-views", "handoff QueryGraph import plan artifact.verification")?,
        "bundleHash": required_str(verification, "bundle-hash", "handoff QueryGraph import plan artifact.verification")?,
        "graphHash": required_str(verification, "graph-hash", "handoff QueryGraph import plan artifact.verification")?,
        "openLineageHash": required_str(verification, "open-lineage-hash", "handoff QueryGraph import plan artifact.verification")?,
        "queryGraphImportHash": required_str(verification, "querygraph-import-hash", "handoff QueryGraph import plan artifact.verification")?,
        "standards": required_value(verification, "standards", "handoff QueryGraph import plan artifact.verification")?,
        "graphNodes": required_u64(plan, "graph-nodes", "handoff QueryGraph import plan artifact")?,
        "graphEdges": required_u64(plan, "graph-edges", "handoff QueryGraph import plan artifact")?,
    }))
}

pub(crate) fn verify_querygraph_import_plan_verification_matches_summary(
    verification: &serde_json::Map<String, Value>,
    import: &serde_json::Map<String, Value>,
    table_scope: &HandoffTableScope,
    view_scope: &HandoffViewScope,
) -> lakecat_core::LakeCatResult<()> {
    let label = "handoff QueryGraph import plan artifact.verification";
    require_string_match(
        verification,
        "warehouse",
        table_scope.warehouse.as_str(),
        label,
    )?;
    require_verified_table_scope(verification, table_scope, label)?;
    require_verified_view_scope(verification, view_scope, label)?;
    require_u64_match(
        verification,
        "table-count",
        required_u64(import, "tableCount", "querygraphImportVerification")?,
        label,
    )?;
    require_u64_match(
        verification,
        "view-count",
        required_u64(import, "viewCount", "querygraphImportVerification")?,
        label,
    )?;
    require_string_match(
        verification,
        "bundle-hash",
        required_str(import, "bundleHash", "querygraphImportVerification")?,
        label,
    )?;
    require_string_match(
        verification,
        "graph-hash",
        required_str(import, "graphHash", "querygraphImportVerification")?,
        label,
    )?;
    require_string_match(
        verification,
        "open-lineage-hash",
        required_str(import, "openLineageHash", "querygraphImportVerification")?,
        label,
    )?;
    require_string_match(
        verification,
        "querygraph-import-hash",
        required_str(
            import,
            "querygraphImportHash",
            "querygraphImportVerification",
        )?,
        label,
    )?;
    require_value_match(
        verification,
        "verified-tables",
        required_value(import, "verifiedTables", "querygraphImportVerification")?,
        label,
    )?;
    require_value_match(
        verification,
        "verified-views",
        required_value(import, "verifiedViews", "querygraphImportVerification")?,
        label,
    )?;
    require_value_match(
        verification,
        "standards",
        required_value(import, "standards", "querygraphImportVerification")?,
        label,
    )
}

pub(crate) fn verify_querygraph_import_plan_artifact_lists(
    plan: &serde_json::Map<String, Value>,
    verification: &serde_json::Map<String, Value>,
) -> lakecat_core::LakeCatResult<()> {
    let label = "handoff QueryGraph import plan artifact";
    let table_count = required_u64(
        verification,
        "table-count",
        "handoff QueryGraph import plan artifact.verification",
    )?;
    let view_count = required_u64(
        verification,
        "view-count",
        "handoff QueryGraph import plan artifact.verification",
    )?;
    let tables = required_array(plan, "tables", label)?;
    let views = required_array(plan, "views", label)?;
    require_import_plan_records_closed(tables, QUERYGRAPH_IMPORT_PLAN_TABLE_FIELDS, "tables")?;
    require_import_plan_records_closed(views, QUERYGRAPH_IMPORT_PLAN_VIEW_FIELDS, "views")?;
    if tables.len() as u64 != table_count {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "{label}.tables count mismatch: expected={table_count} actual={}",
            tables.len()
        )));
    }
    if views.len() as u64 != view_count {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "{label}.views count mismatch: expected={view_count} actual={}",
            views.len()
        )));
    }
    require_import_plan_list_covers_verified_ids(
        tables,
        required_array(
            verification,
            "verified-tables",
            "handoff QueryGraph import plan artifact.verification",
        )?,
        "tables",
    )?;
    require_import_plan_list_covers_verified_ids(
        views,
        required_array(
            verification,
            "verified-views",
            "handoff QueryGraph import plan artifact.verification",
        )?,
        "views",
    )?;
    require_positive_u64(plan, "graph-nodes", label)?;
    require_positive_u64(plan, "graph-edges", label)?;
    Ok(())
}

pub(crate) fn require_import_plan_records_closed(
    records: &[Value],
    allowed_fields: &[&str],
    field: &str,
) -> lakecat_core::LakeCatResult<()> {
    let label = "handoff QueryGraph import plan artifact";
    for (index, record) in records.iter().enumerate() {
        let record = record.as_object().ok_or_else(|| {
            lakecat_core::LakeCatError::InvalidArgument(format!(
                "{label}.{field}[{index}] must be an object"
            ))
        })?;
        require_only_fields(record, allowed_fields, &format!("{label}.{field}[{index}]"))?;
    }
    Ok(())
}

pub(crate) fn require_governed_scan_stats_field_evidence(
    governed_scan: &serde_json::Map<String, Value>,
    planned_restriction: &serde_json::Map<String, Value>,
    fetched_restriction: &serde_json::Map<String, Value>,
) -> lakecat_core::LakeCatResult<()> {
    let planned_requested = required_string_array(
        governed_scan,
        "plannedRequestedStatsFields",
        "governedScanProof",
    )?;
    let planned_effective = required_string_array(
        governed_scan,
        "plannedEffectiveStatsFields",
        "governedScanProof",
    )?;
    if planned_requested.is_empty() || planned_effective.is_empty() {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "governedScanProof stats-field evidence must preserve non-empty requested and effective fields".to_string(),
        ));
    }
    require_non_empty_unique_strings(
        &planned_requested,
        "governedScanProof.plannedRequestedStatsFields",
    )?;
    require_non_empty_unique_strings(
        &planned_effective,
        "governedScanProof.plannedEffectiveStatsFields",
    )?;
    if planned_requested.len() <= planned_effective.len() {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "governedScanProof plannedRequestedStatsFields must prove a wider request than plannedEffectiveStatsFields".to_string(),
        ));
    }
    let planned_requested_set = planned_requested
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    for field in &planned_effective {
        if !planned_requested_set.contains(field.as_str()) {
            return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "governedScanProof plannedEffectiveStatsFields contains {field} that was not requested"
            )));
        }
    }
    require_value_match(
        planned_restriction,
        "allowed-columns",
        required_value(
            governed_scan,
            "plannedEffectiveStatsFields",
            "governedScanProof",
        )?,
        "governedScanProof.plannedReadRestriction",
    )?;
    let fetched_requested = required_string_array(
        governed_scan,
        "fetchedRequestedStatsFields",
        "governedScanProof",
    )?;
    let fetched_effective = required_string_array(
        governed_scan,
        "fetchedEffectiveStatsFields",
        "governedScanProof",
    )?;
    if fetched_requested.is_empty() || fetched_effective.is_empty() {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "governedScanProof fetched stats-field evidence must preserve non-empty requested and effective fields".to_string(),
        ));
    }
    require_non_empty_unique_strings(
        &fetched_requested,
        "governedScanProof.fetchedRequestedStatsFields",
    )?;
    require_non_empty_unique_strings(
        &fetched_effective,
        "governedScanProof.fetchedEffectiveStatsFields",
    )?;
    let fetched_requested_set = fetched_requested
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    for field in &fetched_effective {
        if !fetched_requested_set.contains(field.as_str()) {
            return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "governedScanProof fetchedEffectiveStatsFields contains {field} that was not requested"
            )));
        }
    }
    require_value_match(
        fetched_restriction,
        "allowed-columns",
        required_value(
            governed_scan,
            "fetchedEffectiveStatsFields",
            "governedScanProof",
        )?,
        "governedScanProof.fetchedReadRestriction",
    )?;
    Ok(())
}

pub(crate) fn require_governed_scan_projection_evidence(
    governed_scan: &serde_json::Map<String, Value>,
    planned_restriction: &serde_json::Map<String, Value>,
) -> lakecat_core::LakeCatResult<()> {
    let requested = required_string_array(
        governed_scan,
        "plannedRequestedProjection",
        "governedScanProof",
    )?;
    let effective = required_string_array(
        governed_scan,
        "plannedEffectiveProjection",
        "governedScanProof",
    )?;
    if requested.is_empty() || effective.is_empty() {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "governedScanProof missing requested/effective projection evidence".to_string(),
        ));
    }
    require_non_empty_unique_strings(&requested, "governedScanProof.plannedRequestedProjection")?;
    require_non_empty_unique_strings(&effective, "governedScanProof.plannedEffectiveProjection")?;
    if requested.len() <= effective.len() {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "governedScanProof plannedRequestedProjection does not prove projection narrowing versus plannedEffectiveProjection".to_string(),
        ));
    }
    let requested_set = requested
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    for field in &effective {
        if !requested_set.contains(field.as_str()) {
            return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "governedScanProof plannedEffectiveProjection contains {field} that was not requested"
            )));
        }
    }
    require_value_match(
        planned_restriction,
        "allowed-columns",
        required_value(
            governed_scan,
            "plannedEffectiveProjection",
            "governedScanProof",
        )?,
        "governedScanProof.plannedReadRestriction",
    )?;
    Ok(())
}

pub(crate) fn require_import_plan_list_covers_verified_ids(
    records: &[Value],
    verified_ids: &[Value],
    field: &str,
) -> lakecat_core::LakeCatResult<()> {
    let label = "handoff QueryGraph import plan artifact";
    for (index, verified_id) in verified_ids.iter().enumerate() {
        let verified_id = verified_id.as_str().ok_or_else(|| {
            lakecat_core::LakeCatError::InvalidArgument(format!(
                "{label}.verification.verified-{field}[{index}] must be a string"
            ))
        })?;
        let found = records.iter().any(|record| {
            record
                .as_object()
                .and_then(|record| record.get("stable-id"))
                .and_then(Value::as_str)
                == Some(verified_id)
        });
        if !found {
            return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "{label}.{field} must include stable-id {verified_id}"
            )));
        }
    }
    Ok(())
}

pub(crate) fn verify_qglake_handoff_lineage_drain_artifact_semantics(
    summary_path: &Path,
    summary: &Value,
) -> lakecat_core::LakeCatResult<Value> {
    let summary = summary.as_object().ok_or_else(|| {
        lakecat_core::LakeCatError::InvalidArgument(
            "handoff summary root must be an object".to_string(),
        )
    })?;
    let principal = require_non_blank_str(summary, "principal", "handoff summary")?;
    let warehouse = required_str(summary, "warehouse", "handoff summary")?;
    let querygraph = required_object(summary, "querygraphVerification", "handoff summary")?;
    let lakecat = required_object(summary, "lakecatReplayVerification", "handoff summary")?;
    let bootstrap = required_object(
        lakecat,
        "queryGraphBootstrapProof",
        "lakecatReplayVerification",
    )?;
    let policy_binding_count =
        required_u64(bootstrap, "policyBindingCount", "queryGraphBootstrapProof")? as usize;
    let artifacts = required_object(summary, "artifacts", "handoff summary")?;
    let base_dir = summary_path.parent().unwrap_or_else(|| Path::new(""));
    let drain_value = read_qglake_handoff_artifact_json(artifacts, "lineageDrain", base_dir)?;
    let drain: LineageDrainResponse = serde_json::from_value(drain_value).map_err(|err| {
        lakecat_core::LakeCatError::InvalidArgument(format!(
            "handoff lineage drain artifact is not a LakeCat lineage-drain response: {err}"
        ))
    })?;
    let verification = qglake_verification_from_handoff_summary(warehouse, querygraph, lakecat)?;
    verify_qglake_lineage_drain(&drain, &verification, Some(principal), policy_binding_count)?;
    let replay_evidence = qglake_replay_evidence_json(&drain, Some(principal), &verification);
    let replay = qglake_replay_verification_json(
        &verification,
        qglake_scan_replay_line(&drain),
        qglake_management_replay_line(&drain),
        qglake_credential_replay_line(&drain, Some(principal)),
        qglake_table_commit_history_replay_line(&drain),
        replay_evidence,
    );
    let replay = replay.as_object().ok_or_else(|| {
        lakecat_core::LakeCatError::Internal(
            "lineage drain replay verification must be an object".to_string(),
        )
    })?;
    verify_lakecat_replay_capture_matches_summary(replay, lakecat, querygraph)?;

    Ok(json!({
        "delivered": drain.delivered,
        "eventTypes": drain.event_types,
        "graphEvents": drain.graph_events,
        "lineageEvents": drain.lineage_events,
        "principalSubject": drain.principal_subject,
        "principalKind": drain.principal_kind,
        "authorizationReceiptHash": drain.authorization_receipt_hash,
        "authorizationReceiptAction": drain.authorization_receipt_action,
        "requestIdentitySource": drain.request_identity_source,
        "requestIdentityState": drain.request_identity_state,
        "typedidEnvelopeHash": drain.typedid_envelope_hash,
        "typedidProofHash": drain.typedid_proof_hash,
        "tableCount": verification.table_count,
        "viewCount": verification.view_count,
        "verifiedTables": verification.verified_tables,
        "verifiedViews": verification.verified_views,
        "bundleHash": verification.bundle_hash,
        "graphHash": verification.graph_hash,
        "openLineageHash": verification.open_lineage_hash,
        "queryGraphImportHash": verification.querygraph_import_hash,
        "standards": verification.standards,
        "catalogConfigProof": required_value(
            required_object(replay, "replay-evidence", "lineage drain replay verification")?,
            "catalogConfig",
            "lineage drain replay verification.replay-evidence"
        )?,
    }))
}

pub(crate) fn qglake_verification_from_handoff_summary(
    warehouse: &str,
    querygraph: &serde_json::Map<String, Value>,
    lakecat: &serde_json::Map<String, Value>,
) -> lakecat_core::LakeCatResult<QueryGraphBootstrapVerification> {
    let view_receipts = required_object(
        lakecat,
        "viewReceiptChainProof",
        "lakecatReplayVerification",
    )?;
    let mut verified_view_versions = BTreeMap::new();
    let mut verified_view_receipt_hashes = BTreeMap::new();
    let mut verified_view_receipt_chain_hashes = BTreeMap::new();
    for (index, view) in required_array(view_receipts, "views", "viewReceiptChainProof")?
        .iter()
        .enumerate()
    {
        let view = view.as_object().ok_or_else(|| {
            lakecat_core::LakeCatError::InvalidArgument(format!(
                "viewReceiptChainProof.views[{index}] must be an object"
            ))
        })?;
        let stable_id = required_str(view, "stableId", "viewReceiptChainProof.views[]")?;
        verified_view_versions.insert(
            stable_id.to_string(),
            required_u64(view, "acceptedViewVersion", "viewReceiptChainProof.views[]")?,
        );
        verified_view_receipt_hashes.insert(
            stable_id.to_string(),
            require_hash_str(view, "acceptedReceiptHash", "viewReceiptChainProof.views[]")?
                .to_string(),
        );
        verified_view_receipt_chain_hashes.insert(
            stable_id.to_string(),
            require_hash_str(
                view,
                "acceptedReceiptChainHash",
                "viewReceiptChainProof.views[]",
            )?
            .to_string(),
        );
    }

    Ok(QueryGraphBootstrapVerification {
        warehouse: warehouse.to_string(),
        table_count: required_u64(querygraph, "tableCount", "querygraphVerification")? as usize,
        view_count: required_u64(querygraph, "viewCount", "querygraphVerification")? as usize,
        verified_tables: required_string_array(
            querygraph,
            "verifiedTables",
            "querygraphVerification",
        )?,
        verified_views: required_string_array(
            querygraph,
            "verifiedViews",
            "querygraphVerification",
        )?,
        verified_view_versions,
        verified_view_receipt_hashes,
        verified_view_receipt_chain_hashes,
        bundle_hash: required_str(querygraph, "bundleHash", "querygraphVerification")?.to_string(),
        graph_hash: required_str(querygraph, "graphHash", "querygraphVerification")?.to_string(),
        open_lineage_hash: required_str(querygraph, "openLineageHash", "querygraphVerification")?
            .to_string(),
        querygraph_import_hash: required_str(
            querygraph,
            "querygraphImportHash",
            "querygraphVerification",
        )?
        .to_string(),
        standards: required_string_array(querygraph, "standards", "querygraphVerification")?,
    })
}

pub(crate) fn read_qglake_handoff_artifact_json(
    artifacts: &serde_json::Map<String, Value>,
    field: &str,
    base_dir: &Path,
) -> lakecat_core::LakeCatResult<Value> {
    let artifact = required_object(artifacts, field, "handoff summary artifacts")?;
    let resolved_path = required_resolved_artifact_path(artifact, "path", base_dir)?;
    let bytes = fs::read(&resolved_path).map_err(|err| {
        lakecat_core::LakeCatError::InvalidArgument(format!(
            "failed to read captured handoff output {} at {}: {err}",
            field,
            resolved_path.display()
        ))
    })?;
    serde_json::from_slice(&bytes).map_err(|err| {
        lakecat_core::LakeCatError::InvalidArgument(format!(
            "captured handoff output {} at {} is not JSON: {err}",
            field,
            resolved_path.display()
        ))
    })
}
