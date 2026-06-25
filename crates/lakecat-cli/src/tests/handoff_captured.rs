use super::common::*;
use crate::*;

#[test]
fn qglake_handoff_captured_output_semantics_accepts_omitted_absent_secret_ref_fields() {
    let temp = qglake_temp_dir("handoff-captured-omitted-absent-secret-ref-fields");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    let storage_profile = summary["lakecatReplayVerification"]["storageProfileUpsertProof"]
        .as_object_mut()
        .unwrap();
    storage_profile.remove("secretRefProvider");
    storage_profile.remove("secretRefHash");
    for proof in ["restricted", "trustedHuman"] {
        let storage_profile = summary["lakecatReplayVerification"]["credentialVendingProof"][proof]
            ["storageProfile"]
            .as_object_mut()
            .unwrap();
        storage_profile.remove("secretRefProvider");
        storage_profile.remove("secretRefHash");
    }

    let mut replay =
        read_json_file(&temp.join("lakecat-replay.txt")).expect("read LakeCat replay output");
    let storage_profile = replay["replay-evidence"]["management"]["storageProfileUpsert"]
        .as_object_mut()
        .unwrap();
    storage_profile.remove("secretRefProvider");
    storage_profile.remove("secretRefHash");
    for proof in ["restricted", "trustedHuman"] {
        let storage_profile = replay["replay-evidence"]["credentials"][proof]["storageProfile"]
            .as_object_mut()
            .unwrap();
        storage_profile.remove("secretRefProvider");
        storage_profile.remove("secretRefHash");
    }
    let replay_bytes = serde_json::to_vec_pretty(&replay).expect("replay JSON bytes");
    fs::write(temp.join("lakecat-replay.txt"), &replay_bytes)
        .expect("write normalized LakeCat replay output");
    summary["artifacts"]["capturedOutputs"]["lakecatReplay"]["sha256"] =
        json!(content_hash_bytes(&replay_bytes));

    verify_qglake_handoff_summary_value(&summary).expect(
        "handoff summary should accept omitted secret-ref proof fields when secretRefPresent is false",
    );
    verify_qglake_handoff_captured_output_semantics(&summary_path, &summary).expect(
        "captured replay semantics should accept matching omitted secret-ref proof fields when secretRefPresent is false",
    );
}

#[test]
fn qglake_handoff_captured_output_semantics_accept_matching_files() {
    let temp = qglake_temp_dir("handoff-captured-semantics-ok");
    let summary_path = temp.join("handoff-summary.json");
    let summary = qglake_handoff_summary_json_with_artifacts(&temp);
    let semantics = verify_qglake_handoff_captured_output_semantics(&summary_path, &summary)
        .expect("captured output semantics should verify");
    let (_, expected_view_receipt_hash) =
        qglake_fixture_view_receipt("active_customers_view", 1, None, None, "upsert");

    assert_eq!(
        semantics["lakecatReplay"]["schemaVersion"],
        json!("lakecat.qglake.replay-verification.v1")
    );
    assert_eq!(
        semantics["querygraphVerify"]["bundleHash"],
        json!(content_hash_bytes(b"bundle"))
    );
    assert_eq!(
        semantics["querygraphVerify"]["verifiedTables"],
        json!(["lakecat:table:local:default:events"])
    );
    assert_eq!(
        semantics["querygraphVerify"]["verifiedViews"],
        json!(["lakecat:view:local:default:active_customers_view"])
    );
    assert_eq!(
        semantics["querygraphImport"]["queryGraphImportHash"],
        json!(qglake_fixture_hash("querygraph-import"))
    );
    assert_eq!(
        semantics["lakecatReplay"]["storageProfileUpsertProof"]["locationPrefixHash"],
        json!("sha256:2222222222222222222222222222222222222222222222222222222222222222")
    );
    assert_eq!(
        semantics["lakecatReplay"]["requestIdentityProof"]["principalSubject"],
        json!("did:example:agent")
    );
    assert_eq!(
        semantics["lakecatReplay"]["queryGraphBootstrapProof"]["agentDelegationHash"],
        json!(qglake_fixture_hash("delegation"))
    );
    assert_eq!(
        semantics["lakecatReplay"]["catalogConfigProof"]["authorizationReceiptAction"],
        json!("catalog-config")
    );
    assert_eq!(
        semantics["lakecatReplay"]["governedScanProof"]["planTaskCount"],
        json!(1)
    );
    assert_eq!(
        semantics["lakecatReplay"]["managementProof"]["policyBindingCount"],
        json!(1)
    );
    assert_eq!(
        semantics["lakecatReplay"]["tableCommitHistoryProof"]["commitHashes"],
        json!([qglake_fixture_hash("commit")])
    );
    assert_eq!(
        semantics["lakecatReplay"]["tableCommitHistoryProof"]["graphEvents"],
        json!(1)
    );
    assert_eq!(
        semantics["lakecatReplay"]["viewReceiptChainProof"]["views"][0]["acceptedReceiptHash"],
        json!(expected_view_receipt_hash)
    );
    assert_eq!(
        semantics["lakecatReplay"]["credentialVendingProof"]["restricted"]["blockReason"],
        json!(QGLAKE_RESTRICTED_CREDENTIAL_BLOCK_REASON)
    );
}

#[test]
fn qglake_handoff_captured_output_semantics_rejects_extra_lakecat_replay_root_fields() {
    let temp = qglake_temp_dir("handoff-captured-extra-lakecat-root");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    let mut replay = read_json_file(&temp.join("lakecat-replay.txt")).expect("replay JSON");
    replay["unverifiedReplayRootClaim"] =
        json!(qglake_fixture_hash("unverified-captured-replay-root-claim"));
    let bytes = serde_json::to_vec_pretty(&replay).expect("drifted replay JSON");
    fs::write(temp.join("lakecat-replay.txt"), &bytes).expect("write drifted replay");
    summary["artifacts"]["capturedOutputs"]["lakecatReplay"]["sha256"] =
        json!(content_hash_bytes(&bytes));

    let err = verify_qglake_handoff_captured_output_semantics(&summary_path, &summary)
        .expect_err("captured LakeCat replay output should reject extra root fields");
    let err = err.to_string();

    assert!(err.contains("captured LakeCat replay output"), "{err}");
    assert!(
        err.contains("unexpected field unverifiedReplayRootClaim"),
        "{err}"
    );
}

#[test]
fn qglake_handoff_captured_output_semantics_rejects_extra_querygraph_root_fields() {
    for (capture, file, claim, label) in [
        (
            "querygraphVerify",
            "querygraph-verify.json",
            "unverifiedQueryGraphVerifyClaim",
            "captured QueryGraph verify output",
        ),
        (
            "querygraphImport",
            "querygraph-import.json",
            "unverifiedQueryGraphImportClaim",
            "captured QueryGraph import output",
        ),
    ] {
        let temp = qglake_temp_dir(&format!("handoff-captured-extra-{capture}-root"));
        let summary_path = temp.join("handoff-summary.json");
        let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
        let mut output = read_json_file(&temp.join(file)).expect("read captured QueryGraph output");
        output[claim] = json!(qglake_fixture_hash("unverified-querygraph-root-claim"));
        let bytes = serde_json::to_vec_pretty(&output).expect("drifted QueryGraph JSON");
        fs::write(temp.join(file), &bytes).expect("write drifted QueryGraph output");
        summary["artifacts"]["capturedOutputs"][capture]["sha256"] =
            json!(content_hash_bytes(&bytes));

        let err = verify_qglake_handoff_captured_output_semantics(&summary_path, &summary)
            .expect_err("captured QueryGraph output should reject extra root fields");
        let err = err.to_string();

        assert!(err.contains(label), "{err}");
        assert!(err.contains(&format!("unexpected field {claim}")), "{err}");
    }
}

#[test]
fn qglake_handoff_captured_output_semantics_rejects_catalog_config_drift() {
    let temp = qglake_temp_dir("handoff-captured-catalog-config-drift");
    let summary_path = temp.join("handoff-summary.json");
    let summary = qglake_handoff_summary_json_with_artifacts(&temp);
    let mut replay = read_json_file(&temp.join("lakecat-replay.txt")).expect("replay JSON");
    replay["replay-evidence"]["catalogConfig"]["authorizationReceiptAction"] =
        json!("lineage-read");
    fs::write(
        temp.join("lakecat-replay.txt"),
        serde_json::to_vec_pretty(&replay).expect("drifted replay JSON"),
    )
    .expect("write drifted replay");

    let err = verify_qglake_handoff_captured_output_semantics(&summary_path, &summary)
        .expect_err("captured replay catalog config proof drift should be rejected");

    assert!(err.to_string().contains("catalogConfig"));
    assert!(err.to_string().contains("authorizationReceiptAction"));
}

#[test]
fn qglake_handoff_captured_output_semantics_rejects_relative_path_traversal() {
    let temp = qglake_temp_dir("handoff-captured-relative-path-traversal");
    let outside = qglake_temp_dir("handoff-captured-relative-path-splice");
    let outside_replay = outside.join("lakecat-replay.txt");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    fs::copy(temp.join("lakecat-replay.txt"), &outside_replay)
        .expect("copy replay outside summary dir");
    let outside_name = outside
        .file_name()
        .and_then(|name| name.to_str())
        .expect("outside temp dir name");
    summary["artifacts"]["capturedOutputs"]["lakecatReplay"]["path"] =
        json!(format!("../{outside_name}/lakecat-replay.txt"));
    summary["artifacts"]["lakecatReplayOutput"] =
        json!(format!("../{outside_name}/lakecat-replay.txt"));

    let err = verify_qglake_handoff_captured_output_semantics(&summary_path, &summary)
        .expect_err("captured output reader should reject relative path traversal");
    let err = err.to_string();

    assert!(err.contains("summary directory"), "{err}");
}

#[test]
fn qglake_handoff_captured_output_semantics_rejects_extra_catalog_config_fields() {
    let temp = qglake_temp_dir("handoff-captured-catalog-config-extra-field");
    let summary_path = temp.join("handoff-summary.json");
    let summary = qglake_handoff_summary_json_with_artifacts(&temp);
    let mut replay = read_json_file(&temp.join("lakecat-replay.txt")).expect("replay JSON");
    replay["replay-evidence"]["catalogConfig"]["unverifiedEndpointClaim"] =
        json!(qglake_fixture_hash("unverified-captured-config-claim"));
    fs::write(
        temp.join("lakecat-replay.txt"),
        serde_json::to_vec_pretty(&replay).expect("drifted replay JSON"),
    )
    .expect("write drifted replay");

    let err = verify_qglake_handoff_captured_output_semantics(&summary_path, &summary)
        .expect_err("captured replay catalog config extra fields should be rejected");
    let err = err.to_string();

    assert!(
        err.contains("captured LakeCat replay output.replay-evidence.catalogConfig"),
        "{err}"
    );
    assert!(
        err.contains("unexpected field unverifiedEndpointClaim"),
        "{err}"
    );
}

#[test]
fn qglake_handoff_captured_output_semantics_rejects_summary_verified_table_drift() {
    let temp = qglake_temp_dir("handoff-captured-summary-table-drift");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    summary["querygraphVerification"]["verifiedTables"] =
        json!(["lakecat:table:local:default:events_other"]);

    let err = verify_qglake_handoff_captured_output_semantics(&summary_path, &summary)
        .expect_err("captured output semantics should reject summary verified-table drift");

    assert!(
        err.to_string()
            .contains("captured QueryGraph verify output.verified-tables mismatch")
    );
}

#[test]
fn qglake_handoff_captured_output_semantics_rejects_summary_verified_view_drift() {
    let temp = qglake_temp_dir("handoff-captured-summary-view-drift");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    summary["querygraphVerification"]["verifiedViews"] =
        json!(["lakecat:view:local:default:active_customers_view_other"]);

    let err = verify_qglake_handoff_captured_output_semantics(&summary_path, &summary)
        .expect_err("captured output semantics should reject summary verified-view drift");

    assert!(
        err.to_string()
            .contains("captured QueryGraph verify output.verified-views mismatch")
    );
}

#[test]
fn qglake_handoff_captured_output_semantics_rejects_import_summary_drift() {
    let temp = qglake_temp_dir("handoff-captured-import-summary-drift");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    summary["querygraphImportVerification"]["verifiedTables"] =
        json!(["lakecat:table:local:default:events_other"]);

    let err = verify_qglake_handoff_captured_output_semantics(&summary_path, &summary)
        .expect_err("captured output semantics should reject import summary drift");

    assert!(err.to_string().contains("querygraphImportVerification"));
    assert!(err.to_string().contains("verifiedTables mismatch"));
}

#[test]
fn qglake_handoff_captured_output_semantics_rejects_summary_drift() {
    let temp = qglake_temp_dir("handoff-captured-semantics-drift");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    let drifted = json!({
        "warehouse": "local",
        "table-count": 1,
        "view-count": 1,
        "verified-tables": [
            "lakecat:table:local:default:events"
        ],
        "verified-views": [
            "lakecat:view:local:default:active_customers_view"
        ],
        "bundle-hash": "sha256:other-bundle",
        "graph-hash": "sha256:graph",
        "open-lineage-hash": "sha256:openlineage",
        "querygraph-import-hash": "sha256:querygraph-import",
        "standards": [
            "Iceberg REST",
            "Croissant",
            "CDIF",
            "OSI handoff",
            "ODRL",
            "Grust catalog graph",
            "OpenLineage"
        ]
    });
    let drifted_bytes = serde_json::to_vec_pretty(&drifted).expect("drifted JSON bytes");
    fs::write(temp.join("querygraph-verify.json"), &drifted_bytes)
        .expect("write drifted QueryGraph verify output");
    summary["artifacts"]["capturedOutputs"]["querygraphVerify"]["sha256"] =
        json!(content_hash_bytes(&drifted_bytes));

    let err = verify_qglake_handoff_captured_output_semantics(&summary_path, &summary)
        .expect_err("captured output semantics should reject drift");
    assert!(
        err.to_string()
            .contains("captured QueryGraph verify output.bundle-hash mismatch")
    );
}

#[test]
fn qglake_handoff_captured_output_semantics_rejects_artifact_path_outside_summary_dir() {
    let temp = qglake_temp_dir("handoff-captured-semantics-path-outside");
    let outside = qglake_temp_dir("handoff-captured-semantics-path-outside-splice");
    let outside_querygraph_verify = outside.join("querygraph-verify.json");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    fs::copy(
        temp.join("querygraph-verify.json"),
        &outside_querygraph_verify,
    )
    .expect("copy QueryGraph verify output outside bundle");
    summary["artifacts"]["capturedOutputs"]["querygraphVerify"]["path"] =
        json!(outside_querygraph_verify);

    let err = verify_qglake_handoff_captured_output_semantics(&summary_path, &summary)
        .expect_err("captured output semantics should reject paths outside summary directory");
    let err = err.to_string();

    assert!(err.contains("path"), "{err}");
    assert!(err.contains("summary directory"), "{err}");
}

#[test]
fn qglake_handoff_captured_output_semantics_rejects_querygraph_warehouse_drift() {
    let temp = qglake_temp_dir("handoff-captured-warehouse-drift");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    let mut drifted =
        read_json_file(&temp.join("querygraph-verify.json")).expect("read QueryGraph verify");
    drifted["warehouse"] = json!("other");
    let drifted_bytes = serde_json::to_vec_pretty(&drifted).expect("drifted JSON bytes");
    fs::write(temp.join("querygraph-verify.json"), &drifted_bytes)
        .expect("write drifted QueryGraph verify output");
    summary["artifacts"]["capturedOutputs"]["querygraphVerify"]["sha256"] =
        json!(content_hash_bytes(&drifted_bytes));

    let err = verify_qglake_handoff_captured_output_semantics(&summary_path, &summary)
        .expect_err("captured QueryGraph warehouse drift should be rejected");

    assert!(
        err.to_string()
            .contains("captured QueryGraph verify output.warehouse mismatch")
    );
}

#[test]
fn qglake_handoff_captured_output_semantics_rejects_querygraph_table_scope_drift() {
    let temp = qglake_temp_dir("handoff-captured-table-scope-drift");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    let mut drifted =
        read_json_file(&temp.join("querygraph-verify.json")).expect("read QueryGraph verify");
    drifted["verified-tables"] = json!(["lakecat:table:local:default:other"]);
    let drifted_bytes = serde_json::to_vec_pretty(&drifted).expect("drifted JSON bytes");
    fs::write(temp.join("querygraph-verify.json"), &drifted_bytes)
        .expect("write drifted QueryGraph verify output");
    summary["artifacts"]["capturedOutputs"]["querygraphVerify"]["sha256"] =
        json!(content_hash_bytes(&drifted_bytes));

    let err = verify_qglake_handoff_captured_output_semantics(&summary_path, &summary)
        .expect_err("captured QueryGraph table-scope drift should be rejected");

    assert!(
        err.to_string()
            .contains("captured QueryGraph verify output.verified-tables")
    );
    assert!(
        err.to_string()
            .contains("lakecat:table:local:default:events")
    );
}

#[test]
fn qglake_handoff_captured_output_semantics_rejects_querygraph_view_scope_drift() {
    let temp = qglake_temp_dir("handoff-captured-view-scope-drift");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    let mut drifted =
        read_json_file(&temp.join("querygraph-verify.json")).expect("read QueryGraph verify");
    drifted["verified-views"] = json!(["lakecat:view:local:default:other_view"]);
    let drifted_bytes = serde_json::to_vec_pretty(&drifted).expect("drifted JSON bytes");
    fs::write(temp.join("querygraph-verify.json"), &drifted_bytes)
        .expect("write drifted QueryGraph verify output");
    summary["artifacts"]["capturedOutputs"]["querygraphVerify"]["sha256"] =
        json!(content_hash_bytes(&drifted_bytes));

    let err = verify_qglake_handoff_captured_output_semantics(&summary_path, &summary)
        .expect_err("captured QueryGraph view-scope drift should be rejected");

    assert!(
        err.to_string()
            .contains("captured QueryGraph verify output.verified-views")
    );
    assert!(
        err.to_string()
            .contains("lakecat:view:local:default:active_customers_view")
    );
}

#[test]
fn qglake_handoff_captured_output_semantics_rejects_storage_profile_drift() {
    let temp = qglake_temp_dir("handoff-captured-storage-profile-drift");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    let mut drifted =
        read_json_file(&temp.join("lakecat-replay.txt")).expect("read LakeCat replay output");
    drifted["replay-evidence"]["management"]["storageProfileUpsert"]["locationPrefixHash"] =
        json!("sha256:other-storage-location-prefix");
    let drifted_bytes = serde_json::to_vec_pretty(&drifted).expect("drifted JSON bytes");
    fs::write(temp.join("lakecat-replay.txt"), &drifted_bytes)
        .expect("write drifted LakeCat replay output");
    summary["artifacts"]["capturedOutputs"]["lakecatReplay"]["sha256"] =
        json!(content_hash_bytes(&drifted_bytes));

    let err = verify_qglake_handoff_captured_output_semantics(&summary_path, &summary)
        .expect_err("captured replay storage-profile proof drift should be rejected");
    assert!(
        err.to_string().contains(
            "captured LakeCat replay output.replay-evidence.management.storageProfileUpsert.locationPrefixHash mismatch"
        )
    );
}

#[test]
fn qglake_handoff_captured_output_semantics_rejects_extra_storage_profile_fields() {
    let temp = qglake_temp_dir("handoff-captured-storage-profile-extra-field");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    let mut drifted =
        read_json_file(&temp.join("lakecat-replay.txt")).expect("read LakeCat replay output");
    drifted["replay-evidence"]["management"]["storageProfileUpsert"]["unverifiedStorageClaim"] =
        json!(qglake_fixture_hash("unverified-storage-claim"));
    let drifted_bytes = serde_json::to_vec_pretty(&drifted).expect("drifted JSON bytes");
    fs::write(temp.join("lakecat-replay.txt"), &drifted_bytes)
        .expect("write drifted LakeCat replay output");
    summary["artifacts"]["capturedOutputs"]["lakecatReplay"]["sha256"] =
        json!(content_hash_bytes(&drifted_bytes));

    let err = verify_qglake_handoff_captured_output_semantics(&summary_path, &summary)
        .expect_err("captured replay storage-profile proof should reject extra fields");
    let err = err.to_string();

    assert!(
        err.contains(
            "captured LakeCat replay output.replay-evidence.management.storageProfileUpsert"
        ),
        "{err}"
    );
    assert!(
        err.contains("unexpected field unverifiedStorageClaim"),
        "{err}"
    );
}

#[test]
fn qglake_handoff_captured_output_semantics_rejects_storage_profile_graph_drift() {
    let temp = qglake_temp_dir("handoff-captured-storage-profile-graph-drift");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    let mut drifted =
        read_json_file(&temp.join("lakecat-replay.txt")).expect("read LakeCat replay output");
    drifted["replay-evidence"]["management"]["storageProfileUpsert"]["graphEvents"] = json!(2);
    let drifted_bytes = serde_json::to_vec_pretty(&drifted).expect("drifted JSON bytes");
    fs::write(temp.join("lakecat-replay.txt"), &drifted_bytes)
        .expect("write drifted LakeCat replay output");
    summary["artifacts"]["capturedOutputs"]["lakecatReplay"]["sha256"] =
        json!(content_hash_bytes(&drifted_bytes));

    let err = verify_qglake_handoff_captured_output_semantics(&summary_path, &summary)
        .expect_err("captured replay storage-profile graph proof drift should be rejected");
    assert!(
        err.to_string().contains(
            "captured LakeCat replay output.replay-evidence.management.storageProfileUpsert.graphEvents mismatch"
        )
    );
}

#[test]
fn qglake_handoff_captured_output_semantics_rejects_storage_profile_action_drift() {
    let temp = qglake_temp_dir("handoff-captured-storage-profile-action-drift");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    let mut drifted =
        read_json_file(&temp.join("lakecat-replay.txt")).expect("read LakeCat replay output");
    drifted["replay-evidence"]["management"]["storageProfileUpsert"]["authorizationReceiptAction"] =
        json!("table-load");
    let drifted_bytes = serde_json::to_vec_pretty(&drifted).expect("drifted JSON bytes");
    fs::write(temp.join("lakecat-replay.txt"), &drifted_bytes)
        .expect("write drifted LakeCat replay output");
    summary["artifacts"]["capturedOutputs"]["lakecatReplay"]["sha256"] =
        json!(content_hash_bytes(&drifted_bytes));

    let err = verify_qglake_handoff_captured_output_semantics(&summary_path, &summary)
        .expect_err("captured replay storage-profile action proof drift should be rejected");
    assert!(
        err.to_string().contains(
            "captured LakeCat replay output.replay-evidence.management.storageProfileUpsert.authorizationReceiptAction mismatch"
        )
    );
}

#[test]
fn qglake_handoff_captured_output_semantics_rejects_management_id_drift() {
    let temp = qglake_temp_dir("handoff-captured-management-id-drift");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    let mut drifted =
        read_json_file(&temp.join("lakecat-replay.txt")).expect("read LakeCat replay output");
    drifted["replay-evidence"]["management"]["serverIds"] = json!(["other-server"]);
    let drifted_bytes = serde_json::to_vec_pretty(&drifted).expect("drifted JSON bytes");
    fs::write(temp.join("lakecat-replay.txt"), &drifted_bytes)
        .expect("write drifted LakeCat replay output");
    summary["artifacts"]["capturedOutputs"]["lakecatReplay"]["sha256"] =
        json!(content_hash_bytes(&drifted_bytes));

    let err = verify_qglake_handoff_captured_output_semantics(&summary_path, &summary)
        .expect_err("captured replay management ID proof drift should be rejected");
    assert!(
        err.to_string().contains(
            "captured LakeCat replay output.replay-evidence.management.serverIds mismatch"
        )
    );
}

#[test]
fn qglake_handoff_captured_output_semantics_rejects_extra_management_fields() {
    let temp = qglake_temp_dir("handoff-captured-management-extra-field");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    let mut drifted =
        read_json_file(&temp.join("lakecat-replay.txt")).expect("read LakeCat replay output");
    drifted["replay-evidence"]["management"]["unverifiedManagementClaim"] =
        json!(qglake_fixture_hash("unverified-management-claim"));
    let drifted_bytes = serde_json::to_vec_pretty(&drifted).expect("drifted JSON bytes");
    fs::write(temp.join("lakecat-replay.txt"), &drifted_bytes)
        .expect("write drifted LakeCat replay output");
    summary["artifacts"]["capturedOutputs"]["lakecatReplay"]["sha256"] =
        json!(content_hash_bytes(&drifted_bytes));

    let err = verify_qglake_handoff_captured_output_semantics(&summary_path, &summary)
        .expect_err("captured replay management proof should reject extra fields");
    let err = err.to_string();

    assert!(
        err.contains("captured LakeCat replay output.replay-evidence.management"),
        "{err}"
    );
    assert!(
        err.contains("unexpected field unverifiedManagementClaim"),
        "{err}"
    );
}

#[test]
fn qglake_handoff_captured_output_semantics_rejects_extra_policy_upsert_fields() {
    let temp = qglake_temp_dir("handoff-captured-policy-upsert-extra-field");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    let mut drifted =
        read_json_file(&temp.join("lakecat-replay.txt")).expect("read LakeCat replay output");
    drifted["replay-evidence"]["management"]["policyUpsertProof"]["unverifiedPolicyClaim"] =
        json!(qglake_fixture_hash("unverified-policy-claim"));
    let drifted_bytes = serde_json::to_vec_pretty(&drifted).expect("drifted JSON bytes");
    fs::write(temp.join("lakecat-replay.txt"), &drifted_bytes)
        .expect("write drifted LakeCat replay output");
    summary["artifacts"]["capturedOutputs"]["lakecatReplay"]["sha256"] =
        json!(content_hash_bytes(&drifted_bytes));

    let err = verify_qglake_handoff_captured_output_semantics(&summary_path, &summary)
        .expect_err("captured replay policy-upsert proof should reject extra fields");
    let err = err.to_string();

    assert!(
        err.contains("captured LakeCat replay output.replay-evidence.management.policyUpsertProof"),
        "{err}"
    );
    assert!(
        err.contains("unexpected field unverifiedPolicyClaim"),
        "{err}"
    );
}

#[test]
fn qglake_handoff_captured_output_semantics_rejects_management_scope_drift() {
    let temp = qglake_temp_dir("handoff-captured-management-scope-drift");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    let mut drifted =
        read_json_file(&temp.join("lakecat-replay.txt")).expect("read LakeCat replay output");
    drifted["replay-evidence"]["management"]["warehouseProjectId"] = json!("other-project");
    let drifted_bytes = serde_json::to_vec_pretty(&drifted).expect("drifted JSON bytes");
    fs::write(temp.join("lakecat-replay.txt"), &drifted_bytes)
        .expect("write drifted LakeCat replay output");
    summary["artifacts"]["capturedOutputs"]["lakecatReplay"]["sha256"] =
        json!(content_hash_bytes(&drifted_bytes));

    let err = verify_qglake_handoff_captured_output_semantics(&summary_path, &summary)
        .expect_err("captured replay management scope drift should be rejected");

    assert!(err.to_string().contains(
        "captured LakeCat replay output.replay-evidence.management.warehouseProjectId mismatch"
    ));
}

#[test]
fn qglake_handoff_captured_output_semantics_rejects_governed_scan_drift() {
    let temp = qglake_temp_dir("handoff-captured-scan-drift");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    let mut drifted =
        read_json_file(&temp.join("lakecat-replay.txt")).expect("read LakeCat replay output");
    drifted["replay-evidence"]["scan"]["planTaskCount"] = json!(2);
    let drifted_bytes = serde_json::to_vec_pretty(&drifted).expect("drifted JSON bytes");
    fs::write(temp.join("lakecat-replay.txt"), &drifted_bytes)
        .expect("write drifted LakeCat replay output");
    summary["artifacts"]["capturedOutputs"]["lakecatReplay"]["sha256"] =
        json!(content_hash_bytes(&drifted_bytes));

    let err = verify_qglake_handoff_captured_output_semantics(&summary_path, &summary)
        .expect_err("captured replay governed scan proof drift should be rejected");
    assert!(
        err.to_string()
            .contains("captured LakeCat replay output.replay-evidence.scan.planTaskCount mismatch")
    );
}

#[test]
fn qglake_handoff_captured_output_semantics_rejects_extra_governed_scan_fields() {
    let temp = qglake_temp_dir("handoff-captured-scan-extra-field");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    let mut drifted =
        read_json_file(&temp.join("lakecat-replay.txt")).expect("read LakeCat replay output");
    drifted["replay-evidence"]["scan"]["unverifiedScanClaim"] =
        json!(qglake_fixture_hash("unverified-scan-claim"));
    let drifted_bytes = serde_json::to_vec_pretty(&drifted).expect("drifted JSON bytes");
    fs::write(temp.join("lakecat-replay.txt"), &drifted_bytes)
        .expect("write drifted LakeCat replay output");
    summary["artifacts"]["capturedOutputs"]["lakecatReplay"]["sha256"] =
        json!(content_hash_bytes(&drifted_bytes));

    let err = verify_qglake_handoff_captured_output_semantics(&summary_path, &summary)
        .expect_err("captured replay governed scan proof should reject extra fields");
    let err = err.to_string();

    assert!(
        err.contains("captured LakeCat replay output.replay-evidence.scan"),
        "{err}"
    );
    assert!(
        err.contains("unexpected field unverifiedScanClaim"),
        "{err}"
    );
}

#[test]
fn qglake_handoff_captured_output_semantics_rejects_scan_replay_line_drift() {
    let temp = qglake_temp_dir("handoff-captured-scan-replay-line-drift");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    let mut drifted =
        read_json_file(&temp.join("lakecat-replay.txt")).expect("read LakeCat replay output");
    drifted["scan-replay"] = json!(
        "scan replay plan_tasks=1 plan_graph_events=1 planned_ttl=60 planned_purpose=qglake-agent-demo file_tasks=1 delete_files=1 child_plan_tasks=2 fetched_ttl=300 fetched_purpose=qglake-agent-demo"
    );
    let drifted_bytes = serde_json::to_vec_pretty(&drifted).expect("drifted JSON bytes");
    fs::write(temp.join("lakecat-replay.txt"), &drifted_bytes)
        .expect("write drifted LakeCat replay output");
    summary["artifacts"]["capturedOutputs"]["lakecatReplay"]["sha256"] =
        json!(content_hash_bytes(&drifted_bytes));

    let err = verify_qglake_handoff_captured_output_semantics(&summary_path, &summary)
        .expect_err("captured replay scan replay line drift should be rejected");
    assert!(
        err.to_string()
            .contains("captured LakeCat replay output.scan-replay mismatch")
    );
}

#[test]
fn qglake_handoff_captured_output_semantics_rejects_empty_planned_allowed_columns() {
    let temp = qglake_temp_dir("handoff-captured-scan-empty-planned-allowed-columns");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    summary["lakecatReplayVerification"]["governedScanProof"]["plannedReadRestriction"]["allowed-columns"] =
        json!([]);
    let mut drifted =
        read_json_file(&temp.join("lakecat-replay.txt")).expect("read LakeCat replay output");
    drifted["replay-evidence"]["scan"]["plannedReadRestriction"]["allowed-columns"] = json!([]);
    let drifted_bytes = serde_json::to_vec_pretty(&drifted).expect("drifted JSON bytes");
    fs::write(temp.join("lakecat-replay.txt"), &drifted_bytes)
        .expect("write drifted LakeCat replay output");
    summary["artifacts"]["capturedOutputs"]["lakecatReplay"]["sha256"] =
        json!(content_hash_bytes(&drifted_bytes));

    let err = verify_qglake_handoff_captured_output_semantics(&summary_path, &summary)
        .expect_err("captured scan replay should reject empty planned allowed columns");
    assert!(err.to_string().contains("captured LakeCat replay output"));
    assert!(err.to_string().contains("plannedReadRestriction"));
    assert!(err.to_string().contains("allowed-columns"));
}

#[test]
fn qglake_handoff_captured_output_semantics_rejects_empty_fetched_allowed_columns() {
    let temp = qglake_temp_dir("handoff-captured-scan-empty-fetched-allowed-columns");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    summary["lakecatReplayVerification"]["governedScanProof"]["fetchedReadRestriction"]["allowed-columns"] =
        json!([]);
    let mut drifted =
        read_json_file(&temp.join("lakecat-replay.txt")).expect("read LakeCat replay output");
    drifted["replay-evidence"]["scan"]["fetchedReadRestriction"]["allowed-columns"] = json!([]);
    let drifted_bytes = serde_json::to_vec_pretty(&drifted).expect("drifted JSON bytes");
    fs::write(temp.join("lakecat-replay.txt"), &drifted_bytes)
        .expect("write drifted LakeCat replay output");
    summary["artifacts"]["capturedOutputs"]["lakecatReplay"]["sha256"] =
        json!(content_hash_bytes(&drifted_bytes));

    let err = verify_qglake_handoff_captured_output_semantics(&summary_path, &summary)
        .expect_err("captured scan replay should reject empty fetched allowed columns");
    assert!(err.to_string().contains("captured LakeCat replay output"));
    assert!(err.to_string().contains("fetchedReadRestriction"));
    assert!(err.to_string().contains("allowed-columns"));
}

#[test]
fn qglake_handoff_captured_output_semantics_rejects_extra_planned_restriction_fields() {
    let temp = qglake_temp_dir("handoff-captured-scan-extra-planned-restriction");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    let mut drifted =
        read_json_file(&temp.join("lakecat-replay.txt")).expect("read LakeCat replay output");
    drifted["replay-evidence"]["scan"]["plannedReadRestriction"]["unverifiedRestrictionClaim"] =
        json!(qglake_fixture_hash("unverified-captured-restriction-claim"));
    let drifted_bytes = serde_json::to_vec_pretty(&drifted).expect("drifted JSON bytes");
    fs::write(temp.join("lakecat-replay.txt"), &drifted_bytes)
        .expect("write drifted LakeCat replay output");
    summary["artifacts"]["capturedOutputs"]["lakecatReplay"]["sha256"] =
        json!(content_hash_bytes(&drifted_bytes));

    let err = verify_qglake_handoff_captured_output_semantics(&summary_path, &summary)
        .expect_err("captured replay scan restriction should reject extra fields");
    let err = err.to_string();

    assert!(
        err.contains("captured LakeCat replay output.replay-evidence.scan.plannedReadRestriction"),
        "{err}"
    );
    assert!(
        err.contains("unexpected field unverifiedRestrictionClaim"),
        "{err}"
    );
}

#[test]
fn qglake_handoff_captured_output_semantics_rejects_scan_projection_drift() {
    let temp = qglake_temp_dir("handoff-captured-scan-projection-drift");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    let mut drifted =
        read_json_file(&temp.join("lakecat-replay.txt")).expect("read LakeCat replay output");
    drifted["replay-evidence"]["scan"]["fetchedRequiredProjection"] =
        json!(["event_id", "occurred_at", "severity", "raw_payload"]);
    let drifted_bytes = serde_json::to_vec_pretty(&drifted).expect("drifted JSON bytes");
    fs::write(temp.join("lakecat-replay.txt"), &drifted_bytes)
        .expect("write drifted LakeCat replay output");
    summary["artifacts"]["capturedOutputs"]["lakecatReplay"]["sha256"] =
        json!(content_hash_bytes(&drifted_bytes));

    let err = verify_qglake_handoff_captured_output_semantics(&summary_path, &summary)
        .expect_err("captured replay scan projection proof drift should be rejected");
    assert!(err.to_string().contains(
        "captured LakeCat replay output.replay-evidence.scan.fetchedRequiredProjection mismatch"
    ));
}

#[test]
fn qglake_handoff_captured_output_semantics_rejects_scan_stats_field_drift() {
    let temp = qglake_temp_dir("handoff-captured-scan-stats-field-drift");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    let mut drifted =
        read_json_file(&temp.join("lakecat-replay.txt")).expect("read LakeCat replay output");
    drifted["replay-evidence"]["scan"]["plannedEffectiveStatsFields"] =
        json!(["event_id", "occurred_at", "severity", "raw_payload"]);
    let drifted_bytes = serde_json::to_vec_pretty(&drifted).expect("drifted JSON bytes");
    fs::write(temp.join("lakecat-replay.txt"), &drifted_bytes)
        .expect("write drifted LakeCat replay output");
    summary["artifacts"]["capturedOutputs"]["lakecatReplay"]["sha256"] =
        json!(content_hash_bytes(&drifted_bytes));

    let err = verify_qglake_handoff_captured_output_semantics(&summary_path, &summary)
        .expect_err("captured replay scan stats-field proof drift should be rejected");
    assert!(err.to_string().contains(
        "captured LakeCat replay output.replay-evidence.scan.plannedEffectiveStatsFields mismatch"
    ));
}

#[test]
fn qglake_handoff_captured_output_semantics_rejects_fetched_scan_stats_field_drift() {
    let temp = qglake_temp_dir("handoff-captured-fetched-scan-stats-field-drift");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    let mut drifted =
        read_json_file(&temp.join("lakecat-replay.txt")).expect("read LakeCat replay output");
    drifted["replay-evidence"]["scan"]["fetchedEffectiveStatsFields"] =
        json!(["event_id", "occurred_at", "severity", "raw_payload"]);
    let drifted_bytes = serde_json::to_vec_pretty(&drifted).expect("drifted JSON bytes");
    fs::write(temp.join("lakecat-replay.txt"), &drifted_bytes)
        .expect("write drifted LakeCat replay output");
    summary["artifacts"]["capturedOutputs"]["lakecatReplay"]["sha256"] =
        json!(content_hash_bytes(&drifted_bytes));

    let err = verify_qglake_handoff_captured_output_semantics(&summary_path, &summary)
        .expect_err("captured replay fetched scan stats-field proof drift should be rejected");
    assert!(err.to_string().contains(
        "captured LakeCat replay output.replay-evidence.scan.fetchedEffectiveStatsFields mismatch"
    ));
}

#[test]
fn qglake_handoff_captured_output_semantics_rejects_scan_filter_drift() {
    let temp = qglake_temp_dir("handoff-captured-scan-filter-drift");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    let mut drifted =
        read_json_file(&temp.join("lakecat-replay.txt")).expect("read LakeCat replay output");
    drifted["replay-evidence"]["scan"]["fetchedRequiredFilters"] = json!([{
        "type": "not-eq",
        "term": "severity",
        "value": "trace"
    }]);
    let drifted_bytes = serde_json::to_vec_pretty(&drifted).expect("drifted JSON bytes");
    fs::write(temp.join("lakecat-replay.txt"), &drifted_bytes)
        .expect("write drifted LakeCat replay output");
    summary["artifacts"]["capturedOutputs"]["lakecatReplay"]["sha256"] =
        json!(content_hash_bytes(&drifted_bytes));

    let err = verify_qglake_handoff_captured_output_semantics(&summary_path, &summary)
        .expect_err("captured replay scan filter proof drift should be rejected");
    assert!(err.to_string().contains(
        "captured LakeCat replay output.replay-evidence.scan.fetchedRequiredFilters mismatch"
    ));
}

#[test]
fn qglake_handoff_captured_output_semantics_rejects_table_commit_history_drift() {
    let temp = qglake_temp_dir("handoff-captured-table-commit-history-drift");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    let mut drifted =
        read_json_file(&temp.join("lakecat-replay.txt")).expect("read LakeCat replay output");
    drifted["replay-evidence"]["tableCommitHistory"]["commitCount"] = json!(2);
    let drifted_bytes = serde_json::to_vec_pretty(&drifted).expect("drifted JSON bytes");
    fs::write(temp.join("lakecat-replay.txt"), &drifted_bytes)
        .expect("write drifted LakeCat replay output");
    summary["artifacts"]["capturedOutputs"]["lakecatReplay"]["sha256"] =
        json!(content_hash_bytes(&drifted_bytes));

    let err = verify_qglake_handoff_captured_output_semantics(&summary_path, &summary)
        .expect_err("captured replay commit-history proof drift should be rejected");
    assert!(err.to_string().contains(
        "captured LakeCat replay output.replay-evidence.tableCommitHistory.commitCount mismatch"
    ));
}

#[test]
fn qglake_handoff_captured_output_semantics_rejects_extra_commit_history_fields() {
    let temp = qglake_temp_dir("handoff-captured-table-commit-history-extra-field");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    let mut drifted =
        read_json_file(&temp.join("lakecat-replay.txt")).expect("read LakeCat replay output");
    drifted["replay-evidence"]["tableCommitHistory"]["unverifiedCommitClaim"] =
        json!(qglake_fixture_hash("unverified-captured-commit-claim"));
    let drifted_bytes = serde_json::to_vec_pretty(&drifted).expect("drifted JSON bytes");
    fs::write(temp.join("lakecat-replay.txt"), &drifted_bytes)
        .expect("write drifted LakeCat replay output");
    summary["artifacts"]["capturedOutputs"]["lakecatReplay"]["sha256"] =
        json!(content_hash_bytes(&drifted_bytes));

    let err = verify_qglake_handoff_captured_output_semantics(&summary_path, &summary)
        .expect_err("captured replay commit-history extra fields should be rejected");
    let err = err.to_string();

    assert!(
        err.contains("captured LakeCat replay output.replay-evidence.tableCommitHistory"),
        "{err}"
    );
    assert!(
        err.contains("unexpected field unverifiedCommitClaim"),
        "{err}"
    );
}

#[test]
fn qglake_handoff_captured_output_semantics_rejects_commit_history_principal_drift() {
    let temp = qglake_temp_dir("handoff-captured-table-commit-history-principal-drift");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    let mut drifted =
        read_json_file(&temp.join("lakecat-replay.txt")).expect("read LakeCat replay output");
    drifted["replay-evidence"]["tableCommitHistory"]["principalSubject"] =
        json!("did:example:other");
    let drifted_bytes = serde_json::to_vec_pretty(&drifted).expect("drifted JSON bytes");
    fs::write(temp.join("lakecat-replay.txt"), &drifted_bytes)
        .expect("write drifted LakeCat replay output");
    summary["artifacts"]["capturedOutputs"]["lakecatReplay"]["sha256"] =
        json!(content_hash_bytes(&drifted_bytes));

    let err = verify_qglake_handoff_captured_output_semantics(&summary_path, &summary)
        .expect_err("captured replay commit-history principal drift should be rejected");
    assert!(err.to_string().contains(
        "captured LakeCat replay output.replay-evidence.tableCommitHistory.principalSubject mismatch"
    ));
}

#[test]
fn qglake_handoff_captured_output_semantics_rejects_commit_history_action_drift() {
    let temp = qglake_temp_dir("handoff-captured-table-commit-history-action-drift");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    let mut drifted =
        read_json_file(&temp.join("lakecat-replay.txt")).expect("read LakeCat replay output");
    drifted["replay-evidence"]["tableCommitHistory"]["authorizationReceiptAction"] =
        json!("table-commit");
    let drifted_bytes = serde_json::to_vec_pretty(&drifted).expect("drifted JSON bytes");
    fs::write(temp.join("lakecat-replay.txt"), &drifted_bytes)
        .expect("write drifted LakeCat replay output");
    summary["artifacts"]["capturedOutputs"]["lakecatReplay"]["sha256"] =
        json!(content_hash_bytes(&drifted_bytes));

    let err = verify_qglake_handoff_captured_output_semantics(&summary_path, &summary)
        .expect_err("captured replay commit-history action drift should be rejected");
    assert!(err.to_string().contains(
        "captured LakeCat replay output.replay-evidence.tableCommitHistory.authorizationReceiptAction mismatch"
    ));
}

#[test]
fn qglake_handoff_captured_output_semantics_rejects_zero_commit_history_sequence() {
    let temp = qglake_temp_dir("handoff-captured-table-commit-history-zero-sequence");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    summary["lakecatReplayVerification"]["tableCommitHistoryProof"]["sequenceNumbers"] = json!([0]);
    let mut drifted =
        read_json_file(&temp.join("lakecat-replay.txt")).expect("read LakeCat replay output");
    drifted["replay-evidence"]["tableCommitHistory"]["sequenceNumbers"] = json!([0]);
    let drifted_bytes = serde_json::to_vec_pretty(&drifted).expect("drifted JSON bytes");
    fs::write(temp.join("lakecat-replay.txt"), &drifted_bytes)
        .expect("write drifted LakeCat replay output");
    summary["artifacts"]["capturedOutputs"]["lakecatReplay"]["sha256"] =
        json!(content_hash_bytes(&drifted_bytes));
    let summary_bytes = serde_json::to_vec_pretty(&summary).expect("summary JSON bytes");
    fs::write(&summary_path, summary_bytes).expect("write drifted handoff summary");

    let err = verify_qglake_handoff_captured_output_semantics(&summary_path, &summary)
        .expect_err("captured replay commit-history proof should reject zero sequence");
    assert!(err.to_string().contains("tableCommitHistoryProof"));
    assert!(err.to_string().contains("sequenceNumbers"));
    assert!(err.to_string().contains("positive"));
}

#[test]
fn qglake_handoff_captured_output_semantics_rejects_non_increasing_commit_history_sequences() {
    let temp = qglake_temp_dir("handoff-captured-table-commit-history-sequence-drift");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    summary["lakecatReplayVerification"]["tableCommitHistoryProof"]["commitCount"] = json!(2);
    summary["lakecatReplayVerification"]["tableCommitHistoryProof"]["sequenceNumbers"] =
        json!([1, 1]);
    summary["lakecatReplayVerification"]["tableCommitHistoryProof"]["commitHashes"] = json!([
        qglake_fixture_hash("commit-history-sequence-drift-a"),
        qglake_fixture_hash("commit-history-sequence-drift-b")
    ]);
    let mut drifted =
        read_json_file(&temp.join("lakecat-replay.txt")).expect("read LakeCat replay output");
    drifted["replay-evidence"]["tableCommitHistory"]["commitCount"] = json!(2);
    drifted["replay-evidence"]["tableCommitHistory"]["sequenceNumbers"] = json!([1, 1]);
    drifted["replay-evidence"]["tableCommitHistory"]["commitHashes"] = json!([
        qglake_fixture_hash("commit-history-sequence-drift-a"),
        qglake_fixture_hash("commit-history-sequence-drift-b")
    ]);
    let drifted_bytes = serde_json::to_vec_pretty(&drifted).expect("drifted JSON bytes");
    fs::write(temp.join("lakecat-replay.txt"), &drifted_bytes)
        .expect("write drifted LakeCat replay output");
    summary["artifacts"]["capturedOutputs"]["lakecatReplay"]["sha256"] =
        json!(content_hash_bytes(&drifted_bytes));
    let summary_bytes = serde_json::to_vec_pretty(&summary).expect("summary JSON bytes");
    fs::write(&summary_path, summary_bytes).expect("write drifted handoff summary");

    let err = verify_qglake_handoff_captured_output_semantics(&summary_path, &summary)
        .expect_err("captured replay commit-history proof should reject non-increasing sequences");
    assert!(err.to_string().contains("tableCommitHistoryProof"));
    assert!(err.to_string().contains("sequenceNumbers"));
    assert!(err.to_string().contains("strictly increasing"));
}

#[test]
fn qglake_handoff_captured_output_semantics_rejects_table_commit_history_replay_line_drift() {
    let temp = qglake_temp_dir("handoff-captured-table-commit-history-replay-line-drift");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    let mut drifted =
        read_json_file(&temp.join("lakecat-replay.txt")).expect("read LakeCat replay output");
    drifted["table-commit-history-replay"] = json!(
        "table commit history commits=1 sequences=2 hashes=sha256:other-commit graph_events=1"
    );
    let drifted_bytes = serde_json::to_vec_pretty(&drifted).expect("drifted JSON bytes");
    fs::write(temp.join("lakecat-replay.txt"), &drifted_bytes)
        .expect("write drifted LakeCat replay output");
    summary["artifacts"]["capturedOutputs"]["lakecatReplay"]["sha256"] =
        json!(content_hash_bytes(&drifted_bytes));

    let err = verify_qglake_handoff_captured_output_semantics(&summary_path, &summary)
        .expect_err("captured replay commit-history replay line drift should be rejected");
    assert!(
        err.to_string()
            .contains("captured LakeCat replay output.table-commit-history-replay mismatch")
    );
}

#[test]
fn qglake_handoff_captured_output_semantics_rejects_view_receipt_chain_drift() {
    let temp = qglake_temp_dir("handoff-captured-view-receipt-chain-drift");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    let mut drifted =
        read_json_file(&temp.join("lakecat-replay.txt")).expect("read LakeCat replay output");
    drifted["replay-evidence"]["views"]["views"][0]["acceptedReceiptHash"] =
        json!("sha256:other-view-receipt");
    let drifted_bytes = serde_json::to_vec_pretty(&drifted).expect("drifted JSON bytes");
    fs::write(temp.join("lakecat-replay.txt"), &drifted_bytes)
        .expect("write drifted LakeCat replay output");
    summary["artifacts"]["capturedOutputs"]["lakecatReplay"]["sha256"] =
        json!(content_hash_bytes(&drifted_bytes));

    let err = verify_qglake_handoff_captured_output_semantics(&summary_path, &summary)
        .expect_err("captured replay view receipt-chain proof drift should be rejected");
    assert!(
        err.to_string()
            .contains("captured LakeCat replay output.replay-evidence.views.views mismatch")
    );
}

#[test]
fn qglake_handoff_captured_output_semantics_rejects_extra_view_receipt_chain_fields() {
    let temp = qglake_temp_dir("handoff-captured-view-extra-field");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    let mut drifted =
        read_json_file(&temp.join("lakecat-replay.txt")).expect("read LakeCat replay output");
    drifted["replay-evidence"]["views"]["unverifiedViewClaim"] =
        json!(qglake_fixture_hash("unverified-view-claim"));
    let drifted_bytes = serde_json::to_vec_pretty(&drifted).expect("drifted JSON bytes");
    fs::write(temp.join("lakecat-replay.txt"), &drifted_bytes)
        .expect("write drifted LakeCat replay output");
    summary["artifacts"]["capturedOutputs"]["lakecatReplay"]["sha256"] =
        json!(content_hash_bytes(&drifted_bytes));

    let err = verify_qglake_handoff_captured_output_semantics(&summary_path, &summary)
        .expect_err("captured replay view proof should reject extra fields");
    let err = err.to_string();

    assert!(
        err.contains("captured LakeCat replay output.replay-evidence.views"),
        "{err}"
    );
    assert!(
        err.contains("unexpected field unverifiedViewClaim"),
        "{err}"
    );
}

#[test]
fn qglake_handoff_captured_output_semantics_rejects_extra_view_receipt_fields() {
    let temp = qglake_temp_dir("handoff-captured-view-receipt-extra-field");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    let mut drifted =
        read_json_file(&temp.join("lakecat-replay.txt")).expect("read LakeCat replay output");
    drifted["replay-evidence"]["views"]["receiptChains"][0]["chains"][0]["receipts"][0]["unverifiedReceiptClaim"] =
        json!(qglake_fixture_hash("unverified-receipt-claim"));
    let drifted_bytes = serde_json::to_vec_pretty(&drifted).expect("drifted JSON bytes");
    fs::write(temp.join("lakecat-replay.txt"), &drifted_bytes)
        .expect("write drifted LakeCat replay output");
    summary["artifacts"]["capturedOutputs"]["lakecatReplay"]["sha256"] =
        json!(content_hash_bytes(&drifted_bytes));

    let err = verify_qglake_handoff_captured_output_semantics(&summary_path, &summary)
        .expect_err("captured replay view receipt should reject extra fields");
    let err = err.to_string();

    assert!(
        err.contains(
            "captured LakeCat replay output.replay-evidence.views.receiptChains[].chains[].receipts[]"
        ),
        "{err}"
    );
    assert!(
        err.contains("unexpected field unverifiedReceiptClaim"),
        "{err}"
    );
}

#[test]
fn qglake_handoff_captured_output_semantics_rejects_request_identity_drift() {
    let temp = qglake_temp_dir("handoff-captured-request-identity-drift");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    let mut drifted =
        read_json_file(&temp.join("lakecat-replay.txt")).expect("read LakeCat replay output");
    drifted["replay-evidence"]["requestIdentity"]["authorizationReceiptHash"] =
        json!("sha256:other-identity");
    let drifted_bytes = serde_json::to_vec_pretty(&drifted).expect("drifted JSON bytes");
    fs::write(temp.join("lakecat-replay.txt"), &drifted_bytes)
        .expect("write drifted LakeCat replay output");
    summary["artifacts"]["capturedOutputs"]["lakecatReplay"]["sha256"] =
        json!(content_hash_bytes(&drifted_bytes));

    let err = verify_qglake_handoff_captured_output_semantics(&summary_path, &summary)
        .expect_err("captured replay request-identity proof drift should be rejected");
    assert!(
        err.to_string().contains(
            "captured LakeCat replay output.replay-evidence.requestIdentity.authorizationReceiptHash mismatch"
        )
    );
}

#[test]
fn qglake_handoff_captured_output_semantics_rejects_extra_request_identity_fields() {
    let temp = qglake_temp_dir("handoff-captured-request-identity-extra-field");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    let mut drifted =
        read_json_file(&temp.join("lakecat-replay.txt")).expect("read LakeCat replay output");
    drifted["replay-evidence"]["requestIdentity"]["unverifiedActorClaim"] =
        json!(qglake_fixture_hash("unverified-captured-actor-claim"));
    let drifted_bytes = serde_json::to_vec_pretty(&drifted).expect("drifted JSON bytes");
    fs::write(temp.join("lakecat-replay.txt"), &drifted_bytes)
        .expect("write drifted LakeCat replay output");
    summary["artifacts"]["capturedOutputs"]["lakecatReplay"]["sha256"] =
        json!(content_hash_bytes(&drifted_bytes));

    let err = verify_qglake_handoff_captured_output_semantics(&summary_path, &summary)
        .expect_err("captured replay request-identity extra fields should be rejected");
    let err = err.to_string();

    assert!(
        err.contains("captured LakeCat replay output.replay-evidence.requestIdentity"),
        "{err}"
    );
    assert!(
        err.contains("unexpected field unverifiedActorClaim"),
        "{err}"
    );
}

#[test]
fn qglake_handoff_captured_output_semantics_rejects_request_identity_action_drift() {
    let temp = qglake_temp_dir("handoff-captured-request-identity-action-drift");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    let mut drifted =
        read_json_file(&temp.join("lakecat-replay.txt")).expect("read LakeCat replay output");
    drifted["replay-evidence"]["requestIdentity"]["authorizationReceiptAction"] =
        json!("graph-read");
    let drifted_bytes = serde_json::to_vec_pretty(&drifted).expect("drifted JSON bytes");
    fs::write(temp.join("lakecat-replay.txt"), &drifted_bytes)
        .expect("write drifted LakeCat replay output");
    summary["artifacts"]["capturedOutputs"]["lakecatReplay"]["sha256"] =
        json!(content_hash_bytes(&drifted_bytes));

    let err = verify_qglake_handoff_captured_output_semantics(&summary_path, &summary)
        .expect_err("captured replay request-identity action drift should be rejected");
    assert!(
        err.to_string().contains(
            "captured LakeCat replay output.replay-evidence.requestIdentity.authorizationReceiptAction mismatch"
        )
    );
}

#[test]
fn qglake_handoff_captured_output_semantics_rejects_querygraph_bootstrap_drift() {
    let temp = qglake_temp_dir("handoff-captured-querygraph-bootstrap-drift");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    let mut drifted =
        read_json_file(&temp.join("lakecat-replay.txt")).expect("read LakeCat replay output");
    drifted["replay-evidence"]["queryGraphBootstrap"]["agentDelegationHash"] =
        json!("sha256:other-delegation");
    let drifted_bytes = serde_json::to_vec_pretty(&drifted).expect("drifted JSON bytes");
    fs::write(temp.join("lakecat-replay.txt"), &drifted_bytes)
        .expect("write drifted LakeCat replay output");
    summary["artifacts"]["capturedOutputs"]["lakecatReplay"]["sha256"] =
        json!(content_hash_bytes(&drifted_bytes));

    let err = verify_qglake_handoff_captured_output_semantics(&summary_path, &summary)
        .expect_err("captured replay QueryGraph bootstrap proof drift should be rejected");
    assert!(
        err.to_string().contains(
            "captured LakeCat replay output.replay-evidence.queryGraphBootstrap.agentDelegationHash mismatch"
        )
    );
}

#[test]
fn qglake_handoff_captured_output_semantics_rejects_extra_querygraph_bootstrap_fields() {
    let temp = qglake_temp_dir("handoff-captured-querygraph-bootstrap-extra-field");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    let mut drifted =
        read_json_file(&temp.join("lakecat-replay.txt")).expect("read LakeCat replay output");
    drifted["replay-evidence"]["queryGraphBootstrap"]["unverifiedBootstrapClaim"] =
        json!(qglake_fixture_hash("unverified-captured-bootstrap-claim"));
    let drifted_bytes = serde_json::to_vec_pretty(&drifted).expect("drifted JSON bytes");
    fs::write(temp.join("lakecat-replay.txt"), &drifted_bytes)
        .expect("write drifted LakeCat replay output");
    summary["artifacts"]["capturedOutputs"]["lakecatReplay"]["sha256"] =
        json!(content_hash_bytes(&drifted_bytes));

    let err = verify_qglake_handoff_captured_output_semantics(&summary_path, &summary)
        .expect_err("captured replay QueryGraph bootstrap proof should reject extra fields");
    let err = err.to_string();

    assert!(
        err.contains("captured LakeCat replay output.replay-evidence.queryGraphBootstrap"),
        "{err}"
    );
    assert!(
        err.contains("unexpected field unverifiedBootstrapClaim"),
        "{err}"
    );
}

#[test]
fn qglake_handoff_captured_output_semantics_rejects_credential_drift() {
    let temp = qglake_temp_dir("handoff-captured-credential-drift");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    let mut drifted =
        read_json_file(&temp.join("lakecat-replay.txt")).expect("read LakeCat replay output");
    drifted["replay-evidence"]["credentials"]["restricted"]["blockReason"] =
        json!("raw credentials allowed");
    let drifted_bytes = serde_json::to_vec_pretty(&drifted).expect("drifted JSON bytes");
    fs::write(temp.join("lakecat-replay.txt"), &drifted_bytes)
        .expect("write drifted LakeCat replay output");
    summary["artifacts"]["capturedOutputs"]["lakecatReplay"]["sha256"] =
        json!(content_hash_bytes(&drifted_bytes));

    let err = verify_qglake_handoff_captured_output_semantics(&summary_path, &summary)
        .expect_err("captured replay credential proof drift should be rejected");
    assert!(err.to_string().contains(
        "captured LakeCat replay output.replay-evidence.credentials.restricted.blockReason mismatch"
    ));
}

#[test]
fn qglake_handoff_captured_output_semantics_rejects_extra_credential_fields() {
    let temp = qglake_temp_dir("handoff-captured-credential-extra-field");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    let mut drifted =
        read_json_file(&temp.join("lakecat-replay.txt")).expect("read LakeCat replay output");
    drifted["replay-evidence"]["credentials"]["unverifiedCredentialClaim"] =
        json!(qglake_fixture_hash("unverified-credential-claim"));
    let drifted_bytes = serde_json::to_vec_pretty(&drifted).expect("drifted JSON bytes");
    fs::write(temp.join("lakecat-replay.txt"), &drifted_bytes)
        .expect("write drifted LakeCat replay output");
    summary["artifacts"]["capturedOutputs"]["lakecatReplay"]["sha256"] =
        json!(content_hash_bytes(&drifted_bytes));

    let err = verify_qglake_handoff_captured_output_semantics(&summary_path, &summary)
        .expect_err("captured replay credential proof should reject extra fields");
    let err = err.to_string();

    assert!(
        err.contains("captured LakeCat replay output.replay-evidence.credentials"),
        "{err}"
    );
    assert!(
        err.contains("unexpected field unverifiedCredentialClaim"),
        "{err}"
    );
}

#[test]
fn qglake_handoff_captured_output_semantics_rejects_extra_credential_branch_fields() {
    let temp = qglake_temp_dir("handoff-captured-credential-branch-extra-field");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    let mut drifted =
        read_json_file(&temp.join("lakecat-replay.txt")).expect("read LakeCat replay output");
    drifted["replay-evidence"]["credentials"]["trustedHuman"]["unverifiedRawCredentialClaim"] =
        json!(qglake_fixture_hash("unverified-raw-credential-claim"));
    let drifted_bytes = serde_json::to_vec_pretty(&drifted).expect("drifted JSON bytes");
    fs::write(temp.join("lakecat-replay.txt"), &drifted_bytes)
        .expect("write drifted LakeCat replay output");
    summary["artifacts"]["capturedOutputs"]["lakecatReplay"]["sha256"] =
        json!(content_hash_bytes(&drifted_bytes));

    let err = verify_qglake_handoff_captured_output_semantics(&summary_path, &summary)
        .expect_err("captured replay credential branch should reject extra fields");
    let err = err.to_string();

    assert!(
        err.contains("captured LakeCat replay output.replay-evidence.credentials.trustedHuman"),
        "{err}"
    );
    assert!(
        err.contains("unexpected field unverifiedRawCredentialClaim"),
        "{err}"
    );
}

#[test]
fn qglake_handoff_captured_output_semantics_rejects_extra_credential_storage_profile_fields() {
    let temp = qglake_temp_dir("handoff-captured-credential-storage-profile-extra-field");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    let mut drifted =
        read_json_file(&temp.join("lakecat-replay.txt")).expect("read LakeCat replay output");
    drifted["replay-evidence"]["credentials"]["restricted"]["storageProfile"]["unverifiedStorageScopeClaim"] =
        json!(qglake_fixture_hash("unverified-storage-scope-claim"));
    let drifted_bytes = serde_json::to_vec_pretty(&drifted).expect("drifted JSON bytes");
    fs::write(temp.join("lakecat-replay.txt"), &drifted_bytes)
        .expect("write drifted LakeCat replay output");
    summary["artifacts"]["capturedOutputs"]["lakecatReplay"]["sha256"] =
        json!(content_hash_bytes(&drifted_bytes));

    let err = verify_qglake_handoff_captured_output_semantics(&summary_path, &summary)
        .expect_err("captured replay credential storage profile should reject extra fields");
    let err = err.to_string();

    assert!(
        err.contains(
            "captured LakeCat replay output.replay-evidence.credentials.restricted.storageProfile"
        ),
        "{err}"
    );
    assert!(
        err.contains("unexpected field unverifiedStorageScopeClaim"),
        "{err}"
    );
}

#[test]
fn qglake_handoff_captured_output_semantics_rejects_credential_prefix_hash_drift() {
    let temp = qglake_temp_dir("handoff-captured-credential-prefix-drift");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    let mut drifted =
        read_json_file(&temp.join("lakecat-replay.txt")).expect("read LakeCat replay output");
    drifted["replay-evidence"]["credentials"]["trustedHuman"]["credentialPrefixHashes"] =
        json!([qglake_fixture_hash("other-human-credential-prefix")]);
    let drifted_bytes = serde_json::to_vec_pretty(&drifted).expect("drifted JSON bytes");
    fs::write(temp.join("lakecat-replay.txt"), &drifted_bytes)
        .expect("write drifted LakeCat replay output");
    summary["artifacts"]["capturedOutputs"]["lakecatReplay"]["sha256"] =
        json!(content_hash_bytes(&drifted_bytes));

    let err = verify_qglake_handoff_captured_output_semantics(&summary_path, &summary)
        .expect_err("captured replay credential prefix hash drift should be rejected");
    assert!(
        err.to_string()
            .contains("captured LakeCat replay output.replay-evidence.credentials")
    );
    assert!(err.to_string().contains("credentialPrefixHashes mismatch"));
}

#[test]
fn qglake_handoff_captured_output_semantics_rejects_credential_authorization_action_drift() {
    let temp = qglake_temp_dir("handoff-captured-credential-authorization-action-drift");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    let mut drifted =
        read_json_file(&temp.join("lakecat-replay.txt")).expect("read LakeCat replay output");
    drifted["replay-evidence"]["credentials"]["trustedHuman"]["authorizationReceiptAction"] =
        json!("table-load");
    let drifted_bytes = serde_json::to_vec_pretty(&drifted).expect("drifted JSON bytes");
    fs::write(temp.join("lakecat-replay.txt"), &drifted_bytes)
        .expect("write drifted LakeCat replay output");
    summary["artifacts"]["capturedOutputs"]["lakecatReplay"]["sha256"] =
        json!(content_hash_bytes(&drifted_bytes));

    let err = verify_qglake_handoff_captured_output_semantics(&summary_path, &summary)
        .expect_err("captured replay credential authorization action drift should be rejected");
    assert!(
        err.to_string()
            .contains("captured LakeCat replay output.replay-evidence.credentials")
    );
    assert!(
        err.to_string()
            .contains("authorizationReceiptAction mismatch")
    );
}

#[test]
fn qglake_handoff_captured_output_semantics_rejects_management_replay_line_drift() {
    let temp = qglake_temp_dir("handoff-captured-management-replay-line-drift");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    let mut drifted =
        read_json_file(&temp.join("lakecat-replay.txt")).expect("read LakeCat replay output");
    drifted["management-replay"] = json!(
        "management replay servers=1 projects=1 warehouses=1 policies=1 storage_profiles=1 storage_profile_upserts=1 credential_root=events-local:file:local-file-no-secret:location_prefix_hash=sha256:3333333333333333333333333333333333333333333333333333333333333333:secret_ref=none"
    );
    let drifted_bytes = serde_json::to_vec_pretty(&drifted).expect("drifted JSON bytes");
    fs::write(temp.join("lakecat-replay.txt"), &drifted_bytes)
        .expect("write drifted LakeCat replay output");
    summary["artifacts"]["capturedOutputs"]["lakecatReplay"]["sha256"] =
        json!(content_hash_bytes(&drifted_bytes));

    let err = verify_qglake_handoff_captured_output_semantics(&summary_path, &summary)
        .expect_err("captured replay management replay line drift should be rejected");
    assert!(
        err.to_string()
            .contains("captured LakeCat replay output.management-replay mismatch")
    );
}

#[test]
fn qglake_handoff_captured_output_semantics_rejects_policy_upsert_action_drift() {
    let temp = qglake_temp_dir("handoff-captured-policy-upsert-action-drift");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    let mut drifted =
        read_json_file(&temp.join("lakecat-replay.txt")).expect("read LakeCat replay output");
    drifted["replay-evidence"]["management"]["policyUpsertProof"]["authorizationReceiptAction"] =
        json!("table-load");
    let drifted_bytes = serde_json::to_vec_pretty(&drifted).expect("drifted JSON bytes");
    fs::write(temp.join("lakecat-replay.txt"), &drifted_bytes)
        .expect("write drifted LakeCat replay output");
    summary["artifacts"]["capturedOutputs"]["lakecatReplay"]["sha256"] =
        json!(content_hash_bytes(&drifted_bytes));

    let err = verify_qglake_handoff_captured_output_semantics(&summary_path, &summary)
        .expect_err("captured replay should reject policy upsert action drift");

    assert!(
        err.to_string()
            .contains("captured LakeCat replay output.replay-evidence.management")
    );
    assert!(err.to_string().contains("policyUpsertProof mismatch"));
}

#[test]
fn qglake_handoff_captured_output_semantics_rejects_credential_replay_line_drift() {
    let temp = qglake_temp_dir("handoff-captured-credential-replay-line-drift");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    let mut drifted =
        read_json_file(&temp.join("lakecat-replay.txt")).expect("read LakeCat replay output");
    drifted["credential-replay"] = json!(
        "credential replay restricted=blocked:sail-planned-read-required restricted_count=0 restricted_ttl=60 restricted_profile=events-local:file:local-file-no-secret:location_prefix_hash=sha256:2222222222222222222222222222222222222222222222222222222222222222:secret_ref=none:graph_events=2 human=allowed:trusted-human-audited-raw human_count=1 human_ttl=300 human_profile=events-local:file:local-file-no-secret:location_prefix_hash=sha256:2222222222222222222222222222222222222222222222222222222222222222:secret_ref=none:graph_events=2"
    );
    let drifted_bytes = serde_json::to_vec_pretty(&drifted).expect("drifted JSON bytes");
    fs::write(temp.join("lakecat-replay.txt"), &drifted_bytes)
        .expect("write drifted LakeCat replay output");
    summary["artifacts"]["capturedOutputs"]["lakecatReplay"]["sha256"] =
        json!(content_hash_bytes(&drifted_bytes));

    let err = verify_qglake_handoff_captured_output_semantics(&summary_path, &summary)
        .expect_err("captured replay credential replay line drift should be rejected");
    assert!(
        err.to_string()
            .contains("captured LakeCat replay output.credential-replay mismatch")
    );
}

#[test]
fn qglake_handoff_captured_output_semantics_rejects_restricted_exception_drift() {
    let temp = qglake_temp_dir("handoff-captured-restricted-exception-drift");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    let mut drifted =
        read_json_file(&temp.join("lakecat-replay.txt")).expect("read LakeCat replay output");
    drifted["replay-evidence"]["credentials"]["restricted"]["rawCredentialExceptionAllowed"] =
        json!(true);
    let drifted_bytes = serde_json::to_vec_pretty(&drifted).expect("drifted JSON bytes");
    fs::write(temp.join("lakecat-replay.txt"), &drifted_bytes)
        .expect("write drifted LakeCat replay output");
    summary["artifacts"]["capturedOutputs"]["lakecatReplay"]["sha256"] =
        json!(content_hash_bytes(&drifted_bytes));

    let err = verify_qglake_handoff_captured_output_semantics(&summary_path, &summary)
        .expect_err("captured replay restricted exception drift should be rejected");
    assert!(
        err.to_string().contains(
            "captured LakeCat replay output.replay-evidence.credentials.restricted.rawCredentialExceptionAllowed mismatch"
        )
    );
}

#[test]
fn qglake_handoff_captured_output_semantics_rejects_trusted_block_reason_drift() {
    let temp = qglake_temp_dir("handoff-captured-trusted-block-reason-drift");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    let mut drifted =
        read_json_file(&temp.join("lakecat-replay.txt")).expect("read LakeCat replay output");
    drifted["replay-evidence"]["credentials"]["trustedHuman"]["blockReason"] = json!("blocked");
    let drifted_bytes = serde_json::to_vec_pretty(&drifted).expect("drifted JSON bytes");
    fs::write(temp.join("lakecat-replay.txt"), &drifted_bytes)
        .expect("write drifted LakeCat replay output");
    summary["artifacts"]["capturedOutputs"]["lakecatReplay"]["sha256"] =
        json!(content_hash_bytes(&drifted_bytes));

    let err = verify_qglake_handoff_captured_output_semantics(&summary_path, &summary)
        .expect_err("captured replay trusted-human block reason drift should be rejected");
    assert!(
        err.to_string().contains(
            "captured LakeCat replay output.replay-evidence.credentials.trustedHuman.blockReason mismatch"
        )
    );
}
