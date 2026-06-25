use super::common::*;
use crate::*;

#[test]
fn qglake_lineage_drain_verifier_rejects_blank_replay_event_ids() {
    let verification = qglake_handoff_lineage_verification();
    let mut drain = qglake_handoff_lineage_drain();
    drain.events[0].event_id = "  ".to_string();

    let err = verify_qglake_lineage_drain(&drain, &verification, Some("did:example:agent"), 1)
        .expect_err("QGLake lineage drain should reject blank replay event ids");

    assert!(err.to_string().contains("qglake lineage drain"));
    assert!(err.to_string().contains("event id at index 0"));
    assert!(err.to_string().contains("non-empty"));
}

#[test]
fn qglake_lineage_drain_verifier_rejects_duplicate_replay_event_ids() {
    let verification = qglake_handoff_lineage_verification();
    let mut drain = qglake_handoff_lineage_drain();
    drain.events[1].event_id = drain.events[0].event_id.clone();

    let err = verify_qglake_lineage_drain(&drain, &verification, Some("did:example:agent"), 1)
        .expect_err("QGLake lineage drain should reject duplicate replay event ids");

    assert!(err.to_string().contains("qglake lineage drain"));
    assert!(err.to_string().contains("event id at index 1"));
    assert!(err.to_string().contains("duplicate-free"));
}

#[test]
fn qglake_lineage_drain_verifier_rejects_event_type_multiplicity_drift() {
    let verification = qglake_handoff_lineage_verification();
    let mut drain = qglake_handoff_lineage_drain();
    let credential_event_type = drain
        .event_types
        .iter_mut()
        .find(|event_type| event_type.as_str() == "credentials.vend-attempted")
        .expect("credential replay event type fixture");
    *credential_event_type = "view.upserted".to_string();

    let err = verify_qglake_lineage_drain(&drain, &verification, Some("did:example:agent"), 1)
        .expect_err("QGLake lineage drain should reject event type multiplicity drift");

    assert!(err.to_string().contains("qglake lineage drain"));
    assert!(err.to_string().contains("eventTypes multiset"));
}

#[test]
fn qglake_lineage_drain_verifier_rejects_event_type_order_drift() {
    let verification = qglake_handoff_lineage_verification();
    let mut drain = qglake_handoff_lineage_drain();
    drain.event_types.swap(0, 1);

    let err = verify_qglake_lineage_drain(&drain, &verification, Some("did:example:agent"), 1)
        .expect_err("QGLake lineage drain should reject event type order drift");

    assert!(err.to_string().contains("qglake lineage drain"));
    assert!(err.to_string().contains("eventTypes order drift"));
}

#[test]
fn qglake_lineage_drain_verifier_rejects_duplicate_bootstrap_replay_hashes() {
    let verification = qglake_handoff_lineage_verification();
    let mut drain = qglake_handoff_lineage_drain();
    let bootstrap = drain
        .events
        .iter_mut()
        .find(|event| event.event_type == "querygraph.bootstrap")
        .expect("bootstrap replay fixture");
    let duplicate_hash = bootstrap
        .replay_event_hashes
        .first()
        .expect("bootstrap replay hash")
        .clone();
    bootstrap.replay_event_hashes.push(duplicate_hash);

    let err = verify_qglake_lineage_drain(&drain, &verification, Some("did:example:agent"), 1)
        .expect_err("QGLake lineage drain should reject duplicate bootstrap replay hashes");

    assert!(err.to_string().contains("querygraph.bootstrap"));
    assert!(err.to_string().contains("replayEventHashes"));
    assert!(err.to_string().contains("duplicate-free"));
}

#[test]
fn qglake_lineage_drain_verifier_rejects_short_bootstrap_replay_hashes() {
    let verification = qglake_handoff_lineage_verification();
    let mut drain = qglake_handoff_lineage_drain();
    let bootstrap = drain
        .events
        .iter_mut()
        .find(|event| event.event_type == "querygraph.bootstrap")
        .expect("bootstrap replay fixture");
    bootstrap.replay_event_hashes = vec!["sha256:short".to_string()];

    let err = verify_qglake_lineage_drain(&drain, &verification, Some("did:example:agent"), 1)
        .expect_err("QGLake lineage drain should reject short bootstrap replay hashes");

    assert!(err.to_string().contains("querygraph.bootstrap"));
    assert!(err.to_string().contains("replayEventHashes"));
    assert!(err.to_string().contains("full SHA-256"));
}

#[test]
fn qglake_lineage_drain_verifier_rejects_short_read_authorization_hashes() {
    let verification = qglake_handoff_lineage_verification();
    let mut drain = qglake_handoff_lineage_drain();
    drain.authorization_receipt_hash = Some("sha256:lineage-read".to_string());

    let err = verify_qglake_lineage_drain(&drain, &verification, Some("did:example:agent"), 1)
        .expect_err("QGLake lineage drain should reject short read authorization hashes");

    assert!(err.to_string().contains("lineage drain read"));
    assert!(err.to_string().contains("full SHA-256 authorization"));
}

#[test]
fn qglake_lineage_drain_verifier_rejects_short_core_querygraph_hashes() {
    let verification = qglake_handoff_lineage_verification();
    let mut drain = qglake_handoff_lineage_drain();
    let bootstrap = drain
        .events
        .iter_mut()
        .find(|event| event.event_type == "querygraph.bootstrap")
        .expect("bootstrap replay fixture");
    bootstrap.bundle_hash = Some("sha256:bundle".to_string());

    let err = verify_qglake_lineage_drain(&drain, &verification, Some("did:example:agent"), 1)
        .expect_err("QGLake lineage drain should reject short core QueryGraph hashes");

    assert!(err.to_string().contains("QueryGraph hashes"));
    assert!(err.to_string().contains("full SHA-256"));
}

#[test]
fn qglake_lineage_drain_verifier_rejects_short_typedid_hashes() {
    let verification = qglake_handoff_lineage_verification();
    let mut drain = qglake_handoff_lineage_drain();
    drain.typedid_envelope_hash = Some("sha256:typedid-envelope".to_string());

    let err = verify_qglake_lineage_drain(&drain, &verification, Some("did:example:agent"), 1)
        .expect_err("QGLake lineage drain should reject short TypeDID hashes");

    assert!(err.to_string().contains("TypeDID envelope hash"));
    assert!(err.to_string().contains("full SHA-256"));
}

#[test]
fn qglake_lineage_drain_verifier_rejects_short_agent_proof_hashes() {
    let verification = qglake_handoff_lineage_verification();
    let mut drain = qglake_handoff_lineage_drain();
    let bootstrap = drain
        .events
        .iter_mut()
        .find(|event| event.event_type == "querygraph.bootstrap")
        .expect("bootstrap replay fixture");
    bootstrap.agent_delegation_hash = Some("sha256:delegation".to_string());

    let err = verify_qglake_lineage_drain(&drain, &verification, Some("did:example:agent"), 1)
        .expect_err("QGLake lineage drain should reject short agent proof hashes");

    assert!(err.to_string().contains("agent delegation hash"));
    assert!(err.to_string().contains("full SHA-256"));
}

#[test]
fn qglake_lineage_drain_verifier_rejects_duplicate_bootstrap_openlineage_hashes() {
    let verification = qglake_handoff_lineage_verification();
    let mut drain = qglake_handoff_lineage_drain();
    let bootstrap = drain
        .events
        .iter_mut()
        .find(|event| event.event_type == "querygraph.bootstrap")
        .expect("bootstrap replay fixture");
    let duplicate_hash = qglake_fixture_hash("duplicate-bootstrap-openlineage");
    bootstrap.replay_open_lineage_hashes = vec![duplicate_hash.clone(), duplicate_hash];

    let err = verify_qglake_lineage_drain(&drain, &verification, Some("did:example:agent"), 1)
        .expect_err("QGLake lineage drain should reject duplicate bootstrap OpenLineage hashes");

    assert!(err.to_string().contains("querygraph.bootstrap"));
    assert!(err.to_string().contains("openLineageHashes"));
    assert!(err.to_string().contains("duplicate-free"));
}

#[test]
fn qglake_lineage_drain_verifier_rejects_missing_config_defaults() {
    let verification = qglake_handoff_lineage_verification();
    let mut drain = qglake_handoff_lineage_drain_with_config();
    let config = drain
        .events
        .iter_mut()
        .find(|event| event.event_type == "catalog.config-read")
        .expect("catalog config replay fixture");
    config.catalog_config_defaults.clear();

    let err = verify_qglake_lineage_drain(&drain, &verification, Some("did:example:agent"), 1)
        .expect_err("QGLake lineage drain should reject missing config defaults");

    assert!(err.to_string().contains("catalog config replay"));
    assert!(err.to_string().contains("missing config defaults"));
}

#[test]
fn qglake_lineage_drain_verifier_rejects_unsupported_config_v4_default() {
    let verification = qglake_handoff_lineage_verification();
    let mut drain = qglake_handoff_lineage_drain_with_config();
    let config = drain
        .events
        .iter_mut()
        .find(|event| event.event_type == "catalog.config-read")
        .expect("catalog config replay fixture");
    config.catalog_config_defaults.push(ConfigEntry::new(
        "lakecat.format.v4.typed-sail.preview",
        "available",
    ));

    let err = verify_qglake_lineage_drain(&drain, &verification, Some("did:example:agent"), 1)
        .expect_err("QGLake lineage drain should reject unsupported config v4 defaults");

    assert!(err.to_string().contains("catalog config replay"));
    assert!(err.to_string().contains("unsupported v4 bridge keys"));
}

#[test]
fn qglake_lineage_drain_verifier_rejects_missing_config_endpoint() {
    for missing_endpoint in [
        "GET /querygraph/v1/bootstrap",
        "POST /catalog/v1/namespaces/{namespace}/tables/{table}/plan",
        "POST /catalog/v1/{warehouse}/namespaces/{namespace}/tables/{table}/plan",
        "POST /catalog/v1/namespaces/{namespace}/tables/{table}/fetch-scan-tasks",
        "POST /catalog/v1/{warehouse}/namespaces/{namespace}/tables/{table}/fetch-scan-tasks",
        "GET /catalog/v1/namespaces/{namespace}/tables/{table}/credentials",
        "GET /catalog/v1/{warehouse}/namespaces/{namespace}/tables/{table}/credentials",
    ] {
        let verification = qglake_handoff_lineage_verification();
        let mut drain = qglake_handoff_lineage_drain_with_config();
        let config = drain
            .events
            .iter_mut()
            .find(|event| event.event_type == "catalog.config-read")
            .expect("catalog config replay fixture");
        config
            .catalog_config_endpoints
            .retain(|endpoint| endpoint != missing_endpoint);

        let err = verify_qglake_lineage_drain(&drain, &verification, Some("did:example:agent"), 1)
            .expect_err("QGLake lineage drain should reject missing config endpoints");

        assert!(err.to_string().contains("catalog config replay"));
        assert!(
            err.to_string()
                .contains(&format!("endpoints must include {missing_endpoint}"))
        );
    }
}

#[test]
fn qglake_lineage_drain_verifier_accepts_empty_table_commit_history() {
    let verification = qglake_handoff_lineage_verification();
    let mut drain = qglake_handoff_lineage_drain();
    let commit_history = drain
        .events
        .iter_mut()
        .find(|event| event.event_type == "table.commits-listed")
        .expect("commit history replay fixture");
    commit_history.table_commit_count = Some(0);
    commit_history.table_commit_sequence_numbers.clear();
    commit_history.table_commit_hashes.clear();

    verify_qglake_lineage_drain(&drain, &verification, Some("did:example:agent"), 1)
        .expect("QGLake lineage drain should accept explicit zero-count commit-history proof");
}

#[test]
fn qglake_lineage_drain_verifier_requires_replay_authorization_actions() {
    let verification = qglake_handoff_lineage_verification();
    let mut drain = qglake_handoff_lineage_drain();
    let scan_plan = drain
        .events
        .iter_mut()
        .find(|event| event.event_type == "table.scan-planned")
        .expect("scan plan replay fixture");
    scan_plan.authorization_receipt_action = None;

    let err = verify_qglake_lineage_drain(&drain, &verification, Some("did:example:agent"), 1)
        .expect_err("QGLake lineage drain should reject missing replay action evidence");

    assert!(err.to_string().contains("table.scan-planned"));
    assert!(
        err.to_string()
            .contains("is missing authorization receipt action")
    );
}

#[test]
fn qglake_lineage_drain_verifier_rejects_mismatched_replay_authorization_actions() {
    let verification = qglake_handoff_lineage_verification();
    let mut drain = qglake_handoff_lineage_drain();
    let commit_history = drain
        .events
        .iter_mut()
        .find(|event| event.event_type == "table.commits-listed")
        .expect("table commit history replay fixture");
    commit_history.authorization_receipt_action = Some("table-commit".to_string());

    let err = verify_qglake_lineage_drain(&drain, &verification, Some("did:example:agent"), 1)
        .expect_err("QGLake lineage drain should reject mismatched replay action evidence");

    assert!(err.to_string().contains("table.commits-listed"));
    assert!(err.to_string().contains("authorization receipt action"));
    assert!(err.to_string().contains("expected table-load"));
}

#[test]
fn qglake_lineage_drain_verifier_requires_management_receipt_hash_shape() {
    let verification = qglake_handoff_lineage_verification();
    let mut drain = qglake_handoff_lineage_drain();
    let policy_list = drain
        .events
        .iter_mut()
        .find(|event| event.event_type == "policy-binding.listed")
        .expect("policy list replay fixture");
    policy_list.replay_event_hashes = vec!["not-a-sha256-hash".to_string()];

    let err = verify_qglake_lineage_drain(&drain, &verification, Some("did:example:agent"), 1)
        .expect_err("QGLake lineage drain should reject malformed management receipt hashes");

    assert!(err.to_string().contains("policy list replay"));
    assert!(err.to_string().contains("SHA-256 receipt hashes"));
}

#[test]
fn qglake_lineage_drain_verifier_requires_management_authorization_hash_shape() {
    let verification = qglake_handoff_lineage_verification();
    let mut drain = qglake_handoff_lineage_drain();
    let server_list = drain
        .events
        .iter_mut()
        .find(|event| event.event_type == "server.listed")
        .expect("server list replay fixture");
    server_list.authorization_receipt_hash = Some("sha256:server-list-auth".to_string());

    let err = verify_qglake_lineage_drain(&drain, &verification, Some("did:example:agent"), 1)
        .expect_err("QGLake lineage drain should reject short management authorization hashes");

    assert!(err.to_string().contains("server list replay"));
    assert!(err.to_string().contains("authorization receipt hash"));
    assert!(err.to_string().contains("full SHA-256"));
}

#[test]
fn qglake_lineage_drain_verifier_requires_management_principal_evidence() {
    let verification = qglake_handoff_lineage_verification();
    let mut drain = qglake_handoff_lineage_drain();
    let warehouse_list = drain
        .events
        .iter_mut()
        .find(|event| event.event_type == "warehouse.listed")
        .expect("warehouse list replay fixture");
    warehouse_list.principal_subject = None;

    let err = verify_qglake_lineage_drain(&drain, &verification, Some("did:example:agent"), 1)
        .expect_err("QGLake lineage drain should reject actorless management replay");

    assert!(err.to_string().contains("warehouse list replay"));
    assert!(err.to_string().contains("principal evidence"));
}

#[test]
fn qglake_lineage_drain_verifier_rejects_missing_policy_upsert() {
    let verification = qglake_handoff_lineage_verification();
    let mut drain = qglake_handoff_lineage_drain();
    drain
        .events
        .retain(|event| event.event_type != "policy-binding.upserted");
    drain
        .event_types
        .retain(|event_type| event_type != "policy-binding.upserted");
    drain.delivered = drain.events.len();
    drain.graph_events = drain.events.iter().map(|event| event.graph_events).sum();
    drain.lineage_events = drain.events.iter().map(|event| event.lineage_events).sum();

    let err = verify_qglake_lineage_drain(&drain, &verification, Some("did:example:agent"), 1)
        .expect_err("QGLake lineage drain should reject missing policy upsert proof");

    assert!(err.to_string().contains("policy binding upsert evidence"));
}

#[test]
fn qglake_lineage_drain_verifier_rejects_policy_upsert_odrl_hash_drift() {
    let verification = qglake_handoff_lineage_verification();
    let mut drain = qglake_handoff_lineage_drain();
    let policy_upsert = drain
        .events
        .iter_mut()
        .find(|event| event.event_type == "policy-binding.upserted")
        .expect("policy upsert replay fixture");
    policy_upsert.policy_odrl_hash = Some("sha256:short-policy-odrl".to_string());

    let err = verify_qglake_lineage_drain(&drain, &verification, Some("did:example:agent"), 1)
        .expect_err("QGLake lineage drain should reject malformed policy ODRL proof");

    assert!(err.to_string().contains("policy binding upsert replay"));
    assert!(err.to_string().contains("ODRL hash"));
}

#[test]
fn qglake_lineage_drain_verifier_rejects_policy_upsert_authorization_action_drift() {
    let verification = qglake_handoff_lineage_verification();
    let mut drain = qglake_handoff_lineage_drain();
    let policy_upsert = drain
        .events
        .iter_mut()
        .find(|event| event.event_type == "policy-binding.upserted")
        .expect("policy upsert replay fixture");
    policy_upsert.authorization_receipt_action = Some("table-load".to_string());

    let err = verify_qglake_lineage_drain(&drain, &verification, Some("did:example:agent"), 1)
        .expect_err("QGLake lineage drain should reject policy upsert action drift");

    assert!(err.to_string().contains("policy binding upsert replay"));
    assert!(err.to_string().contains("policy-manage"));
}

#[test]
fn qglake_lineage_drain_verifier_rejects_policy_upsert_authorization_hash_drift() {
    let verification = qglake_handoff_lineage_verification();
    let mut drain = qglake_handoff_lineage_drain();
    let policy_upsert = drain
        .events
        .iter_mut()
        .find(|event| event.event_type == "policy-binding.upserted")
        .expect("policy upsert replay fixture");
    policy_upsert.authorization_receipt_hash = Some("sha256:short-policy-auth".to_string());

    let err = verify_qglake_lineage_drain(&drain, &verification, Some("did:example:agent"), 1)
        .expect_err("QGLake lineage drain should reject malformed policy upsert receipt hash");

    assert!(err.to_string().contains("policy binding upsert replay"));
    assert!(err.to_string().contains("authorization receipt hash"));
}

#[test]
fn qglake_lineage_drain_verifier_rejects_policy_upsert_missing_principal_proof() {
    let verification = qglake_handoff_lineage_verification();
    let mut drain = qglake_handoff_lineage_drain();
    let policy_upsert = drain
        .events
        .iter_mut()
        .find(|event| event.event_type == "policy-binding.upserted")
        .expect("policy upsert replay fixture");
    policy_upsert.principal_kind = None;

    let err = verify_qglake_lineage_drain(&drain, &verification, Some("did:example:agent"), 1)
        .expect_err("QGLake lineage drain should reject actorless policy upsert proof");

    assert!(err.to_string().contains("policy binding upsert replay"));
    assert!(err.to_string().contains("principal evidence"));
}

#[test]
fn qglake_lineage_drain_verifier_rejects_count_drift() {
    let verification = qglake_handoff_lineage_verification();
    let mut drain = qglake_handoff_lineage_drain();
    drain.delivered -= 1;

    let err = verify_qglake_lineage_drain(&drain, &verification, Some("did:example:agent"), 1)
        .expect_err("QGLake lineage drain should reject delivered count drift");

    assert!(err.to_string().contains("qglake lineage drain"));
    assert!(err.to_string().contains("delivered count"));
    assert!(err.to_string().contains("eventTypes count"));
}
