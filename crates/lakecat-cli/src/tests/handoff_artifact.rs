use super::common::*;
use crate::*;

#[test]
fn qglake_handoff_artifact_verifier_accepts_matching_files() {
    let temp = qglake_temp_dir("handoff-artifacts-ok");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    qglake_bind_handoff_verify_output_artifact(&temp, &mut summary);
    fs::write(
        &summary_path,
        serde_json::to_vec_pretty(&summary).expect("summary JSON"),
    )
    .expect("write summary");

    let verification = verify_qglake_handoff_artifact_files(&summary_path, &summary)
        .expect("artifact hashes should verify");
    assert_eq!(
        verification["bundle"]["sha256"],
        json!(content_hash_bytes(b"bundle"))
    );
    assert_eq!(
        verification["capturedOutputs"]["querygraphVerify"]["sha256"],
        summary["artifacts"]["capturedOutputs"]["querygraphVerify"]["sha256"]
    );
    assert_eq!(
        verification["pathAliases"]["querygraphVerifyOutput"],
        json!(
            fs::canonicalize(
                summary["artifacts"]["querygraphVerifyOutput"]
                    .as_str()
                    .unwrap()
            )
            .expect("canonical QueryGraph verify output")
        )
    );
    assert_eq!(
        verification["pathAliases"]["serviceLog"],
        json!(
            fs::canonicalize(summary["artifacts"]["serviceLog"].as_str().unwrap())
                .expect("canonical service log")
        )
    );
}

#[test]
fn qglake_handoff_artifact_verifier_rejects_artifact_path_outside_summary_dir() {
    let temp = qglake_temp_dir("handoff-artifacts-path-outside");
    let outside = qglake_temp_dir("handoff-artifacts-path-outside-splice");
    let outside_bundle = outside.join("bundle");
    fs::write(&outside_bundle, b"bundle").expect("write outside bundle");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    summary["artifacts"]["bundle"]["path"] = json!(outside_bundle);

    let err = verify_qglake_handoff_artifact_files(&summary_path, &summary)
        .expect_err("handoff verifier should reject artifacts outside summary directory");
    let err = err.to_string();

    assert!(err.contains("bundle"), "{err}");
    assert!(err.contains("summary directory"), "{err}");
}

#[test]
fn qglake_handoff_artifact_verifier_rejects_relative_artifact_path_traversal() {
    let temp = qglake_temp_dir("handoff-artifacts-relative-path-traversal");
    let outside = qglake_temp_dir("handoff-artifacts-relative-path-splice");
    let outside_bundle = outside.join("bundle");
    fs::write(&outside_bundle, b"bundle").expect("write outside bundle");
    let outside_name = outside
        .file_name()
        .and_then(|name| name.to_str())
        .expect("outside temp dir name");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    summary["artifacts"]["bundle"]["path"] = json!(format!("../{outside_name}/bundle"));

    let err = verify_qglake_handoff_artifact_files(&summary_path, &summary)
        .expect_err("handoff verifier should reject relative artifact path traversal");
    let err = err.to_string();

    assert!(err.contains("bundle"), "{err}");
    assert!(err.contains("summary directory"), "{err}");
}

#[test]
fn qglake_handoff_artifact_verifier_rejects_short_artifact_hashes() {
    let temp = qglake_temp_dir("handoff-artifacts-short-hash");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    summary["artifacts"]["bundle"]["sha256"] = json!("sha256:bundle");

    let err = verify_qglake_handoff_artifact_files(&summary_path, &summary)
        .expect_err("artifact verifier should reject short artifact hashes");

    assert!(err.to_string().contains("bundle"));
    assert!(err.to_string().contains("sha256"));
    assert!(err.to_string().contains("full SHA-256"));
}

#[test]
fn qglake_handoff_artifact_verifier_rejects_short_non_bundle_summary_artifact_hashes() {
    for (artifact, short_hash) in [
        ("lineageDrain", "sha256:lineage-drain"),
        ("querygraphImportPlan", "sha256:querygraph-import-plan"),
    ] {
        let temp = qglake_temp_dir(&format!("handoff-artifacts-short-{artifact}-hash"));
        let summary_path = temp.join("handoff-summary.json");
        let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
        summary["artifacts"][artifact]["sha256"] = json!(short_hash);

        let err = verify_qglake_handoff_artifact_files(&summary_path, &summary)
            .expect_err("artifact verifier should reject short non-bundle artifact hashes");
        let err = err.to_string();

        assert!(err.contains(artifact), "{err}");
        assert!(err.contains("full SHA-256"), "{err}");
    }
}

#[test]
fn qglake_handoff_artifact_verifier_rejects_short_service_log_hash() {
    let temp = qglake_temp_dir("handoff-artifacts-short-service-log-hash");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    summary["artifacts"]["serviceLogHash"] = json!("sha256:service-log");
    qglake_bind_handoff_verify_output_artifact(&temp, &mut summary);

    let err = verify_qglake_handoff_artifact_files(&summary_path, &summary)
        .expect_err("artifact verifier should reject short service-log hashes");

    assert!(err.to_string().contains("serviceLogHash"));
    assert!(err.to_string().contains("full SHA-256"));
}

#[test]
fn qglake_handoff_artifact_verifier_accepts_handoff_verify_output_hash() {
    let temp = qglake_temp_dir("handoff-artifacts-self-verify-ok");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    qglake_bind_handoff_verify_output_artifact(&temp, &mut summary);
    fs::write(
        &summary_path,
        serde_json::to_vec_pretty(&summary).expect("summary JSON"),
    )
    .expect("write summary");

    let verification = verify_qglake_handoff_artifact_files(&summary_path, &summary)
        .expect("artifact hashes should verify with handoff verifier output");

    assert_eq!(
        verification["pathAliases"]["lakecatHandoffVerifyOutputHash"],
        summary["artifacts"]["lakecatHandoffVerifyOutputHash"]
    );
}

#[test]
fn qglake_handoff_artifact_verifier_requires_handoff_verify_output_hash() {
    let temp = qglake_temp_dir("handoff-artifacts-missing-self-verify-hash");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    qglake_bind_handoff_verify_output_artifact(&temp, &mut summary);
    summary["artifacts"]
        .as_object_mut()
        .expect("artifacts object")
        .remove("lakecatHandoffVerifyOutputHash");

    let err = verify_qglake_handoff_artifact_files(&summary_path, &summary)
        .expect_err("artifact verifier should require handoff verifier hashes");

    assert!(err.to_string().contains("lakecatHandoffVerifyOutputHash"));
    assert!(err.to_string().contains("required"));
}

#[test]
fn qglake_handoff_artifact_verifier_rejects_null_handoff_verify_output_hash() {
    let temp = qglake_temp_dir("handoff-artifacts-null-self-verify-hash");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    qglake_bind_handoff_verify_output_artifact(&temp, &mut summary);
    summary["artifacts"]["lakecatHandoffVerifyOutputHash"] = Value::Null;

    let err = verify_qglake_handoff_artifact_files(&summary_path, &summary)
        .expect_err("artifact verifier should reject null handoff verifier hashes");

    assert!(err.to_string().contains("lakecatHandoffVerifyOutputHash"));
    assert!(err.to_string().contains("full SHA-256"));
}

#[test]
fn qglake_handoff_artifact_verifier_rejects_short_handoff_verify_output_hash() {
    let temp = qglake_temp_dir("handoff-artifacts-short-self-verify-hash");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    qglake_bind_handoff_verify_output_artifact(&temp, &mut summary);
    summary["artifacts"]["lakecatHandoffVerifyOutputHash"] = json!("sha256:handoff-verify-output");

    let err = verify_qglake_handoff_artifact_files(&summary_path, &summary)
        .expect_err("artifact verifier should reject short handoff verifier hashes");

    assert!(err.to_string().contains("lakecatHandoffVerifyOutputHash"));
    assert!(err.to_string().contains("full SHA-256"));
}

#[test]
fn qglake_handoff_artifact_verifier_rejects_handoff_verify_output_hash_drift() {
    let temp = qglake_temp_dir("handoff-artifacts-self-verify-hash-drift");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    qglake_bind_handoff_verify_output_artifact(&temp, &mut summary);
    fs::write(
        temp.join("lakecat-handoff-verify.json"),
        b"tampered verification",
    )
    .expect("tamper handoff verify output");
    fs::write(
        &summary_path,
        serde_json::to_vec_pretty(&summary).expect("summary JSON"),
    )
    .expect("write summary");

    let err = verify_qglake_handoff_artifact_files(&summary_path, &summary)
        .expect_err("artifact verifier should reject handoff verifier output hash drift");

    assert!(err.to_string().contains("lakecatHandoffVerifyOutput"));
    assert!(err.to_string().contains("hash mismatch"));
}

#[test]
fn qglake_handoff_artifact_verifier_rejects_handoff_verify_output_extra_top_level_proof() {
    let temp = qglake_temp_dir("handoff-artifacts-self-verify-extra-root-proof");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    let mut output = qglake_bind_handoff_verify_output_artifact(&temp, &mut summary);
    output["unverifiedProof"] = json!({
        "sha256": qglake_fixture_hash("unverified-proof")
    });
    let bytes = serde_json::to_vec_pretty(&output).expect("drifted handoff verify JSON");
    fs::write(temp.join("lakecat-handoff-verify.json"), &bytes)
        .expect("write drifted handoff verify output");
    summary["artifacts"]["lakecatHandoffVerifyOutputHash"] = json!(content_hash_bytes(&bytes));
    fs::write(
        &summary_path,
        serde_json::to_vec_pretty(&summary).expect("summary JSON"),
    )
    .expect("write summary");

    let err = verify_qglake_handoff_artifact_files(&summary_path, &summary)
        .expect_err("artifact verifier should reject extra sidecar root proof claims");
    let err = err.to_string();

    assert!(err.contains("lakecatHandoffVerifyOutput"), "{err}");
    assert!(err.contains("unexpected field unverifiedProof"), "{err}");
}

#[test]
fn qglake_handoff_artifact_verifier_rejects_handoff_verify_output_scope_drift() {
    let temp = qglake_temp_dir("handoff-artifacts-self-verify-scope-drift");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    let mut output = qglake_bind_handoff_verify_output_artifact(&temp, &mut summary);
    output["table"] = json!("other_events");
    let bytes = serde_json::to_vec_pretty(&output).expect("drifted handoff verify JSON");
    fs::write(temp.join("lakecat-handoff-verify.json"), &bytes)
        .expect("write drifted handoff verify output");
    summary["artifacts"]["lakecatHandoffVerifyOutputHash"] = json!(content_hash_bytes(&bytes));
    fs::write(
        &summary_path,
        serde_json::to_vec_pretty(&summary).expect("summary JSON"),
    )
    .expect("write summary");

    let err = verify_qglake_handoff_artifact_files(&summary_path, &summary)
        .expect_err("artifact verifier should reject handoff verifier output scope drift");

    assert!(err.to_string().contains("lakecatHandoffVerifyOutput"));
    assert!(err.to_string().contains("table mismatch"));
}

#[test]
fn qglake_handoff_artifact_verifier_rejects_handoff_verify_output_graph_projection_drift() {
    let temp = qglake_temp_dir("handoff-artifacts-self-verify-graph-projection-drift");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    let mut output = qglake_bind_handoff_verify_output_artifact(&temp, &mut summary);
    output["graphProjectionProof"]["backend"] = json!("grust-memory");
    let bytes = serde_json::to_vec_pretty(&output).expect("drifted handoff verify JSON");
    fs::write(temp.join("lakecat-handoff-verify.json"), &bytes)
        .expect("write drifted handoff verify output");
    summary["artifacts"]["lakecatHandoffVerifyOutputHash"] = json!(content_hash_bytes(&bytes));
    fs::write(
        &summary_path,
        serde_json::to_vec_pretty(&summary).expect("summary JSON"),
    )
    .expect("write summary");

    let err = verify_qglake_handoff_artifact_files(&summary_path, &summary)
        .expect_err("artifact verifier should reject handoff verifier graph proof drift");

    assert!(err.to_string().contains("lakecatHandoffVerifyOutput"));
    assert!(err.to_string().contains("graphProjectionProof mismatch"));
}

#[test]
fn qglake_handoff_artifact_verifier_rejects_handoff_verify_output_graph_projection_table_prefix_drift()
 {
    let temp = qglake_temp_dir("handoff-artifacts-self-verify-graph-projection-table-prefix-drift");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    let mut output = qglake_bind_handoff_verify_output_artifact(&temp, &mut summary);
    output["graphProjectionProof"]["tablePrefix"] = json!("other_graph");
    let bytes = serde_json::to_vec_pretty(&output).expect("drifted handoff verify JSON");
    fs::write(temp.join("lakecat-handoff-verify.json"), &bytes)
        .expect("write drifted handoff verify output");
    summary["artifacts"]["lakecatHandoffVerifyOutputHash"] = json!(content_hash_bytes(&bytes));
    fs::write(
        &summary_path,
        serde_json::to_vec_pretty(&summary).expect("summary JSON"),
    )
    .expect("write summary");

    let err = verify_qglake_handoff_artifact_files(&summary_path, &summary)
        .expect_err("artifact verifier should reject handoff verifier graph table prefix drift");

    assert!(err.to_string().contains("lakecatHandoffVerifyOutput"));
    assert!(err.to_string().contains("graphProjectionProof mismatch"));
}

#[test]
fn qglake_handoff_artifact_verifier_rejects_handoff_verify_output_semantic_drift() {
    let temp = qglake_temp_dir("handoff-artifacts-self-verify-semantic-drift");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    let mut output = qglake_bind_handoff_verify_output_artifact(&temp, &mut summary);
    output["verifiedTables"] = json!(["lakecat:table:local:default:other_events"]);
    let bytes = serde_json::to_vec_pretty(&output).expect("drifted handoff verify JSON");
    fs::write(temp.join("lakecat-handoff-verify.json"), &bytes)
        .expect("write drifted handoff verify output");
    summary["artifacts"]["lakecatHandoffVerifyOutputHash"] = json!(content_hash_bytes(&bytes));
    fs::write(
        &summary_path,
        serde_json::to_vec_pretty(&summary).expect("summary JSON"),
    )
    .expect("write summary");

    let err = verify_qglake_handoff_artifact_files(&summary_path, &summary)
        .expect_err("artifact verifier should reject handoff verifier semantic drift");

    assert!(err.to_string().contains("lakecatHandoffVerifyOutput"));
    assert!(err.to_string().contains("verifiedTables mismatch"));
}

#[test]
fn qglake_handoff_artifact_verifier_rejects_handoff_verify_output_proof_drift() {
    let temp = qglake_temp_dir("handoff-artifacts-self-verify-proof-drift");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    let mut output = qglake_bind_handoff_verify_output_artifact(&temp, &mut summary);
    output["queryGraphBootstrapProof"]["bundleHash"] = json!("sha256:other-bundle");
    let bytes = serde_json::to_vec_pretty(&output).expect("drifted handoff verify JSON");
    fs::write(temp.join("lakecat-handoff-verify.json"), &bytes)
        .expect("write drifted handoff verify output");
    summary["artifacts"]["lakecatHandoffVerifyOutputHash"] = json!(content_hash_bytes(&bytes));
    fs::write(
        &summary_path,
        serde_json::to_vec_pretty(&summary).expect("summary JSON"),
    )
    .expect("write summary");

    let err = verify_qglake_handoff_artifact_files(&summary_path, &summary)
        .expect_err("artifact verifier should reject handoff verifier proof drift");

    assert!(err.to_string().contains("lakecatHandoffVerifyOutput"));
    assert!(
        err.to_string()
            .contains("queryGraphBootstrapProof mismatch")
    );
}

#[test]
fn qglake_handoff_artifact_verifier_rejects_handoff_verify_output_request_action_drift() {
    let temp = qglake_temp_dir("handoff-artifacts-self-verify-request-action-drift");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    let mut output = qglake_bind_handoff_verify_output_artifact(&temp, &mut summary);
    output["requestIdentityProof"]["authorizationReceiptAction"] = json!("graph-read");
    let bytes = serde_json::to_vec_pretty(&output).expect("drifted handoff verify JSON");
    fs::write(temp.join("lakecat-handoff-verify.json"), &bytes)
        .expect("write drifted handoff verify output");
    summary["artifacts"]["lakecatHandoffVerifyOutputHash"] = json!(content_hash_bytes(&bytes));
    fs::write(
        &summary_path,
        serde_json::to_vec_pretty(&summary).expect("summary JSON"),
    )
    .expect("write summary");

    let err = verify_qglake_handoff_artifact_files(&summary_path, &summary)
        .expect_err("artifact verifier should reject handoff verifier request action drift");

    assert!(err.to_string().contains("lakecatHandoffVerifyOutput"));
    assert!(err.to_string().contains("requestIdentityProof mismatch"));
}

#[test]
fn qglake_handoff_artifact_verifier_rejects_handoff_verify_output_bootstrap_action_drift() {
    let temp = qglake_temp_dir("handoff-artifacts-self-verify-bootstrap-action-drift");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    let mut output = qglake_bind_handoff_verify_output_artifact(&temp, &mut summary);
    output["queryGraphBootstrapProof"]["authorizationReceiptAction"] = json!("lineage-read");
    let bytes = serde_json::to_vec_pretty(&output).expect("drifted handoff verify JSON");
    fs::write(temp.join("lakecat-handoff-verify.json"), &bytes)
        .expect("write drifted handoff verify output");
    summary["artifacts"]["lakecatHandoffVerifyOutputHash"] = json!(content_hash_bytes(&bytes));
    fs::write(
        &summary_path,
        serde_json::to_vec_pretty(&summary).expect("summary JSON"),
    )
    .expect("write summary");

    let err = verify_qglake_handoff_artifact_files(&summary_path, &summary)
        .expect_err("artifact verifier should reject handoff verifier bootstrap action drift");

    assert!(err.to_string().contains("lakecatHandoffVerifyOutput"));
    assert!(
        err.to_string()
            .contains("queryGraphBootstrapProof mismatch")
    );
}

#[test]
fn qglake_handoff_artifact_verifier_rejects_handoff_verify_output_artifact_hash_drift() {
    let temp = qglake_temp_dir("handoff-artifacts-self-verify-artifact-hash-drift");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    let mut output = qglake_bind_handoff_verify_output_artifact(&temp, &mut summary);
    output["artifactFiles"]["bundle"]["sha256"] = json!(qglake_fixture_hash("other-bundle"));
    let bytes = serde_json::to_vec_pretty(&output).expect("drifted handoff verify JSON");
    fs::write(temp.join("lakecat-handoff-verify.json"), &bytes)
        .expect("write drifted handoff verify output");
    summary["artifacts"]["lakecatHandoffVerifyOutputHash"] = json!(content_hash_bytes(&bytes));
    fs::write(
        &summary_path,
        serde_json::to_vec_pretty(&summary).expect("summary JSON"),
    )
    .expect("write summary");

    let err = verify_qglake_handoff_artifact_files(&summary_path, &summary)
        .expect_err("artifact verifier should reject handoff verifier artifact hash drift");

    assert!(err.to_string().contains("lakecatHandoffVerifyOutput"));
    assert!(err.to_string().contains("artifactFiles"));
    assert!(err.to_string().contains("bundle"));
    assert!(err.to_string().contains("sha256 mismatch"));
}

#[test]
fn qglake_handoff_artifact_verifier_rejects_non_bundle_artifact_hash_drift() {
    for artifact in ["lineageDrain", "querygraphImportPlan"] {
        let temp = qglake_temp_dir(&format!(
            "handoff-artifacts-self-verify-{artifact}-artifact-hash-drift"
        ));
        let summary_path = temp.join("handoff-summary.json");
        let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
        let mut output = qglake_bind_handoff_verify_output_artifact(&temp, &mut summary);
        output["artifactFiles"][artifact]["sha256"] =
            json!(qglake_fixture_hash(&format!("other-{artifact}-artifact")));
        let bytes = serde_json::to_vec_pretty(&output).expect("drifted handoff verify JSON");
        fs::write(temp.join("lakecat-handoff-verify.json"), &bytes)
            .expect("write drifted handoff verify output");
        summary["artifacts"]["lakecatHandoffVerifyOutputHash"] = json!(content_hash_bytes(&bytes));
        fs::write(
            &summary_path,
            serde_json::to_vec_pretty(&summary).expect("summary JSON"),
        )
        .expect("write summary");

        let err = verify_qglake_handoff_artifact_files(&summary_path, &summary)
            .expect_err("artifact verifier should reject non-bundle artifact hash drift");
        let err = err.to_string();

        assert!(err.contains("lakecatHandoffVerifyOutput"), "{err}");
        assert!(err.contains("artifactFiles"), "{err}");
        assert!(err.contains(artifact), "{err}");
        assert!(err.contains("sha256 mismatch"), "{err}");
    }
}

#[test]
fn qglake_handoff_artifact_verifier_requires_handoff_verify_output_bundle_artifact() {
    let temp = qglake_temp_dir("handoff-artifacts-self-verify-missing-bundle-artifact");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    let mut output = qglake_bind_handoff_verify_output_artifact(&temp, &mut summary);
    output["artifactFiles"]
        .as_object_mut()
        .expect("artifactFiles object")
        .remove("bundle");
    let bytes = serde_json::to_vec_pretty(&output).expect("drifted handoff verify JSON");
    fs::write(temp.join("lakecat-handoff-verify.json"), &bytes)
        .expect("write drifted handoff verify output");
    summary["artifacts"]["lakecatHandoffVerifyOutputHash"] = json!(content_hash_bytes(&bytes));
    fs::write(
        &summary_path,
        serde_json::to_vec_pretty(&summary).expect("summary JSON"),
    )
    .expect("write summary");

    let err = verify_qglake_handoff_artifact_files(&summary_path, &summary)
        .expect_err("artifact verifier should require handoff verifier bundle artifact");
    let err = err.to_string();

    assert!(err.contains("lakecatHandoffVerifyOutput"), "{err}");
    assert!(err.contains("artifactFiles"), "{err}");
    assert!(err.contains("bundle"), "{err}");
}

#[test]
fn qglake_handoff_artifact_verifier_requires_handoff_verify_output_lineage_drain_artifact() {
    let temp = qglake_temp_dir("handoff-artifacts-self-verify-missing-lineage-drain-artifact");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    let mut output = qglake_bind_handoff_verify_output_artifact(&temp, &mut summary);
    output["artifactFiles"]
        .as_object_mut()
        .expect("artifactFiles object")
        .remove("lineageDrain");
    let bytes = serde_json::to_vec_pretty(&output).expect("drifted handoff verify JSON");
    fs::write(temp.join("lakecat-handoff-verify.json"), &bytes)
        .expect("write drifted handoff verify output");
    summary["artifacts"]["lakecatHandoffVerifyOutputHash"] = json!(content_hash_bytes(&bytes));
    fs::write(
        &summary_path,
        serde_json::to_vec_pretty(&summary).expect("summary JSON"),
    )
    .expect("write summary");

    let err = verify_qglake_handoff_artifact_files(&summary_path, &summary)
        .expect_err("artifact verifier should require handoff verifier lineage drain artifact");
    let err = err.to_string();

    assert!(err.contains("lakecatHandoffVerifyOutput"), "{err}");
    assert!(err.contains("artifactFiles"), "{err}");
    assert!(err.contains("lineageDrain"), "{err}");
}

#[test]
fn qglake_handoff_artifact_verifier_requires_handoff_verify_output_import_plan_artifact() {
    let temp = qglake_temp_dir("handoff-artifacts-self-verify-missing-import-plan-artifact");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    let mut output = qglake_bind_handoff_verify_output_artifact(&temp, &mut summary);
    output["artifactFiles"]
        .as_object_mut()
        .expect("artifactFiles object")
        .remove("querygraphImportPlan");
    let bytes = serde_json::to_vec_pretty(&output).expect("drifted handoff verify JSON");
    fs::write(temp.join("lakecat-handoff-verify.json"), &bytes)
        .expect("write drifted handoff verify output");
    summary["artifacts"]["lakecatHandoffVerifyOutputHash"] = json!(content_hash_bytes(&bytes));
    fs::write(
        &summary_path,
        serde_json::to_vec_pretty(&summary).expect("summary JSON"),
    )
    .expect("write summary");

    let err = verify_qglake_handoff_artifact_files(&summary_path, &summary)
        .expect_err("artifact verifier should require handoff verifier import plan artifact");
    let err = err.to_string();

    assert!(err.contains("lakecatHandoffVerifyOutput"), "{err}");
    assert!(err.contains("artifactFiles"), "{err}");
    assert!(err.contains("querygraphImportPlan"), "{err}");
}

#[test]
fn qglake_handoff_artifact_verifier_rejects_handoff_verify_output_service_log_hash_drift() {
    let temp = qglake_temp_dir("handoff-artifacts-self-verify-service-log-hash-drift");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    let mut output = qglake_bind_handoff_verify_output_artifact(&temp, &mut summary);
    output["artifactFiles"]["serviceLogHash"] =
        json!(qglake_fixture_hash("other-service-log-hash"));
    let bytes = serde_json::to_vec_pretty(&output).expect("drifted handoff verify JSON");
    fs::write(temp.join("lakecat-handoff-verify.json"), &bytes)
        .expect("write drifted handoff verify output");
    summary["artifacts"]["lakecatHandoffVerifyOutputHash"] = json!(content_hash_bytes(&bytes));
    fs::write(
        &summary_path,
        serde_json::to_vec_pretty(&summary).expect("summary JSON"),
    )
    .expect("write summary");

    let err = verify_qglake_handoff_artifact_files(&summary_path, &summary)
        .expect_err("artifact verifier should reject handoff verifier service-log hash drift");

    assert!(err.to_string().contains("lakecatHandoffVerifyOutput"));
    assert!(err.to_string().contains("artifactFiles"));
    assert!(err.to_string().contains("serviceLogHash mismatch"));
}

#[test]
fn qglake_handoff_artifact_verifier_requires_handoff_verify_output_service_log_hash() {
    let temp = qglake_temp_dir("handoff-artifacts-self-verify-missing-service-log-hash");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    let mut output = qglake_bind_handoff_verify_output_artifact(&temp, &mut summary);
    output["artifactFiles"]
        .as_object_mut()
        .expect("artifactFiles object")
        .remove("serviceLogHash");
    let bytes = serde_json::to_vec_pretty(&output).expect("drifted handoff verify JSON");
    fs::write(temp.join("lakecat-handoff-verify.json"), &bytes)
        .expect("write drifted handoff verify output");
    summary["artifacts"]["lakecatHandoffVerifyOutputHash"] = json!(content_hash_bytes(&bytes));
    fs::write(
        &summary_path,
        serde_json::to_vec_pretty(&summary).expect("summary JSON"),
    )
    .expect("write summary");

    let err = verify_qglake_handoff_artifact_files(&summary_path, &summary)
        .expect_err("artifact verifier should require handoff verifier service-log hash");
    let err = err.to_string();

    assert!(err.contains("lakecatHandoffVerifyOutput"), "{err}");
    assert!(err.contains("artifactFiles"), "{err}");
    assert!(err.contains("serviceLogHash"), "{err}");
}

#[test]
fn qglake_handoff_artifact_verifier_rejects_handoff_verify_output_null_service_log_hash() {
    let temp = qglake_temp_dir("handoff-artifacts-self-verify-null-service-log-hash");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    let mut output = qglake_bind_handoff_verify_output_artifact(&temp, &mut summary);
    output["artifactFiles"]["serviceLogHash"] = Value::Null;
    let bytes = serde_json::to_vec_pretty(&output).expect("drifted handoff verify JSON");
    fs::write(temp.join("lakecat-handoff-verify.json"), &bytes)
        .expect("write drifted handoff verify output");
    summary["artifacts"]["lakecatHandoffVerifyOutputHash"] = json!(content_hash_bytes(&bytes));
    fs::write(
        &summary_path,
        serde_json::to_vec_pretty(&summary).expect("summary JSON"),
    )
    .expect("write summary");

    let err = verify_qglake_handoff_artifact_files(&summary_path, &summary)
        .expect_err("artifact verifier should reject null handoff verifier service-log hash");
    let err = err.to_string();

    assert!(err.contains("lakecatHandoffVerifyOutput"), "{err}");
    assert!(err.contains("artifactFiles"), "{err}");
    assert!(err.contains("serviceLogHash"), "{err}");
    assert!(err.contains("must be a string"), "{err}");
}

#[test]
fn qglake_handoff_artifact_verifier_rejects_handoff_verify_output_short_service_log_hash() {
    let temp = qglake_temp_dir("handoff-artifacts-self-verify-short-service-log-hash");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    let mut output = qglake_bind_handoff_verify_output_artifact(&temp, &mut summary);
    output["artifactFiles"]["serviceLogHash"] = json!("sha256:service-log");
    let bytes = serde_json::to_vec_pretty(&output).expect("drifted handoff verify JSON");
    fs::write(temp.join("lakecat-handoff-verify.json"), &bytes)
        .expect("write drifted handoff verify output");
    summary["artifacts"]["lakecatHandoffVerifyOutputHash"] = json!(content_hash_bytes(&bytes));
    fs::write(
        &summary_path,
        serde_json::to_vec_pretty(&summary).expect("summary JSON"),
    )
    .expect("write summary");

    let err = verify_qglake_handoff_artifact_files(&summary_path, &summary)
        .expect_err("artifact verifier should reject short handoff verifier service-log hash");
    let err = err.to_string();

    assert!(err.contains("lakecatHandoffVerifyOutput"), "{err}");
    assert!(err.contains("artifactFiles"), "{err}");
    assert!(err.contains("serviceLogHash"), "{err}");
    assert!(err.contains("full SHA-256"), "{err}");
}

#[test]
fn qglake_handoff_artifact_verifier_rejects_handoff_verify_output_extra_artifact_hash() {
    let temp = qglake_temp_dir("handoff-artifacts-self-verify-extra-artifact-hash");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    let mut output = qglake_bind_handoff_verify_output_artifact(&temp, &mut summary);
    output["artifactFiles"]["unverifiedSidecar"] =
        json!({ "sha256": qglake_fixture_hash("unverified-sidecar") });
    let bytes = serde_json::to_vec_pretty(&output).expect("drifted handoff verify JSON");
    fs::write(temp.join("lakecat-handoff-verify.json"), &bytes)
        .expect("write drifted handoff verify output");
    summary["artifacts"]["lakecatHandoffVerifyOutputHash"] = json!(content_hash_bytes(&bytes));
    fs::write(
        &summary_path,
        serde_json::to_vec_pretty(&summary).expect("summary JSON"),
    )
    .expect("write summary");

    let err = verify_qglake_handoff_artifact_files(&summary_path, &summary)
        .expect_err("artifact verifier should reject extra sidecar artifact hash claims");
    let err = err.to_string();

    assert!(err.contains("lakecatHandoffVerifyOutput"), "{err}");
    assert!(err.contains("artifactFiles"), "{err}");
    assert!(err.contains("unexpected field unverifiedSidecar"), "{err}");
}

#[test]
fn qglake_handoff_artifact_verifier_rejects_handoff_verify_output_extra_artifact_hash_field() {
    let temp = qglake_temp_dir("handoff-artifacts-self-verify-extra-artifact-hash-field");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    let mut output = qglake_bind_handoff_verify_output_artifact(&temp, &mut summary);
    output["artifactFiles"]["bundle"]["unverifiedHash"] =
        json!(qglake_fixture_hash("unverified-bundle-hash"));
    let bytes = serde_json::to_vec_pretty(&output).expect("drifted handoff verify JSON");
    fs::write(temp.join("lakecat-handoff-verify.json"), &bytes)
        .expect("write drifted handoff verify output");
    summary["artifacts"]["lakecatHandoffVerifyOutputHash"] = json!(content_hash_bytes(&bytes));
    fs::write(
        &summary_path,
        serde_json::to_vec_pretty(&summary).expect("summary JSON"),
    )
    .expect("write summary");

    let err = verify_qglake_handoff_artifact_files(&summary_path, &summary)
        .expect_err("artifact verifier should reject extra artifact hash object fields");
    let err = err.to_string();

    assert!(err.contains("lakecatHandoffVerifyOutput"), "{err}");
    assert!(err.contains("artifactFiles"), "{err}");
    assert!(err.contains("unexpected field unverifiedHash"), "{err}");
}

#[test]
fn qglake_handoff_artifact_verifier_rejects_extra_non_bundle_artifact_hash_field() {
    for artifact in ["lineageDrain", "querygraphImportPlan"] {
        let temp = qglake_temp_dir(&format!(
            "handoff-artifacts-self-verify-extra-{artifact}-artifact-hash-field"
        ));
        let summary_path = temp.join("handoff-summary.json");
        let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
        let mut output = qglake_bind_handoff_verify_output_artifact(&temp, &mut summary);
        output["artifactFiles"][artifact]["unverifiedHash"] =
            json!(qglake_fixture_hash("unverified-non-bundle-artifact-hash"));
        let bytes = serde_json::to_vec_pretty(&output).expect("drifted handoff verify JSON");
        fs::write(temp.join("lakecat-handoff-verify.json"), &bytes)
            .expect("write drifted handoff verify output");
        summary["artifacts"]["lakecatHandoffVerifyOutputHash"] = json!(content_hash_bytes(&bytes));
        fs::write(
            &summary_path,
            serde_json::to_vec_pretty(&summary).expect("summary JSON"),
        )
        .expect("write summary");

        let err = verify_qglake_handoff_artifact_files(&summary_path, &summary)
            .expect_err("artifact verifier should reject extra non-bundle artifact hash fields");
        let err = err.to_string();

        assert!(err.contains("lakecatHandoffVerifyOutput"), "{err}");
        assert!(err.contains("artifactFiles"), "{err}");
        assert!(err.contains(artifact), "{err}");
        assert!(err.contains("unexpected field unverifiedHash"), "{err}");
    }
}

#[test]
fn qglake_handoff_artifact_verifier_rejects_handoff_verify_output_short_artifact_hash() {
    let temp = qglake_temp_dir("handoff-artifacts-self-verify-short-artifact-hash");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    let mut output = qglake_bind_handoff_verify_output_artifact(&temp, &mut summary);
    output["artifactFiles"]["bundle"]["sha256"] = json!("sha256:bundle");
    let bytes = serde_json::to_vec_pretty(&output).expect("drifted handoff verify JSON");
    fs::write(temp.join("lakecat-handoff-verify.json"), &bytes)
        .expect("write drifted handoff verify output");
    summary["artifacts"]["lakecatHandoffVerifyOutputHash"] = json!(content_hash_bytes(&bytes));
    fs::write(
        &summary_path,
        serde_json::to_vec_pretty(&summary).expect("summary JSON"),
    )
    .expect("write summary");

    let err = verify_qglake_handoff_artifact_files(&summary_path, &summary)
        .expect_err("artifact verifier should reject short sidecar artifact hashes");
    let err = err.to_string();

    assert!(err.contains("lakecatHandoffVerifyOutput"), "{err}");
    assert!(err.contains("artifactFiles"), "{err}");
    assert!(err.contains("full SHA-256"), "{err}");
}

#[test]
fn qglake_handoff_artifact_verifier_rejects_short_non_bundle_artifact_hash() {
    for (artifact, short_hash) in [
        ("lineageDrain", "sha256:lineage-drain"),
        ("querygraphImportPlan", "sha256:querygraph-import-plan"),
    ] {
        let temp = qglake_temp_dir(&format!(
            "handoff-artifacts-self-verify-short-{artifact}-artifact-hash"
        ));
        let summary_path = temp.join("handoff-summary.json");
        let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
        let mut output = qglake_bind_handoff_verify_output_artifact(&temp, &mut summary);
        output["artifactFiles"][artifact]["sha256"] = json!(short_hash);
        let bytes = serde_json::to_vec_pretty(&output).expect("drifted handoff verify JSON");
        fs::write(temp.join("lakecat-handoff-verify.json"), &bytes)
            .expect("write drifted handoff verify output");
        summary["artifacts"]["lakecatHandoffVerifyOutputHash"] = json!(content_hash_bytes(&bytes));
        fs::write(
            &summary_path,
            serde_json::to_vec_pretty(&summary).expect("summary JSON"),
        )
        .expect("write summary");

        let err = verify_qglake_handoff_artifact_files(&summary_path, &summary)
            .expect_err("artifact verifier should reject short non-bundle artifact hashes");
        let err = err.to_string();

        assert!(err.contains("lakecatHandoffVerifyOutput"), "{err}");
        assert!(err.contains("artifactFiles"), "{err}");
        assert!(err.contains(artifact), "{err}");
        assert!(err.contains("full SHA-256"), "{err}");
    }
}

#[test]
fn qglake_handoff_artifact_verifier_rejects_handoff_verify_output_capture_hash_drift() {
    let temp = qglake_temp_dir("handoff-artifacts-self-verify-capture-hash-drift");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    let mut output = qglake_bind_handoff_verify_output_artifact(&temp, &mut summary);
    output["artifactFiles"]["capturedOutputs"]["lakecatReplay"]["sha256"] =
        json!(qglake_fixture_hash("other-replay-capture"));
    let bytes = serde_json::to_vec_pretty(&output).expect("drifted handoff verify JSON");
    fs::write(temp.join("lakecat-handoff-verify.json"), &bytes)
        .expect("write drifted handoff verify output");
    summary["artifacts"]["lakecatHandoffVerifyOutputHash"] = json!(content_hash_bytes(&bytes));
    fs::write(
        &summary_path,
        serde_json::to_vec_pretty(&summary).expect("summary JSON"),
    )
    .expect("write summary");

    let err = verify_qglake_handoff_artifact_files(&summary_path, &summary)
        .expect_err("artifact verifier should reject handoff verifier capture hash drift");

    assert!(err.to_string().contains("lakecatHandoffVerifyOutput"));
    assert!(err.to_string().contains("capturedOutputs"));
    assert!(err.to_string().contains("sha256 mismatch"));
}

#[test]
fn qglake_handoff_artifact_verifier_rejects_querygraph_capture_hash_drift() {
    for capture in ["querygraphVerify", "querygraphImport"] {
        let temp = qglake_temp_dir(&format!(
            "handoff-artifacts-self-verify-{capture}-capture-hash-drift"
        ));
        let summary_path = temp.join("handoff-summary.json");
        let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
        let mut output = qglake_bind_handoff_verify_output_artifact(&temp, &mut summary);
        output["artifactFiles"]["capturedOutputs"][capture]["sha256"] =
            json!(qglake_fixture_hash(&format!("other-{capture}-capture")));
        let bytes = serde_json::to_vec_pretty(&output).expect("drifted handoff verify JSON");
        fs::write(temp.join("lakecat-handoff-verify.json"), &bytes)
            .expect("write drifted handoff verify output");
        summary["artifacts"]["lakecatHandoffVerifyOutputHash"] = json!(content_hash_bytes(&bytes));
        fs::write(
            &summary_path,
            serde_json::to_vec_pretty(&summary).expect("summary JSON"),
        )
        .expect("write summary");

        let err = verify_qglake_handoff_artifact_files(&summary_path, &summary)
            .expect_err("artifact verifier should reject QueryGraph capture hash drift");
        let err = err.to_string();

        assert!(err.contains("lakecatHandoffVerifyOutput"), "{err}");
        assert!(err.contains("capturedOutputs"), "{err}");
        assert!(err.contains(capture), "{err}");
        assert!(err.contains("sha256 mismatch"), "{err}");
    }
}

#[test]
fn qglake_handoff_artifact_verifier_requires_handoff_verify_output_captured_outputs() {
    let temp = qglake_temp_dir("handoff-artifacts-self-verify-missing-captured-outputs");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    let mut output = qglake_bind_handoff_verify_output_artifact(&temp, &mut summary);
    output["artifactFiles"]
        .as_object_mut()
        .expect("artifactFiles object")
        .remove("capturedOutputs");
    let bytes = serde_json::to_vec_pretty(&output).expect("drifted handoff verify JSON");
    fs::write(temp.join("lakecat-handoff-verify.json"), &bytes)
        .expect("write drifted handoff verify output");
    summary["artifacts"]["lakecatHandoffVerifyOutputHash"] = json!(content_hash_bytes(&bytes));
    fs::write(
        &summary_path,
        serde_json::to_vec_pretty(&summary).expect("summary JSON"),
    )
    .expect("write summary");

    let err = verify_qglake_handoff_artifact_files(&summary_path, &summary)
        .expect_err("artifact verifier should require handoff verifier captured outputs");
    let err = err.to_string();

    assert!(err.contains("lakecatHandoffVerifyOutput"), "{err}");
    assert!(err.contains("artifactFiles"), "{err}");
    assert!(err.contains("capturedOutputs"), "{err}");
}

#[test]
fn qglake_handoff_artifact_verifier_requires_handoff_verify_output_lakecat_replay_capture() {
    let temp = qglake_temp_dir("handoff-artifacts-self-verify-missing-lakecat-replay-capture");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    let mut output = qglake_bind_handoff_verify_output_artifact(&temp, &mut summary);
    output["artifactFiles"]["capturedOutputs"]
        .as_object_mut()
        .expect("capturedOutputs object")
        .remove("lakecatReplay");
    let bytes = serde_json::to_vec_pretty(&output).expect("drifted handoff verify JSON");
    fs::write(temp.join("lakecat-handoff-verify.json"), &bytes)
        .expect("write drifted handoff verify output");
    summary["artifacts"]["lakecatHandoffVerifyOutputHash"] = json!(content_hash_bytes(&bytes));
    fs::write(
        &summary_path,
        serde_json::to_vec_pretty(&summary).expect("summary JSON"),
    )
    .expect("write summary");

    let err = verify_qglake_handoff_artifact_files(&summary_path, &summary)
        .expect_err("artifact verifier should require handoff verifier LakeCat replay capture");
    let err = err.to_string();

    assert!(err.contains("lakecatHandoffVerifyOutput"), "{err}");
    assert!(err.contains("capturedOutputs"), "{err}");
    assert!(err.contains("lakecatReplay"), "{err}");
}

#[test]
fn qglake_handoff_artifact_verifier_requires_handoff_verify_output_querygraph_verify_capture() {
    let temp = qglake_temp_dir("handoff-artifacts-self-verify-missing-querygraph-verify-capture");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    let mut output = qglake_bind_handoff_verify_output_artifact(&temp, &mut summary);
    output["artifactFiles"]["capturedOutputs"]
        .as_object_mut()
        .expect("capturedOutputs object")
        .remove("querygraphVerify");
    let bytes = serde_json::to_vec_pretty(&output).expect("drifted handoff verify JSON");
    fs::write(temp.join("lakecat-handoff-verify.json"), &bytes)
        .expect("write drifted handoff verify output");
    summary["artifacts"]["lakecatHandoffVerifyOutputHash"] = json!(content_hash_bytes(&bytes));
    fs::write(
        &summary_path,
        serde_json::to_vec_pretty(&summary).expect("summary JSON"),
    )
    .expect("write summary");

    let err = verify_qglake_handoff_artifact_files(&summary_path, &summary)
        .expect_err("artifact verifier should require handoff verifier QueryGraph verify capture");
    let err = err.to_string();

    assert!(err.contains("lakecatHandoffVerifyOutput"), "{err}");
    assert!(err.contains("capturedOutputs"), "{err}");
    assert!(err.contains("querygraphVerify"), "{err}");
}

#[test]
fn qglake_handoff_artifact_verifier_requires_handoff_verify_output_querygraph_import_capture() {
    let temp = qglake_temp_dir("handoff-artifacts-self-verify-missing-querygraph-import-capture");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    let mut output = qglake_bind_handoff_verify_output_artifact(&temp, &mut summary);
    output["artifactFiles"]["capturedOutputs"]
        .as_object_mut()
        .expect("capturedOutputs object")
        .remove("querygraphImport");
    let bytes = serde_json::to_vec_pretty(&output).expect("drifted handoff verify JSON");
    fs::write(temp.join("lakecat-handoff-verify.json"), &bytes)
        .expect("write drifted handoff verify output");
    summary["artifacts"]["lakecatHandoffVerifyOutputHash"] = json!(content_hash_bytes(&bytes));
    fs::write(
        &summary_path,
        serde_json::to_vec_pretty(&summary).expect("summary JSON"),
    )
    .expect("write summary");

    let err = verify_qglake_handoff_artifact_files(&summary_path, &summary)
        .expect_err("artifact verifier should require handoff verifier QueryGraph import capture");
    let err = err.to_string();

    assert!(err.contains("lakecatHandoffVerifyOutput"), "{err}");
    assert!(err.contains("capturedOutputs"), "{err}");
    assert!(err.contains("querygraphImport"), "{err}");
}

#[test]
fn qglake_handoff_artifact_verifier_rejects_handoff_verify_output_extra_captured_semantics() {
    let temp = qglake_temp_dir("handoff-artifacts-self-verify-extra-captured-semantics");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    let mut output = qglake_bind_handoff_verify_output_artifact(&temp, &mut summary);
    output["capturedOutputSemantics"]["unverifiedCapture"] = json!({
        "sha256": qglake_fixture_hash("unverified-captured-semantics")
    });
    let bytes = serde_json::to_vec_pretty(&output).expect("drifted handoff verify JSON");
    fs::write(temp.join("lakecat-handoff-verify.json"), &bytes)
        .expect("write drifted handoff verify output");
    summary["artifacts"]["lakecatHandoffVerifyOutputHash"] = json!(content_hash_bytes(&bytes));
    fs::write(
        &summary_path,
        serde_json::to_vec_pretty(&summary).expect("summary JSON"),
    )
    .expect("write summary");

    let err = verify_qglake_handoff_artifact_files(&summary_path, &summary)
        .expect_err("artifact verifier should reject extra captured semantic proof claims");
    let err = err.to_string();

    assert!(err.contains("lakecatHandoffVerifyOutput"), "{err}");
    assert!(err.contains("capturedOutputSemantics"), "{err}");
    assert!(err.contains("unexpected field unverifiedCapture"), "{err}");
}

#[test]
fn qglake_handoff_artifact_verifier_rejects_handoff_verify_output_extra_capture_hash() {
    let temp = qglake_temp_dir("handoff-artifacts-self-verify-extra-capture-hash");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    let mut output = qglake_bind_handoff_verify_output_artifact(&temp, &mut summary);
    output["artifactFiles"]["capturedOutputs"]["unverifiedCapture"] =
        json!({ "sha256": qglake_fixture_hash("unverified-capture") });
    let bytes = serde_json::to_vec_pretty(&output).expect("drifted handoff verify JSON");
    fs::write(temp.join("lakecat-handoff-verify.json"), &bytes)
        .expect("write drifted handoff verify output");
    summary["artifacts"]["lakecatHandoffVerifyOutputHash"] = json!(content_hash_bytes(&bytes));
    fs::write(
        &summary_path,
        serde_json::to_vec_pretty(&summary).expect("summary JSON"),
    )
    .expect("write summary");

    let err = verify_qglake_handoff_artifact_files(&summary_path, &summary)
        .expect_err("artifact verifier should reject extra sidecar capture hash claims");
    let err = err.to_string();

    assert!(err.contains("lakecatHandoffVerifyOutput"), "{err}");
    assert!(err.contains("capturedOutputs"), "{err}");
    assert!(err.contains("unexpected field unverifiedCapture"), "{err}");
}

#[test]
fn qglake_handoff_artifact_verifier_rejects_handoff_verify_output_extra_capture_hash_field() {
    let temp = qglake_temp_dir("handoff-artifacts-self-verify-extra-capture-hash-field");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    let mut output = qglake_bind_handoff_verify_output_artifact(&temp, &mut summary);
    output["artifactFiles"]["capturedOutputs"]["lakecatReplay"]["unverifiedHash"] =
        json!(qglake_fixture_hash("unverified-capture-hash"));
    let bytes = serde_json::to_vec_pretty(&output).expect("drifted handoff verify JSON");
    fs::write(temp.join("lakecat-handoff-verify.json"), &bytes)
        .expect("write drifted handoff verify output");
    summary["artifacts"]["lakecatHandoffVerifyOutputHash"] = json!(content_hash_bytes(&bytes));
    fs::write(
        &summary_path,
        serde_json::to_vec_pretty(&summary).expect("summary JSON"),
    )
    .expect("write summary");

    let err = verify_qglake_handoff_artifact_files(&summary_path, &summary)
        .expect_err("artifact verifier should reject extra capture hash object fields");
    let err = err.to_string();

    assert!(err.contains("lakecatHandoffVerifyOutput"), "{err}");
    assert!(err.contains("capturedOutputs"), "{err}");
    assert!(err.contains("unexpected field unverifiedHash"), "{err}");
}

#[test]
fn qglake_handoff_artifact_verifier_rejects_extra_querygraph_capture_hash_field() {
    for capture in ["querygraphVerify", "querygraphImport"] {
        let temp = qglake_temp_dir(&format!(
            "handoff-artifacts-self-verify-extra-{capture}-capture-hash-field"
        ));
        let summary_path = temp.join("handoff-summary.json");
        let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
        let mut output = qglake_bind_handoff_verify_output_artifact(&temp, &mut summary);
        output["artifactFiles"]["capturedOutputs"][capture]["unverifiedHash"] =
            json!(qglake_fixture_hash("unverified-querygraph-capture-hash"));
        let bytes = serde_json::to_vec_pretty(&output).expect("drifted handoff verify JSON");
        fs::write(temp.join("lakecat-handoff-verify.json"), &bytes)
            .expect("write drifted handoff verify output");
        summary["artifacts"]["lakecatHandoffVerifyOutputHash"] = json!(content_hash_bytes(&bytes));
        fs::write(
            &summary_path,
            serde_json::to_vec_pretty(&summary).expect("summary JSON"),
        )
        .expect("write summary");

        let err = verify_qglake_handoff_artifact_files(&summary_path, &summary).expect_err(
            "artifact verifier should reject extra QueryGraph capture hash object fields",
        );
        let err = err.to_string();

        assert!(err.contains("lakecatHandoffVerifyOutput"), "{err}");
        assert!(err.contains("capturedOutputs"), "{err}");
        assert!(err.contains(capture), "{err}");
        assert!(err.contains("unexpected field unverifiedHash"), "{err}");
    }
}

#[test]
fn qglake_handoff_artifact_verifier_rejects_handoff_verify_output_short_capture_hash() {
    let temp = qglake_temp_dir("handoff-artifacts-self-verify-short-capture-hash");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    let mut output = qglake_bind_handoff_verify_output_artifact(&temp, &mut summary);
    output["artifactFiles"]["capturedOutputs"]["lakecatReplay"]["sha256"] =
        json!("sha256:replay-capture");
    let bytes = serde_json::to_vec_pretty(&output).expect("drifted handoff verify JSON");
    fs::write(temp.join("lakecat-handoff-verify.json"), &bytes)
        .expect("write drifted handoff verify output");
    summary["artifacts"]["lakecatHandoffVerifyOutputHash"] = json!(content_hash_bytes(&bytes));
    fs::write(
        &summary_path,
        serde_json::to_vec_pretty(&summary).expect("summary JSON"),
    )
    .expect("write summary");

    let err = verify_qglake_handoff_artifact_files(&summary_path, &summary)
        .expect_err("artifact verifier should reject short sidecar capture hashes");
    let err = err.to_string();

    assert!(err.contains("lakecatHandoffVerifyOutput"), "{err}");
    assert!(err.contains("capturedOutputs"), "{err}");
    assert!(err.contains("full SHA-256"), "{err}");
}

#[test]
fn qglake_handoff_artifact_verifier_rejects_short_querygraph_capture_hash() {
    for (capture, short_hash) in [
        ("querygraphVerify", "sha256:querygraph-verify"),
        ("querygraphImport", "sha256:querygraph-import"),
    ] {
        let temp = qglake_temp_dir(&format!(
            "handoff-artifacts-self-verify-short-{capture}-capture-hash"
        ));
        let summary_path = temp.join("handoff-summary.json");
        let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
        let mut output = qglake_bind_handoff_verify_output_artifact(&temp, &mut summary);
        output["artifactFiles"]["capturedOutputs"][capture]["sha256"] = json!(short_hash);
        let bytes = serde_json::to_vec_pretty(&output).expect("drifted handoff verify JSON");
        fs::write(temp.join("lakecat-handoff-verify.json"), &bytes)
            .expect("write drifted handoff verify output");
        summary["artifacts"]["lakecatHandoffVerifyOutputHash"] = json!(content_hash_bytes(&bytes));
        fs::write(
            &summary_path,
            serde_json::to_vec_pretty(&summary).expect("summary JSON"),
        )
        .expect("write summary");

        let err = verify_qglake_handoff_artifact_files(&summary_path, &summary)
            .expect_err("artifact verifier should reject short QueryGraph capture hashes");
        let err = err.to_string();

        assert!(err.contains("lakecatHandoffVerifyOutput"), "{err}");
        assert!(err.contains("capturedOutputs"), "{err}");
        assert!(err.contains(capture), "{err}");
        assert!(err.contains("full SHA-256"), "{err}");
    }
}

#[test]
fn qglake_handoff_artifact_verifier_rejects_handoff_verify_output_captured_semantic_drift() {
    let temp = qglake_temp_dir("handoff-artifacts-self-verify-captured-semantic-drift");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    let mut output = qglake_bind_handoff_verify_output_artifact(&temp, &mut summary);
    output["capturedOutputSemantics"]["querygraphVerify"]["graphHash"] =
        json!("sha256:other-graph");
    let bytes = serde_json::to_vec_pretty(&output).expect("drifted handoff verify JSON");
    fs::write(temp.join("lakecat-handoff-verify.json"), &bytes)
        .expect("write drifted handoff verify output");
    summary["artifacts"]["lakecatHandoffVerifyOutputHash"] = json!(content_hash_bytes(&bytes));
    fs::write(
        &summary_path,
        serde_json::to_vec_pretty(&summary).expect("summary JSON"),
    )
    .expect("write summary");

    let err = verify_qglake_handoff_artifact_files(&summary_path, &summary)
        .expect_err("artifact verifier should reject handoff verifier captured semantic drift");

    assert!(err.to_string().contains("lakecatHandoffVerifyOutput"));
    assert!(err.to_string().contains("capturedOutputSemantics"));
    assert!(err.to_string().contains("graphHash mismatch"));
}

#[test]
fn qglake_handoff_artifact_verifier_rejects_handoff_verify_output_extra_lakecat_semantics() {
    let temp = qglake_temp_dir("handoff-artifacts-self-verify-extra-lakecat-semantics");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    let mut output = qglake_bind_handoff_verify_output_artifact(&temp, &mut summary);
    output["capturedOutputSemantics"]["lakecatReplay"]["unverifiedReplayProof"] = json!({
        "sha256": qglake_fixture_hash("unverified-replay-proof")
    });
    let bytes = serde_json::to_vec_pretty(&output).expect("drifted handoff verify JSON");
    fs::write(temp.join("lakecat-handoff-verify.json"), &bytes)
        .expect("write drifted handoff verify output");
    summary["artifacts"]["lakecatHandoffVerifyOutputHash"] = json!(content_hash_bytes(&bytes));
    fs::write(
        &summary_path,
        serde_json::to_vec_pretty(&summary).expect("summary JSON"),
    )
    .expect("write summary");

    let err = verify_qglake_handoff_artifact_files(&summary_path, &summary)
        .expect_err("artifact verifier should reject extra LakeCat semantic proof claims");
    let err = err.to_string();

    assert!(err.contains("lakecatHandoffVerifyOutput"), "{err}");
    assert!(
        err.contains("capturedOutputSemantics.lakecatReplay"),
        "{err}"
    );
    assert!(
        err.contains("unexpected field unverifiedReplayProof"),
        "{err}"
    );
}

#[test]
fn qglake_handoff_artifact_verifier_rejects_handoff_verify_output_extra_querygraph_semantics() {
    let temp = qglake_temp_dir("handoff-artifacts-self-verify-extra-querygraph-semantics");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    let mut output = qglake_bind_handoff_verify_output_artifact(&temp, &mut summary);
    output["capturedOutputSemantics"]["querygraphVerify"]["unverifiedGraphProof"] = json!({
        "sha256": qglake_fixture_hash("unverified-graph-proof")
    });
    let bytes = serde_json::to_vec_pretty(&output).expect("drifted handoff verify JSON");
    fs::write(temp.join("lakecat-handoff-verify.json"), &bytes)
        .expect("write drifted handoff verify output");
    summary["artifacts"]["lakecatHandoffVerifyOutputHash"] = json!(content_hash_bytes(&bytes));
    fs::write(
        &summary_path,
        serde_json::to_vec_pretty(&summary).expect("summary JSON"),
    )
    .expect("write summary");

    let err = verify_qglake_handoff_artifact_files(&summary_path, &summary)
        .expect_err("artifact verifier should reject extra QueryGraph semantic proof claims");
    let err = err.to_string();

    assert!(err.contains("lakecatHandoffVerifyOutput"), "{err}");
    assert!(
        err.contains("capturedOutputSemantics.querygraphVerify"),
        "{err}"
    );
    assert!(
        err.contains("unexpected field unverifiedGraphProof"),
        "{err}"
    );
}

#[test]
fn qglake_handoff_artifact_verifier_rejects_handoff_verify_output_management_id_drift() {
    let temp = qglake_temp_dir("handoff-artifacts-self-verify-management-id-drift");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    let mut output = qglake_bind_handoff_verify_output_artifact(&temp, &mut summary);
    output["capturedOutputSemantics"]["lakecatReplay"]["managementProof"]["serverIds"] =
        json!(["other-server"]);
    let bytes = serde_json::to_vec_pretty(&output).expect("drifted handoff verify JSON");
    fs::write(temp.join("lakecat-handoff-verify.json"), &bytes)
        .expect("write drifted handoff verify output");
    summary["artifacts"]["lakecatHandoffVerifyOutputHash"] = json!(content_hash_bytes(&bytes));
    fs::write(
        &summary_path,
        serde_json::to_vec_pretty(&summary).expect("summary JSON"),
    )
    .expect("write summary");

    let err = verify_qglake_handoff_artifact_files(&summary_path, &summary)
        .expect_err("artifact verifier should reject handoff verifier management ID drift");

    assert!(err.to_string().contains("lakecatHandoffVerifyOutput"));
    assert!(err.to_string().contains("capturedOutputSemantics"));
    assert!(err.to_string().contains("managementProof mismatch"));
}

#[test]
fn qglake_handoff_artifact_verifier_rejects_handoff_verify_output_lineage_identity_drift() {
    let temp = qglake_temp_dir("handoff-artifacts-self-verify-lineage-identity-drift");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    let mut output = qglake_bind_handoff_verify_output_artifact(&temp, &mut summary);
    output["lineageDrainArtifactSemantics"]["requestIdentitySource"] = json!("x-lakecat-human-did");
    let bytes = serde_json::to_vec_pretty(&output).expect("drifted handoff verify JSON");
    fs::write(temp.join("lakecat-handoff-verify.json"), &bytes)
        .expect("write drifted handoff verify output");
    summary["artifacts"]["lakecatHandoffVerifyOutputHash"] = json!(content_hash_bytes(&bytes));
    fs::write(
        &summary_path,
        serde_json::to_vec_pretty(&summary).expect("summary JSON"),
    )
    .expect("write summary");

    let err = verify_qglake_handoff_artifact_files(&summary_path, &summary)
        .expect_err("artifact verifier should reject handoff verifier identity drift");

    assert!(err.to_string().contains("lakecatHandoffVerifyOutput"));
    assert!(err.to_string().contains("lineageDrainArtifactSemantics"));
    assert!(err.to_string().contains("requestIdentitySource mismatch"));
}

#[test]
fn qglake_handoff_artifact_verifier_rejects_handoff_verify_output_lineage_action_drift() {
    let temp = qglake_temp_dir("handoff-artifacts-self-verify-lineage-action-drift");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    let mut output = qglake_bind_handoff_verify_output_artifact(&temp, &mut summary);
    output["lineageDrainArtifactSemantics"]["authorizationReceiptAction"] = json!("graph-read");
    let bytes = serde_json::to_vec_pretty(&output).expect("drifted handoff verify JSON");
    fs::write(temp.join("lakecat-handoff-verify.json"), &bytes)
        .expect("write drifted handoff verify output");
    summary["artifacts"]["lakecatHandoffVerifyOutputHash"] = json!(content_hash_bytes(&bytes));
    fs::write(
        &summary_path,
        serde_json::to_vec_pretty(&summary).expect("summary JSON"),
    )
    .expect("write summary");

    let err = verify_qglake_handoff_artifact_files(&summary_path, &summary)
        .expect_err("artifact verifier should reject handoff verifier action drift");

    assert!(err.to_string().contains("lakecatHandoffVerifyOutput"));
    assert!(err.to_string().contains("lineageDrainArtifactSemantics"));
    assert!(
        err.to_string()
            .contains("authorizationReceiptAction mismatch")
    );
}

#[test]
fn qglake_handoff_artifact_verifier_rejects_handoff_verify_output_lineage_config_drift() {
    let temp = qglake_temp_dir("handoff-artifacts-self-verify-lineage-config-drift");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    let mut output = qglake_bind_handoff_verify_output_artifact(&temp, &mut summary);
    output["lineageDrainArtifactSemantics"]["catalogConfigProof"]["authorizationReceiptAction"] =
        json!("graph-read");
    let bytes = serde_json::to_vec_pretty(&output).expect("drifted handoff verify JSON");
    fs::write(temp.join("lakecat-handoff-verify.json"), &bytes)
        .expect("write drifted handoff verify output");
    summary["artifacts"]["lakecatHandoffVerifyOutputHash"] = json!(content_hash_bytes(&bytes));
    fs::write(
        &summary_path,
        serde_json::to_vec_pretty(&summary).expect("summary JSON"),
    )
    .expect("write summary");

    let err = verify_qglake_handoff_artifact_files(&summary_path, &summary)
        .expect_err("artifact verifier should reject handoff verifier config proof drift");
    let err = err.to_string();

    assert!(err.contains("lakecatHandoffVerifyOutput"), "{err}");
    assert!(err.contains("lineageDrainArtifactSemantics"), "{err}");
    assert!(err.contains("catalogConfigProof mismatch"), "{err}");
}

#[test]
fn qglake_handoff_artifact_verifier_rejects_handoff_verify_output_missing_lineage_config() {
    let temp = qglake_temp_dir("handoff-artifacts-self-verify-missing-lineage-config");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    let mut output = qglake_bind_handoff_verify_output_artifact(&temp, &mut summary);
    output["lineageDrainArtifactSemantics"]
        .as_object_mut()
        .expect("lineage drain semantics should be an object")
        .remove("catalogConfigProof");
    let bytes = serde_json::to_vec_pretty(&output).expect("drifted handoff verify JSON");
    fs::write(temp.join("lakecat-handoff-verify.json"), &bytes)
        .expect("write drifted handoff verify output");
    summary["artifacts"]["lakecatHandoffVerifyOutputHash"] = json!(content_hash_bytes(&bytes));
    fs::write(
        &summary_path,
        serde_json::to_vec_pretty(&summary).expect("summary JSON"),
    )
    .expect("write summary");

    let err = verify_qglake_handoff_artifact_files(&summary_path, &summary)
        .expect_err("artifact verifier should reject missing lineage config proof");
    let err = err.to_string();

    assert!(err.contains("lakecatHandoffVerifyOutput"), "{err}");
    assert!(err.contains("lineageDrainArtifactSemantics"), "{err}");
    assert!(err.contains("catalogConfigProof"), "{err}");
}

#[test]
fn qglake_handoff_artifact_verifier_rejects_handoff_verify_output_extra_lineage_config_field() {
    let temp = qglake_temp_dir("handoff-artifacts-self-verify-extra-lineage-config-field");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    let mut output = qglake_bind_handoff_verify_output_artifact(&temp, &mut summary);
    output["lineageDrainArtifactSemantics"]["catalogConfigProof"]
        .as_object_mut()
        .expect("lineage drain catalog config proof should be an object")
        .insert(
            "unverifiedEndpointClaim".to_string(),
            json!("GET /catalog/v1/unverified"),
        );
    let bytes = serde_json::to_vec_pretty(&output).expect("drifted handoff verify JSON");
    fs::write(temp.join("lakecat-handoff-verify.json"), &bytes)
        .expect("write drifted handoff verify output");
    summary["artifacts"]["lakecatHandoffVerifyOutputHash"] = json!(content_hash_bytes(&bytes));
    fs::write(
        &summary_path,
        serde_json::to_vec_pretty(&summary).expect("summary JSON"),
    )
    .expect("write summary");

    let err = verify_qglake_handoff_artifact_files(&summary_path, &summary)
        .expect_err("artifact verifier should reject extra lineage config proof fields");
    let err = err.to_string();

    assert!(err.contains("lakecatHandoffVerifyOutput"), "{err}");
    assert!(err.contains("lineageDrainArtifactSemantics"), "{err}");
    assert!(err.contains("catalogConfigProof mismatch"), "{err}");
}

#[test]
fn qglake_handoff_artifact_verifier_rejects_handoff_verify_output_extra_lineage_semantics() {
    let temp = qglake_temp_dir("handoff-artifacts-self-verify-extra-lineage-semantics");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    let mut output = qglake_bind_handoff_verify_output_artifact(&temp, &mut summary);
    output["lineageDrainArtifactSemantics"]["unverifiedLineageProof"] = json!({
        "sha256": qglake_fixture_hash("unverified-lineage-proof")
    });
    let bytes = serde_json::to_vec_pretty(&output).expect("drifted handoff verify JSON");
    fs::write(temp.join("lakecat-handoff-verify.json"), &bytes)
        .expect("write drifted handoff verify output");
    summary["artifacts"]["lakecatHandoffVerifyOutputHash"] = json!(content_hash_bytes(&bytes));
    fs::write(
        &summary_path,
        serde_json::to_vec_pretty(&summary).expect("summary JSON"),
    )
    .expect("write summary");

    let err = verify_qglake_handoff_artifact_files(&summary_path, &summary)
        .expect_err("artifact verifier should reject extra lineage semantic proof claims");
    let err = err.to_string();

    assert!(err.contains("lakecatHandoffVerifyOutput"), "{err}");
    assert!(err.contains("lineageDrainArtifactSemantics"), "{err}");
    assert!(
        err.contains("unexpected field unverifiedLineageProof"),
        "{err}"
    );
}

#[test]
fn qglake_handoff_artifact_verifier_rejects_handoff_verify_output_event_type_drift() {
    let temp = qglake_temp_dir("handoff-artifacts-self-verify-event-type-drift");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    let mut output = qglake_bind_handoff_verify_output_artifact(&temp, &mut summary);
    output["lineageDrainArtifactSemantics"]["eventTypes"][0] = json!("querygraph.bootstrap-shadow");
    let bytes = serde_json::to_vec_pretty(&output).expect("drifted handoff verify JSON");
    fs::write(temp.join("lakecat-handoff-verify.json"), &bytes)
        .expect("write drifted handoff verify output");
    summary["artifacts"]["lakecatHandoffVerifyOutputHash"] = json!(content_hash_bytes(&bytes));
    fs::write(
        &summary_path,
        serde_json::to_vec_pretty(&summary).expect("summary JSON"),
    )
    .expect("write summary");

    let err = verify_qglake_handoff_artifact_files(&summary_path, &summary)
        .expect_err("artifact verifier should reject handoff verifier event-type drift");
    let err = err.to_string();

    assert!(err.contains("lakecatHandoffVerifyOutput"), "{err}");
    assert!(err.contains("lineageDrainArtifactSemantics"), "{err}");
    assert!(err.contains("eventTypes mismatch"), "{err}");
}

#[test]
fn qglake_handoff_artifact_verifier_rejects_handoff_verify_output_delivered_count_drift() {
    let temp = qglake_temp_dir("handoff-artifacts-self-verify-delivered-count-drift");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    let mut output = qglake_bind_handoff_verify_output_artifact(&temp, &mut summary);
    output["lineageDrainArtifactSemantics"]["delivered"] = json!(12);
    let bytes = serde_json::to_vec_pretty(&output).expect("drifted handoff verify JSON");
    fs::write(temp.join("lakecat-handoff-verify.json"), &bytes)
        .expect("write drifted handoff verify output");
    summary["artifacts"]["lakecatHandoffVerifyOutputHash"] = json!(content_hash_bytes(&bytes));
    fs::write(
        &summary_path,
        serde_json::to_vec_pretty(&summary).expect("summary JSON"),
    )
    .expect("write summary");

    let err = verify_qglake_handoff_artifact_files(&summary_path, &summary)
        .expect_err("artifact verifier should reject handoff verifier delivered count drift");
    let err = err.to_string();

    assert!(err.contains("lakecatHandoffVerifyOutput"), "{err}");
    assert!(err.contains("lineageDrainArtifactSemantics"), "{err}");
    assert!(err.contains("delivered mismatch"), "{err}");
}

#[test]
fn qglake_handoff_artifact_verifier_rejects_handoff_verify_output_graph_event_count_drift() {
    let temp = qglake_temp_dir("handoff-artifacts-self-verify-graph-event-count-drift");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    let mut output = qglake_bind_handoff_verify_output_artifact(&temp, &mut summary);
    output["lineageDrainArtifactSemantics"]["graphEvents"] = json!(16);
    let bytes = serde_json::to_vec_pretty(&output).expect("drifted handoff verify JSON");
    fs::write(temp.join("lakecat-handoff-verify.json"), &bytes)
        .expect("write drifted handoff verify output");
    summary["artifacts"]["lakecatHandoffVerifyOutputHash"] = json!(content_hash_bytes(&bytes));
    fs::write(
        &summary_path,
        serde_json::to_vec_pretty(&summary).expect("summary JSON"),
    )
    .expect("write summary");

    let err = verify_qglake_handoff_artifact_files(&summary_path, &summary)
        .expect_err("artifact verifier should reject handoff verifier graph event count drift");
    let err = err.to_string();

    assert!(err.contains("lakecatHandoffVerifyOutput"), "{err}");
    assert!(err.contains("lineageDrainArtifactSemantics"), "{err}");
    assert!(err.contains("graphEvents mismatch"), "{err}");
}

#[test]
fn qglake_handoff_artifact_verifier_rejects_handoff_verify_output_lineage_count_drift() {
    let temp = qglake_temp_dir("handoff-artifacts-self-verify-lineage-count-drift");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    let mut output = qglake_bind_handoff_verify_output_artifact(&temp, &mut summary);
    output["lineageDrainArtifactSemantics"]["lineageEvents"] = json!(12);
    let bytes = serde_json::to_vec_pretty(&output).expect("drifted handoff verify JSON");
    fs::write(temp.join("lakecat-handoff-verify.json"), &bytes)
        .expect("write drifted handoff verify output");
    summary["artifacts"]["lakecatHandoffVerifyOutputHash"] = json!(content_hash_bytes(&bytes));
    fs::write(
        &summary_path,
        serde_json::to_vec_pretty(&summary).expect("summary JSON"),
    )
    .expect("write summary");

    let err = verify_qglake_handoff_artifact_files(&summary_path, &summary)
        .expect_err("artifact verifier should reject handoff verifier lineage count drift");
    let err = err.to_string();

    assert!(err.contains("lakecatHandoffVerifyOutput"), "{err}");
    assert!(err.contains("lineageDrainArtifactSemantics"), "{err}");
    assert!(err.contains("lineageEvents mismatch"), "{err}");
}

#[test]
fn qglake_handoff_artifact_verifier_rejects_handoff_verify_output_graph_count_drift() {
    let temp = qglake_temp_dir("handoff-artifacts-self-verify-graph-count-drift");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    let mut output = qglake_bind_handoff_verify_output_artifact(&temp, &mut summary);
    output["querygraphImportPlanSemantics"]["graphNodes"] = json!(5);
    let bytes = serde_json::to_vec_pretty(&output).expect("drifted handoff verify JSON");
    fs::write(temp.join("lakecat-handoff-verify.json"), &bytes)
        .expect("write drifted handoff verify output");
    summary["artifacts"]["lakecatHandoffVerifyOutputHash"] = json!(content_hash_bytes(&bytes));
    fs::write(
        &summary_path,
        serde_json::to_vec_pretty(&summary).expect("summary JSON"),
    )
    .expect("write summary");

    let err = verify_qglake_handoff_artifact_files(&summary_path, &summary)
        .expect_err("artifact verifier should reject handoff verifier graph count drift");

    assert!(err.to_string().contains("lakecatHandoffVerifyOutput"));
    assert!(err.to_string().contains("querygraphImportPlanSemantics"));
    assert!(err.to_string().contains("graphNodes mismatch"));
}

#[test]
fn qglake_handoff_artifact_verifier_rejects_handoff_verify_output_graph_edge_count_drift() {
    let temp = qglake_temp_dir("handoff-artifacts-self-verify-graph-edge-count-drift");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    let mut output = qglake_bind_handoff_verify_output_artifact(&temp, &mut summary);
    output["querygraphImportPlanSemantics"]["graphEdges"] = json!(5);
    let bytes = serde_json::to_vec_pretty(&output).expect("drifted handoff verify JSON");
    fs::write(temp.join("lakecat-handoff-verify.json"), &bytes)
        .expect("write drifted handoff verify output");
    summary["artifacts"]["lakecatHandoffVerifyOutputHash"] = json!(content_hash_bytes(&bytes));
    fs::write(
        &summary_path,
        serde_json::to_vec_pretty(&summary).expect("summary JSON"),
    )
    .expect("write summary");

    let err = verify_qglake_handoff_artifact_files(&summary_path, &summary)
        .expect_err("artifact verifier should reject handoff verifier graph edge count drift");

    let err = err.to_string();
    assert!(err.contains("lakecatHandoffVerifyOutput"), "{err}");
    assert!(err.contains("querygraphImportPlanSemantics"), "{err}");
    assert!(err.contains("graphEdges mismatch"), "{err}");
}

#[test]
fn qglake_handoff_artifact_verifier_rejects_handoff_verify_output_extra_bundle_semantics() {
    let temp = qglake_temp_dir("handoff-artifacts-self-verify-extra-bundle-semantics");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    let mut output = qglake_bind_handoff_verify_output_artifact(&temp, &mut summary);
    output["bundleArtifactSemantics"]["unverifiedBundleProof"] = json!({
        "sha256": qglake_fixture_hash("unverified-bundle-proof")
    });
    let bytes = serde_json::to_vec_pretty(&output).expect("drifted handoff verify JSON");
    fs::write(temp.join("lakecat-handoff-verify.json"), &bytes)
        .expect("write drifted handoff verify output");
    summary["artifacts"]["lakecatHandoffVerifyOutputHash"] = json!(content_hash_bytes(&bytes));
    fs::write(
        &summary_path,
        serde_json::to_vec_pretty(&summary).expect("summary JSON"),
    )
    .expect("write summary");

    let err = verify_qglake_handoff_artifact_files(&summary_path, &summary)
        .expect_err("artifact verifier should reject extra bundle semantic proof claims");
    let err = err.to_string();

    assert!(err.contains("lakecatHandoffVerifyOutput"), "{err}");
    assert!(err.contains("bundleArtifactSemantics"), "{err}");
    assert!(
        err.contains("unexpected field unverifiedBundleProof"),
        "{err}"
    );
}

#[test]
fn qglake_handoff_artifact_verifier_rejects_handoff_verify_output_extra_import_plan_semantics() {
    let temp = qglake_temp_dir("handoff-artifacts-self-verify-extra-import-plan-semantics");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    let mut output = qglake_bind_handoff_verify_output_artifact(&temp, &mut summary);
    output["querygraphImportPlanSemantics"]["unverifiedImportPlanProof"] = json!({
        "sha256": qglake_fixture_hash("unverified-import-plan-proof")
    });
    let bytes = serde_json::to_vec_pretty(&output).expect("drifted handoff verify JSON");
    fs::write(temp.join("lakecat-handoff-verify.json"), &bytes)
        .expect("write drifted handoff verify output");
    summary["artifacts"]["lakecatHandoffVerifyOutputHash"] = json!(content_hash_bytes(&bytes));
    fs::write(
        &summary_path,
        serde_json::to_vec_pretty(&summary).expect("summary JSON"),
    )
    .expect("write summary");

    let err = verify_qglake_handoff_artifact_files(&summary_path, &summary)
        .expect_err("artifact verifier should reject extra import-plan semantic proof claims");
    let err = err.to_string();

    assert!(err.contains("lakecatHandoffVerifyOutput"), "{err}");
    assert!(err.contains("querygraphImportPlanSemantics"), "{err}");
    assert!(
        err.contains("unexpected field unverifiedImportPlanProof"),
        "{err}"
    );
}

#[test]
fn qglake_handoff_artifact_verifier_rejects_drifted_path_alias() {
    let temp = qglake_temp_dir("handoff-artifacts-alias-drift");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    summary["artifacts"]["querygraphVerifyOutput"] =
        json!(temp.join("other-querygraph-verify.json"));
    fs::write(temp.join("other-querygraph-verify.json"), b"{}")
        .expect("write drifted QueryGraph verify alias target");
    fs::write(
        &summary_path,
        serde_json::to_vec_pretty(&summary).expect("summary JSON"),
    )
    .expect("write summary");

    let err = verify_qglake_handoff_artifact_files(&summary_path, &summary)
        .expect_err("artifact verifier should reject alias drift");

    assert!(err.to_string().contains("querygraphVerifyOutput"));
    assert!(
        err.to_string()
            .contains("capturedOutputs.querygraphVerify.path")
    );
}

#[test]
fn qglake_handoff_artifact_verifier_rejects_service_log_hash_drift() {
    let temp = qglake_temp_dir("handoff-artifacts-service-log-drift");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    qglake_bind_handoff_verify_output_artifact(&temp, &mut summary);
    fs::write(temp.join("lakecat-service.log"), b"tampered service log")
        .expect("tamper service log");
    fs::write(
        &summary_path,
        serde_json::to_vec_pretty(&summary).expect("summary JSON"),
    )
    .expect("write summary");

    let err = verify_qglake_handoff_artifact_files(&summary_path, &summary)
        .expect_err("artifact verifier should reject service log hash drift");

    assert!(err.to_string().contains("serviceLog"));
    assert!(err.to_string().contains("hash mismatch"));
}

#[test]
fn qglake_handoff_artifact_semantics_reject_saved_import_plan_graph_count_drift() {
    let temp = qglake_temp_dir("handoff-import-plan-graph-drift");
    let summary_path = temp.join("handoff-summary.json");
    let (mut summary, _) = qglake_handoff_summary_json_with_verified_bundle(&temp);
    summary["lakecatReplayVerification"]["viewReceiptChainProof"]["views"] = json!([]);
    let mut plan = qglake_write_handoff_import_plan_artifact(&temp, &mut summary);
    plan["graph-nodes"] = json!(5);
    let bytes = serde_json::to_vec_pretty(&plan).expect("drifted import plan JSON");
    fs::write(temp.join("querygraph-import-plan.json"), &bytes).expect("write drifted import plan");
    summary["artifacts"]["querygraphImportPlan"]["sha256"] = json!(content_hash_bytes(&bytes));
    fs::write(
        &summary_path,
        serde_json::to_vec_pretty(&summary).expect("summary JSON"),
    )
    .expect("write summary");

    let bundle_semantics = verify_qglake_handoff_bundle_artifact_semantics(&summary_path, &summary)
        .expect("bundle artifact semantics should verify");
    let import_plan_semantics =
        verify_qglake_handoff_querygraph_import_plan_semantics(&summary_path, &summary)
            .expect("import plan artifact semantics should verify before cross-check");
    let err = require_qglake_import_plan_graph_counts_match_bundle(
        &bundle_semantics,
        &import_plan_semantics,
    )
    .expect_err("handoff should reject saved import-plan graph count drift");

    assert!(err.to_string().contains("querygraphImportPlanSemantics"));
    assert!(err.to_string().contains("graphNodes mismatch"));
}

#[test]
fn qglake_handoff_artifact_semantics_reject_saved_import_plan_graph_edge_count_drift() {
    let temp = qglake_temp_dir("handoff-import-plan-graph-edge-drift");
    let summary_path = temp.join("handoff-summary.json");
    let (mut summary, bundle) = qglake_handoff_summary_json_with_verified_bundle(&temp);
    summary["lakecatReplayVerification"]["viewReceiptChainProof"]["views"] = json!([]);
    let mut plan = qglake_write_handoff_import_plan_artifact(&temp, &mut summary);
    plan["graph-nodes"] = json!(bundle.graph.nodes.len());
    plan["graph-edges"] = json!(bundle.graph.edges.len() + 1);
    let bytes = serde_json::to_vec_pretty(&plan).expect("drifted import plan JSON");
    fs::write(temp.join("querygraph-import-plan.json"), &bytes).expect("write drifted import plan");
    summary["artifacts"]["querygraphImportPlan"]["sha256"] = json!(content_hash_bytes(&bytes));
    fs::write(
        &summary_path,
        serde_json::to_vec_pretty(&summary).expect("summary JSON"),
    )
    .expect("write summary");

    let bundle_semantics = verify_qglake_handoff_bundle_artifact_semantics(&summary_path, &summary)
        .expect("bundle artifact semantics should verify");
    let import_plan_semantics =
        verify_qglake_handoff_querygraph_import_plan_semantics(&summary_path, &summary)
            .expect("import plan artifact semantics should verify before cross-check");
    let err = require_qglake_import_plan_graph_counts_match_bundle(
        &bundle_semantics,
        &import_plan_semantics,
    )
    .expect_err("handoff should reject saved import-plan graph edge count drift");

    let err = err.to_string();
    assert!(err.contains("querygraphImportPlanSemantics"), "{err}");
    assert!(err.contains("graphEdges mismatch"), "{err}");
}

#[test]
fn qglake_handoff_artifact_verifier_rejects_hash_mismatch() {
    let temp = qglake_temp_dir("handoff-artifacts-mismatch");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    fs::write(temp.join("lakecat-bootstrap.json"), b"tampered").expect("tamper bundle");
    summary["artifacts"]["bundle"]["sha256"] = json!(content_hash_bytes(b"bundle"));

    let err = verify_qglake_handoff_artifact_files(&summary_path, &summary)
        .expect_err("artifact hashes should reject tampered files");
    assert!(
        err.to_string()
            .contains("handoff artifact bundle hash mismatch")
    );
}

#[test]
fn qglake_handoff_artifact_verifier_rejects_captured_output_mismatch() {
    let temp = qglake_temp_dir("handoff-captured-output-mismatch");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    fs::write(temp.join("querygraph-verify.json"), b"tampered")
        .expect("tamper QueryGraph verify output");
    summary["artifacts"]["capturedOutputs"]["querygraphVerify"]["sha256"] =
        json!(content_hash_bytes(b"querygraph-verify"));

    let err = verify_qglake_handoff_artifact_files(&summary_path, &summary)
        .expect_err("captured output hashes should reject tampered files");
    assert!(
        err.to_string()
            .contains("handoff artifact querygraphVerify hash mismatch")
    );
}

#[test]
fn qglake_handoff_artifact_verifier_rejects_extra_artifact_fields() {
    let temp = qglake_temp_dir("handoff-extra-artifact-fields");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    summary["artifacts"]["bundle"]["unverifiedMirrorHash"] =
        json!(qglake_fixture_hash("unverified-mirror"));

    let err = verify_qglake_handoff_artifact_files(&summary_path, &summary)
        .expect_err("artifact verifier should reject extra artifact object fields");
    let err = err.to_string();
    assert!(err.contains("handoff summary artifact"), "{err}");
    assert!(
        err.contains("unexpected field unverifiedMirrorHash"),
        "{err}"
    );

    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    summary["artifacts"]["unverifiedArtifact"] = json!({
        "path": temp.join("shadow.json"),
        "sha256": qglake_fixture_hash("shadow")
    });
    let err = verify_qglake_handoff_artifact_files(&summary_path, &summary)
        .expect_err("artifact verifier should reject extra artifact manifest fields");
    let err = err.to_string();
    assert!(err.contains("handoff summary artifacts"), "{err}");
    assert!(err.contains("unexpected field unverifiedArtifact"), "{err}");
}

#[test]
fn qglake_handoff_artifact_verifier_rejects_extra_non_bundle_summary_artifact_fields() {
    for artifact in ["lineageDrain", "querygraphImportPlan"] {
        let temp = qglake_temp_dir(&format!("handoff-extra-{artifact}-artifact-fields"));
        let summary_path = temp.join("handoff-summary.json");
        let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
        summary["artifacts"][artifact]["unverifiedMirrorHash"] =
            json!(qglake_fixture_hash("unverified-non-bundle-mirror"));

        let err = verify_qglake_handoff_artifact_files(&summary_path, &summary)
            .expect_err("artifact verifier should reject extra non-bundle artifact fields");
        let err = err.to_string();

        assert!(
            err.contains(&format!("handoff summary artifacts.{artifact}")),
            "{err}"
        );
        assert!(
            err.contains("unexpected field unverifiedMirrorHash"),
            "{err}"
        );
    }
}

#[test]
fn qglake_handoff_artifact_verifier_rejects_extra_captured_output_fields() {
    let temp = qglake_temp_dir("handoff-extra-captured-output-fields");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    summary["artifacts"]["capturedOutputs"]["querygraphVerify"]["alternateHash"] =
        json!(qglake_fixture_hash("alternate-querygraph-verify"));

    let err = verify_qglake_handoff_artifact_files(&summary_path, &summary)
        .expect_err("artifact verifier should reject extra captured-output artifact fields");
    let err = err.to_string();
    assert!(err.contains("handoff summary artifact"), "{err}");
    assert!(err.contains("unexpected field alternateHash"), "{err}");

    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    summary["artifacts"]["capturedOutputs"]["unverifiedCapture"] = json!({
        "path": temp.join("shadow-output.json"),
        "sha256": qglake_fixture_hash("shadow-output")
    });
    let err = verify_qglake_handoff_artifact_files(&summary_path, &summary)
        .expect_err("artifact verifier should reject extra captured-output manifest fields");
    let err = err.to_string();
    assert!(
        err.contains("handoff summary artifacts.capturedOutputs"),
        "{err}"
    );
    assert!(err.contains("unexpected field unverifiedCapture"), "{err}");
}
