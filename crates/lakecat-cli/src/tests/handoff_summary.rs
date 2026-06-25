use super::common::*;
use crate::*;

#[test]
fn qglake_handoff_summary_verifier_accepts_compact_proofs() {
    let summary = qglake_handoff_summary_json();
    let verification =
        verify_qglake_handoff_summary_value(&summary).expect("handoff summary should verify");

    assert_eq!(
        verification["schemaVersion"],
        json!("lakecat.qglake.handoff-verification.v1")
    );
    assert_eq!(verification["status"], json!("verified"));
    assert_eq!(verification["principal"], json!("did:example:agent"));
    assert_eq!(verification["warehouse"], json!("local"));
    assert_eq!(verification["namespace"], json!("default"));
    assert_eq!(verification["table"], json!("events"));
    assert_eq!(verification["tableCount"], json!(1));
    assert_eq!(verification["viewCount"], json!(1));
    assert_eq!(
        verification["verifiedTables"],
        json!(["lakecat:table:local:default:events"])
    );
    assert_eq!(
        verification["verifiedViews"],
        json!(["lakecat:view:local:default:active_customers_view"])
    );
    assert_eq!(
        verification["queryGraphBootstrapProof"]["bundleHash"],
        json!(qglake_fixture_hash("bundle"))
    );
    assert_eq!(
        verification["graphProjectionProof"]["backend"],
        json!("grust-turso")
    );
    assert_eq!(
        verification["graphProjectionProof"]["feature"],
        json!("grust-turso-local")
    );
    assert_eq!(
        verification["graphProjectionProof"]["tablePrefix"],
        json!(QGLAKE_GRUST_TURSO_TABLE_PREFIX)
    );
}

#[test]
fn qglake_handoff_summary_verifier_rejects_extra_root_fields() {
    let mut summary = qglake_handoff_summary_json();
    summary["unverifiedTopLevelProof"] = json!({
        "sha256": qglake_fixture_hash("unverified-top-level-proof")
    });

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject extra root proof fields");
    let err = err.to_string();

    assert!(err.contains("handoff summary"), "{err}");
    assert!(
        err.contains("unexpected field unverifiedTopLevelProof"),
        "{err}"
    );
}

#[test]
fn qglake_handoff_summary_verifier_requires_graph_projection_proof() {
    let mut summary = qglake_handoff_summary_json();
    summary
        .as_object_mut()
        .unwrap()
        .remove("graphProjectionProof");

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should require graph projection proof");

    assert!(err.to_string().contains("graphProjectionProof"));
}

#[test]
fn qglake_handoff_summary_verifier_rejects_graph_projection_backend_drift() {
    let mut summary = qglake_handoff_summary_json();
    summary["graphProjectionProof"]["backend"] = json!("grust-memory");

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject graph projection backend drift");

    assert!(err.to_string().contains("graphProjectionProof"));
    assert!(err.to_string().contains("backend"));
}

#[test]
fn qglake_handoff_summary_verifier_rejects_graph_projection_path_hash_drift() {
    let mut summary = qglake_handoff_summary_json();
    summary["graphProjectionProof"]["pathHash"] = json!("sha256:short");

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject malformed graph projection path hash");

    assert!(err.to_string().contains("graphProjectionProof"));
    assert!(err.to_string().contains("pathHash"));
    assert!(err.to_string().contains("full SHA-256"));
}

#[test]
fn qglake_handoff_summary_verifier_rejects_graph_projection_table_prefix_drift() {
    let mut summary = qglake_handoff_summary_json();
    summary["graphProjectionProof"]["tablePrefix"] = json!("other_graph");

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject graph projection table prefix drift");

    assert!(err.to_string().contains("graphProjectionProof"));
    assert!(err.to_string().contains("tablePrefix"));
}

#[test]
fn qglake_handoff_summary_verifier_rejects_extra_graph_projection_fields() {
    let mut summary = qglake_handoff_summary_json();
    summary["graphProjectionProof"]["unverifiedBackendClaim"] =
        json!(qglake_fixture_hash("unverified-graph-backend-claim"));

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject extra graph projection proof fields");

    let err = err.to_string();
    assert!(err.contains("graphProjectionProof"), "{err}");
    assert!(
        err.contains("unexpected field unverifiedBackendClaim"),
        "{err}"
    );
}

#[test]
fn qglake_handoff_summary_verifier_rejects_blank_principal_anchor() {
    let mut summary = qglake_handoff_summary_json();
    summary["principal"] = json!("   ");
    summary["lakecatReplayVerification"]["requestIdentityProof"]["principalSubject"] = json!("   ");
    summary["lakecatReplayVerification"]["queryGraphBootstrapProof"]["principalSubject"] =
        json!("   ");
    summary["lakecatReplayVerification"]["governedScanProof"]["plannedPrincipalSubject"] =
        json!("   ");
    summary["lakecatReplayVerification"]["governedScanProof"]["fetchedPrincipalSubject"] =
        json!("   ");
    summary["lakecatReplayVerification"]["tableCommitHistoryProof"]["principalSubject"] =
        json!("   ");
    summary["lakecatReplayVerification"]["credentialVendingProof"]["restricted"]["principalSubject"] =
        json!("   ");

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject a blank principal anchor");

    assert!(err.to_string().contains("handoff summary.principal"));
    assert!(err.to_string().contains("blank"));
}

#[test]
fn qglake_handoff_summary_verifier_rejects_blank_warehouse_anchor() {
    let mut summary = qglake_handoff_summary_json();
    summary["warehouse"] = json!("   ");
    let mirrored_table = json!(["lakecat:table:   :default:events"]);
    summary["querygraphVerification"]["verifiedTables"] = mirrored_table.clone();
    summary["querygraphImportVerification"]["verifiedTables"] = mirrored_table;

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject a blank warehouse anchor");

    assert!(err.to_string().contains("handoff summary.warehouse"));
    assert!(err.to_string().contains("blank"));
}

#[test]
fn qglake_handoff_summary_verifier_rejects_blank_table_scope_anchors() {
    let mut summary = qglake_handoff_summary_json();
    summary["namespace"] = json!("   ");
    summary["table"] = json!("   ");
    let mirrored_table = json!(["lakecat:table:local:   :   "]);
    summary["querygraphVerification"]["verifiedTables"] = mirrored_table.clone();
    summary["querygraphImportVerification"]["verifiedTables"] = mirrored_table;

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject blank namespace/table anchors");

    assert!(err.to_string().contains("handoff summary.namespace"));
    assert!(err.to_string().contains("blank"));
}

#[test]
fn qglake_handoff_summary_verifier_requires_catalog_scope() {
    let mut summary = qglake_handoff_summary_json();
    summary.as_object_mut().unwrap().remove("warehouse");

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject missing warehouse scope");

    assert!(err.to_string().contains("handoff summary"));
    assert!(err.to_string().contains("warehouse"));
}

#[test]
fn qglake_handoff_summary_verifier_rejects_empty_catalog_scope() {
    let mut summary = qglake_handoff_summary_json();
    summary["namespace"] = json!("");

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject empty namespace scope");

    assert!(err.to_string().contains("handoff summary"));
    assert!(err.to_string().contains("namespace"));
    assert!(err.to_string().contains("must not be empty"));
}

#[test]
fn qglake_handoff_summary_verifier_rejects_malformed_catalog_url() {
    let mut summary = qglake_handoff_summary_json();
    summary["catalogUrl"] = json!("not a url");

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject malformed catalog URLs");

    assert!(err.to_string().contains("catalogUrl"));
    assert!(err.to_string().contains("HTTP(S) URL"));
}

#[test]
fn qglake_handoff_summary_verifier_rejects_non_http_catalog_url() {
    let mut summary = qglake_handoff_summary_json();
    summary["catalogUrl"] = json!("file:///tmp/lakecat");

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject non-HTTP catalog URLs");

    assert!(err.to_string().contains("catalogUrl"));
    assert!(err.to_string().contains("HTTP(S) URL"));
}

#[test]
fn qglake_handoff_summary_verifier_requires_verified_table_scope() {
    let mut summary = qglake_handoff_summary_json();
    summary["querygraphVerification"]["verifiedTables"] =
        json!(["lakecat:table:local:default:other"]);
    summary["querygraphImportVerification"]["verifiedTables"] =
        json!(["lakecat:table:local:default:other"]);

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject table scope drift");

    assert!(err.to_string().contains("querygraphVerification"));
    assert!(err.to_string().contains("verifiedTables"));
    assert!(
        err.to_string()
            .contains("lakecat:table:local:default:events")
    );
}

#[test]
fn qglake_handoff_summary_verifier_rejects_verified_table_count_mismatch() {
    let mut summary = qglake_handoff_summary_json();
    summary["querygraphVerification"]["verifiedTables"] = json!([
        "lakecat:table:local:default:events",
        "lakecat:table:local:default:other"
    ]);
    summary["querygraphImportVerification"]["verifiedTables"] = json!([
        "lakecat:table:local:default:events",
        "lakecat:table:local:default:other"
    ]);

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject verified table count drift");

    assert!(err.to_string().contains("querygraphVerification"));
    assert!(err.to_string().contains("verifiedTables length mismatch"));
}

#[test]
fn qglake_handoff_summary_verifier_rejects_duplicate_verified_tables() {
    let mut summary = qglake_handoff_summary_json();
    summary["querygraphVerification"]["tableCount"] = json!(2);
    summary["querygraphVerification"]["verifiedTables"] = json!([
        "lakecat:table:local:default:events",
        "lakecat:table:local:default:events"
    ]);
    summary["querygraphImportVerification"]["tableCount"] = json!(2);
    summary["querygraphImportVerification"]["verifiedTables"] = json!([
        "lakecat:table:local:default:events",
        "lakecat:table:local:default:events"
    ]);
    summary["lakecatReplayVerification"]["queryGraphBootstrapProof"]["tableArtifactCount"] =
        json!(2);

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject duplicated verified tables");

    assert!(err.to_string().contains("querygraphVerification"));
    assert!(err.to_string().contains("verifiedTables"));
    assert!(err.to_string().contains("duplicate-free"));
}

#[test]
fn qglake_handoff_summary_verifier_requires_verified_view_scope() {
    let mut summary = qglake_handoff_summary_json();
    summary["querygraphVerification"]["verifiedViews"] =
        json!(["lakecat:view:local:default:other_view"]);
    summary["querygraphImportVerification"]["verifiedViews"] =
        json!(["lakecat:view:local:default:other_view"]);

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject view scope drift");

    assert!(err.to_string().contains("querygraphVerification"));
    assert!(err.to_string().contains("verifiedViews"));
    assert!(
        err.to_string()
            .contains("lakecat:view:local:default:active_customers_view")
    );
}

#[test]
fn qglake_handoff_summary_verifier_rejects_verified_view_count_mismatch() {
    let mut summary = qglake_handoff_summary_json();
    summary["querygraphVerification"]["verifiedViews"] = json!([
        "lakecat:view:local:default:active_customers_view",
        "lakecat:view:local:default:other_view"
    ]);
    summary["querygraphImportVerification"]["verifiedViews"] = json!([
        "lakecat:view:local:default:active_customers_view",
        "lakecat:view:local:default:other_view"
    ]);

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject verified view count drift");

    assert!(err.to_string().contains("querygraphVerification"));
    assert!(err.to_string().contains("verifiedViews length mismatch"));
}

#[test]
fn qglake_handoff_summary_verifier_rejects_duplicate_verified_views() {
    let mut summary = qglake_handoff_summary_json();
    summary["querygraphVerification"]["viewCount"] = json!(2);
    summary["querygraphVerification"]["verifiedViews"] = json!([
        "lakecat:view:local:default:active_customers_view",
        "lakecat:view:local:default:active_customers_view"
    ]);
    summary["querygraphImportVerification"]["viewCount"] = json!(2);
    summary["querygraphImportVerification"]["verifiedViews"] = json!([
        "lakecat:view:local:default:active_customers_view",
        "lakecat:view:local:default:active_customers_view"
    ]);
    summary["lakecatReplayVerification"]["queryGraphBootstrapProof"]["viewArtifactCount"] =
        json!(2);
    summary["lakecatReplayVerification"]["viewReceiptChainProof"]["viewCount"] = json!(2);

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject duplicated verified views");

    assert!(err.to_string().contains("querygraphVerification"));
    assert!(err.to_string().contains("verifiedViews"));
    assert!(err.to_string().contains("duplicate-free"));
}

#[test]
fn qglake_handoff_summary_verifier_rejects_import_verified_table_drift() {
    let mut summary = qglake_handoff_summary_json();
    summary["querygraphImportVerification"]["verifiedTables"] =
        json!(["lakecat:table:local:default:events_other"]);

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject import verified-table drift");

    assert!(err.to_string().contains("querygraphImportVerification"));
    assert!(err.to_string().contains("verifiedTables mismatch"));
}

#[test]
fn qglake_handoff_summary_verifier_rejects_import_hash_drift() {
    let mut summary = qglake_handoff_summary_json();
    summary["querygraphImportVerification"]["querygraphImportHash"] =
        json!(qglake_fixture_hash("other-querygraph-import"));

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject import hash drift");

    assert!(err.to_string().contains("querygraphImportVerification"));
    assert!(err.to_string().contains("querygraphImportHash mismatch"));
}

#[test]
fn qglake_handoff_summary_verifier_rejects_extra_querygraph_root_fields() {
    let mut summary = qglake_handoff_summary_json();
    summary["querygraphVerification"]["unverifiedQueryGraphClaim"] =
        json!(qglake_fixture_hash("unverified-querygraph-claim"));

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject extra QueryGraph verify root fields");
    let err = err.to_string();

    assert!(err.contains("querygraphVerification"), "{err}");
    assert!(
        err.contains("unexpected field unverifiedQueryGraphClaim"),
        "{err}"
    );
}

#[test]
fn qglake_handoff_summary_verifier_rejects_extra_querygraph_import_root_fields() {
    let mut summary = qglake_handoff_summary_json();
    summary["querygraphImportVerification"]["unverifiedImportClaim"] =
        json!(qglake_fixture_hash("unverified-querygraph-import-claim"));

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject extra QueryGraph import root fields");
    let err = err.to_string();

    assert!(err.contains("querygraphImportVerification"), "{err}");
    assert!(
        err.contains("unexpected field unverifiedImportClaim"),
        "{err}"
    );
}

#[test]
fn qglake_handoff_summary_verifier_rejects_extra_lakecat_replay_root_fields() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["unverifiedReplayClaim"] =
        json!(qglake_fixture_hash("unverified-lakecat-replay-claim"));

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject extra LakeCat replay root fields");
    let err = err.to_string();

    assert!(err.contains("lakecatReplayVerification"), "{err}");
    assert!(
        err.contains("unexpected field unverifiedReplayClaim"),
        "{err}"
    );
}

#[test]
fn qglake_handoff_summary_verifier_requires_core_bundle_hash_shape() {
    let mut summary = qglake_handoff_summary_json();
    summary["querygraphVerification"]["bundleHash"] = json!("not-a-sha256-hash");
    summary["querygraphImportVerification"]["bundleHash"] = json!("not-a-sha256-hash");
    summary["lakecatReplayVerification"]["queryGraphBootstrapProof"]["bundleHash"] =
        json!("not-a-sha256-hash");

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject malformed bundle proof anchors");

    assert!(err.to_string().contains("querygraphVerification"));
    assert!(err.to_string().contains("bundleHash"));
    assert!(err.to_string().contains("full SHA-256"));
}

#[test]
fn qglake_handoff_summary_verifier_requires_core_import_hash_shape() {
    let mut summary = qglake_handoff_summary_json();
    summary["querygraphVerification"]["querygraphImportHash"] = json!("not-a-sha256-hash");
    summary["querygraphImportVerification"]["querygraphImportHash"] = json!("not-a-sha256-hash");
    summary["lakecatReplayVerification"]["queryGraphBootstrapProof"]["queryGraphImportHash"] =
        json!("not-a-sha256-hash");

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject malformed QueryGraph import proof anchors");

    assert!(err.to_string().contains("querygraphVerification"));
    assert!(err.to_string().contains("querygraphImportHash"));
    assert!(err.to_string().contains("full SHA-256"));
}

#[test]
fn qglake_handoff_summary_verifier_rejects_short_bootstrap_replay_hashes() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["queryGraphBootstrapProof"]["replayEventHashes"] =
        json!(["sha256:bootstrap-replay"]);

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject short bootstrap replay hashes");

    assert!(err.to_string().contains("queryGraphBootstrapProof"));
    assert!(err.to_string().contains("replayEventHashes"));
    assert!(err.to_string().contains("full SHA-256"));
}

#[test]
fn qglake_handoff_summary_verifier_rejects_duplicate_bootstrap_replay_hashes() {
    let mut summary = qglake_handoff_summary_json();
    let duplicate_hash = qglake_fixture_hash("duplicate-bootstrap-replay");
    summary["lakecatReplayVerification"]["queryGraphBootstrapProof"]["replayEventHashes"] =
        json!([duplicate_hash, duplicate_hash]);

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject duplicate bootstrap replay hashes");

    assert!(err.to_string().contains("queryGraphBootstrapProof"));
    assert!(err.to_string().contains("replayEventHashes"));
    assert!(err.to_string().contains("duplicate-free"));
}

#[test]
fn qglake_handoff_summary_verifier_rejects_duplicate_bootstrap_openlineage_hashes() {
    let mut summary = qglake_handoff_summary_json();
    let duplicate_hash = qglake_fixture_hash("duplicate-bootstrap-openlineage");
    summary["lakecatReplayVerification"]["queryGraphBootstrapProof"]["openLineageHashes"] =
        json!([duplicate_hash, duplicate_hash]);

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject duplicate bootstrap OpenLineage hashes");

    assert!(err.to_string().contains("queryGraphBootstrapProof"));
    assert!(err.to_string().contains("openLineageHashes"));
    assert!(err.to_string().contains("duplicate-free"));
}

#[test]
fn qglake_handoff_summary_verifier_rejects_extra_querygraph_bootstrap_fields() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["queryGraphBootstrapProof"]["unverifiedBootstrapClaim"] =
        json!(qglake_fixture_hash("unverified-bootstrap-claim"));

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject extra QueryGraph bootstrap fields");
    let err = err.to_string();

    assert!(err.contains("queryGraphBootstrapProof"), "{err}");
    assert!(
        err.contains("unexpected field unverifiedBootstrapClaim"),
        "{err}"
    );
}

#[test]
fn qglake_handoff_summary_verifier_requires_required_standards() {
    let mut summary = qglake_handoff_summary_json();
    let incomplete = json!([
        "Iceberg REST",
        "Croissant",
        "CDIF",
        "OSI handoff",
        "Grust catalog graph",
        "OpenLineage"
    ]);
    summary["querygraphVerification"]["standards"] = incomplete.clone();
    summary["querygraphImportVerification"]["standards"] = incomplete.clone();
    summary["lakecatReplayVerification"]["queryGraphBootstrapProof"]["standards"] = incomplete;

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject incomplete QGLake standards");

    assert!(err.to_string().contains("querygraphVerification.standards"));
    assert!(err.to_string().contains("ODRL"));
}

#[test]
fn qglake_handoff_summary_verifier_rejects_malformed_standards() {
    let cases = [
        (json!(["Iceberg REST", "Iceberg REST"]), "duplicate-free"),
        (
            json!([
                "Iceberg REST",
                "Croissant",
                "CDIF",
                "OSI handoff",
                "Grust catalog graph",
                "OpenLineage",
                "ODRL",
                "Unverified local standard"
            ]),
            "unsupported QGLake standard",
        ),
        (
            json!([
                "Iceberg REST",
                "Croissant",
                "CDIF",
                "OSI handoff",
                "Grust catalog graph",
                "OpenLineage",
                "ODRL",
                42
            ]),
            "must be a string",
        ),
        (
            json!([
                "Iceberg REST",
                "Croissant",
                "CDIF",
                "OSI handoff",
                "Grust catalog graph",
                "OpenLineage",
                "ODRL",
                " "
            ]),
            "must contain non-empty strings",
        ),
    ];

    for (standards, expected_message) in cases {
        let mut summary = qglake_handoff_summary_json();
        summary["querygraphVerification"]["standards"] = standards.clone();
        summary["querygraphImportVerification"]["standards"] = standards.clone();
        summary["lakecatReplayVerification"]["queryGraphBootstrapProof"]["standards"] = standards;

        let err = verify_qglake_handoff_summary_value(&summary)
            .expect_err("handoff summary should reject malformed QGLake standards");
        let err = err.to_string();

        assert!(err.contains("querygraphVerification.standards"), "{err}");
        assert!(err.contains(expected_message), "{err}");
    }
}

#[test]
fn qglake_handoff_summary_verifier_rejects_bootstrap_hash_mismatch() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["queryGraphBootstrapProof"]["bundleHash"] =
        json!(qglake_fixture_hash("other-bundle"));

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject mismatched bootstrap hash");
    assert!(err.to_string().contains("bundleHash mismatch"));
}

#[test]
fn qglake_handoff_summary_verifier_requires_management_policy_count_match() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["managementProof"]["policyBindingCount"] = json!(2);

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject management policy-count drift");

    assert!(err.to_string().contains("managementProof"));
    assert!(err.to_string().contains("policyBindingCount mismatch"));
}

#[test]
fn qglake_handoff_summary_verifier_rejects_extra_management_fields() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["managementProof"]["unverifiedManagementClaim"] =
        json!(qglake_fixture_hash("unverified-management-claim"));

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject extra management proof fields");
    let err = err.to_string();

    assert!(err.contains("managementProof"), "{err}");
    assert!(
        err.contains("unexpected field unverifiedManagementClaim"),
        "{err}"
    );
}

#[test]
fn qglake_handoff_summary_verifier_rejects_extra_policy_upsert_fields() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["managementProof"]["policyUpsertProof"]["unverifiedPolicyClaim"] =
        json!(qglake_fixture_hash("unverified-policy-claim"));

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject extra policy-upsert proof fields");
    let err = err.to_string();

    assert!(err.contains("managementProof.policyUpsertProof"), "{err}");
    assert!(
        err.contains("unexpected field unverifiedPolicyClaim"),
        "{err}"
    );
}

#[test]
fn qglake_handoff_summary_verifier_requires_management_graph_events() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["managementProof"]["serverGraphEvents"] = json!(0);

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject missing management graph proof");

    assert!(err.to_string().contains("managementProof"));
    assert!(err.to_string().contains("serverGraphEvents"));
    assert!(err.to_string().contains("positive"));
}

#[test]
fn qglake_handoff_summary_verifier_rejects_policy_upsert_odrl_hash_drift() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["managementProof"]["policyUpsertProof"]["odrlHash"] =
        json!("sha256:short-policy-odrl");

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject malformed policy ODRL proof");

    assert!(
        err.to_string()
            .contains("managementProof.policyUpsertProof")
    );
    assert!(err.to_string().contains("odrlHash"));
    assert!(err.to_string().contains("full SHA-256"));
}

#[test]
fn qglake_handoff_summary_verifier_rejects_policy_upsert_id_drift() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["managementProof"]["policyUpsertProof"]["policyId"] =
        json!("unlisted-policy");

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject policy upsert id drift");

    assert!(
        err.to_string()
            .contains("managementProof.policyUpsertProof")
    );
    assert!(err.to_string().contains("policyId must match policyIds"));
}

#[test]
fn qglake_handoff_summary_verifier_requires_policy_upsert_authorization_hash() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["managementProof"]["policyUpsertProof"]["authorizationReceiptHash"] =
        json!("sha256:short-policy-authorization");

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject malformed policy upsert authorization hash");

    assert!(
        err.to_string()
            .contains("managementProof.policyUpsertProof")
    );
    assert!(err.to_string().contains("authorizationReceiptHash"));
    assert!(err.to_string().contains("full SHA-256"));
}

#[test]
fn qglake_handoff_summary_verifier_requires_policy_upsert_authorization_action() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["managementProof"]["policyUpsertProof"]["authorizationReceiptAction"] =
        json!("table-load");

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject drifted policy upsert authorization action");

    assert!(
        err.to_string()
            .contains("managementProof.policyUpsertProof")
    );
    assert!(err.to_string().contains("authorizationReceiptAction"));
    assert!(err.to_string().contains("policy-manage"));
}

#[test]
fn qglake_handoff_summary_verifier_requires_policy_upsert_principal_proof() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["managementProof"]["policyUpsertProof"]
        .as_object_mut()
        .unwrap()
        .remove("principalSubject");

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject missing policy upsert principal proof");

    assert!(
        err.to_string()
            .contains("managementProof.policyUpsertProof")
    );
    assert!(err.to_string().contains("principalSubject"));
}

#[test]
fn qglake_handoff_summary_verifier_requires_management_receipt_hashes() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["managementProof"]["storageProfileReplayEventHashes"] =
        json!([]);

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject missing management receipt hashes");

    assert!(err.to_string().contains("managementProof"));
    assert!(err.to_string().contains("storageProfileReplayEventHashes"));
    assert!(err.to_string().contains("full SHA-256"));
}

#[test]
fn qglake_handoff_summary_verifier_requires_management_ids() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["managementProof"]["serverIds"] = json!([]);

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject missing management ids");

    assert!(err.to_string().contains("managementProof"));
    assert!(err.to_string().contains("serverIds"));
    assert!(err.to_string().contains("count mismatch"));
}

#[test]
fn qglake_handoff_summary_verifier_rejects_duplicate_management_ids() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["managementProof"]["serverCount"] = json!(2);
    summary["lakecatReplayVerification"]["managementProof"]["serverIds"] =
        json!(["qglake-server", "qglake-server"]);

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject duplicate management ids");

    assert!(err.to_string().contains("managementProof"));
    assert!(err.to_string().contains("serverIds"));
    assert!(err.to_string().contains("duplicate-free"));
}

#[test]
fn qglake_handoff_summary_verifier_rejects_path_decorated_management_ids() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["managementProof"]["projectIds"] =
        json!(["analytics/../../secret"]);

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject path-decorated management ids");

    assert!(err.to_string().contains("managementProof"));
    assert!(err.to_string().contains("projectIds"));
    assert!(
        err.to_string()
            .contains("syntactically invalid compact management ID evidence")
    );
}

#[test]
fn qglake_handoff_summary_verifier_rejects_query_decorated_management_ids() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["managementProof"]["storageProfileIds"] =
        json!(["events?token=secret"]);

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject query-decorated management ids");

    assert!(err.to_string().contains("managementProof"));
    assert!(err.to_string().contains("storageProfileIds"));
    assert!(
        err.to_string()
            .contains("syntactically invalid compact management ID evidence")
    );
}

#[test]
fn qglake_handoff_summary_verifier_rejects_invalid_warehouse_project_scope() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["managementProof"]["warehouseProjectId"] =
        json!("analytics/../../secret");

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject invalid warehouse project scope");

    assert!(err.to_string().contains("managementProof"));
    assert!(err.to_string().contains("warehouseProjectId"));
    assert!(
        err.to_string()
            .contains("syntactically invalid compact management ID evidence")
    );
}

#[test]
fn qglake_handoff_summary_verifier_rejects_unknown_warehouse_project_scope() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["managementProof"]["warehouseProjectId"] =
        json!("unlisted-project");

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject unknown warehouse project scope");

    assert!(err.to_string().contains("managementProof"));
    assert!(
        err.to_string()
            .contains("warehouseProjectId must match projectIds")
    );
}

#[test]
fn qglake_handoff_summary_verifier_rejects_short_management_receipt_hashes() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["managementProof"]["serverReplayEventHashes"] =
        json!(["sha256:server-list-replay-event"]);

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject short management receipt hashes");

    assert!(err.to_string().contains("managementProof"));
    assert!(err.to_string().contains("serverReplayEventHashes"));
    assert!(err.to_string().contains("full SHA-256"));
}

#[test]
fn qglake_handoff_summary_verifier_rejects_duplicate_management_receipt_hashes() {
    let mut summary = qglake_handoff_summary_json();
    let duplicate_hash = qglake_fixture_hash("duplicate-server-list-replay");
    summary["lakecatReplayVerification"]["managementProof"]["serverReplayEventHashes"] =
        json!([duplicate_hash, duplicate_hash]);

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject duplicate management receipt hashes");

    assert!(err.to_string().contains("managementProof"));
    assert!(err.to_string().contains("serverReplayEventHashes"));
    assert!(err.to_string().contains("duplicate-free"));
}

#[test]
fn qglake_handoff_summary_verifier_requires_request_identity_typedid_hash_shape() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["requestIdentityProof"]["typedidEnvelopeHash"] =
        json!("not-a-sha256-hash");

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject invalid request identity TypeDID hash");

    assert!(err.to_string().contains("requestIdentityProof"));
    assert!(err.to_string().contains("typedidEnvelopeHash"));
    assert!(err.to_string().contains("full SHA-256"));
}

#[test]
fn qglake_handoff_summary_verifier_rejects_extra_request_identity_fields() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["requestIdentityProof"]["unverifiedActorClaim"] =
        json!(qglake_fixture_hash("unverified-actor-claim"));

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject unverified request identity fields");
    let err = err.to_string();

    assert!(err.contains("requestIdentityProof"), "{err}");
    assert!(
        err.contains("unexpected field unverifiedActorClaim"),
        "{err}"
    );
}

#[test]
fn qglake_handoff_summary_verifier_rejects_short_typedid_hashes() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["requestIdentityProof"]["typedidEnvelopeHash"] =
        json!("sha256:typedid-envelope");

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject short TypeDID hashes");

    assert!(err.to_string().contains("requestIdentityProof"));
    assert!(err.to_string().contains("typedidEnvelopeHash"));
    assert!(err.to_string().contains("full SHA-256"));
}

#[test]
fn qglake_handoff_summary_verifier_rejects_request_identity_typedid_proof_without_envelope() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["requestIdentityProof"]["typedidProofHash"] =
        json!(qglake_fixture_hash("typedid-proof"));

    let err = verify_qglake_handoff_summary_value(&summary).expect_err(
        "handoff summary should reject request identity TypeDID proof without envelope",
    );

    assert!(err.to_string().contains("requestIdentityProof"));
    assert!(err.to_string().contains("typedidProofHash"));
    assert!(err.to_string().contains("typedidEnvelopeHash"));
}

#[test]
fn qglake_handoff_summary_verifier_rejects_blank_request_identity_provenance() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["requestIdentityProof"]["requestIdentitySource"] =
        json!("   ");
    summary["lakecatReplayVerification"]["queryGraphBootstrapProof"]["requestIdentitySource"] =
        json!("   ");

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject blank request identity provenance");

    assert!(err.to_string().contains("requestIdentityProof"));
    assert!(err.to_string().contains("requestIdentitySource"));
    assert!(err.to_string().contains("blank"));
}

#[test]
fn qglake_handoff_summary_verifier_requires_request_identity_authorization_action() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["requestIdentityProof"]["authorizationReceiptAction"] =
        json!("graph-read");

    let err = verify_qglake_handoff_summary_value(&summary).expect_err(
        "handoff summary should reject non-lineage request identity authorization action",
    );

    assert!(err.to_string().contains("requestIdentityProof"));
    assert!(err.to_string().contains("authorizationReceiptAction"));
    assert!(err.to_string().contains("lineage-read"));
}

#[test]
fn qglake_handoff_summary_verifier_requires_bootstrap_typedid_hash_shape() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["queryGraphBootstrapProof"]["typedidEnvelopeHash"] =
        json!("not-a-sha256-hash");

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject invalid bootstrap TypeDID hash");

    assert!(err.to_string().contains("queryGraphBootstrapProof"));
    assert!(err.to_string().contains("typedidEnvelopeHash"));
    assert!(err.to_string().contains("full SHA-256"));
}

#[test]
fn qglake_handoff_summary_verifier_rejects_bootstrap_typedid_proof_without_envelope() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["queryGraphBootstrapProof"]["typedidProofHash"] =
        json!(qglake_fixture_hash("typedid-proof"));

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject bootstrap TypeDID proof without envelope");

    assert!(err.to_string().contains("queryGraphBootstrapProof"));
    assert!(err.to_string().contains("typedidProofHash"));
    assert!(err.to_string().contains("typedidEnvelopeHash"));
}

#[test]
fn qglake_handoff_summary_verifier_requires_bootstrap_authorization_action() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["queryGraphBootstrapProof"]["authorizationReceiptAction"] =
        json!("lineage-read");

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject non-graph bootstrap authorization action");

    assert!(err.to_string().contains("queryGraphBootstrapProof"));
    assert!(err.to_string().contains("authorizationReceiptAction"));
    assert!(err.to_string().contains("graph-read"));
}

#[test]
fn qglake_handoff_summary_verifier_allows_distinct_bootstrap_typedid_envelope() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["requestIdentityProof"]["typedidEnvelopeHash"] =
        json!(qglake_fixture_hash("typedid-envelope"));
    summary["lakecatReplayVerification"]["queryGraphBootstrapProof"]["typedidEnvelopeHash"] =
        json!(qglake_fixture_hash("other-typedid-envelope"));

    verify_qglake_handoff_summary_value(&summary)
        .expect("handoff summary should allow distinct request/bootstrap TypeDID envelopes");
}

#[test]
fn qglake_handoff_summary_verifier_allows_distinct_bootstrap_typedid_proof() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["requestIdentityProof"]["typedidEnvelopeHash"] =
        json!(qglake_fixture_hash("typedid-envelope"));
    summary["lakecatReplayVerification"]["requestIdentityProof"]["typedidProofHash"] =
        json!(qglake_fixture_hash("typedid-proof"));
    summary["lakecatReplayVerification"]["queryGraphBootstrapProof"]["typedidEnvelopeHash"] =
        json!(qglake_fixture_hash("typedid-envelope"));
    summary["lakecatReplayVerification"]["queryGraphBootstrapProof"]["typedidProofHash"] =
        json!(qglake_fixture_hash("other-typedid-proof"));

    verify_qglake_handoff_summary_value(&summary)
        .expect("handoff summary should allow distinct request/bootstrap TypeDID proofs");
}

#[test]
fn qglake_handoff_summary_verifier_rejects_bootstrap_identity_source_drift() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["queryGraphBootstrapProof"]["requestIdentitySource"] =
        json!("authorization-bearer");

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject bootstrap identity source drift");

    assert!(err.to_string().contains("queryGraphBootstrapProof"));
    assert!(err.to_string().contains("requestIdentitySource mismatch"));
}

#[test]
fn qglake_handoff_summary_verifier_rejects_bootstrap_identity_state_drift() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["queryGraphBootstrapProof"]["requestIdentityState"] =
        json!("verified");

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject bootstrap identity state drift");

    assert!(err.to_string().contains("queryGraphBootstrapProof"));
    assert!(err.to_string().contains("requestIdentityState mismatch"));
}

#[test]
fn qglake_handoff_summary_verifier_requires_catalog_config_proof() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]
        .as_object_mut()
        .expect("lakecat replay object")
        .remove("catalogConfigProof");

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject missing catalog config proof");

    assert!(err.to_string().contains("lakecatReplayVerification"));
    assert!(err.to_string().contains("catalogConfigProof"));
}

#[test]
fn qglake_handoff_summary_verifier_rejects_extra_catalog_config_fields() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["catalogConfigProof"]["unverifiedEndpointClaim"] =
        json!(qglake_fixture_hash("unverified-config-claim"));

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject unverified catalog config fields");
    let err = err.to_string();

    assert!(err.contains("catalogConfigProof"), "{err}");
    assert!(
        err.contains("unexpected field unverifiedEndpointClaim"),
        "{err}"
    );
}

#[test]
fn qglake_handoff_summary_verifier_rejects_extra_catalog_config_entry_fields() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["catalogConfigProof"]["defaults"][0]["unverifiedV4Claim"] =
        json!("available");

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject unverified config entry fields");
    let err = err.to_string();

    assert!(err.contains("catalogConfigProof.defaults"), "{err}");
    assert!(err.contains("unexpected field unverifiedV4Claim"), "{err}");
}

#[test]
fn qglake_handoff_summary_verifier_rejects_unsupported_config_v4_default() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["catalogConfigProof"]["defaults"]
        .as_array_mut()
        .expect("config defaults")
        .push(json!({
            "key": "lakecat.format.v4.typed-sail.preview",
            "value": "available"
        }));

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject unsupported compact config v4 defaults");

    assert!(err.to_string().contains("catalogConfigProof"));
    assert!(err.to_string().contains("unsupported v4 bridge keys"));
}

#[test]
fn qglake_handoff_summary_verifier_rejects_missing_config_endpoint() {
    for missing_endpoint in [
        "GET /querygraph/v1/bootstrap",
        "POST /catalog/v1/namespaces/{namespace}/tables/{table}/plan",
        "POST /catalog/v1/{warehouse}/namespaces/{namespace}/tables/{table}/plan",
        "POST /catalog/v1/namespaces/{namespace}/tables/{table}/fetch-scan-tasks",
        "POST /catalog/v1/{warehouse}/namespaces/{namespace}/tables/{table}/fetch-scan-tasks",
        "GET /catalog/v1/namespaces/{namespace}/tables/{table}/credentials",
        "GET /catalog/v1/{warehouse}/namespaces/{namespace}/tables/{table}/credentials",
    ] {
        let mut summary = qglake_handoff_summary_json();
        let endpoints = summary["lakecatReplayVerification"]["catalogConfigProof"]["endpoints"]
            .as_array_mut()
            .expect("config endpoints");
        endpoints.retain(|endpoint| endpoint != missing_endpoint);

        let err = verify_qglake_handoff_summary_value(&summary)
            .expect_err("handoff summary should reject missing compact config endpoint");

        assert!(err.to_string().contains("catalogConfigProof"));
        assert!(err.to_string().contains(missing_endpoint));
    }
}

#[test]
fn qglake_handoff_summary_verifier_allows_distinct_bootstrap_authorization_receipt() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["queryGraphBootstrapProof"]["authorizationReceiptHash"] =
        json!(qglake_fixture_hash("other-authorization"));

    verify_qglake_handoff_summary_value(&summary)
        .expect("handoff summary should allow distinct request/bootstrap authorization receipts");
}

#[test]
fn qglake_handoff_summary_verifier_rejects_short_authorization_hashes() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["requestIdentityProof"]["authorizationReceiptHash"] =
        json!("sha256:identity");

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject short authorization hashes");

    assert!(err.to_string().contains("requestIdentityProof"));
    assert!(err.to_string().contains("authorizationReceiptHash"));
    assert!(err.to_string().contains("full SHA-256"));
}

#[test]
fn qglake_handoff_summary_verifier_requires_storage_profile_issuance_mode() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["storageProfileUpsertProof"]
        .as_object_mut()
        .unwrap()
        .remove("issuanceMode");

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject missing issuance mode");

    assert!(err.to_string().contains("storageProfileUpsertProof"));
    assert!(err.to_string().contains("issuanceMode"));
}

#[test]
fn qglake_handoff_summary_verifier_rejects_extra_storage_profile_fields() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["storageProfileUpsertProof"]["unverifiedStorageClaim"] =
        json!(qglake_fixture_hash("unverified-storage-claim"));

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject extra storage-profile proof fields");
    let err = err.to_string();

    assert!(err.contains("storageProfileUpsertProof"), "{err}");
    assert!(
        err.contains("unexpected field unverifiedStorageClaim"),
        "{err}"
    );
}

#[test]
fn qglake_handoff_summary_verifier_rejects_storage_profile_provider_issuance_mismatch() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["storageProfileUpsertProof"]["provider"] = json!("s3");
    summary["lakecatReplayVerification"]["credentialVendingProof"]["restricted"]["storageProfile"]
        ["provider"] = json!("s3");
    summary["lakecatReplayVerification"]["credentialVendingProof"]["trustedHuman"]["storageProfile"]
        ["provider"] = json!("s3");

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject local-file-no-secret on remote provider");

    assert!(err.to_string().contains("storageProfileUpsertProof"));
    assert!(err.to_string().contains("local-file-no-secret"));
    assert!(err.to_string().contains("file provider"));

    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["storageProfileUpsertProof"]["issuanceMode"] =
        json!("short-lived-secret-ref");
    summary["lakecatReplayVerification"]["credentialVendingProof"]["restricted"]["storageProfile"]
        ["issuanceMode"] = json!("short-lived-secret-ref");
    summary["lakecatReplayVerification"]["credentialVendingProof"]["trustedHuman"]["storageProfile"]
        ["issuanceMode"] = json!("short-lived-secret-ref");

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject short-lived-secret-ref on file provider");

    assert!(err.to_string().contains("storageProfileUpsertProof"));
    assert!(err.to_string().contains("short-lived-secret-ref"));
    assert!(err.to_string().contains("s3, gcs, or azure"));
}

#[test]
fn qglake_handoff_summary_verifier_requires_storage_profile_location_prefix_hash() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["storageProfileUpsertProof"]
        .as_object_mut()
        .unwrap()
        .remove("locationPrefixHash");

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject missing location-prefix hash");

    assert!(err.to_string().contains("storageProfileUpsertProof"));
    assert!(err.to_string().contains("locationPrefixHash"));
}

#[test]
fn qglake_handoff_summary_verifier_requires_storage_profile_location_hash_shape() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["storageProfileUpsertProof"]["locationPrefixHash"] =
        json!("not-a-sha256-hash");

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject invalid location-prefix hash evidence");

    assert!(err.to_string().contains("storageProfileUpsertProof"));
    assert!(err.to_string().contains("locationPrefixHash"));
    assert!(err.to_string().contains("full SHA-256"));

    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["storageProfileUpsertProof"]["locationPrefixHash"] =
        json!("sha256:storage-location-prefix");

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject short location-prefix hash evidence");

    assert!(err.to_string().contains("storageProfileUpsertProof"));
    assert!(err.to_string().contains("locationPrefixHash"));
    assert!(err.to_string().contains("full SHA-256"));
}

#[test]
fn qglake_handoff_summary_verifier_requires_storage_profile_authorization_hash() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["storageProfileUpsertProof"]["authorizationReceiptHash"] =
        json!("sha256:short-storage-profile-authorization");

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject malformed storage-profile authorization hash");

    assert!(err.to_string().contains("storageProfileUpsertProof"));
    assert!(err.to_string().contains("authorizationReceiptHash"));
    assert!(err.to_string().contains("full SHA-256"));
}

#[test]
fn qglake_handoff_summary_verifier_requires_storage_profile_authorization_action() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["storageProfileUpsertProof"]["authorizationReceiptAction"] =
        json!("table-load");

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject drifted storage-profile authorization action");

    assert!(err.to_string().contains("storageProfileUpsertProof"));
    assert!(err.to_string().contains("authorizationReceiptAction"));
    assert!(err.to_string().contains("storage-profile-manage"));
}

#[test]
fn qglake_handoff_summary_verifier_requires_storage_profile_graph_events() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["storageProfileUpsertProof"]
        .as_object_mut()
        .unwrap()
        .remove("graphEvents");

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject missing storage-profile graph evidence");

    assert!(err.to_string().contains("storageProfileUpsertProof"));
    assert!(err.to_string().contains("graphEvents"));

    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["storageProfileUpsertProof"]["graphEvents"] = json!(0);

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject empty storage-profile graph evidence");

    assert!(err.to_string().contains("storageProfileUpsertProof"));
    assert!(err.to_string().contains("graphEvents"));
    assert!(err.to_string().contains("positive"));
}

#[test]
fn qglake_handoff_summary_verifier_rejects_short_storage_profile_replay_hashes() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["storageProfileUpsertProof"]["replayEventHashes"] =
        json!(["sha256:storage-replay"]);

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject short storage-profile replay hashes");

    assert!(err.to_string().contains("storageProfileUpsertProof"));
    assert!(err.to_string().contains("replayEventHashes"));
    assert!(err.to_string().contains("full SHA-256"));
}

#[test]
fn qglake_handoff_summary_verifier_requires_secret_ref_provider_when_present() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["storageProfileUpsertProof"]["secretRefPresent"] =
        json!(true);
    summary["lakecatReplayVerification"]["storageProfileUpsertProof"]["secretRefProvider"] =
        Value::Null;

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject secret-ref evidence without provider");

    assert!(err.to_string().contains("storageProfileUpsertProof"));
    assert!(err.to_string().contains("secretRefProvider"));
}

#[test]
fn qglake_handoff_summary_verifier_rejects_blank_secret_ref_provider_when_present() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["storageProfileUpsertProof"]["secretRefPresent"] =
        json!(true);
    summary["lakecatReplayVerification"]["storageProfileUpsertProof"]["secretRefProvider"] =
        json!("   ");
    summary["lakecatReplayVerification"]["storageProfileUpsertProof"]["secretRefHash"] =
        json!(qglake_fixture_hash("storage-secret-ref"));

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject blank secret-ref providers");

    assert!(err.to_string().contains("storageProfileUpsertProof"));
    assert!(err.to_string().contains("secretRefProvider"));
    assert!(err.to_string().contains("blank"));
}

#[test]
fn qglake_handoff_summary_verifier_requires_secret_ref_hash_when_present() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["storageProfileUpsertProof"]["secretRefPresent"] =
        json!(true);
    summary["lakecatReplayVerification"]["storageProfileUpsertProof"]["secretRefProvider"] =
        json!("vault");
    summary["lakecatReplayVerification"]["storageProfileUpsertProof"]["secretRefHash"] =
        Value::Null;

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject secret-ref evidence without hash");

    assert!(err.to_string().contains("storageProfileUpsertProof"));
    assert!(err.to_string().contains("secretRefHash"));

    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["storageProfileUpsertProof"]["secretRefPresent"] =
        json!(true);
    summary["lakecatReplayVerification"]["storageProfileUpsertProof"]["secretRefProvider"] =
        json!("vault");
    summary["lakecatReplayVerification"]["storageProfileUpsertProof"]["secretRefHash"] =
        json!("not-a-sha256-hash");

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject malformed secret-ref hash");

    assert!(err.to_string().contains("storageProfileUpsertProof"));
    assert!(err.to_string().contains("secretRefHash"));
    assert!(err.to_string().contains("full SHA-256"));
}

#[test]
fn qglake_handoff_summary_verifier_rejects_short_secret_ref_hashes() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["storageProfileUpsertProof"]["secretRefPresent"] =
        json!(true);
    summary["lakecatReplayVerification"]["storageProfileUpsertProof"]["secretRefProvider"] =
        json!("vault");
    summary["lakecatReplayVerification"]["storageProfileUpsertProof"]["secretRefHash"] =
        json!("sha256:storage-secret-ref");

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject short storage-profile secret-ref hashes");

    assert!(err.to_string().contains("storageProfileUpsertProof"));
    assert!(err.to_string().contains("secretRefHash"));
    assert!(err.to_string().contains("full SHA-256"));
}

#[test]
fn qglake_handoff_summary_verifier_rejects_secret_ref_provider_when_absent() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["storageProfileUpsertProof"]["secretRefPresent"] =
        json!(false);
    summary["lakecatReplayVerification"]["storageProfileUpsertProof"]["secretRefProvider"] =
        json!("vault");

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject a provider when no secret ref is present");

    assert!(err.to_string().contains("storageProfileUpsertProof"));
    assert!(err.to_string().contains("secretRefProvider"));
    assert!(err.to_string().contains("null"));
}

#[test]
fn qglake_handoff_summary_verifier_rejects_secret_ref_hash_when_absent() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["storageProfileUpsertProof"]["secretRefPresent"] =
        json!(false);
    summary["lakecatReplayVerification"]["storageProfileUpsertProof"]["secretRefHash"] =
        json!("sha256:storage-secret-ref");

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject a hash when no secret ref is present");

    assert!(err.to_string().contains("storageProfileUpsertProof"));
    assert!(err.to_string().contains("secretRefHash"));
    assert!(err.to_string().contains("null"));
}

#[test]
fn qglake_handoff_summary_verifier_allows_omitted_secret_ref_fields_when_absent() {
    let mut summary = qglake_handoff_summary_json();
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

    verify_qglake_handoff_summary_value(&summary).expect(
        "handoff summary should accept omitted secret-ref proof fields when secretRefPresent is false",
    );
}

#[test]
fn qglake_handoff_summary_verifier_requires_governed_scan_read_restriction() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["governedScanProof"]
        .as_object_mut()
        .unwrap()
        .remove("plannedReadRestriction");

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject missing governed scan restriction");

    assert!(err.to_string().contains("governedScanProof"));
    assert!(err.to_string().contains("plannedReadRestriction"));
}

#[test]
fn qglake_handoff_summary_verifier_rejects_extra_governed_scan_fields() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["governedScanProof"]["unverifiedScanClaim"] =
        json!(qglake_fixture_hash("unverified-scan-claim"));

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject extra governed scan proof fields");
    let err = err.to_string();

    assert!(err.contains("governedScanProof"), "{err}");
    assert!(
        err.contains("unexpected field unverifiedScanClaim"),
        "{err}"
    );
}

#[test]
fn qglake_handoff_summary_verifier_requires_scan_delete_file_count() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["governedScanProof"]
        .as_object_mut()
        .unwrap()
        .remove("deleteFileCount");

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject missing scan delete-file count");

    assert!(err.to_string().contains("governedScanProof"));
    assert!(err.to_string().contains("deleteFileCount"));
}

#[test]
fn qglake_handoff_summary_verifier_requires_scan_plan_graph_events() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["governedScanProof"]["planGraphEvents"] = json!(0);

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject missing scan-plan graph proof");

    assert!(err.to_string().contains("governedScanProof"));
    assert!(err.to_string().contains("planGraphEvents"));
    assert!(err.to_string().contains("positive"));
}

#[test]
fn qglake_handoff_summary_verifier_requires_scan_receipt_identity() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["governedScanProof"]
        .as_object_mut()
        .unwrap()
        .remove("plannedPrincipalSubject");

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject missing scan principal proof");

    assert!(err.to_string().contains("governedScanProof"));
    assert!(err.to_string().contains("plannedPrincipalSubject"));

    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["governedScanProof"]["fetchedPrincipalSubject"] =
        json!("did:example:other-agent");

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject drifted fetched scan principal proof");

    assert!(err.to_string().contains("governedScanProof"));
    assert!(err.to_string().contains("fetchedPrincipalSubject mismatch"));
}

#[test]
fn qglake_handoff_summary_verifier_requires_scan_receipt_action_and_hash() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["governedScanProof"]
        .as_object_mut()
        .unwrap()
        .remove("plannedAuthorizationReceiptHash");

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject missing scan receipt hash proof");

    assert!(err.to_string().contains("governedScanProof"));
    assert!(err.to_string().contains("plannedAuthorizationReceiptHash"));

    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["governedScanProof"]["fetchedAuthorizationReceiptAction"] =
        json!("table-load");

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject drifted fetched scan action proof");

    assert!(err.to_string().contains("governedScanProof"));
    assert!(
        err.to_string()
            .contains("fetchedAuthorizationReceiptAction mismatch")
    );
}

#[test]
fn qglake_handoff_summary_verifier_requires_scan_child_plan_task_count() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["governedScanProof"]["childPlanTaskCount"] = json!(0);

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject empty scan child-plan-task proof");

    assert!(err.to_string().contains("governedScanProof"));
    assert!(err.to_string().contains("childPlanTaskCount"));
    assert!(err.to_string().contains("positive"));
}

#[test]
fn qglake_handoff_summary_verifier_requires_scan_stats_field_evidence() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["governedScanProof"]
        .as_object_mut()
        .unwrap()
        .remove("plannedRequestedStatsFields");

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject missing scan stats-field evidence");

    assert!(err.to_string().contains("governedScanProof"));
    assert!(err.to_string().contains("plannedRequestedStatsFields"));
}

#[test]
fn qglake_handoff_summary_verifier_requires_scan_projection_evidence() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["governedScanProof"]
        .as_object_mut()
        .unwrap()
        .remove("plannedRequestedProjection");

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject missing scan projection evidence");

    assert!(err.to_string().contains("governedScanProof"));
    assert!(err.to_string().contains("plannedRequestedProjection"));
}

#[test]
fn qglake_handoff_summary_verifier_rejects_scan_projection_widening() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["governedScanProof"]["plannedEffectiveProjection"] =
        json!(["event_id", "occurred_at", "severity", "raw_payload"]);

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject widened scan projection");

    assert!(err.to_string().contains("governedScanProof"));
}

#[test]
fn qglake_handoff_summary_verifier_rejects_unrequested_effective_scan_projection() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["governedScanProof"]["plannedRequestedProjection"] =
        json!(["event_id", "occurred_at", "severity", "raw_payload"]);
    summary["lakecatReplayVerification"]["governedScanProof"]["plannedEffectiveProjection"] =
        json!(["event_id", "occurred_at", "tenant_id"]);

    let err = verify_qglake_handoff_summary_value(&summary).expect_err(
        "handoff summary should reject effective projection fields that were never requested",
    );

    assert!(err.to_string().contains("governedScanProof"));
    assert!(err.to_string().contains("plannedEffectiveProjection"));
    assert!(err.to_string().contains("tenant_id"));
    assert!(err.to_string().contains("not requested"));
}

#[test]
fn qglake_handoff_summary_verifier_rejects_duplicate_requested_scan_projection() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["governedScanProof"]["plannedRequestedProjection"] =
        json!([
            "event_id",
            "occurred_at",
            "severity",
            "raw_payload",
            "raw_payload"
        ]);

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject duplicate requested scan projection");

    assert!(err.to_string().contains("governedScanProof"));
    assert!(err.to_string().contains("plannedRequestedProjection"));
    assert!(err.to_string().contains("duplicate-free"));
}

#[test]
fn qglake_handoff_summary_verifier_requires_effective_scan_stats_field_evidence() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["governedScanProof"]
        .as_object_mut()
        .unwrap()
        .remove("plannedEffectiveStatsFields");

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject missing effective stats-field evidence");

    assert!(err.to_string().contains("governedScanProof"));
    assert!(err.to_string().contains("plannedEffectiveStatsFields"));
}

#[test]
fn qglake_handoff_summary_verifier_requires_fetched_scan_stats_field_evidence() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["governedScanProof"]
        .as_object_mut()
        .unwrap()
        .remove("fetchedRequestedStatsFields");

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject missing fetched stats-field evidence");

    assert!(err.to_string().contains("governedScanProof"));
    assert!(err.to_string().contains("fetchedRequestedStatsFields"));
}

#[test]
fn qglake_handoff_summary_verifier_requires_fetched_effective_scan_stats_field_evidence() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["governedScanProof"]
        .as_object_mut()
        .unwrap()
        .remove("fetchedEffectiveStatsFields");

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject missing fetched effective stats-field evidence");

    assert!(err.to_string().contains("governedScanProof"));
    assert!(err.to_string().contains("fetchedEffectiveStatsFields"));
}

#[test]
fn qglake_handoff_summary_verifier_rejects_fetched_scan_stats_field_drift() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["governedScanProof"]["fetchedRequestedStatsFields"] =
        json!(["event_id", "occurred_at", "severity", "raw_payload"]);
    summary["lakecatReplayVerification"]["governedScanProof"]["fetchedEffectiveStatsFields"] =
        json!(["event_id", "occurred_at", "raw_payload"]);

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject drifted fetched scan stats fields");

    assert!(err.to_string().contains("governedScanProof"));
    assert!(err.to_string().contains("allowed-columns mismatch"));
}

#[test]
fn qglake_handoff_summary_verifier_rejects_unrequested_fetched_scan_stats_field() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["governedScanProof"]["fetchedRequestedStatsFields"] =
        json!(["event_id", "occurred_at"]);
    summary["lakecatReplayVerification"]["governedScanProof"]["fetchedEffectiveStatsFields"] =
        json!(["event_id", "occurred_at", "severity"]);

    let err = verify_qglake_handoff_summary_value(&summary).expect_err(
        "handoff summary should reject fetched effective stats fields that were never requested",
    );

    assert!(err.to_string().contains("governedScanProof"));
    assert!(err.to_string().contains("fetchedEffectiveStatsFields"));
    assert!(err.to_string().contains("severity"));
    assert!(err.to_string().contains("not requested"));
}
