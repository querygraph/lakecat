use super::common::*;
use crate::*;

#[test]
fn qglake_namespace_validator_accepts_matching_namespace() {
    let response = ListNamespacesResponse {
        namespaces: vec![
            vec!["default".to_string()],
            vec!["demo".to_string(), "ops".to_string()],
        ],
    };

    assert!(namespace_list_contains(
        &response,
        &["demo".to_string(), "ops".to_string()]
    ));
    assert!(!namespace_list_contains(
        &response,
        &["missing".to_string()]
    ));
}

#[test]
fn qglake_replay_artifact_verifier_accepts_matching_bundle_and_drain() {
    let projection = qglake_querygraph_projection(qglake_odrl_policy("events"));
    let output = serde_json::json!({
        "name": "events",
        "facets": {
            "queryGraph_catalog": {
                "stableId": projection.stable_id.clone(),
                "metadataLocation": projection.metadata_location.clone()
            }
        }
    });
    let bundle = qglake_querygraph_bundle(vec![projection], vec![output]);
    let verification = bundle.verify_manifest().unwrap();
    let policy_binding_count = qglake_policy_binding_count(&bundle);
    let events = vec![
        qglake_bootstrap_lineage_summary_for(&verification, policy_binding_count),
        qglake_restricted_credential_summary(),
        qglake_human_credential_summary(),
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
    ];
    let drain = LineageDrainResponse {
        delivered: events.len(),
        event_types: events
            .iter()
            .map(|event| event.event_type.clone())
            .collect(),
        graph_events: events.iter().map(|event| event.graph_events).sum(),
        lineage_events: events.iter().map(|event| event.lineage_events).sum(),
        principal_subject: Some("did:example:agent".to_string()),
        principal_kind: Some("agent".to_string()),
        authorization_receipt_hash: Some(qglake_fixture_hash("lineage-read")),
        authorization_receipt_action: Some("lineage-read".to_string()),
        request_identity_state: Some("verified".to_string()),
        request_identity_source: Some("x-lakecat-agent-did".to_string()),
        typedid_envelope_hash: None,
        typedid_proof_hash: None,
        events,
    };

    let replay_verification =
        verify_qglake_replay_artifacts(&bundle, &drain, Some("did:example:agent"))
            .expect("matching saved bundle and lineage drain should verify");
    assert_eq!(replay_verification.bundle_hash, verification.bundle_hash);
    assert_eq!(
        replay_verification.querygraph_import_hash,
        verification.querygraph_import_hash
    );
    let replay_json = qglake_replay_verification_json(
        &replay_verification,
        qglake_scan_replay_line(&drain),
        qglake_management_replay_line(&drain),
        qglake_credential_replay_line(&drain, Some("did:example:agent")),
        qglake_table_commit_history_replay_line(&drain),
        qglake_replay_evidence_json(&drain, Some("did:example:agent"), &replay_verification),
    );
    assert_eq!(
        replay_json["schema-version"],
        json!("lakecat.qglake.replay-verification.v1")
    );
    assert_eq!(
        replay_json["replay-evidence"]["scan"]["planTaskCount"],
        json!(1)
    );
    assert_eq!(
        replay_json["replay-evidence"]["requestIdentity"]["principalSubject"],
        json!("did:example:agent")
    );
    assert_eq!(
        replay_json["replay-evidence"]["requestIdentity"]["principalKind"],
        json!("agent")
    );
    assert_eq!(
        replay_json["replay-evidence"]["requestIdentity"]["requestIdentitySource"],
        json!("x-lakecat-agent-did")
    );
    assert_eq!(
        replay_json["replay-evidence"]["requestIdentity"]["requestIdentityState"],
        json!("verified")
    );
    assert_eq!(
        replay_json["replay-evidence"]["requestIdentity"]["authorizationReceiptHash"],
        json!(qglake_fixture_hash("lineage-read"))
    );
    assert_eq!(
        replay_json["replay-evidence"]["queryGraphBootstrap"]["bundleHash"],
        json!(verification.bundle_hash)
    );
    assert_eq!(
        replay_json["replay-evidence"]["queryGraphBootstrap"]["queryGraphImportHash"],
        json!(verification.querygraph_import_hash)
    );
    assert_eq!(
        replay_json["replay-evidence"]["queryGraphBootstrap"]["policyBindingCount"],
        json!(1)
    );
    assert_eq!(
        replay_json["replay-evidence"]["queryGraphBootstrap"]["agentDelegationHash"],
        json!(qglake_fixture_hash("delegation"))
    );
    assert_eq!(
        replay_json["replay-evidence"]["queryGraphBootstrap"]["agentSummarySignatureHash"],
        json!(qglake_fixture_hash("summary"))
    );
    assert_eq!(
        replay_json["replay-evidence"]["management"]["policyBindingCount"],
        json!(1)
    );
    assert_eq!(
        replay_json["replay-evidence"]["management"]["storageProfileUpsert"]["provider"],
        json!("file")
    );
    assert_eq!(
        replay_json["replay-evidence"]["management"]["storageProfileUpsert"]["issuanceMode"],
        json!("local-file-no-secret")
    );
    assert_eq!(
        replay_json["replay-evidence"]["management"]["storageProfileUpsert"]["locationPrefixHash"],
        json!("sha256:2222222222222222222222222222222222222222222222222222222222222222")
    );
    assert_eq!(
        replay_json["replay-evidence"]["management"]["storageProfileUpsert"]["secretRefPresent"],
        json!(false)
    );
    assert_eq!(
        replay_json["replay-evidence"]["credentials"]["restricted"]["blockReason"],
        json!(QGLAKE_RESTRICTED_CREDENTIAL_BLOCK_REASON)
    );
    assert_eq!(
        replay_json["replay-evidence"]["credentials"]["trustedHuman"]["rawCredentialExceptionAllowed"],
        json!(true)
    );
    assert_eq!(
        replay_json["replay-evidence"]["tableCommitHistory"]["sequenceNumbers"],
        json!([1])
    );
    assert_eq!(
        replay_json["replay-evidence"]["views"]["viewCount"],
        json!(0)
    );

    let view_verification = qglake_view_lineage_verification();
    let mut bootstrap_with_view = qglake_bootstrap_lineage_summary();
    bootstrap_with_view.view_artifact_count = 1;
    bootstrap_with_view.view_version_receipt_hashes =
        vec![qglake_fixture_hash("view-version-receipt")];
    let mut namespace_receipt_chain = qglake_view_receipt_chain_lineage_summary();
    namespace_receipt_chain.view_version_receipt_chain_hashes =
        vec![qglake_fixture_hash("namespace-receipt-chain")];
    namespace_receipt_chain.view_version_receipt_chain_verified_count = 1;
    let view_drain = LineageDrainResponse {
        delivered: 15,
        event_types: vec![
            "table.scan-planned".to_string(),
            "table.scan-tasks-fetched".to_string(),
            "credentials.vend-attempted".to_string(),
            "credentials.vend-attempted".to_string(),
            "view.upserted".to_string(),
            "view.dropped".to_string(),
            "view.version-receipts-listed".to_string(),
            "view.version-receipt-chains-listed".to_string(),
            "policy-binding.listed".to_string(),
            "storage-profile.listed".to_string(),
            "server.listed".to_string(),
            "project.listed".to_string(),
            "warehouse.listed".to_string(),
            "table.commits-listed".to_string(),
            "querygraph.bootstrap".to_string(),
        ],
        graph_events: 4,
        lineage_events: 16,
        principal_subject: Some("did:example:agent".to_string()),
        principal_kind: Some("agent".to_string()),
        authorization_receipt_hash: Some(qglake_fixture_hash("lineage-read")),
        authorization_receipt_action: Some("lineage-read".to_string()),
        request_identity_state: Some("verified".to_string()),
        request_identity_source: Some("x-lakecat-agent-did".to_string()),
        typedid_envelope_hash: None,
        typedid_proof_hash: None,
        events: vec![
            bootstrap_with_view,
            qglake_restricted_credential_summary(),
            qglake_human_credential_summary(),
            qglake_view_lineage_summary(),
            qglake_view_drop_lineage_summary(),
            qglake_view_tombstone_receipt_lineage_summary(),
            namespace_receipt_chain,
            qglake_policy_list_lineage_summary(),
            qglake_storage_profile_list_lineage_summary(),
            qglake_storage_profile_upsert_lineage_summary(),
            qglake_server_list_lineage_summary(),
            qglake_project_list_lineage_summary(),
            qglake_warehouse_list_lineage_summary(),
            qglake_table_commit_history_lineage_summary(),
            qglake_scan_planned_lineage_summary(),
            qglake_scan_tasks_fetched_lineage_summary(),
        ],
    };
    let view_replay_json =
        qglake_replay_evidence_json(&view_drain, Some("did:example:agent"), &view_verification);
    assert_eq!(
        view_replay_json["views"]["views"][0]["stableId"],
        json!("lakecat:view:local:default:active_customers")
    );
    assert_eq!(
        view_replay_json["views"]["views"][0]["acceptedViewVersion"],
        json!(2)
    );
    assert_eq!(
        view_replay_json["views"]["views"][0]["expectedViewVersion"],
        json!(1)
    );
    assert_eq!(
        view_replay_json["views"]["views"][0]["acceptedReceiptHash"],
        json!(qglake_fixture_hash("view-version-receipt"))
    );
    assert_eq!(
        view_replay_json["views"]["views"][0]["acceptedReceiptChainHash"],
        json!(qglake_fixture_hash("view-receipt-chain"))
    );
    assert_eq!(
        view_replay_json["views"]["tombstoneReceipts"][0]["expectedViewVersion"],
        json!(2)
    );
    assert_eq!(
        view_replay_json["views"]["tombstoneReceipts"][0]["receiptHashes"],
        json!([qglake_fixture_hash("view-drop-receipt")])
    );
    assert_eq!(
        view_replay_json["views"]["receiptChains"][0]["chainHashes"],
        json!([
            qglake_fixture_hash("namespace-receipt-chain"),
            qglake_fixture_hash("view-receipt-chain")
        ])
    );
    assert_eq!(
        view_replay_json["views"]["receiptChains"][0]["verifiedChainCount"],
        json!(1)
    );
    assert_eq!(
        view_replay_json["views"]["receiptChains"][0]["chains"][0]["latestOperation"],
        json!("drop")
    );
    assert_eq!(
        view_replay_json["views"]["receiptChains"][0]["chains"][0]["receipts"][2]["previousReceiptHash"],
        json!(qglake_fixture_hash("view-version-receipt"))
    );
}

#[test]
fn qglake_commit_history_verifier_requires_iceberg_summary() {
    let record = qglake_table_commit_record_summary();
    verify_qglake_table_commit_record_evidence(&record, "local", "default", "events")
        .expect("QGLake commit history should accept compact Iceberg summary evidence");

    let mut missing_summary = record;
    missing_summary.format_version = None;
    missing_summary.snapshot_id = None;
    let err =
        verify_qglake_table_commit_record_evidence(&missing_summary, "local", "default", "events")
            .expect_err("QGLake commit history should require format/snapshot summary evidence");
    assert!(err.to_string().contains(
        "qglake table commit history for local.default.events is missing Iceberg format/snapshot summary evidence"
    ));
}

#[test]
fn qglake_commit_history_verifier_rejects_short_pointer_hashes() {
    let mut record = qglake_table_commit_record_summary();
    record.request_hash = "sha256:request".to_string();

    let err = verify_qglake_table_commit_record_evidence(&record, "local", "default", "events")
        .expect_err("QGLake commit history should reject short pointer-log hashes");

    assert!(err.to_string().contains(
        "qglake table commit history for local.default.events must expose full SHA-256 pointer-log hash evidence"
    ));
}

#[test]
fn qglake_fixture_policy_installs_read_restriction() {
    let policy = qglake_odrl_policy("events");
    assert_eq!(
        policy["lakecat:read-restriction"]["allowed-columns"],
        serde_json::json!(["event_id", "occurred_at", "severity"])
    );
    assert_eq!(
        policy["lakecat:read-restriction"]["row-predicate"],
        serde_json::json!({
            "type": "not-eq",
            "term": "severity",
            "value": "debug"
        })
    );
    assert_eq!(
        policy["lakecat:read-restriction"]["purpose"],
        serde_json::json!("qglake-agent-demo")
    );
    assert_eq!(
        policy["lakecat:read-restriction"]["max-credential-ttl-seconds"],
        serde_json::json!(300)
    );
    let restriction = lakecat_security::ReadRestriction::from_odrl_policies([&policy])
        .expect("qglake policy should parse as LakeCat read restriction");
    assert_eq!(
        restriction.allowed_columns.as_deref(),
        Some(
            &[
                "event_id".to_string(),
                "occurred_at".to_string(),
                "severity".to_string()
            ][..]
        )
    );
    assert_eq!(
        restriction.row_predicate,
        Some(serde_json::json!({
            "type": "not-eq",
            "term": "severity",
            "value": "debug"
        }))
    );
    assert_eq!(restriction.purpose.as_deref(), Some("qglake-agent-demo"));
    assert_eq!(restriction.max_credential_ttl_seconds, Some(300));
}

#[test]
fn qglake_commit_history_replay_line_summarizes_verified_evidence() {
    let line = qglake_table_commit_history_replay_line(&LineageDrainResponse {
        delivered: 1,
        event_types: vec!["table.commits-listed".to_string()],
        graph_events: 1,
        lineage_events: 1,
        principal_subject: Some("did:example:agent".to_string()),
        principal_kind: Some("agent".to_string()),
        authorization_receipt_hash: Some(qglake_fixture_hash("lineage-read")),
        authorization_receipt_action: Some("lineage-read".to_string()),
        request_identity_state: Some("verified".to_string()),
        request_identity_source: Some("x-lakecat-agent-did".to_string()),
        typedid_envelope_hash: None,
        typedid_proof_hash: None,
        events: vec![qglake_table_commit_history_lineage_summary()],
    })
    .expect("commit-history replay line should be present");

    assert_eq!(
        line,
        format!(
            "table commit history commits=1 sequences=1 hashes={} graph_events=1",
            qglake_fixture_hash("table-commit")
        )
    );
}

#[test]
fn qglake_management_replay_line_summarizes_verified_evidence() {
    let line = qglake_management_replay_line(&LineageDrainResponse {
        delivered: 6,
        event_types: vec![
            "server.listed".to_string(),
            "project.listed".to_string(),
            "warehouse.listed".to_string(),
            "policy-binding.listed".to_string(),
            "policy-binding.upserted".to_string(),
            "storage-profile.listed".to_string(),
        ],
        graph_events: 0,
        lineage_events: 6,
        principal_subject: Some("did:example:agent".to_string()),
        principal_kind: Some("agent".to_string()),
        authorization_receipt_hash: Some(qglake_fixture_hash("lineage-read")),
        authorization_receipt_action: Some("lineage-read".to_string()),
        request_identity_state: Some("verified".to_string()),
        request_identity_source: Some("x-lakecat-agent-did".to_string()),
        typedid_envelope_hash: None,
        typedid_proof_hash: None,
        events: vec![
            qglake_server_list_lineage_summary(),
            qglake_project_list_lineage_summary(),
            qglake_warehouse_list_lineage_summary(),
            qglake_policy_list_lineage_summary(),
            qglake_policy_upsert_lineage_summary(),
            qglake_storage_profile_list_lineage_summary(),
            qglake_storage_profile_upsert_lineage_summary(),
        ],
    })
    .expect("management replay line should be present");

    assert_eq!(
        line,
        "management replay servers=1 projects=1 warehouses=1 policies=1 policy_upserts=1 policy=agent-columns:odrl_hash=sha256:8f0ab09903123af3536f8bd6b9ef59a2429fb46b2235c44c8865aac5b388db1c storage_profiles=1 storage_profile_upserts=1 credential_root=events-local:file:local-file-no-secret:location_prefix_hash=sha256:2222222222222222222222222222222222222222222222222222222222222222:secret_ref=none"
    );

    let mut upsert_without_location_hash = qglake_storage_profile_upsert_lineage_summary();
    upsert_without_location_hash.storage_profile_location_prefix_hash = None;
    assert!(
        qglake_management_replay_line(&LineageDrainResponse {
            delivered: 5,
            event_types: vec![
                "server.listed".to_string(),
                "project.listed".to_string(),
                "warehouse.listed".to_string(),
                "policy-binding.listed".to_string(),
                "policy-binding.upserted".to_string(),
                "storage-profile.listed".to_string(),
            ],
            graph_events: 0,
            lineage_events: 5,
            principal_subject: Some("did:example:agent".to_string()),
            principal_kind: Some("agent".to_string()),
            authorization_receipt_hash: Some(qglake_fixture_hash("lineage-read")),
            authorization_receipt_action: Some("lineage-read".to_string()),
            request_identity_state: Some("verified".to_string()),
            request_identity_source: Some("x-lakecat-agent-did".to_string()),
            typedid_envelope_hash: None,
            typedid_proof_hash: None,
            events: vec![
                qglake_server_list_lineage_summary(),
                qglake_project_list_lineage_summary(),
                qglake_warehouse_list_lineage_summary(),
                qglake_policy_list_lineage_summary(),
                qglake_policy_upsert_lineage_summary(),
                qglake_storage_profile_list_lineage_summary(),
                upsert_without_location_hash,
            ],
        })
        .is_none(),
        "management replay line should require storage-profile location hash"
    );

    let mut upsert_with_contradictory_secret_ref = qglake_storage_profile_upsert_lineage_summary();
    upsert_with_contradictory_secret_ref.storage_profile_secret_ref_provider =
        Some("vault".to_string());
    assert!(
        qglake_management_replay_line(&LineageDrainResponse {
            delivered: 5,
            event_types: vec![
                "server.listed".to_string(),
                "project.listed".to_string(),
                "warehouse.listed".to_string(),
                "policy-binding.listed".to_string(),
                "policy-binding.upserted".to_string(),
                "storage-profile.listed".to_string(),
            ],
            graph_events: 0,
            lineage_events: 5,
            principal_subject: Some("did:example:agent".to_string()),
            principal_kind: Some("agent".to_string()),
            authorization_receipt_hash: Some(qglake_fixture_hash("lineage-read")),
            authorization_receipt_action: Some("lineage-read".to_string()),
            request_identity_state: Some("verified".to_string()),
            request_identity_source: Some("x-lakecat-agent-did".to_string()),
            typedid_envelope_hash: None,
            typedid_proof_hash: None,
            events: vec![
                qglake_server_list_lineage_summary(),
                qglake_project_list_lineage_summary(),
                qglake_warehouse_list_lineage_summary(),
                qglake_policy_list_lineage_summary(),
                qglake_policy_upsert_lineage_summary(),
                qglake_storage_profile_list_lineage_summary(),
                upsert_with_contradictory_secret_ref,
            ],
        })
        .is_none(),
        "management replay line should reject secret-ref provider without presence"
    );

    let mut upsert_with_short_secret_ref_hash = qglake_storage_profile_upsert_lineage_summary();
    upsert_with_short_secret_ref_hash.storage_profile_secret_ref_present = Some(true);
    upsert_with_short_secret_ref_hash.storage_profile_secret_ref_provider =
        Some("typesec".to_string());
    upsert_with_short_secret_ref_hash.storage_profile_secret_ref_hash =
        Some("sha256:short-secret-ref".to_string());
    assert!(
        qglake_management_replay_line(&LineageDrainResponse {
            delivered: 5,
            event_types: vec![
                "server.listed".to_string(),
                "project.listed".to_string(),
                "warehouse.listed".to_string(),
                "policy-binding.listed".to_string(),
                "policy-binding.upserted".to_string(),
                "storage-profile.listed".to_string(),
            ],
            graph_events: 0,
            lineage_events: 5,
            principal_subject: Some("did:example:agent".to_string()),
            principal_kind: Some("agent".to_string()),
            authorization_receipt_hash: Some(qglake_fixture_hash("lineage-read")),
            authorization_receipt_action: Some("lineage-read".to_string()),
            request_identity_state: Some("verified".to_string()),
            request_identity_source: Some("x-lakecat-agent-did".to_string()),
            typedid_envelope_hash: None,
            typedid_proof_hash: None,
            events: vec![
                qglake_server_list_lineage_summary(),
                qglake_project_list_lineage_summary(),
                qglake_warehouse_list_lineage_summary(),
                qglake_policy_list_lineage_summary(),
                qglake_policy_upsert_lineage_summary(),
                qglake_storage_profile_list_lineage_summary(),
                upsert_with_short_secret_ref_hash,
            ],
        })
        .is_none(),
        "management replay line should reject short secret-ref hash evidence"
    );
}

#[test]
fn qglake_storage_profile_upsert_replay_rejects_short_location_prefix_hash() {
    let mut event = qglake_storage_profile_upsert_lineage_summary();
    event.storage_profile_location_prefix_hash = Some("sha256:storage-location-prefix".to_string());

    let err = verify_qglake_storage_profile_upsert_replay(&event)
        .expect_err("storage-profile replay should reject short location-prefix hash evidence");

    let err = err.to_string();
    assert!(
        err.contains(
            "qglake lineage drain storage profile upsert replay did not expose redacted credential-root evidence"
        ) || err.contains("storage-profile evidence does not match storage profile upsert replay"),
        "{err}"
    );
}

#[test]
fn qglake_storage_profile_upsert_replay_rejects_short_authorization_hash() {
    let mut event = qglake_storage_profile_upsert_lineage_summary();
    event.authorization_receipt_hash = Some("sha256:short-storage-profile-auth".to_string());

    let err = verify_qglake_storage_profile_upsert_replay(&event)
        .expect_err("storage-profile replay should reject short authorization hash evidence");

    assert!(err.to_string().contains("storage profile upsert replay"));
    assert!(err.to_string().contains("authorization receipt hash"));
}

#[test]
fn qglake_storage_profile_upsert_replay_rejects_authorization_action_drift() {
    let mut event = qglake_storage_profile_upsert_lineage_summary();
    event.authorization_receipt_hash =
        Some(qglake_fixture_hash("storage-profile-upsert-authorization"));
    event.authorization_receipt_action = Some("table-load".to_string());

    let err = verify_qglake_storage_profile_upsert_replay(&event)
        .expect_err("storage-profile replay should reject authorization action drift");

    let err = err.to_string();
    assert!(err.contains("storage profile upsert"), "{err}");
    assert!(err.contains("authorization receipt action"), "{err}");
    assert!(err.contains("storage-profile-manage"), "{err}");
}
