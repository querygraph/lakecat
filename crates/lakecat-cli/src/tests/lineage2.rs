use super::common::*;
use crate::*;

#[test]
fn qglake_lineage_drain_verifier_rejects_duplicate_scan_openlineage_hashes() {
    let verification = qglake_handoff_lineage_verification();
    let mut drain = qglake_handoff_lineage_drain();
    let scan_plan = drain
        .events
        .iter_mut()
        .find(|event| event.event_type == "table.scan-planned")
        .expect("scan planning replay fixture");
    let duplicate_hash = qglake_fixture_hash("duplicate-scan-plan-openlineage");
    scan_plan.replay_open_lineage_hashes = vec![duplicate_hash.clone(), duplicate_hash];

    let err = verify_qglake_lineage_drain(&drain, &verification, Some("did:example:agent"), 1)
        .expect_err("QGLake lineage drain should reject duplicate scan OpenLineage hashes");

    assert!(err.to_string().contains("table.scan-planned"));
    assert!(err.to_string().contains("openLineageHashes"));
    assert!(err.to_string().contains("duplicate-free"));
}

#[test]
fn qglake_lineage_drain_verifier_rejects_duplicate_management_receipt_hashes() {
    let verification = qglake_handoff_lineage_verification();
    let mut drain = qglake_handoff_lineage_drain();
    let server_list = drain
        .events
        .iter_mut()
        .find(|event| event.event_type == "server.listed")
        .expect("server list replay fixture");
    let duplicate_hash = qglake_fixture_hash("duplicate-server-list-replay-event");
    server_list.replay_event_hashes = vec![duplicate_hash.clone(), duplicate_hash];

    let err = verify_qglake_lineage_drain(&drain, &verification, Some("did:example:agent"), 1)
        .expect_err("QGLake lineage drain should reject duplicate management receipt hashes");

    assert!(err.to_string().contains("server.listed"));
    assert!(err.to_string().contains("replayEventHashes"));
    assert!(err.to_string().contains("duplicate-free"));
}

#[test]
fn qglake_lineage_drain_verifier_rejects_invalid_warehouse_project_scope() {
    let verification = qglake_handoff_lineage_verification();
    let mut drain = qglake_handoff_lineage_drain();
    let warehouse_list = drain
        .events
        .iter_mut()
        .find(|event| event.event_type == "warehouse.listed")
        .expect("warehouse list replay fixture");
    warehouse_list.management_scope_project_id = Some("analytics/../../secret".to_string());

    let err = verify_qglake_lineage_drain(&drain, &verification, Some("did:example:agent"), 1)
        .expect_err("QGLake lineage drain should reject malformed warehouse project scope");

    assert!(err.to_string().contains("warehouse list"));
    assert!(
        err.to_string()
            .contains("syntactically invalid project scope")
    );
}

#[test]
fn qglake_lineage_drain_verifier_rejects_unknown_warehouse_project_scope() {
    let verification = qglake_handoff_lineage_verification();
    let mut drain = qglake_handoff_lineage_drain();
    let warehouse_list = drain
        .events
        .iter_mut()
        .find(|event| event.event_type == "warehouse.listed")
        .expect("warehouse list replay fixture");
    warehouse_list.management_scope_project_id = Some("unlisted-project".to_string());

    let err = verify_qglake_lineage_drain(&drain, &verification, Some("did:example:agent"), 1)
        .expect_err("QGLake lineage drain should reject unlisted warehouse project scope");

    assert!(err.to_string().contains("warehouse list"));
    assert!(
        err.to_string()
            .contains("project scope does not match project list")
    );
}

#[test]
fn qglake_lineage_drain_verifier_rejects_duplicate_view_tombstone_receipt_hashes() {
    let verification = qglake_view_lineage_verification();
    let mut tombstone = qglake_view_tombstone_receipt_lineage_summary();
    let duplicate_hash = tombstone
        .view_version_receipt_hashes
        .first()
        .expect("tombstone receipt hash fixture")
        .clone();
    tombstone.view_version_receipt_hashes = vec![duplicate_hash.clone(), duplicate_hash];
    let drain = qglake_lineage_drain_from_summaries(vec![
        qglake_bootstrap_lineage_summary_for(&verification, 1),
        qglake_restricted_credential_summary(),
        qglake_human_credential_summary(),
        qglake_view_lineage_summary(),
        qglake_view_drop_lineage_summary(),
        tombstone,
        qglake_view_receipt_chain_lineage_summary(),
        qglake_policy_list_lineage_summary(),
        qglake_policy_upsert_lineage_summary(),
        qglake_storage_profile_list_lineage_summary(),
        qglake_storage_profile_upsert_lineage_summary(),
        qglake_server_list_lineage_summary(),
        qglake_project_list_lineage_summary(),
        qglake_warehouse_list_lineage_summary(),
        qglake_table_commit_history_lineage_summary(),
        qglake_scan_planned_lineage_summary(),
        qglake_scan_tasks_fetched_lineage_summary(),
    ]);

    let err = verify_qglake_lineage_drain(&drain, &verification, Some("did:example:agent"), 1)
        .expect_err("QGLake lineage drain should reject duplicate tombstone receipt hashes");

    assert!(err.to_string().contains("view.version-receipts-listed"));
    assert!(err.to_string().contains("viewVersionReceiptHashes"));
    assert!(err.to_string().contains("duplicate-free"));
}

#[test]
fn qglake_lineage_drain_verifier_rejects_duplicate_view_receipt_chain_hashes() {
    let verification = qglake_view_lineage_verification();
    let mut chain = qglake_view_receipt_chain_lineage_summary();
    let duplicate_hash = chain
        .view_version_receipt_chain_hashes
        .first()
        .expect("receipt-chain hash fixture")
        .clone();
    chain.view_version_receipt_chain_verified_count = 2;
    chain.view_version_receipt_chain_hashes = vec![duplicate_hash.clone(), duplicate_hash];
    chain.view_version_receipt_hashes = vec![
        qglake_fixture_hash("view-drop-receipt"),
        qglake_fixture_hash("view-receipt-v2"),
    ];
    let drain = qglake_lineage_drain_from_summaries(vec![
        qglake_bootstrap_lineage_summary_for(&verification, 1),
        qglake_restricted_credential_summary(),
        qglake_human_credential_summary(),
        qglake_view_lineage_summary(),
        qglake_view_drop_lineage_summary(),
        qglake_view_tombstone_receipt_lineage_summary(),
        chain,
        qglake_policy_list_lineage_summary(),
        qglake_policy_upsert_lineage_summary(),
        qglake_storage_profile_list_lineage_summary(),
        qglake_storage_profile_upsert_lineage_summary(),
        qglake_server_list_lineage_summary(),
        qglake_project_list_lineage_summary(),
        qglake_warehouse_list_lineage_summary(),
        qglake_table_commit_history_lineage_summary(),
        qglake_scan_planned_lineage_summary(),
        qglake_scan_tasks_fetched_lineage_summary(),
    ]);

    let err = verify_qglake_lineage_drain(&drain, &verification, Some("did:example:agent"), 1)
        .expect_err("QGLake lineage drain should reject duplicate receipt-chain hashes");

    assert!(
        err.to_string()
            .contains("view.version-receipt-chains-listed")
    );
    assert!(err.to_string().contains("viewVersionReceiptChainHashes"));
    assert!(err.to_string().contains("duplicate-free"));
}

#[test]
fn qglake_lineage_drain_verifier_rejects_short_view_replay_hashes() {
    let verification = qglake_view_lineage_verification();
    let mut view = qglake_view_lineage_summary();
    view.replay_event_hashes = vec!["sha256:view-replay-event".to_string()];
    let drain = qglake_lineage_drain_from_summaries(vec![
        qglake_bootstrap_lineage_summary_for(&verification, 1),
        qglake_restricted_credential_summary(),
        qglake_human_credential_summary(),
        view,
        qglake_view_drop_lineage_summary(),
        qglake_view_tombstone_receipt_lineage_summary(),
        qglake_view_receipt_chain_lineage_summary(),
        qglake_policy_list_lineage_summary(),
        qglake_policy_upsert_lineage_summary(),
        qglake_storage_profile_list_lineage_summary(),
        qglake_storage_profile_upsert_lineage_summary(),
        qglake_server_list_lineage_summary(),
        qglake_project_list_lineage_summary(),
        qglake_warehouse_list_lineage_summary(),
        qglake_table_commit_history_lineage_summary(),
        qglake_scan_planned_lineage_summary(),
        qglake_scan_tasks_fetched_lineage_summary(),
    ]);

    let err = verify_qglake_lineage_drain(&drain, &verification, Some("did:example:agent"), 1)
        .expect_err("QGLake lineage drain should reject short view replay hashes");

    assert!(err.to_string().contains("view replay"));
    assert!(err.to_string().contains("full SHA-256"));
}

#[test]
fn qglake_lineage_drain_verifier_rejects_short_bootstrap_view_receipt_hashes() {
    let verification = qglake_handoff_lineage_verification();
    let mut drain = qglake_handoff_lineage_drain();
    let bootstrap = drain
        .events
        .iter_mut()
        .find(|event| event.event_type == "querygraph.bootstrap")
        .expect("bootstrap replay fixture");
    bootstrap.view_version_receipt_hashes = vec!["sha256:view-version-receipt".to_string()];

    let err = verify_qglake_lineage_drain(&drain, &verification, Some("did:example:agent"), 1)
        .expect_err("QGLake lineage drain should reject short bootstrap view receipt hashes");

    assert!(err.to_string().contains("view version receipt hashes"));
    assert!(err.to_string().contains("full SHA-256"));
}

#[test]
fn qglake_lineage_drain_verifier_rejects_short_view_receipt_hashes() {
    let verification = qglake_view_lineage_verification();
    let mut tombstone = qglake_view_tombstone_receipt_lineage_summary();
    tombstone.view_version_receipt_hashes = vec!["sha256:view-drop-receipt".to_string()];
    let drain = qglake_lineage_drain_from_summaries(vec![
        qglake_bootstrap_lineage_summary_for(&verification, 1),
        qglake_restricted_credential_summary(),
        qglake_human_credential_summary(),
        qglake_view_lineage_summary(),
        qglake_view_drop_lineage_summary(),
        tombstone,
        qglake_view_receipt_chain_lineage_summary(),
        qglake_policy_list_lineage_summary(),
        qglake_policy_upsert_lineage_summary(),
        qglake_storage_profile_list_lineage_summary(),
        qglake_storage_profile_upsert_lineage_summary(),
        qglake_server_list_lineage_summary(),
        qglake_project_list_lineage_summary(),
        qglake_warehouse_list_lineage_summary(),
        qglake_table_commit_history_lineage_summary(),
        qglake_scan_planned_lineage_summary(),
        qglake_scan_tasks_fetched_lineage_summary(),
    ]);

    let err = verify_qglake_lineage_drain(&drain, &verification, Some("did:example:agent"), 1)
        .expect_err("QGLake lineage drain should reject short view receipt hashes");

    assert!(err.to_string().contains("tombstone receipt"));
    assert!(err.to_string().contains("full SHA-256"));
}

#[test]
fn qglake_lineage_drain_verifier_requires_management_ids() {
    let verification = qglake_handoff_lineage_verification();
    let mut drain = qglake_handoff_lineage_drain();
    let storage_profile_list = drain
        .events
        .iter_mut()
        .find(|event| event.event_type == "storage-profile.listed")
        .expect("storage profile list replay fixture");
    storage_profile_list.storage_profile_ids.clear();

    let err = verify_qglake_lineage_drain(&drain, &verification, Some("did:example:agent"), 1)
        .expect_err("QGLake lineage drain should reject missing management IDs");

    assert!(err.to_string().contains("storage profile list replay"));
    assert!(err.to_string().contains("compact management ID evidence"));
}

#[test]
fn qglake_lineage_drain_verifier_rejects_storage_profile_provider_issuance_mismatch() {
    let verification = qglake_handoff_lineage_verification();
    let mut drain = qglake_handoff_lineage_drain();
    let storage_profile_upsert = drain
        .events
        .iter_mut()
        .find(|event| event.event_type == "storage-profile.upserted")
        .expect("storage profile upsert replay fixture");
    storage_profile_upsert.storage_profile_provider = Some("s3".to_string());

    let err = verify_qglake_lineage_drain(&drain, &verification, Some("did:example:agent"), 1)
        .expect_err("QGLake lineage drain should reject local-file-no-secret on remote provider");

    assert!(err.to_string().contains("storage profile upsert"));
    assert!(err.to_string().contains("local-file-no-secret"));
    assert!(err.to_string().contains("file provider"));

    let mut drain = qglake_handoff_lineage_drain();
    let storage_profile_upsert = drain
        .events
        .iter_mut()
        .find(|event| event.event_type == "storage-profile.upserted")
        .expect("storage profile upsert replay fixture");
    storage_profile_upsert.storage_profile_issuance_mode =
        Some("short-lived-secret-ref".to_string());

    let err = verify_qglake_lineage_drain(&drain, &verification, Some("did:example:agent"), 1)
        .expect_err("QGLake lineage drain should reject short-lived-secret-ref on file provider");

    assert!(err.to_string().contains("storage profile upsert"));
    assert!(err.to_string().contains("short-lived-secret-ref"));
    assert!(err.to_string().contains("s3, gcs, or azure"));
}

#[test]
fn qglake_lineage_drain_verifier_rejects_duplicate_management_ids() {
    let verification = qglake_handoff_lineage_verification();
    let mut drain = qglake_handoff_lineage_drain();
    let server_list = drain
        .events
        .iter_mut()
        .find(|event| event.event_type == "server.listed")
        .expect("server list replay fixture");
    server_list.server_count = Some(2);
    server_list.server_ids = vec!["qglake-server".to_string(), "qglake-server".to_string()];

    let err = verify_qglake_lineage_drain(&drain, &verification, Some("did:example:agent"), 1)
        .expect_err("QGLake lineage drain should reject duplicate management IDs");

    assert!(err.to_string().contains("server list replay"));
    assert!(
        err.to_string()
            .contains("duplicate compact management ID evidence")
    );
}

#[test]
fn qglake_lineage_drain_verifier_rejects_path_decorated_management_ids() {
    let verification = qglake_handoff_lineage_verification();
    let mut drain = qglake_handoff_lineage_drain();
    let project_list = drain
        .events
        .iter_mut()
        .find(|event| event.event_type == "project.listed")
        .expect("project list replay fixture");
    project_list.project_ids = vec!["analytics/../../secret".to_string()];

    let err = verify_qglake_lineage_drain(&drain, &verification, Some("did:example:agent"), 1)
        .expect_err("QGLake lineage drain should reject path-decorated management IDs");

    assert!(err.to_string().contains("project list replay"));
    assert!(
        err.to_string()
            .contains("syntactically invalid compact management ID evidence")
    );
}

#[test]
fn qglake_lineage_drain_verifier_rejects_query_decorated_management_ids() {
    let verification = qglake_handoff_lineage_verification();
    let mut drain = qglake_handoff_lineage_drain();
    let storage_profile_list = drain
        .events
        .iter_mut()
        .find(|event| event.event_type == "storage-profile.listed")
        .expect("storage profile list replay fixture");
    storage_profile_list.storage_profile_ids = vec!["events?token=secret".to_string()];

    let err = verify_qglake_lineage_drain(&drain, &verification, Some("did:example:agent"), 1)
        .expect_err("QGLake lineage drain should reject query-decorated management IDs");

    assert!(err.to_string().contains("storage profile list replay"));
    assert!(
        err.to_string()
            .contains("syntactically invalid compact management ID evidence")
    );
}

#[test]
fn qglake_lineage_drain_verifier_requires_scan_receipt_hash_shape() {
    let verification = qglake_handoff_lineage_verification();
    let mut drain = qglake_handoff_lineage_drain();
    let scan_plan = drain
        .events
        .iter_mut()
        .find(|event| event.event_type == "table.scan-planned")
        .expect("scan plan replay fixture");
    scan_plan.replay_open_lineage_hashes = vec!["not-a-sha256-hash".to_string()];

    let err = verify_qglake_lineage_drain(&drain, &verification, Some("did:example:agent"), 1)
        .expect_err("QGLake lineage drain should reject malformed scan receipt hashes");

    assert!(err
        .to_string()
        .contains("qglake lineage drain scan planning replay is missing compact task, graph, or SHA-256 receipt evidence"));
}

#[test]
fn qglake_lineage_drain_verifier_rejects_short_scan_receipt_hashes() {
    let verification = qglake_handoff_lineage_verification();
    let mut drain = qglake_handoff_lineage_drain();
    let scan_fetch = drain
        .events
        .iter_mut()
        .find(|event| event.event_type == "table.scan-tasks-fetched")
        .expect("scan fetch replay fixture");
    scan_fetch.replay_event_hashes = vec!["sha256:scan-fetch-replay".to_string()];

    let err = verify_qglake_lineage_drain(&drain, &verification, Some("did:example:agent"), 1)
        .expect_err("QGLake lineage drain should reject short scan receipt hashes");

    assert!(err
        .to_string()
        .contains("qglake lineage drain scan task fetch replay is missing compact file/delete task or SHA-256 receipt evidence"));
}

#[test]
fn qglake_lineage_drain_verifier_rejects_short_credential_receipt_hashes() {
    let verification = qglake_handoff_lineage_verification();
    let mut drain = qglake_handoff_lineage_drain();
    let credential = drain
        .events
        .iter_mut()
        .find(|event| {
            event.event_type == "credentials.vend-attempted"
                && event.principal_kind.as_deref() == Some("agent")
        })
        .expect("restricted credential replay fixture");
    credential.replay_event_hashes = vec!["sha256:restricted-credential-replay".to_string()];

    let err = verify_qglake_lineage_drain(&drain, &verification, Some("did:example:agent"), 1)
        .expect_err("QGLake lineage drain should reject short credential receipt hashes");

    assert!(err.to_string().contains("credential replay"));
    assert!(err.to_string().contains("full SHA-256 sink receipt hashes"));
}

#[test]
fn qglake_lineage_drain_verifier_rejects_short_commit_history_receipt_hashes() {
    let verification = qglake_handoff_lineage_verification();
    let mut drain = qglake_handoff_lineage_drain();
    let commit_history = drain
        .events
        .iter_mut()
        .find(|event| event.event_type == "table.commits-listed")
        .expect("table commit history replay fixture");
    commit_history.replay_open_lineage_hashes =
        vec!["sha256:table-commits-openlineage".to_string()];

    let err = verify_qglake_lineage_drain(&drain, &verification, Some("did:example:agent"), 1)
        .expect_err("QGLake lineage drain should reject short commit-history receipt hashes");

    assert!(err.to_string().contains("table commit history replay"));
    assert!(err.to_string().contains("full SHA-256 receipt hashes"));
}

#[test]
fn qglake_lineage_drain_verifier_rejects_short_scan_authorization_hashes() {
    let verification = qglake_handoff_lineage_verification();
    let mut drain = qglake_handoff_lineage_drain();
    let scan_plan = drain
        .events
        .iter_mut()
        .find(|event| event.event_type == "table.scan-planned")
        .expect("scan plan replay fixture");
    scan_plan.authorization_receipt_hash = Some("sha256:scan-plan-auth".to_string());

    let err = verify_qglake_lineage_drain(&drain, &verification, Some("did:example:agent"), 1)
        .expect_err("QGLake lineage drain should reject short scan authorization hashes");

    assert!(err
        .to_string()
        .contains("qglake lineage drain scan planning replay is missing compact task, graph, or SHA-256 receipt evidence"));
}

#[test]
fn qglake_lineage_drain_verifier_rejects_missing_scan_authorization_hashes() {
    let verification = qglake_handoff_lineage_verification();
    let mut drain = qglake_handoff_lineage_drain();
    let scan_fetch = drain
        .events
        .iter_mut()
        .find(|event| event.event_type == "table.scan-tasks-fetched")
        .expect("scan fetch replay fixture");
    scan_fetch.authorization_receipt_hash = None;

    let err = verify_qglake_lineage_drain(&drain, &verification, Some("did:example:agent"), 1)
        .expect_err("QGLake lineage drain should reject missing scan authorization hashes");

    assert!(err
        .to_string()
        .contains("qglake lineage drain scan task fetch replay is missing compact file/delete task or SHA-256 receipt evidence"));
}

#[test]
fn qglake_lineage_drain_verifier_rejects_short_scan_policy_hashes() {
    let verification = qglake_handoff_lineage_verification();
    let mut drain = qglake_handoff_lineage_drain();
    let scan_plan = drain
        .events
        .iter_mut()
        .find(|event| event.event_type == "table.scan-planned")
        .expect("scan plan replay fixture");
    scan_plan.read_restriction = Some(json!({
        "allowed-columns": ["event_id", "occurred_at", "severity"],
        "row-predicate": {
            "type": "not-eq",
            "term": "severity",
            "value": "debug"
        },
        "purpose": "qglake-agent-demo",
        "max-credential-ttl-seconds": 300,
        "policy-hashes": ["sha256:scan-policy"]
    }));

    let err = verify_qglake_lineage_drain(&drain, &verification, Some("did:example:agent"), 1)
        .expect_err("QGLake lineage drain should reject short scan policy hashes");

    assert!(
        err.to_string()
            .contains("qglake lineage drain scan planning read restriction")
    );
    assert!(err.to_string().contains("policy-hashes"));
    assert!(err.to_string().contains("full SHA-256"));
}

#[test]
fn qglake_lineage_drain_verifier_rejects_empty_scan_allowed_columns() {
    let verification = qglake_handoff_lineage_verification();
    let mut drain = qglake_handoff_lineage_drain();
    let scan_plan = drain
        .events
        .iter_mut()
        .find(|event| event.event_type == "table.scan-planned")
        .expect("scan plan replay fixture");
    scan_plan.read_restriction = Some(json!({
        "allowed-columns": ["event_id", ""],
        "row-predicate": {
            "type": "not-eq",
            "term": "severity",
            "value": "debug"
        },
        "purpose": "qglake-agent-demo",
        "max-credential-ttl-seconds": 300,
        "policy-hashes": [qglake_fixture_hash("scan-policy")]
    }));

    let err = verify_qglake_lineage_drain(&drain, &verification, Some("did:example:agent"), 1)
        .expect_err("QGLake lineage drain should reject empty scan allowed columns");

    assert!(
        err.to_string()
            .contains("qglake lineage drain scan planning read restriction")
    );
    assert!(err.to_string().contains("allowed-columns"));
    assert!(err.to_string().contains("column names"));
}

#[test]
fn qglake_lineage_drain_verifier_rejects_duplicate_scan_allowed_columns() {
    let verification = qglake_handoff_lineage_verification();
    let mut drain = qglake_handoff_lineage_drain();
    let scan_plan = drain
        .events
        .iter_mut()
        .find(|event| event.event_type == "table.scan-planned")
        .expect("scan plan replay fixture");
    scan_plan.read_restriction = Some(json!({
        "allowed-columns": ["event_id", "event_id"],
        "row-predicate": {
            "type": "not-eq",
            "term": "severity",
            "value": "debug"
        },
        "purpose": "qglake-agent-demo",
        "max-credential-ttl-seconds": 300,
        "policy-hashes": [qglake_fixture_hash("scan-policy")]
    }));

    let err = verify_qglake_lineage_drain(&drain, &verification, Some("did:example:agent"), 1)
        .expect_err("QGLake lineage drain should reject duplicate scan allowed columns");

    assert!(
        err.to_string()
            .contains("qglake lineage drain scan planning read restriction")
    );
    assert!(err.to_string().contains("allowed-columns"));
    assert!(err.to_string().contains("duplicate-free"));
}

#[test]
fn qglake_lineage_drain_verifier_rejects_empty_scan_row_predicate() {
    let verification = qglake_handoff_lineage_verification();
    let mut drain = qglake_handoff_lineage_drain();
    let scan_plan = drain
        .events
        .iter_mut()
        .find(|event| event.event_type == "table.scan-planned")
        .expect("scan plan replay fixture");
    scan_plan.read_restriction = Some(json!({
        "allowed-columns": ["event_id", "occurred_at", "severity"],
        "row-predicate": {},
        "purpose": "qglake-agent-demo",
        "max-credential-ttl-seconds": 300,
        "policy-hashes": [qglake_fixture_hash("scan-policy")]
    }));

    let err = verify_qglake_lineage_drain(&drain, &verification, Some("did:example:agent"), 1)
        .expect_err("QGLake lineage drain should reject empty scan row predicate");

    assert!(
        err.to_string()
            .contains("qglake lineage drain scan planning read restriction")
    );
    assert!(err.to_string().contains("row-predicate"));
    assert!(err.to_string().contains("predicate evidence"));
}

#[test]
fn qglake_lineage_drain_verifier_rejects_blank_scan_row_predicate_type() {
    let verification = qglake_handoff_lineage_verification();
    let mut drain = qglake_handoff_lineage_drain();
    let scan_plan = drain
        .events
        .iter_mut()
        .find(|event| event.event_type == "table.scan-planned")
        .expect("scan plan replay fixture");
    scan_plan.read_restriction = Some(json!({
        "allowed-columns": ["event_id", "occurred_at", "severity"],
        "row-predicate": {
            "type": " ",
            "term": "severity",
            "value": "debug"
        },
        "purpose": "qglake-agent-demo",
        "max-credential-ttl-seconds": 300,
        "policy-hashes": [qglake_fixture_hash("scan-policy")]
    }));

    let err = verify_qglake_lineage_drain(&drain, &verification, Some("did:example:agent"), 1)
        .expect_err("QGLake lineage drain should reject blank scan row predicate type");

    assert!(
        err.to_string()
            .contains("qglake lineage drain scan planning read restriction")
    );
    assert!(err.to_string().contains("row-predicate"));
    assert!(err.to_string().contains("type"));
}

#[test]
fn qglake_lineage_drain_verifier_rejects_termless_scan_row_predicate() {
    let verification = qglake_handoff_lineage_verification();
    let mut drain = qglake_handoff_lineage_drain();
    let scan_plan = drain
        .events
        .iter_mut()
        .find(|event| event.event_type == "table.scan-planned")
        .expect("scan plan replay fixture");
    scan_plan.read_restriction = Some(json!({
        "allowed-columns": ["event_id", "occurred_at", "severity"],
        "row-predicate": {
            "type": "not-eq",
            "value": "debug"
        },
        "purpose": "qglake-agent-demo",
        "max-credential-ttl-seconds": 300,
        "policy-hashes": [qglake_fixture_hash("scan-policy")]
    }));

    let err = verify_qglake_lineage_drain(&drain, &verification, Some("did:example:agent"), 1)
        .expect_err("QGLake lineage drain should reject termless scan row predicate");

    assert!(
        err.to_string()
            .contains("qglake lineage drain scan planning read restriction")
    );
    assert!(err.to_string().contains("row-predicate"));
    assert!(err.to_string().contains("term"));
}

#[test]
fn qglake_lineage_drain_verifier_requires_scan_plan_graph_events() {
    let verification = qglake_handoff_lineage_verification();
    let mut drain = qglake_handoff_lineage_drain();
    let scan_plan = drain
        .events
        .iter_mut()
        .find(|event| event.event_type == "table.scan-planned")
        .expect("scan plan replay fixture");
    scan_plan.graph_events = 0;
    drain.graph_events -= 1;

    let err = verify_qglake_lineage_drain(&drain, &verification, Some("did:example:agent"), 1)
        .expect_err("QGLake lineage drain should reject missing scan-plan graph proof");

    assert!(err
        .to_string()
        .contains("qglake lineage drain scan planning replay is missing compact task, graph, or SHA-256 receipt evidence"));
}
