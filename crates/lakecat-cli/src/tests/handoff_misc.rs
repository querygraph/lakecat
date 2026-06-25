use super::common::*;
use crate::*;

#[test]
fn qglake_handoff_bundle_artifact_semantics_accept_verified_bundle() {
    let temp = qglake_temp_dir("handoff-bundle-semantics-ok");
    let summary_path = temp.join("handoff-summary.json");
    let (summary, bundle) = qglake_handoff_summary_json_with_verified_bundle(&temp);
    let bundle_verification = bundle.verify_manifest().expect("bundle verifies");

    let semantics = verify_qglake_handoff_bundle_artifact_semantics(&summary_path, &summary)
        .expect("bundle artifact semantics should verify");

    assert_eq!(
        semantics["bundleHash"],
        json!(bundle_verification.bundle_hash)
    );
    assert_eq!(
        semantics["verifiedTables"],
        json!(["lakecat:table:local:default:events"])
    );
    assert_eq!(semantics["viewCount"], json!(0));
}

#[test]
fn qglake_handoff_bundle_artifact_semantics_rejects_detached_tenant_graph() {
    let temp = qglake_temp_dir("handoff-bundle-semantics-tenant-drift");
    let summary_path = temp.join("handoff-summary.json");
    let (mut summary, mut bundle) = qglake_handoff_summary_json_with_verified_bundle(&temp);
    bundle.graph.edges.retain(|edge| edge.label != "HAS_SERVER");
    qglake_resync_bundle_hashes(&mut bundle);
    qglake_write_handoff_bundle_artifact(&temp, &mut summary, &bundle);

    let err = verify_qglake_handoff_bundle_artifact_semantics(&summary_path, &summary)
        .expect_err("bundle artifact semantics should reject detached tenant graph");

    assert!(err.to_string().contains("Catalog to a Server"));
}

#[test]
fn qglake_handoff_querygraph_import_plan_semantics_accept_matching_plan() {
    let temp = qglake_temp_dir("handoff-import-plan-semantics-ok");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    qglake_write_handoff_import_plan_artifact(&temp, &mut summary);

    let semantics = verify_qglake_handoff_querygraph_import_plan_semantics(&summary_path, &summary)
        .expect("QueryGraph import plan artifact semantics should verify");

    assert_eq!(
        semantics["verifiedTables"],
        json!(["lakecat:table:local:default:events"])
    );
    assert_eq!(
        semantics["verifiedViews"],
        json!(["lakecat:view:local:default:active_customers_view"])
    );
    assert_eq!(semantics["graphNodes"], json!(8));
    assert_eq!(semantics["graphEdges"], json!(8));
}

#[test]
fn qglake_handoff_querygraph_import_plan_semantics_rejects_table_drift() {
    let temp = qglake_temp_dir("handoff-import-plan-semantics-table-drift");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    let mut plan = qglake_write_handoff_import_plan_artifact(&temp, &mut summary);
    plan["tables"][0]["stable-id"] = json!("lakecat:table:local:default:other_events");
    let bytes = serde_json::to_vec_pretty(&plan).expect("drifted import plan JSON");
    fs::write(temp.join("querygraph-import-plan.json"), &bytes).expect("write drifted import plan");
    summary["artifacts"]["querygraphImportPlan"]["sha256"] = json!(content_hash_bytes(&bytes));

    let err = verify_qglake_handoff_querygraph_import_plan_semantics(&summary_path, &summary)
        .expect_err("QueryGraph import plan artifact semantics should reject table drift");

    assert!(
        err.to_string()
            .contains("tables must include stable-id lakecat:table:local:default:events")
    );
}

#[test]
fn qglake_handoff_querygraph_import_plan_semantics_rejects_extra_root_fields() {
    let temp = qglake_temp_dir("handoff-import-plan-semantics-extra-root");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    let mut plan = qglake_write_handoff_import_plan_artifact(&temp, &mut summary);
    plan["unverified-import-root-claim"] = json!({
        "sha256": qglake_fixture_hash("unverified-import-root-claim")
    });
    let bytes = serde_json::to_vec_pretty(&plan).expect("drifted import plan JSON");
    fs::write(temp.join("querygraph-import-plan.json"), &bytes).expect("write drifted import plan");
    summary["artifacts"]["querygraphImportPlan"]["sha256"] = json!(content_hash_bytes(&bytes));

    let err = verify_qglake_handoff_querygraph_import_plan_semantics(&summary_path, &summary)
        .expect_err("QueryGraph import plan artifact semantics should reject extra root fields");
    let err = err.to_string();

    assert!(
        err.contains("handoff QueryGraph import plan artifact"),
        "{err}"
    );
    assert!(
        err.contains("unexpected field unverified-import-root-claim"),
        "{err}"
    );
}

#[test]
fn qglake_handoff_querygraph_import_plan_semantics_rejects_extra_verification_fields() {
    let temp = qglake_temp_dir("handoff-import-plan-semantics-extra-verification");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    let mut plan = qglake_write_handoff_import_plan_artifact(&temp, &mut summary);
    plan["verification"]["unverified-import-verification-claim"] = json!({
        "sha256": qglake_fixture_hash("unverified-import-verification-claim")
    });
    let bytes = serde_json::to_vec_pretty(&plan).expect("drifted import plan JSON");
    fs::write(temp.join("querygraph-import-plan.json"), &bytes).expect("write drifted import plan");
    summary["artifacts"]["querygraphImportPlan"]["sha256"] = json!(content_hash_bytes(&bytes));

    let err = verify_qglake_handoff_querygraph_import_plan_semantics(&summary_path, &summary)
        .expect_err(
            "QueryGraph import plan artifact semantics should reject extra verification fields",
        );
    let err = err.to_string();

    assert!(
        err.contains("handoff QueryGraph import plan artifact.verification"),
        "{err}"
    );
    assert!(
        err.contains("unexpected field unverified-import-verification-claim"),
        "{err}"
    );
}

#[test]
fn qglake_handoff_querygraph_import_plan_semantics_rejects_extra_table_fields() {
    let temp = qglake_temp_dir("handoff-import-plan-semantics-extra-table");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    let mut plan = qglake_write_handoff_import_plan_artifact(&temp, &mut summary);
    plan["tables"][0]["unverified-table-import-claim"] = json!({
        "sha256": qglake_fixture_hash("unverified-table-import-claim")
    });
    let bytes = serde_json::to_vec_pretty(&plan).expect("drifted import plan JSON");
    fs::write(temp.join("querygraph-import-plan.json"), &bytes).expect("write drifted import plan");
    summary["artifacts"]["querygraphImportPlan"]["sha256"] = json!(content_hash_bytes(&bytes));

    let err = verify_qglake_handoff_querygraph_import_plan_semantics(&summary_path, &summary)
        .expect_err("QueryGraph import plan artifact semantics should reject extra table fields");
    let err = err.to_string();

    assert!(
        err.contains("handoff QueryGraph import plan artifact.tables[0]"),
        "{err}"
    );
    assert!(
        err.contains("unexpected field unverified-table-import-claim"),
        "{err}"
    );
}

#[test]
fn qglake_handoff_querygraph_import_plan_semantics_rejects_extra_view_fields() {
    let temp = qglake_temp_dir("handoff-import-plan-semantics-extra-view");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    let mut plan = qglake_write_handoff_import_plan_artifact(&temp, &mut summary);
    plan["views"][0]["unverified-view-import-claim"] = json!({
        "sha256": qglake_fixture_hash("unverified-view-import-claim")
    });
    let bytes = serde_json::to_vec_pretty(&plan).expect("drifted import plan JSON");
    fs::write(temp.join("querygraph-import-plan.json"), &bytes).expect("write drifted import plan");
    summary["artifacts"]["querygraphImportPlan"]["sha256"] = json!(content_hash_bytes(&bytes));

    let err = verify_qglake_handoff_querygraph_import_plan_semantics(&summary_path, &summary)
        .expect_err("QueryGraph import plan artifact semantics should reject extra view fields");
    let err = err.to_string();

    assert!(
        err.contains("handoff QueryGraph import plan artifact.views[0]"),
        "{err}"
    );
    assert!(
        err.contains("unexpected field unverified-view-import-claim"),
        "{err}"
    );
}

#[test]
fn qglake_handoff_rejects_import_plan_graph_count_drift() {
    let bundle = json!({
        "graphNodes": 8,
        "graphEdges": 8
    });
    let import_plan = json!({
        "graphNodes": 7,
        "graphEdges": 8
    });

    let err = require_qglake_import_plan_graph_counts_match_bundle(&bundle, &import_plan)
        .expect_err("handoff should reject import-plan graph count drift");

    assert!(err.to_string().contains("querygraphImportPlanSemantics"));
    assert!(err.to_string().contains("graphNodes mismatch"));
}

#[test]
fn qglake_handoff_rejects_import_plan_graph_edge_count_drift() {
    let bundle = json!({
        "graphNodes": 8,
        "graphEdges": 8
    });
    let import_plan = json!({
        "graphNodes": 8,
        "graphEdges": 7
    });

    let err = require_qglake_import_plan_graph_counts_match_bundle(&bundle, &import_plan)
        .expect_err("handoff should reject import-plan graph edge count drift");

    let err = err.to_string();
    assert!(err.contains("querygraphImportPlanSemantics"), "{err}");
    assert!(err.contains("graphEdges mismatch"), "{err}");
}

#[test]
fn qglake_handoff_lineage_drain_artifact_semantics_accept_matching_drain() {
    let temp = qglake_temp_dir("handoff-lineage-drain-semantics-ok");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    let drain = qglake_handoff_lineage_drain_with_config();
    qglake_write_handoff_lineage_drain_artifact(&temp, &mut summary, &drain);

    let semantics = verify_qglake_handoff_lineage_drain_artifact_semantics(&summary_path, &summary)
        .expect("lineage drain artifact semantics should verify");

    assert_eq!(semantics["delivered"], json!(15));
    assert_eq!(
        semantics["verifiedViews"],
        json!(["lakecat:view:local:default:active_customers_view"])
    );
    assert_eq!(
        semantics["queryGraphImportHash"],
        json!(qglake_fixture_hash("querygraph-import"))
    );
    assert_eq!(
        semantics["requestIdentitySource"],
        json!("x-lakecat-agent-did")
    );
    assert_eq!(semantics["requestIdentityState"], json!("unverified"));
    assert_eq!(semantics["typedidEnvelopeHash"], Value::Null);
    assert_eq!(semantics["typedidProofHash"], Value::Null);
    assert_eq!(
        semantics["catalogConfigProof"]["authorizationReceiptAction"],
        json!("catalog-config")
    );
}

#[test]
fn qglake_handoff_lineage_drain_artifact_semantics_rejects_replay_drift() {
    let temp = qglake_temp_dir("handoff-lineage-drain-semantics-replay-drift");
    let summary_path = temp.join("handoff-summary.json");
    let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
    let mut drain = qglake_handoff_lineage_drain_with_config();
    qglake_write_handoff_lineage_drain_artifact(&temp, &mut summary, &drain);
    let bootstrap = drain
        .events
        .iter_mut()
        .find(|event| event.event_type == "querygraph.bootstrap")
        .expect("bootstrap replay event");
    bootstrap.replay_event_hashes = vec![qglake_fixture_hash("drifted-bootstrap-replay")];
    let bytes = serde_json::to_vec_pretty(&drain).expect("drifted lineage drain JSON");
    fs::write(temp.join("lineage-drain.json"), &bytes).expect("write drifted lineage drain");
    summary["artifacts"]["lineageDrain"]["sha256"] = json!(content_hash_bytes(&bytes));

    let err = verify_qglake_handoff_lineage_drain_artifact_semantics(&summary_path, &summary)
        .expect_err("lineage drain artifact semantics should reject replay drift");

    assert!(
        err.to_string()
            .contains("captured LakeCat replay output.replay-evidence.queryGraphBootstrap")
    );
}
